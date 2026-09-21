#!/usr/bin/env python3
"""Build the evidence tables from the probe's outputs. Prints Markdown."""
import json, os, sys, collections
here = sys.argv[1]
cfgs = sorted(d for d in os.listdir(f"{here}/out") if os.path.exists(f"{here}/out/{d}/result/summary.json"))

def load(cfg):
    s = json.load(open(f"{here}/out/{cfg}/result/summary.json"))
    pins = json.load(open(f"{here}/out/{cfg}/pins.json"))
    return s, pins

print("## Pins\n")
print("| configuration | features declared | resolved | declared but not resolved | crates documented | failed to document | toolchain | rustdoc | format |")
print("|---|---:|---:|---|---:|---|---|---|---:|")
for cfg in cfgs:
    s, p = load(cfg)
    dnr = p.get('declared_not_resolved', [])
    dnr_s = 'none' if not dnr else (', '.join(dnr) if len(dnr) <= 12 else f"{len(dnr)} (the adapter set is intentionally narrow)")
    print(f"| {cfg} | {len(p.get('declared_polars_features', []))} | {len(p['resolved_polars_features'])} | {dnr_s} | {len(p['crates']) - len(p['failed'])} | {', '.join(p['failed']) or 'none'} | {p.get('toolchain', 'nightly')} | {p['rustdoc'].split()[1]} | {p['format_version']} |")

print("\n## Denominators\n")
print("| configuration | gross public callables | reachable, deduplicated | excluded | unknown re-exports | unknown types (U1) | eligible | of which derived impls | trait unreachable |")
print("|---|---:|---:|---:|---:|---:|---:|---:|---:|")
for cfg in cfgs:
    s, _ = load(cfg)
    print(f"| {cfg} | {s['gross_total_callables']} | {s['reachable_callables']} | {s['excluded']} | {s['unknown_reexports']} | {s['unknown_types']} | **{s['eligible']}** | {s['derived_callables']} | {s['trait_methods_trait_unreachable']} |")

print("\n## Predicted buckets (rule output, not demonstrated)\n")
print("| configuration | mechanical | conversion | option struct | callback | generic |")
print("|---|---:|---:|---:|---:|---:|")
for cfg in cfgs:
    s, _ = load(cfg)
    pr = s["predicted"]; pc = s["predicted_pct_of_eligible"]
    cells = [f"{pr.get(k,0)} ({pc.get(k,0):.1f}%)" for k in ("1_mechanical", "2_conversion", "3_optionstruct", "4_callback", "5_generic")]
    print(f"| {cfg} | " + " | ".join(cells) + " |")

print("\n## By defining crate, 0.55.2 adapter configuration\n")
s, _ = load("0.55.2-adapter")
print("| crate | eligible | mechanical | conversion | option struct | callback | generic | excluded |")
print("|---|---:|---:|---:|---:|---:|---:|---:|")
for k, v in s["by_crate"].items():
    g = lambda n: v.get(n, 0)
    print(f"| {k} | {g('eligible')} | {g('1_mechanical')} | {g('2_conversion')} | {g('3_optionstruct')} | {g('4_callback')} | {g('5_generic')} | {g('excluded')} |")

API = ["polars_core", "polars_plan", "polars_lazy", "polars_io", "polars_ops", "polars_time", "polars_dtype", "polars_schema", "polars_error"]
print("\n## API crates versus internals reachable through the prelude, per configuration\n")
print("| configuration | subset | eligible | mechanical | conversion | option struct | callback | generic |")
print("|---|---|---:|---:|---:|---:|---:|---:|")
for cfg in cfgs:
    s2, _ = load(cfg)
    for label, pick in (("API crates", lambda k: k in API), ("internals (arrow, utils, row, parquet, compute, config)", lambda k: k not in API)):
        tot = collections.Counter()
        for k, v in s2["by_crate"].items():
            if pick(k):
                for kk, vv in v.items(): tot[kk] += vv
        e = tot["eligible"] or 1
        cells = [f"{tot[b]} ({100*tot[b]/e:.1f}%)" for b in ("1_mechanical", "2_conversion", "3_optionstruct", "4_callback", "5_generic")]
        print(f"| {cfg} | {label} | {tot['eligible']} | " + " | ".join(cells) + " |")

print("\n## What makes an entry generic, 0.55.2 adapter configuration, API crates\n")
inv0 = json.load(open(f"{here}/out/0.55.2-adapter/result/inventory.json"))
gen = [c for c in inv0["callables"] if c["bucket"] == "generic" and c["krate"] in API]
by = collections.Counter()
for c in gen:
    d = c["decided_by"]
    if d == "owner": d = "owner has type parameters (O2, alias-instantiable)" if "O2" in c["rules"] else "owner has type parameters (O1)"
    elif d.startswith("param"):
        rs = c["rules"]
        d = ("a parameter (P8 foreign type)" if "P8" in rs else "a parameter (P9 unreachable polars type)" if "P9" in rs else "a parameter (L1 lifetime type)" if "L1" in rs else "a parameter (P7 generic/dyn/assoc)")
    by[d] += 1
print("| cause | entries |\n|---|---:|")
for k, v in by.most_common(): print(f"| {k} | {v} |")
p8 = collections.Counter()
for c in gen:
    if c["decided_by"].startswith("param") and "P8" in c["rules"]:
        n = c["decided_by"].split(" ", 1)[1]
        for prm in c["params"]:
            if prm["name"] == n: p8[prm["ty"]] += 1
print("\nMost common foreign parameter types (P8): " + ", ".join(f"`{t}` {n}" for t, n in p8.most_common(10)))

print("\n## Rule hits, 0.55.2 adapter configuration\n")
inv = json.load(open(f"{here}/out/0.55.2-adapter/result/inventory.json"))
rules = dict(inv["rules"])
print("| rule | shape | hits |")
print("|---|---|---:|")
for r, n in sorted(s["rule_hits"].items()):
    print(f"| {r} | {rules.get(r, '')} | {n} |")

if os.path.exists(f"{here}/samples/results.json"):
    res = json.load(open(f"{here}/samples/results.json"))
    print("\n## Demonstrated: samples\n")
    tally = collections.defaultdict(collections.Counter)
    for smp in res["samples"]:
        tally[smp["bucket"]][smp["status"]] += 1
    statuses = sorted({st for b in tally.values() for st in b})
    print("| bucket | " + " | ".join(statuses) + " | total |")
    print("|---|" + "---:|" * (len(statuses) + 1))
    for b in ("mechanical", "conversion", "option_struct", "callback"):
        row = tally.get(b, collections.Counter())
        print(f"| {b} | " + " | ".join(str(row.get(st, 0)) for st in statuses) + f" | {sum(row.values())} |")
    print(f"\nRunner: cargo test exit {res.get('cargo_test_exit')}; identity {res.get('identity')}")
    cb = [smp for smp in res["samples"] if smp["bucket"] == "callback" and smp["status"] == "executed_match"]
    print("\n| callback control | outcome | samples |\n|---|---|---:|")
    for key in ("error_propagation", "const_capture", "native_capture"):
        c = collections.Counter(smp.get(key) for smp in cb)
        for k, v in sorted(c.items()): print(f"| {key} | {k} | {v} |")
    print("\nSkipped before generation (fixture or generator limits): " + str(len(res["skipped"])))
    skip_reasons = collections.Counter(sk["reason"].split(" ")[0] + " " + sk["reason"].split(" ")[1] if " " in sk["reason"] else sk["reason"] for sk in res["skipped"])
    print("\n| skip reason (prefix) | count |\n|---|---:|")
    for k, v in skip_reasons.most_common(12):
        print(f"| {k} | {v} |")
    print("\n### Per sample\n")
    print("| id | bucket | entry | shape | status | notes |")
    print("|---|---|---|---|---|---|")
    for smp in res["samples"]:
        short = "::".join(smp["path"].split("::")[-2:])
        notes = "; ".join(smp.get("notes", []))
        if smp["status"] == "compile_failed":
            notes = smp.get("error", "").splitlines()[0][:120]
        elif smp["status"] in ("executed_mismatch", "runtime_error"):
            notes = (smp.get("detail", "") or "")[:160].replace("\n", " ")
        if smp["status"] == "feature_gated":
            notes = (smp.get("detail", "") or "")[:120]
        if smp.get("error_propagation"):
            notes += f"; error: {smp['error_propagation']}; const: {smp.get('const_capture')}; native: {smp.get('native_capture')}"
        print(f"| {smp['id']} | {smp['bucket']} | `{short}` | `{smp['shape']}` | {smp['status']} | {notes} |")

if os.path.exists(f"{here}/samples/results-0.54.4.json"):
    r54 = json.load(open(f"{here}/samples/results-0.54.4.json"))
    r55 = json.load(open(f"{here}/samples/results.json"))
    print("\n## Frozen generator against 0.54.4\n")
    if "fatal" in r54:
        print("Harness failed to build: " + r54["fatal"].splitlines()[0])
    print(f"Runner: cargo test exit {r54.get('cargo_test_exit')}; identity {r54.get('identity')}\n")
    tally = collections.defaultdict(collections.Counter)
    for smp in r54["samples"]: tally[smp["bucket"]][smp["status"]] += 1
    statuses = sorted({st for b in tally.values() for st in b})
    print("| bucket | " + " | ".join(statuses) + " | total |")
    print("|---|" + "---:|" * (len(statuses) + 1))
    for b in ("mechanical", "conversion", "option_struct", "callback"):
        row = tally.get(b, collections.Counter())
        print(f"| {b} | " + " | ".join(str(row.get(st, 0)) for st in statuses) + f" | {sum(row.values())} |")
    k55 = {(smp["path"], smp["shape"]) for smp in r55["samples"]}
    k54 = {(smp["path"], smp["shape"]) for smp in r54["samples"]}
    print(f"\nSame entry and shape selected in both releases: {len(k55 & k54)} of {len(k55)}; the selection is deterministic per inventory, so entries added or reshaped between releases move the sample set.")
    print("Harness fixed part (fixtures, runner, oracle formatting): " + ("built unchanged against 0.54.4." if "fatal" not in r54 else "needed changes, see above."))

# adjacent release delta
if "0.54.4-adapter" in cfgs and "0.55.2-adapter" in cfgs:
    print("\n## Adjacent release: 0.54.4 versus 0.55.2 under frozen rules, adapter configuration\n")
    a = json.load(open(f"{here}/out/0.54.4-adapter/result/inventory.json"))
    b = json.load(open(f"{here}/out/0.55.2-adapter/result/inventory.json"))
    def keyed(inv):
        return {c["canonical_path"] + "|" + c["shape"]: c for c in inv["callables"]}
    ka, kb = keyed(a), keyed(b)
    pa = {c["canonical_path"] for c in a["callables"]}; pb = {c["canonical_path"] for c in b["callables"]}
    added = pb - pa; removed = pa - pb
    same_path = pa & pb
    sig_changed = sum(1 for p in same_path if {k for k in ka if k.startswith(p + "|")} != {k for k in kb if k.startswith(p + "|")})
    bucket_changed = 0
    for p in same_path:
        ba = {ka[k]["bucket"] for k in ka if k.startswith(p + "|")}
        bb = {kb[k]["bucket"] for k in kb if k.startswith(p + "|")}
        if ba != bb: bucket_changed += 1
    print("| measure | count |\n|---|---:|")
    print(f"| callables only in 0.54.4 (removed by 0.55.2) | {len(removed)} |")
    print(f"| callables only in 0.55.2 (added) | {len(added)} |")
    print(f"| same path, signature shape changed | {sig_changed} |")
    print(f"| same path, predicted bucket changed | {bucket_changed} |")
    print(f"| same path, unchanged | {len(same_path) - sig_changed} |")
