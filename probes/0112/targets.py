#!/usr/bin/env python3
"""Record 0112: the target table. Every row the 0111 v2 census assigned to
the `generic_impls` rule. Serde rows carry their 0112 disposition
(generated or refused, with its reason); every other row is recorded as
`deferred to 0113, unproved`, neither unsupported by a failed mapping nor
counted available (the scope split agreed before the implementation).

  targets.py <0111 batches.json> <inventory.json> <0112 surface-v2.json> <out targets.json>
  targets.py --self-test
"""
import collections, json, sys

SERDE = {"serde_core::ser::Serialize": "Serialize", "serde_core::de::Deserialize": "Deserialize"}


def trait_path(path):
    return path.split(" as ", 1)[1].split("<")[0] if " as " in path else ""


def row(c, entry):
    tp = trait_path(c["canonical_path"])
    lane = "serde" if tp in SERDE else "non-serde"
    base = {"key": c["key"], "path": c["canonical_path"], "trait": tp.rsplit("::", 1)[-1], "owner": c["owner"],
            "impl_for": c.get("impl_for"), "lane": lane}
    if lane == "non-serde":
        return {**base, "disposition": "deferred to 0113, unproved", "reason": ""}
    if entry is None:
        return {**base, "disposition": "absent", "reason": ""}
    if entry["status"] == "generated":
        return {**base, "disposition": "generated", "reason": ""}
    return {**base, "disposition": entry["status"], "reason": entry.get("reason") or ""}


def table(batches, inv, surface):
    rule = next(r for r in batches["rules"] if r["rule"] == "generic_impls")
    keys = rule["keys"]
    callables = {c["key"]: c for c in inv["callables"]}
    entries = {e["key"]: e for e in surface["entries"]}
    rows = [row(callables[k], entries.get(k)) for k in keys]
    assert len(rows) == len(keys) == len(set(keys))
    by_lane = collections.Counter((r["lane"], r["disposition"]) for r in rows)
    return {"rows": rows, "counts": {f"{l}: {d}": n for (l, d), n in sorted(by_lane.items())}}


def self_test():
    ser = {"key": "a", "canonical_path": "x::A as serde_core::ser::Serialize", "owner": "x::A", "impl_for": "x::A"}
    other = {"key": "b", "canonical_path": "x::A as core::ops::arith::Add<i64>", "owner": "x::A", "impl_for": "x::A"}
    assert row(ser, {"status": "generated"})["disposition"] == "generated"
    assert row(ser, {"status": "unsupported", "reason": "serde shape: …"})["disposition"] == "unsupported"
    r = row(other, {"status": "generated"})
    assert r["lane"] == "non-serde" and r["disposition"] == "deferred to 0113, unproved", r
    print("targets self-test: ok")


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        self_test()
    else:
        b, i, s, out = sys.argv[1:5]
        t = table(json.load(open(b)), json.load(open(i)), json.load(open(s)))
        json.dump(t, open(out, "w"), indent=1)
        print(json.dumps(t["counts"], indent=1))
