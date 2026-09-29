#!/usr/bin/env python3
"""Record 0118: the audit, per pin, from committed surfaces.

  audit.py <census.json> <0117 surface.json> <0118 surface.json> <out.json>

Sections:
  census     every stage-1 census row, with its final disposition in the 0118
             surface; a path the census predicted reachable that did not
             generate is `prediction_missed`, with the generator's reason, and
             is never counted available
  polars_io  every polars_io row missing in 0117, one disposition each:
             generated (bindings), refused (reason), or out of scope (async,
             cloud, object store)
  other      every other 0117 -> 0118 status movement, by name (the approved
             `Self: Sized` exception)
"""
import json
import re
import sys
from collections import Counter

census_p, old_p, new_p, out_p = sys.argv[1:5]
census = json.load(open(census_p))
old = {e["key"]: e for e in json.load(open(old_p))["entries"]}
new = {e["key"]: e for e in json.load(open(new_p))["entries"]}

OUT_OF_SCOPE = re.compile(r"async|object_store|ObjectStore|cloud|Cloud|file_handle|PathBuf|&std::path::Path")


def final(k):
    n = new[k]
    if n["status"] == "generated":
        return "generated", f"{len(n.get('bindings', []))} bindings"
    reason = n.get("reason") or ""
    ex = [x["reason"] for x in n.get("exceptions", [])]
    if ex and "[" not in reason and ex[0] not in reason:
        reason = f"{reason} | {ex[0]}"
    return n["status"], reason


rows = []
for r in census["rows"]:
    disp, detail = final(r["key"])
    predicted = r["disposition"]
    missed = predicted == "reachable" and disp != "generated"
    rows.append({
        "key": r["key"], "path": r["path"], "census": predicted,
        "final": "prediction_missed" if missed else disp, "detail": detail,
    })
census_keys = {r["key"] for r in census["rows"]}

io = []
for k, e in old.items():
    if e["status"] == "generated" or not e["canonical_path"].startswith("polars_io::"):
        continue
    disp, detail = final(k)
    if disp != "generated" and OUT_OF_SCOPE.search(e["canonical_path"] + " " + detail):
        disp = "out of scope"
    io.append({"key": k, "path": e["canonical_path"], "final": disp, "detail": detail})

other = []
for k, e in old.items():
    n = new.get(k)
    if n is None or e["status"] == n["status"] or k in census_keys or e["canonical_path"].startswith("polars_io::"):
        continue
    other.append({"key": k, "path": e["canonical_path"], "from": e["status"], "to": n["status"],
                  "old_reason": e.get("reason"), "bindings": len(n.get("bindings", []))})

for x in (rows, io, other):
    x.sort(key=lambda r: (r["path"], r["key"]))
doc = {
    "counts": {
        "census": dict(sorted(Counter(r["final"] for r in rows).items())),
        "polars_io": dict(sorted(Counter(r["final"] for r in io).items())),
        "other_moves": len(other),
    },
    "census": rows,
    "polars_io": io,
    "other": other,
}
json.dump(doc, open(out_p, "w"), indent=1, sort_keys=True)
open(out_p, "a").write("\n")
print(json.dumps(doc["counts"]))
