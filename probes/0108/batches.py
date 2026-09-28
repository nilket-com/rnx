#!/usr/bin/env python3
"""Record 0108: every missing applicable v2 callable assigned to exactly one
proposed category-sized rule, from out/census.json.

  batches.py [out-dir]       # writes out/batches.json, prints the ranked table
  batches.py --self-test

A rule is a mapping or execution pattern the generator can apply to every
callable of its shape; the counts are upper bounds (what the rule admits),
not promises: each follow-on record measures its own yield and names its
refusals. Cumulative coverage assumes rules land in the listed order.
"""
import collections, json, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))

# (rule id, title, predicate over a census row). First match wins; order is
# the proposed record order (largest useful capability first).
RULES = [
    ("namespace_replay", "Expr namespaces and consuming builders: keep the Clone source, replay the constructor per call (take-once for builders)",
     lambda r: r["cause"] == "non_clone_move"),
    ("trait_dispatch", "trait methods whose implementors are wrapped types (StringNameSpaceImpl on StringChunked, TemporalMethods, ...): dispatch through the wrapper",
     lambda r: r["cause"] == "owner_wrapper" and r["reason"].startswith("no wrapped implementor")),
    ("protocols", "std traits as Rune protocols or methods: Hash -> hash token, PartialEq/Ord -> comparison protocols, TrivialClone/Flags markers reported apart",
     lambda r: r["cause"] == "trait_protocol"),
    ("generic_impls", "generic trait impls (operators, From, Default ...) instantiated over the proven dtype table",
     lambda r: r["cause"] == "generic_inference" and r["kind"] == "foreign_trait_impl"),
    ("generic_methods", "generic inherent/trait methods and free functions: instantiate by dtype table, infer from arguments",
     lambda r: r["cause"] == "generic_inference"),
    ("internal_crates", "arrow/utils/compute/row/parquet: widen the API-crate scope, then re-classify under the other rules",
     lambda r: r["cause"] == "internal_crate"),
    ("owner_wrappers", "wrap the remaining owner types (Clone owners get a generated wrapper; no-public-path owners stay refused)",
     lambda r: r["cause"] == "owner_wrapper"),
    ("callbacks", "the 0079 callback contract extended to the remaining audited signatures",
     lambda r: r["cause"] == "callback"),
    ("value_grammar", "script values for foreign types, borrowed/lifetime values, fixed arrays, outward conversions, &mut and arity",
     lambda r: r["cause"] in ("foreign_type", "lifetime_borrow", "fixed_array", "conversion", "mutable_reference", "arity", "other")),
    ("async", "async callables through the rnx driver",
     lambda r: r["cause"] == "async"),
]


def assign(rows):
    out = collections.defaultdict(list)
    for r in rows:
        for rid, _, pred in RULES:
            if pred(r):
                out[rid].append(r); break
        else:
            raise SystemExit(f"no rule admits {r['path']} ({r.get('cause')})")
    return out


def table(census):
    s = census["scoreboard"]
    rows = [r for r in census["rows"] if r["status"] == "missing"]
    groups = assign(rows)
    assert sum(len(v) for v in groups.values()) == s["missing"]
    avail, appl = s["available"], s["applicable"]
    ranked, cum = [], avail
    for rid, title, _ in RULES:
        g = groups.get(rid, [])
        cum += len(g)
        ranked.append({"rule": rid, "title": title, "admits": len(g),
                       "cumulative_available_pct": round(100 * cum / appl, 1),
                       "owners": dict(collections.Counter((r["owner"] or "(free)").split("::")[-1] for r in g).most_common(8)),
                       "crates": dict(collections.Counter(r["krate"] for r in g).most_common()),
                       "sample": [r["path"] for r in g[:6]], "keys": [r["key"] for r in g]})
    return {"available": avail, "applicable": appl, "rules": ranked}


def main(out):
    census = json.load(open(os.path.join(out, "census.json")))
    t = table(census)
    json.dump(t, open(os.path.join(out, "batches.json"), "w"), indent=1)
    print(f"start: {t['available']} / {t['applicable']} applicable ({round(100 * t['available'] / t['applicable'], 1)}%)")
    print("| # | rule | admits | cumulative if all admitted land | top owners |")
    print("|---|---|---:|---:|---|")
    for i, r in enumerate(t["rules"], 1):
        print(f"| {i} | {r['rule']} | {r['admits']} | {r['cumulative_available_pct']}% | "
              + ", ".join(f"{k} {v}" for k, v in list(r["owners"].items())[:5]) + " |")


def self_test():
    mk = lambda cause, kind="inherent", reason="x": {"status": "missing", "cause": cause, "kind": kind, "reason": reason,
                                                     "owner": "a::B", "krate": "polars_core", "path": "a::B::f", "key": cause + kind}
    rows = [mk("non_clone_move"), mk("generic_inference", "foreign_trait_impl"), mk("generic_inference"),
            mk("owner_wrapper", reason="no wrapped implementor: X"), mk("owner_wrapper", reason="owner not wrapped: X"),
            mk("other"), mk("async")]
    g = assign(rows)
    assert {k: len(v) for k, v in g.items()} == {"namespace_replay": 1, "generic_impls": 1, "generic_methods": 1,
                                                  "trait_dispatch": 1, "owner_wrappers": 1, "value_grammar": 1, "async": 1}
    try:
        assign([mk("unheard_of")]); raise AssertionError("unassigned row accepted")
    except SystemExit:
        pass
    t = table({"scoreboard": {"missing": 7, "available": 3, "applicable": 10}, "rows": rows})
    assert t["rules"][-1]["cumulative_available_pct"] == 100.0
    print("batches self-test: ok")


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        self_test()
    else:
        main(sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "out"))
