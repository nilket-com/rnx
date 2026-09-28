#!/usr/bin/env python3
"""Record 0111: the target table. Every row the 0110 v2 census assigned to
the `protocols` rule, with its trait, owner and disposition in the 0111
surface: generated (and the protocol), marker (with its contract), or
refused (with its reason).

  targets.py <0110 batches.json> <inventory.json> <0111 surface-v2.json> <out targets.json>
  targets.py --self-test
"""
import collections, json, sys


def trait_of(path):
    if " as " in path:
        return path.split(" as ", 1)[1].split("<")[0].rsplit("::", 1)[-1]
    return path.rsplit("::", 2)[-2]


def disposition(entry):
    if entry is None:
        return "absent", ""
    st, why = entry["status"], entry.get("reason") or ""
    if st == "generated":
        return "generated", ""
    if st == "adapted" and why.startswith("marker"):
        return "marker", why
    return st, why


def table(batches, inv, surface):
    rule = next(r for r in batches["rules"] if r["rule"] == "protocols")
    keys = rule["keys"]
    callables = {c["key"]: c for c in inv["callables"]}
    entries = {e["key"]: e for e in surface["entries"]}
    rows = []
    for k in keys:
        c = callables[k]
        d, why = disposition(entries.get(k))
        rows.append({"key": k, "path": c["canonical_path"], "trait": trait_of(c["canonical_path"]), "owner": c["owner"], "disposition": d, "reason": why})
    assert len(rows) == len(keys) == len(set(keys))
    by = collections.Counter((r["trait"], r["disposition"]) for r in rows)
    return {"rows": rows, "counts": dict(collections.Counter(r["disposition"] for r in rows).most_common()),
            "by_trait": {f"{t} {d}": n for (t, d), n in sorted(by.items())}}


def self_test():
    assert trait_of("a::B as core::hash::Hash") == "Hash"
    assert trait_of("a::B as core::convert::TryFrom<x::Y>") == "TryFrom"
    assert trait_of("polars_io::utils::byte_source::ByteSource::get_range") == "ByteSource"
    assert disposition({"status": "adapted", "reason": "marker: x"}) == ("marker", "marker: x")
    assert disposition({"status": "generated"}) == ("generated", "")
    assert disposition({"status": "unsupported", "reason": "r"}) == ("unsupported", "r")
    print("targets self-test: ok")


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        self_test()
    else:
        b, i, s, out = sys.argv[1:5]
        t = table(json.load(open(b)), json.load(open(i)), json.load(open(s)))
        json.dump(t, open(out, "w"), indent=1)
        print(json.dumps({"counts": t["counts"], "by_trait": t["by_trait"]}, indent=1))
