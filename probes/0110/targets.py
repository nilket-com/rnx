#!/usr/bin/env python3
"""Record 0110: the target table. Every row the 0109 v2 census assigned to
the `trait_dispatch` rule, with its recovered impl records, the receivers
the generator proved, and its disposition in the 0110 surface.

  targets.py <0109 batches.json> <0110 inventory.json> <0110 surface-v2.json> <out targets.json>
  targets.py --self-test
"""
import collections, json, sys


def disposition(entry):
    if entry is None:
        return "absent"
    if entry["status"] == "generated":
        return "available"
    return entry["status"] + ": " + (entry.get("reason") or "")


def classify(reason):
    """The class of a refusal that remains after the rule."""
    if reason.startswith("available"):
        return "available"
    if "no wrapped implementor: " in reason and reason.rstrip().endswith("no wrapped implementor:"):
        return "no impl record"
    if "no wrapped implementor" in reason:
        if "generic trait" in reason:
            return "generic trait"
        heads = reason.split("no wrapped implementor: ", 1)[1].split(" [")[0]
        if heads and all(h.strip() in ("blanket",) or h.strip().startswith("blanket") for h in heads.split(",")):
            return "blanket only"
        return "impl recorded, no proven wrapped receiver"
    return "proven receiver, refused by an independent check"


def table(batches, inv, surface):
    rule = next(r for r in batches["rules"] if r["rule"] == "trait_dispatch")
    keys = rule["keys"]
    callables = {c["key"]: c for c in inv["callables"]}
    supporting = {s["canonical_path"]: s for s in inv["supporting"]}
    entries = {e["key"]: e for e in surface["entries"]}
    rows = []
    for k in keys:
        c = callables[k]
        tr = supporting.get(c["owner"], {})
        e = entries.get(k)
        d = disposition(e)
        rows.append({
            "key": k, "path": c["canonical_path"], "trait": c["owner"],
            "impl_records": [{"for": i["for_type"], "blanket": i["blanket"], "bounds": i["bounds"], "where": i["where_predicates"]} for i in tr.get("impls", [])],
            "receivers": sorted({b.get("receiver") for b in (e or {}).get("bindings", []) if b.get("receiver")}),
            "bindings": len((e or {}).get("bindings", [])),
            "disposition": d, "class": classify(d),
        })
    assert len(rows) == len(keys) == len(set(keys))
    return {"rows": rows, "counts": dict(collections.Counter(r["class"] for r in rows).most_common()),
            "bindings": sum(r["bindings"] for r in rows)}


def self_test():
    assert classify("available") == "available"
    assert classify("unsupported: no wrapped implementor: ") == "no impl record"
    assert classify("unsupported: no wrapped implementor: blanket, blanket") == "blanket only"
    assert classify("unsupported: no wrapped implementor: T [x]") == "impl recorded, no proven wrapped receiver"
    assert classify("unsupported: no wrapped implementor: a, b [generic trait]") == "generic trait"
    assert classify("unsupported: on every implementor: arity: method") == "proven receiver, refused by an independent check"
    print("targets self-test: ok")


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        self_test()
    else:
        b, i, s, out = sys.argv[1:5]
        t = table(json.load(open(b)), json.load(open(i)), json.load(open(s)))
        json.dump(t, open(out, "w"), indent=1)
        print(json.dumps({"counts": t["counts"], "bindings": t["bindings"]}, indent=1))
