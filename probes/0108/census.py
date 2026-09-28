#!/usr/bin/env python3
"""Record 0108: the Polars v2 Rust API scoreboard, root-cause census and
0.55.2 -> v2 callable diff.

  census.py <out-dir>        # reads out/, writes out/census.json and prints census.md
  census.py --self-test

Inputs (all under probes/0108/out unless named):
  v2py/result/inventory.json   every public callable reachable from `polars` at
                               da47b74 with the Python wheel's 145 features
  surface-v2.json              the current generator's disposition of each
  oracle-results-v2.json       the direct Rust-vs-Rune oracle (optional: absent
                               when the build failed; value-tested is then 0)
  baseline: probes/0072/out/0.55.2-adapter-narrow/result/inventory.json and
            adapters/polars/surface.json (the production 0.55.2 adapter)

Three denominators, always side by side:
  full        every reachable public callable identity (inventory `callables`)
  applicable  full minus the explicit exclusions in EXCLUSIONS, each with its
              contract; markers and duplicate listings are reported apart
  value       applicable callables with at least one direct oracle value match
"""
import collections, json, os, re, sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
REV = "da47b7405f0e2d188e71cce7c2d588c695f4098d"

# The accepted 0074 identity and delta, executed from their source (the
# script itself runs a report at import, so only its definitions are taken).
class _R0074: pass
r0074 = _R0074()
_src = open(os.path.join(ROOT, "probes/0074/report.py")).read()
exec(compile(_src[:_src.index("\ndef self_test():")], "probes/0074/report.py", "exec"), r0074.__dict__)
identity, delta = r0074.identity, r0074.delta

# Exclusions from the applicable denominator. Each names the contract that
# makes the callable unreachable or meaningless from a script; everything
# else, however internal it looks, stays applicable.
EXCLUSIONS = {
    "unsafe": "the caller must uphold invariants the compiler cannot check; a script cannot promise them (Rust safety contract)",
    "raw_pointer": "takes or returns a raw pointer; no script value is a valid pointer (Rust safety contract)",
    "hidden": "#[doc(hidden)]: public for macro/crate plumbing, declared by Polars as not part of its API",
}

# Root causes for an applicable callable that is not available, from the
# generator's own refusal reason (surface.json) — first match wins.
CAUSES = [
    ("internal_crate", r"^internal crate"),
    ("callback", r"callback|PlanCallback|closure|Fn\(|FnMut|FnOnce"),
    ("async", r"\basync\b|Future"),
    ("generic_inference", r"^bucket: generic|generic parameter not inferable|^generic type|^generic owner|on every implementor: generic|^bound:|no proven instantiation|instantiations were not emitted"),
    ("trait_protocol", r"trait impl not mapped to a Rune protocol|^trait has no public path"),
    ("owner_wrapper", r"no wrapped implementor|^owner not wrapped|^unwrapped type|^owner has no public path"),
    ("lifetime_borrow", r"^lifetime owner|^shape: return \(&|borrowed"),
    ("non_clone_move", r"non-Clone"),
    ("mutable_reference", r"^mutable reference"),
    ("foreign_type", r"^foreign type"),
    ("arity", r"^arity"),
    ("conversion", r"^outward conversion|^conversion source"),
    ("fixed_array", r"\[\w+; \.\.\]"),
]


def cause(reason):
    for name, pat in CAUSES:
        if re.search(pat, reason or ""):
            return name
    return "other"


def trait_of(reason):
    m = re.search(r"Rune protocol: (\w+)", reason or "")
    return m.group(1) if m else None


def exclusion(c):
    if c["bucket"] != "unsupported":
        return None
    if c.get("is_unsafe"):
        return "unsafe"
    if c.get("hidden"):
        return "hidden"
    return "raw_pointer"


def classify(inv, surface, results):
    """One row per inventory callable: its denominator membership and status."""
    status = {r["id"]: r["status"] for r in (results or {}).get("results", [])}
    by_key = {e["key"]: e for e in surface["entries"]}
    rows = []
    for c in inv["callables"]:
        row = {"key": c["key"], "path": c["canonical_path"], "kind": c["kind"], "krate": c["krate"],
               "owner": c.get("owner"), "bucket": c["bucket"], "exclusion": exclusion(c)}
        e = by_key.get(c["key"])
        if row["exclusion"]:
            row["status"] = "excluded"
        elif e is None:
            raise SystemExit(f"eligible callable {c['key']} {c['canonical_path']} has no surface entry (stale surface?)")
        elif e["status"] == "generated":
            row["status"] = "available"
            row["value_tested"] = any(status.get(b.get("case_id")) in ("match", "row_order_differs")
                                      for b in e.get("bindings", []) if b.get("case_id"))
        elif e["status"] == "adapted":
            r = e.get("reason", "")
            row["status"] = "available" if r.startswith("hand-written") else ("duplicate" if e.get("counterpart") else "marker")
            row["value_tested"] = False
        elif e["status"] == "out_of_scope":
            row["status"] = "missing"; row["reason"] = "internal crate (" + e.get("reason", "") + ")"
            row["cause"] = "internal_crate"
        else:
            row["status"] = "missing"; row["reason"] = e.get("reason", "")
            row["cause"] = cause(row["reason"])
            if row["cause"] == "trait_protocol":
                row["trait"] = trait_of(row["reason"])
        rows.append(row)
    keys = [c["key"] for c in inv["callables"]]
    assert len(set(keys)) == len(keys), "duplicate inventory keys"
    assert len(surface["entries"]) == sum(r["status"] != "excluded" for r in rows), \
        (len(surface["entries"]), sum(r["status"] != "excluded" for r in rows))
    return rows


def scoreboard(rows):
    C = collections.Counter
    full = len(rows)
    excluded = C(r["exclusion"] for r in rows if r["exclusion"])
    apart = C(r["status"] for r in rows if r["status"] in ("marker", "duplicate"))
    applicable = [r for r in rows if r["status"] in ("available", "missing")]
    avail = [r for r in applicable if r["status"] == "available"]
    value = [r for r in avail if r.get("value_tested")]
    missing = [r for r in applicable if r["status"] == "missing"]
    assert full == sum(excluded.values()) + sum(apart.values()) + len(applicable)
    pct = lambda n, d: round(100 * n / d, 1) if d else 0.0
    return {
        "full": full, "excluded": dict(excluded), "markers": apart.get("marker", 0), "duplicates": apart.get("duplicate", 0),
        "applicable": len(applicable), "available": len(avail), "value_tested": len(value), "missing": len(missing),
        "available_pct_of_applicable": pct(len(avail), len(applicable)),
        "available_pct_of_full": pct(len(avail), full),
        "value_pct_of_applicable": pct(len(value), len(applicable)),
        "to_90_pct_applicable": max(0, -(-9 * len(applicable) // 10) - len(avail)),
        "missing_by_cause": dict(C(r["cause"] for r in missing).most_common()),
        "missing_by_crate": dict(C(r["krate"] for r in missing).most_common()),
        "missing_trait_protocols": dict(C(r.get("trait") for r in missing if r.get("trait")).most_common()),
    }


def diff(base_inv, v2_inv):
    same, reshaped, removed, added = delta(base_inv["callables"], v2_inv["callables"])
    return {"base": len(base_inv["callables"]), "v2": len(v2_inv["callables"]), "same": len(same),
            "reshaped": sorted((a["canonical_path"], r0074.sig(a), r0074.sig(b)) for a, b in reshaped),  # the delta walks a set: sorted for a reproducible file
            "removed": sorted(c["canonical_path"] for c in removed), "added": len(added),
            "added_by_crate": dict(collections.Counter(c["krate"] for c in added).most_common())}


def migration(base_inv, base_surface, v2_rows, v2_inv):
    """0.55.2 bindings a script has today that v2 loses: identity removed or
    reshaped, or present but no longer available."""
    avail_keys = {e["key"] for e in base_surface["entries"]
                  if e["status"] == "generated" or (e["status"] == "adapted" and e.get("reason", "").startswith("hand-written"))}
    base_by_key = {c["key"]: c for c in base_inv["callables"]}
    v2_by_id = collections.defaultdict(list)
    for c in v2_inv["callables"]:
        v2_by_id[identity(c)].append(c["key"])
    v2_status = {r["key"]: r for r in v2_rows}
    lost = collections.defaultdict(list)
    for k in sorted(avail_keys):
        c = base_by_key.get(k)
        if c is None:
            lost["baseline surface key not in baseline inventory"].append(k); continue
        keys = v2_by_id.get(identity(c), [])
        if not keys:
            lost["identity removed or reshaped at v2"].append(c["canonical_path"])
        elif not any(v2_status[x]["status"] == "available" for x in keys):
            why = v2_status[keys[0]].get("cause") or v2_status[keys[0]]["status"]
            lost[f"present at v2, not available ({why})"].append(c["canonical_path"])
    return {"baseline_available": len(avail_keys), "lost": {k: v for k, v in sorted(lost.items())},
            "lost_total": sum(len(v) for v in lost.values())}


# Record 0108 review: the inputs are pinned, never read from a moving worktree.
BASELINE_COMMIT = "1eecbf1e50a7cd32980c7c5d35c7283a7ffedcc3"   # 0106 impl, the production 0.55.2 adapter
BASELINE_SURFACE_SHA = "723580cbfcedc554204f8b1cff716e93a526e947aada41daaf17c4ae4741ab35"
BASELINE_INVENTORY_SHA = "26cf5f875a106f40da9311912a1f5d60af06d324047509b06d28e386ab674f96"


def sha256(b):
    import hashlib
    return hashlib.sha256(b).hexdigest()


def pinned(data, want, what):
    got = sha256(data)
    if got != want:
        raise SystemExit(f"{what}: sha256 {got} is not the pinned {want}")
    return json.loads(data)


def inputs(d):
    """The census inputs, from a build directory (out/) or a flat replay of
    the committed evidence bundle; baselines are hash-pinned either way."""
    def first(*names):
        for n in names:
            p = os.path.join(d, n)
            if os.path.exists(p):
                return p
        return None
    inv_p = first("v2py/result/inventory.json", "inventory.json")
    surf_p = first("surface-v2.json")
    if not inv_p or not surf_p:
        raise SystemExit(f"{d}: no inventory or surface")
    bs = first("baseline-surface-0106.json")
    bs = open(bs, "rb").read() if bs else __import__("subprocess").check_output(
        ["git", "-C", ROOT, "show", f"{BASELINE_COMMIT}:adapters/polars/surface.json"])
    bi = first("baseline-inventory.json") or os.path.join(ROOT, "probes/0072/out/0.55.2-adapter-narrow/result/inventory.json")
    raw = {"inventory": open(inv_p, "rb").read(), "surface": open(surf_p, "rb").read()}
    rp = first("oracle-results-v2.json")
    if rp:
        raw["oracle"] = open(rp, "rb").read()
    return raw, pinned(bs, BASELINE_SURFACE_SHA, "baseline surface"), pinned(open(bi, "rb").read(), BASELINE_INVENTORY_SHA, "baseline inventory")


def check_provenance(inv, surface):
    """The inventory is the v2py documentation run with exactly features.txt;
    the surface was generated from this probe's v2.toml."""
    p = inv["provenance"]
    assert p.get("rev") == REV and p.get("cfg") == "v2py", p
    feats = open(os.path.join(HERE, "features.txt")).read().split()
    assert sorted(p.get("features", [])) == sorted(feats), "inventory features != features.txt"
    rel = surface.get("release") or {}
    # a replay of committed evidence pins the release its surface was made from;
    # a live run checks the current v2.toml
    want = os.environ.get("CENSUS_RELEASE_SHA") or sha256(open(os.path.join(HERE, "v2.toml"), "rb").read())
    assert rel.get("name") == "v2-rc2" and rel.get("sha256") == want, f"surface release {rel} is not v2.toml {want}"
    c = surface["counts"]
    assert sum(c.values()) == len(surface["entries"]), "surface counts do not reconcile"


def main(d):
    raw, base_surface, base_inv = inputs(d)
    inv = json.loads(raw["inventory"]); surface = json.loads(raw["surface"])
    check_provenance(inv, surface)
    results = json.loads(raw["oracle"]) if "oracle" in raw else None
    if results is not None:
        assert results["cases"] == surface["oracle_cases"], "oracle results are not this surface's"
    rows = classify(inv, surface, results)
    doc = {"rev": REV, "exclusions": EXCLUSIONS, "oracle_ran": results is not None,
           "inputs": {k: sha256(v) for k, v in sorted(raw.items())} | {"baseline_commit": BASELINE_COMMIT},
           "scoreboard": scoreboard(rows), "diff_from_0.55.2": diff(base_inv, inv),
           "migration": migration(base_inv, base_surface, rows, inv), "rows": rows}
    json.dump(doc, open(os.path.join(d, "census.json"), "w"), indent=1)
    s = doc["scoreboard"]
    print(json.dumps({k: v for k, v in s.items()}, indent=1))
    dd = doc["diff_from_0.55.2"]
    print(json.dumps({"diff": {k: (len(v) if isinstance(v, list) else v) for k, v in dd.items()},
                      "migration": {"baseline_available": doc["migration"]["baseline_available"],
                                    "lost": {k: len(v) for k, v in doc["migration"]["lost"].items()}}}, indent=1))


def self_test():
    assert cause("bucket: generic") == "generic_inference"
    assert cause("bucket: generic (no proven instantiation)") == "generic_inference"
    assert cause("callback audit: refused") == "callback"
    assert cause("generic type: f (polars_plan::callback::PlanCallback<(Series)>)") == "callback"
    assert cause("trait impl not mapped to a Rune protocol: Hash") == "trait_protocol"
    assert trait_of("trait impl not mapped to a Rune protocol: Hash") == "Hash"
    assert cause("receiver consumes a non-Clone type: X") == "non_clone_move"
    assert cause("lifetime owner: AnyValue") == "lifetime_borrow"
    assert cause("something new") == "other"
    mk = lambda key, bucket="mechanical", **k: {"key": key, "canonical_path": f"p::{key}", "kind": "inherent",
                                                  "krate": "polars_core", "owner": "p", "bucket": bucket, **k}
    inv = {"callables": [mk("a"), mk("b"), mk("c"), mk("d", "unsupported", is_unsafe=True),
                         mk("e", "unsupported", hidden=True), mk("f"), mk("g"), mk("h")]}
    surface = {"entries": [
        {"key": "a", "status": "generated", "bindings": [{"case_id": 1}]},
        {"key": "b", "status": "generated", "bindings": [{"case_id": 2}]},
        {"key": "c", "status": "unsupported", "reason": "bucket: generic"},
        {"key": "f", "status": "adapted", "reason": "marker impl"},
        {"key": "g", "status": "out_of_scope", "reason": "x"},
        {"key": "h", "status": "adapted", "reason": "hand-written equivalent"}]}
    rows = classify(inv, surface, {"results": [{"id": 1, "status": "match"}, {"id": 2, "status": "both_error"}]})
    s = scoreboard(rows)
    assert s["full"] == 8 and s["excluded"] == {"unsafe": 1, "hidden": 1} and s["markers"] == 1
    assert s["applicable"] == 5 and s["available"] == 3 and s["value_tested"] == 1 and s["missing"] == 2
    assert s["missing_by_cause"] == {"generic_inference": 1, "internal_crate": 1}
    # a stale surface (an eligible callable without an entry) is refused
    try:
        classify(inv, {"entries": surface["entries"][1:]}, None); raise AssertionError("stale surface accepted")
    except SystemExit:
        pass
    # a surface with an extra entry is refused
    try:
        classify(inv, {"entries": surface["entries"] + [{"key": "z", "status": "generated"}]}, None)
        raise AssertionError("extra entry accepted")
    except AssertionError as e:
        assert "extra entry accepted" not in str(e)
    # a pinned input that changed is refused
    try:
        pinned(b"{}", "0" * 64, "x"); raise AssertionError("changed pinned input accepted")
    except SystemExit:
        pass
    print("census self-test: ok")


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        self_test()
    else:
        main(sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "out"))
