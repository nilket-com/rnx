#!/usr/bin/env python3
"""Record 0113: the audit of the 345 non-serde generic trait impl rows that
0112 deferred. Every row carries its family, trait arguments (from the
inventory's `name`), head (`impl_for`), disposition in the 0113 surface
(generated, with its binding count, or refused, with its reason).

  targets.py <0112 targets.json> <inventory.json> <0113 surface-v2.json> <out targets.json>
  targets.py --self-test
"""
import collections, json, sys

FAMILIES = {
    "operator": ("core::ops::arith::Add", "core::ops::arith::Sub", "core::ops::arith::Mul", "core::ops::arith::Div", "core::ops::arith::Rem",
                 "core::ops::bit::BitAnd", "core::ops::bit::BitOr", "core::ops::bit::BitXor"),
    "unary": ("core::ops::arith::Neg", "core::ops::bit::Not"),
    "from_iter": ("core::iter::traits::collect::FromIterator",),
    "index": ("core::ops::index::Index",),
}


def family(trait_path):
    for f, paths in FAMILIES.items():
        if trait_path in paths:
            return f
    return "other"


def row(c, e):
    tp = c["canonical_path"].split(" as ", 1)[1].split("<")[0] if " as " in c["canonical_path"] else ""
    base = {"key": c["key"], "path": c["canonical_path"], "trait": c["name"], "family": family(tp), "owner": c["owner"], "impl_for": c.get("impl_for")}
    if e is None:
        return {**base, "disposition": "absent", "reason": "", "bindings": 0}
    return {**base, "disposition": e["status"], "reason": e.get("reason") or "", "bindings": len(e.get("bindings") or [])}


def table(prev, inv, surface):
    keys = [r["key"] for r in prev["rows"] if r["lane"] == "non-serde"]
    callables = {c["key"]: c for c in inv["callables"]}
    entries = {e["key"]: e for e in surface["entries"]}
    rows = [row(callables[k], entries.get(k)) for k in keys]
    assert len(rows) == len(set(keys)) == 345, len(rows)
    by = collections.Counter((r["family"], r["disposition"]) for r in rows)
    return {"rows": rows,
            "counts": {f"{f}: {d}": n for (f, d), n in sorted(by.items())},
            "bindings": sum(r["bindings"] for r in rows if r["disposition"] == "generated")}


def self_test():
    assert family("core::ops::arith::Add") == "operator" and family("core::ops::index::Index") == "index"
    assert family("core::ops::deref::Deref") == "other"
    c = {"key": "k", "canonical_path": "a::B as core::ops::arith::Add", "name": "Add<T>", "owner": "a::B", "impl_for": "&a::B"}
    r = row(c, {"status": "generated", "bindings": [1, 2]})
    assert r["family"] == "operator" and r["bindings"] == 2 and r["trait"] == "Add<T>"
    assert row(c, {"status": "unsupported", "reason": "x"})["reason"] == "x"
    print("targets self-test: ok")


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        self_test()
    else:
        p, i, s, out = sys.argv[1:5]
        t = table(json.load(open(p)), json.load(open(i)), json.load(open(s)))
        json.dump(t, open(out, "w"), indent=1)
        print(json.dumps({"counts": t["counts"], "bindings": t["bindings"]}, indent=1))
