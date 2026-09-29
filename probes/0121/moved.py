#!/usr/bin/env python3
"""Record 0121: every oracle case whose status moved from 0120, per pin.

  moved.py <0120 oracle-results.json[.gz]> <0121 oracle-results.json[.gz]> [<out.json>]

Lists each moved case (old -> new, with the new detail), the new and
removed cases, and groups the repaired cases that now error on both sides
by operation and error kind: Polars agreeing with the adapter that the
call fails is compared, not value-tested.
"""
import gzip
import json
import re
import sys
from collections import Counter


def load(p):
    return {x["id"]: x for x in json.load(gzip.open(p) if p.endswith(".gz") else open(p))["results"]}


old, new = load(sys.argv[1]), load(sys.argv[2])
moved = []
for i, x in sorted(new.items()):
    o = old.get(i)
    if o is not None and o["status"] != x["status"]:
        moved.append({"id": i, "from": o["status"], "to": x["status"], "unordered": x.get("unordered", False),
                      "detail": x["detail"][:200]})
op = lambda i: re.sub(r"__on__\w+$", "", i).split("__")[-1]
errors = Counter((op(m["id"]), m["detail"].split()[0] if m["detail"] else "") for m in moved if m["to"] == "both_error")
doc = {
    "transitions": dict(sorted(Counter(f"{m['from']} -> {m['to']}" for m in moved).items())),
    "new_cases": len(set(new) - set(old)),
    "removed": sorted(set(old) - set(new)),
    "both_error_by_operation": {f"{k[0]} ({k[1]})": n for k, n in sorted(errors.items())},
    "regressions": [m for m in moved if m["from"] == "match" and not m["unordered"]],
    "moved": moved,
}
if len(sys.argv) > 3:
    json.dump(doc, open(sys.argv[3], "w"), indent=1, sort_keys=True)
    open(sys.argv[3], "a").write("\n")
print(json.dumps({k: doc[k] for k in ("transitions", "new_cases")} | {"removed": len(doc["removed"]), "regressions": len(doc["regressions"])}))
for k, n in doc["both_error_by_operation"].items():
    print(f"  {n} {k}")
