#!/usr/bin/env python3
"""Record 0121: the fixture_failed baseline, from committed oracle results.

  baseline.py <oracle-results.json[.gz]> [<out.json>]

Every fixture_failed case, classified by the fixture that failed (the
recipe name in "fixture: <name> failed: ..."), with its obstacle text.
"""
import gzip
import json
import re
import sys
from collections import Counter

p = sys.argv[1]
data = json.load(gzip.open(p) if p.endswith(".gz") else open(p))
rows = []
for x in data["results"]:
    if x["status"] != "fixture_failed":
        continue
    m = re.search(r"fixture: (\w+) failed: ([^\"]*)", x["detail"])
    fam, why = (m.group(1), m.group(2).strip()[:160]) if m else ("unclassified", x["detail"][:160])
    rows.append({"id": x["id"], "fixture": fam, "obstacle": why})
counts = dict(sorted(Counter(r["fixture"] for r in rows).items()))
doc = {"total": len(rows), "by_fixture": counts, "cases": sorted(rows, key=lambda r: (r["fixture"], r["id"]))}
if len(sys.argv) > 2:
    json.dump(doc, open(sys.argv[2], "w"), indent=1, sort_keys=True)
    open(sys.argv[2], "a").write("\n")
print(json.dumps({"total": len(rows), "by_fixture": counts}))
for f in counts:
    ex = next(r for r in rows if r["fixture"] == f)
    print(f"  {f}: {ex['obstacle'][:110]}")
