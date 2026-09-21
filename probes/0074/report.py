#!/usr/bin/env python3
"""Record 0074 report: the frozen 0073 generator against Polars 2.0 rc2.

  report.py <probe dir> <repo root>            # tables from out/status.json and the run's outputs
  report.py --self-test                        # the identity controls

Entries are identified by canonical path AND signature; a path group is
matched exact-signature first, then a reshape is reported only when one
unmatched entry remains on each side of a group. Sections are kept apart:
pins, denominators, API delta, accounting under the unchanged generator,
compilation, oracle differences, excluded cases, a census of fixture-free
receivers, and the labeled join diagnostic.
"""
import collections, json, os, re, sys

API = {"polars_core", "polars_plan", "polars_lazy", "polars_io", "polars_ops", "polars_time", "polars_dtype", "polars_schema", "polars_error"}

def sig(c):
    """Parameters, return, and the generic parameters with their canonical
    bounds: a changed bound (a callback's mutability, an added `Clone`) is a
    reshape."""
    generics = "; ".join(f"{n}: {b}" for n, b in sorted(c.get("generics_canonical") or []))
    return "(" + ", ".join(p["ty_canonical"] for p in c["params"]) + ") -> " + (c.get("ret_canonical") or "()") + (f" where {generics}" if generics else "")

def identity(c):
    """Cross-release identity: path, kind, receiver and semantic signature
    including generic bounds. Trait methods carry the trait in the path."""
    return (c["canonical_path"], c["kind"], c["receiver"], sig(c))

def eligible(inv):
    return [c for c in inv["callables"] if c["bucket"] not in ("unsupported", "unknown") and c["krate"] in API]

def delta(base, other):
    """Match by full identity first; then within a path group pair one
    remaining entry per side as a reshape; the rest are added/removed.
    Multiplicity is preserved: counts reconcile to the input lengths."""
    ga, gb = collections.defaultdict(list), collections.defaultdict(list)
    for c in base: ga[c["canonical_path"]].append(c)
    for c in other: gb[c["canonical_path"]].append(c)
    same, reshaped, removed, added = [], [], [], []
    for path in set(ga) | set(gb):
        xa, xb = list(ga.get(path, [])), list(gb.get(path, []))
        # exact identity matches, with multiplicity
        ida = collections.Counter(identity(c) for c in xa)
        idb = collections.Counter(identity(c) for c in xb)
        for k in list(ida):
            n = min(ida[k], idb.get(k, 0))
            same.extend([k] * n)
            ida[k] -= n; idb[k] -= n
        # the entries left after exact matching, multiplicity respected
        ra, rb = [], []
        for c in xa:
            if ida[identity(c)] > 0:
                ida[identity(c)] -= 1; ra.append(c)
        for c in xb:
            if idb[identity(c)] > 0:
                idb[identity(c)] -= 1; rb.append(c)
        if len(ra) == 1 and len(rb) == 1 and ra[0]["kind"] == rb[0]["kind"] and ra[0]["receiver"] == rb[0]["receiver"]:
            reshaped.append((ra[0], rb[0]))
        else:
            removed.extend(ra); added.extend(rb)
    assert len(same) + len(reshaped) + len(removed) == len(base), "base entries lost in the delta"
    assert len(same) + len(reshaped) + len(added) == len(other), "rc2 entries lost in the delta"
    return same, reshaped, removed, added

def self_test():
    # the review's counterexample: two baseline par_iter entries, one kept at rc2
    def mk(path, ret, kind="inherent", receiver="&self", generics=None):
        return {"canonical_path": path, "kind": kind, "receiver": receiver, "params": [], "ret_canonical": ret, "bucket": "generic", "krate": "polars_core", "generics_canonical": generics or []}
    p = "polars_core::chunked_array::ChunkedArray::par_iter"
    base = [mk(p, "impl ParallelIterator<Item = Option<&str>>"), mk(p, "impl ParallelIterator<Item = Option<Series>>")]
    rc2 = [mk(p, "impl ParallelIterator<Item = Option<Series>>")]
    same, reshaped, removed, added = delta(base, rc2)
    assert (len(same), len(reshaped), len(removed), len(added)) == (1, 0, 1, 0), (same, reshaped, removed, added)
    # a genuine reshape: one entry per side with different signatures
    base = [mk("a::f", "u32")]; rc2 = [mk("a::f", "u64")]
    assert [len(x) for x in delta(base, rc2)] == [0, 1, 0, 0]
    # multiplicity survives: two identical entries stay two
    base = [mk("a::g", "T"), mk("a::g", "T")]; rc2 = [mk("a::g", "T"), mk("a::g", "T")]
    assert [len(x) for x in delta(base, rc2)] == [2, 0, 0, 0]
    # a receiver change is not paired as a reshape
    base = [mk("a::h", "T", receiver="self")]; rc2 = [mk("a::h", "T", receiver="&self")]
    assert [len(x) for x in delta(base, rc2)] == [0, 0, 1, 1]
    # a changed generic bound is a reshape (the review's retain_mut and gather_after_filter cases)
    base = [mk("a::retain_mut", "()", generics=[["F", "core::ops::FnMut(&str) -> bool"]])]
    rc2 = [mk("a::retain_mut", "()", generics=[["F", "core::ops::Fn(&str) -> bool"]])]
    assert [len(x) for x in delta(base, rc2)] == [0, 1, 0, 0]
    base = [mk("a::gather", "T", generics=[["I", "core::iter::IntoIterator"]])]
    rc2 = [mk("a::gather", "T", generics=[["I", "core::iter::IntoIterator + core::clone::Clone"]])]
    assert [len(x) for x in delta(base, rc2)] == [0, 1, 0, 0]
    print("report.py self-test: ok")

if "--self-test" in sys.argv:
    self_test(); sys.exit(0)

here, root = sys.argv[1], sys.argv[2]
out = sys.argv[3] if len(sys.argv) > 3 else os.path.join(here, "out")
def load(p): return json.load(open(p))
status = load(os.path.join(out, "status.json"))
experimental = status.get("experimental", False)
print(("# EXPERIMENTAL RUN (frozen inputs not verified)\n" if experimental else "") + f"Run {status['run_id']}; stages: " + ", ".join(f"{k}={v}" for k, v in status["stages"].items()) + "\n")
frozen = status["frozen"]
base = load(frozen["baseline_inventory"]); rc2 = load(os.path.join(out, "rc2/result/inventory.json"))
base_sum = load(frozen["baseline_summary"]); rc2_sum = load(os.path.join(out, "rc2/result/summary.json"))
pins = load(os.path.join(out, "rc2/pins.json")); base_pins = load(frozen["baseline_pins"])

print("## Pins\n")
print("| side | source | resolved features | crates documented | failed | toolchain | format |")
print("|---|---|---:|---:|---|---|---:|")
print(f"| 0.55.2 | crates.io =0.55.2 | {len(base_pins['resolved_polars_features'])} | {len(base_pins['crates']) - len(base_pins['failed'])} | {', '.join(base_pins['failed']) or 'none'} | {base_pins.get('toolchain')} | {base_pins['format_version']} |")
print(f"| rc2 | Rust workspace sources at git {pins['rev']} | {len(pins['resolved_polars_features'])} | {len(pins['crates']) - len(pins['failed'])} | {', '.join(pins['failed']) or 'none'} | {pins.get('toolchain')} | {pins['format_version']} |")
fa, fb = set(base_pins['resolved_polars_features']), set(pins['resolved_polars_features'])
print("\nResolved feature sets are identical." if fa == fb else f"\nResolved feature sets differ: only 0.55.2 {sorted(fa - fb)}; only rc2 {sorted(fb - fa)}.")
ca, cb = set(base_pins['crates']), set(pins['crates'])
if ca != cb: print(f"Crate sets differ: only 0.55.2 {sorted(ca - cb)}; only rc2 {sorted(cb - ca)}.")
print(f"Locks: {frozen['doc_lock']} and {frozen['adapter_lock']}, digests verified at the start of the run; the {status.get('registry_packages', '?')} non-Polars packages in the adapter lock come from crates.io. Wheel-build equivalence with the Python release was not tested.")

print("\n## Denominators, same extractor and rules\n")
print("| side | gross | reachable | excluded | unknown | eligible | derived among callables |")
print("|---|---:|---:|---:|---:|---:|---:|")
for name, s in (("0.55.2", base_sum), ("rc2", rc2_sum)):
    print(f"| {name} | {s['gross_total_callables']} | {s['reachable_callables']} | {s['excluded']} | {s['unknown']} | **{s['eligible']}** | {s['derived_callables']} |")

ea, eb = eligible(base), eligible(rc2)
same, reshaped, removed, added = delta(ea, eb)
sa = load(frozen["baseline_surface"]); sb = load(os.path.join(out, "surface-rc2.json"))
in_scope_a = sum(1 for e in sa["entries"] if e["status"] != "out_of_scope")
print("\n## API delta, API crates, eligible callables, identity = path + kind + receiver + signature\n")
print("| measure | count |\n|---|---:|")
print(f"| eligible entries in 0.55.2 (reconciles with surface.json in-scope entries: {in_scope_a}) | {len(ea)} |\n| eligible entries at rc2 | {len(eb)} |\n| identical entries | {len(same)} |\n| reshaped (one unmatched entry per side of a path group, same kind and receiver) | {len(reshaped)} |\n| removed at rc2 | {len(removed)} |\n| added at rc2 | {len(added)} |")
assert len(ea) == in_scope_a, f"eligible entries {len(ea)} do not reconcile with surface.json {in_scope_a}"
def bykind(cs): return ", ".join(f"{k} {v}" for k, v in collections.Counter(c["kind"] for c in cs).most_common()) or "none"
print(f"\nAdded by kind: {bykind(added)}. Removed by kind: {bykind(removed)}.")
def owners(cs): return collections.Counter("::".join(c["canonical_path"].split("::")[:-1]) for c in cs).most_common(8)
print("\nMost-changed owners (added): " + ", ".join(f"`{o}` {n}" for o, n in owners(added)))
print("\nMost-changed owners (removed): " + ", ".join(f"`{o}` {n}" for o, n in owners(removed)))
print("\n| reshaped entry | 0.55.2 | rc2 |\n|---|---|---|")
for a, b in sorted(reshaped, key=lambda x: x[0]["canonical_path"])[:40]:
    print(f"| `{a['canonical_path']}` | `{sig(a)}` | `{sig(b)}` |")
if len(reshaped) > 40: print(f"\n… and {len(reshaped) - 40} more.")
ta = {s["canonical_path"] for s in base["supporting"] if s["kind"] in ("struct", "enum", "union")}
tb = {s["canonical_path"] for s in rc2["supporting"] if s["kind"] in ("struct", "enum", "union")}
print(f"\nTypes: {len(tb - ta)} added, {len(ta - tb)} removed at rc2.")

print("\n## Accounting under the unchanged generator\n")
print("| status | 0.55.2 | rc2 |\n|---|---:|---:|")
for st in ("generated", "adapted", "unsupported", "out_of_scope"):
    print(f"| {st} | {sa['counts'].get(st, 0)} | {sb['counts'].get(st, 0)} |")
def acct_key(e): return (e["canonical_path"], e["kind"], e["signature"])
ma = collections.defaultdict(list); mb = collections.defaultdict(list)
for e in sa["entries"]: ma[acct_key(e)].append(e["status"])
for e in sb["entries"]: mb[acct_key(e)].append(e["status"])
trans = collections.Counter()
for k in set(ma) & set(mb):
    for x, y in zip(sorted(ma[k]), sorted(mb[k])):
        if x != y: trans[(x, y)] += 1
print(f"\nSame identity, status changed: {sum(trans.values())}.")
for (a, b), n in trans.most_common(): print(f"- {a} → {b}: {n}")
print(f"\nWrapper types: {len(sa['wrappers'])} on 0.55.2, {len(sb['wrappers'])} at rc2. Oracle cases emitted: {sa['oracle_cases']} and {sb['oracle_cases']}.")

print("\n## Compilation of the generated module against rc2\n")
bs = status["stages"].get("build")
print(f"`cargo build --locked --release --features test-support`: {bs}.")
errors = []
try:
    for line in open(os.path.join(out, "build.json")):
        try: m = json.loads(line)
        except Exception: continue
        if m.get("reason") == "compiler-message" and m["message"]["level"] == "error": errors.append(m["message"])
except FileNotFoundError: pass
if bs != "ok" and not errors:
    print("\nThe build failed without compiler diagnostics; see out/build.stderr:\n")
    try: print("```\n" + open(os.path.join(out, "build.stderr")).read()[-2000:] + "\n```")
    except FileNotFoundError: print("(no stderr captured)")
elif errors:
    gen_src = open(os.path.join(status["scratch"], "polars/src/generated/functions.rs")).read().splitlines()
    def entry_at(ln):
        for i in range(ln - 1, -1, -1):
            m = re.match(r"^/// Polars: `([^`]+)`", gen_src[i])
            if m: return m.group(1)
            if gen_src[i].startswith("pub fn install"): return None
    by_msg = collections.Counter(); by_entry = {}
    for e in errors:
        by_msg[re.sub(r"`[^`]*`", "`X`", e["message"])] += 1
        for sp in e.get("spans", []):
            if sp["file_name"].endswith("functions.rs"):
                ent = entry_at(sp["line_start"])
                if ent and ent not in by_entry: by_entry[ent] = e["message"][:160]
    print(f"\n{len(errors)} errors, {len(by_entry)} generated entries involved.\n")
    print("| error class | count |\n|---|---:|")
    for k, v in by_msg.most_common(15): print(f"| {k[:140]} | {v} |")
    print("\n| entry | first error |\n|---|---|")
    for k, v in sorted(by_entry.items())[:40]: print(f"| `{k}` | {v} |")
else:
    print("\nNo errors.")

print("\n## Oracle differences\n")
os_ = status["stages"].get("oracle")
p = os.path.join(out, "oracle-results-rc2.json")
if os_ in ("ok", "cases_failed") and os.path.exists(p):
    ra = load(frozen["baseline_oracle_results"]); rb = load(p)
    assert rb.get("run_id") == status["run_id"], "oracle results are not from this run"
    assert rb["cases"] == sb["oracle_cases"], "oracle results do not match the generated case count"
    ka = {r["path"]: r for r in ra["results"]}; kb = {r["path"]: r for r in rb["results"]}
    print(f"Controls: {status['stages'].get('oracle_controls')}. Cases: {rb['cases']} emitted at rc2 (results verified as this run's, run {rb['run_id']}).\n")
    print("| tally | 0.55.2 | rc2 |\n|---|---:|---:|")
    for k in sorted(set(ra["tally"]) | set(rb["tally"])): print(f"| {k} | {ra['tally'].get(k, 0)} | {rb['tally'].get(k, 0)} |")
    both = set(ka) & set(kb)
    diff = [(k, ka[k]["status"], kb[k]["status"], kb[k]["detail"]) for k in both if ka[k]["status"] != kb[k]["status"]]
    print(f"\nCases in both runs: {len(both)}; only 0.55.2: {len(set(ka) - set(kb))}; only rc2: {len(set(kb) - set(ka))}; outcome changed among common cases: {len(diff)}.")
    new_by = collections.Counter(kb[k]["status"] for k in set(kb) - set(ka)); gone_by = collections.Counter(ka[k]["status"] for k in set(ka) - set(kb))
    print(f"\nNew cases at rc2 by outcome: {dict(new_by)}. Cases gone at rc2 by their 0.55.2 outcome: {dict(gone_by)}.")
    print("\n| common case whose outcome changed | 0.55.2 | rc2 | detail |\n|---|---|---|---|")
    for k, a, b, d in sorted(diff)[:40]: print(f"| `{k}` | {a} | {b} | {d[:140].replace('|', '/')} |")
elif os_ == "not_run_build_failed":
    print("Not run: the scratch adapter did not build.")
else:
    print(f"Not available: oracle stage `{os_}`; this run is invalid for oracle conclusions.")

print("\n## Excluded cases at rc2\n")
disp = collections.Counter(e["execution"].split(" (")[0] for e in sb["entries"] if e["status"] == "generated")
print("| disposition | entries |\n|---|---:|")
for k, v in disp.most_common(): print(f"| {k} | {v} |")

print("\n## Census: generated bindings without a receiver fixture at rc2\n")
types = {s["canonical_path"]: s for s in rc2["supporting"]}
cb_ = collections.Counter()
for e in sb["entries"]:
    if e["status"] == "generated" and e["execution"].startswith("no fixture for the receiver type"):
        path = e["canonical_path"].split(" as ")[0]   # protocol entries: "<type> as <trait>"
        owner = path if " as " in e["canonical_path"] else path.rsplit("::", 1)[0]
        s = types.get(owner)
        if s is None: why = "owner not in inventory"
        elif s["kind"] in ("trait", "trait_alias"): why = "trait-owned method: no wrapped implementor has a fixture"
        elif s["kind"] == "enum": why = "enum without a unit variant" if not any(not d for _, d in s["variant_shapes"]) else "enum (unit variant exists; wrapper missing)"
        elif s["public_fields"] > 0: why = "struct with public fields, no Default"
        else: why = "struct with no public fields (private fields or unit), no Default, no core fixture"
        cb_[(why, owner.split("::")[-1])] += 1
byw = collections.Counter(); 
for (why, _), n in cb_.items(): byw[why] += n
print("| why the frozen rules give no fixture | entries |\n|---|---:|")
for k, v in byw.most_common(): print(f"| {k} | {v} |")
print("\nLargest receiver types: " + ", ".join(f"`{t}` {n} ({w})" for (w, t), n in cb_.most_common(12)))

dj = os.path.join(out, "diag-joins.json")
print("\n## Diagnostic (labeled, outside the classifier): join row multisets, binding versus Rust\n")
if os.path.exists(dj):
    d = load(dj)
    print("| join | schema equal | row multiset equal | rows (binding / Rust) | orders seen in 6 Rust runs |\n|---|---|---|---|---:|")
    for j in d["joins"]:
        print(f"| {j['name']} | {j['schema_equal']} | {j['multiset_equal']} | {j['rows_binding']} / {j['rows_rust']} | {j['distinct_orders']} |")
else:
    print("Not available.")
