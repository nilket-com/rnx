#!/usr/bin/env python3
"""Record 0117 stage A: what the 0116 generator's output changes when only
the inventory changes (0112's inventory -> 0117's). The approved exception is
exactly 9 already-unsupported entries per pin whose `reason` text changes;
everything else must be identical: every generated file byte for byte (the
bindings, types, catalogue, fixtures and oracle cases), every other surface
field, and every other entry field. Prints the key/old/new triples.

  drift.py <out.json> <pin>=<old adapter dir>,<new adapter dir> ...
"""
import filecmp
import json
import os
import sys

EXPECTED = 9
out = sys.argv[1]
doc = {}
problems = []
for arg in sys.argv[2:]:
    pin, dirs = arg.split("=", 1)
    old_d, new_d = dirs.split(",")
    # every generated file except the surface, byte for byte
    for rel, _, files in os.walk(old_d):
        for f in files:
            a = os.path.join(rel, f)
            r = os.path.relpath(a, old_d)
            if r == "surface.json":
                continue
            b = os.path.join(new_d, r)
            if not os.path.exists(b) or not filecmp.cmp(a, b, shallow=False):
                problems.append(f"{pin}: {r} differs")
    for rel, _, files in os.walk(new_d):
        for f in files:
            r = os.path.relpath(os.path.join(rel, f), new_d)
            if not os.path.exists(os.path.join(old_d, r)):
                problems.append(f"{pin}: {r} is new")
    old = json.load(open(os.path.join(old_d, "surface.json")))
    new = json.load(open(os.path.join(new_d, "surface.json")))
    for k in old:
        if k != "entries" and old[k] != new.get(k):
            problems.append(f"{pin}: surface field {k} differs")
    oe, ne = old["entries"], new["entries"]
    if [e["key"] for e in oe] != [e["key"] for e in ne]:
        problems.append(f"{pin}: entry keys or order differ")
    triples = []
    for a, b in zip(oe, ne):
        if a == b:
            continue
        other = sorted(f for f in set(a) | set(b) if f != "reason" and a.get(f) != b.get(f))
        if other:
            problems.append(f"{pin}: {a['key']} differs in {other}")
        elif a["status"] != "unsupported":
            problems.append(f"{pin}: {a['key']} is {a['status']}, not unsupported")
        else:
            triples.append({"key": a["key"], "path": a["canonical_path"], "status": a["status"],
                            "old_reason": a.get("reason"), "new_reason": b.get("reason")})
    if len(triples) != EXPECTED:
        problems.append(f"{pin}: {len(triples)} reason changes, the exception names {EXPECTED}")
    doc[pin] = sorted(triples, key=lambda x: x["key"])
if problems:
    sys.exit("stage A drift outside the approved exception:\n  " + "\n  ".join(problems))
json.dump(doc, open(out, "w"), indent=1)
for pin, t in doc.items():
    print(pin, len(t), "reason-only changes on unsupported entries; nothing else differs")
