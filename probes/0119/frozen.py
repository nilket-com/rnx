#!/usr/bin/env python3
"""Record 0119: the frozen bindings, rebuilt from 0118's surface and
catalogue, and checked against 0119's surface.

  frozen.py build <0118 surface.json> <0118 catalogue.rs> <out.json>
  frozen.py check <frozen.json> <0119 surface.json>

`build` writes every binding 0118 generated as (key, id, Rune path,
catalogue summary). `check` requires each to be generated again with the
same key, id and path (the generator itself also compares the summaries
against its own catalogue, and refuses on any change).
"""
import json
import re
import sys


def catalogue(p):
    pat = re.compile(r'^    \("((?:[^"\\]|\\.)*)", "((?:[^"\\]|\\.)*)"\),$')
    out = {}
    for line in open(p):
        m = pat.match(line.rstrip("\n"))
        if m:
            text = re.sub(r"\\u\{([0-9a-fA-F]+)\}", lambda x: "\\u%04x" % int(x.group(1), 16), m.group(2))
            out[json.loads('"' + m.group(1) + '"')] = json.loads('"' + text + '"')
    return out


if sys.argv[1] == "build":
    surface, cat_p, out = sys.argv[2:5]
    cat = catalogue(cat_p)
    s = json.load(open(surface))
    rows = sorted({(e["key"], b["id"], b["rune"]) for e in s["entries"] if e["status"] == "generated" for b in e.get("bindings", [])})
    json.dump([{"key": k, "id": i, "rune": r, "summary": cat.get(r)} for k, i, r in rows], open(out, "w"), indent=0)
    open(out, "a").write("\n")
else:
    frozen_p, surface = sys.argv[2:4]
    frozen = json.load(open(frozen_p))
    s = json.load(open(surface))
    now = {(e["key"], b["id"], b["rune"]) for e in s["entries"] if e["status"] == "generated" for b in e.get("bindings", [])}
    missing = [f for f in frozen if (f["key"], f["id"], f["rune"]) not in now]
    if missing:
        sys.exit(f"{len(missing)} frozen bindings moved or disappeared: {missing[:5]}")
    print(f"{len(frozen)} frozen bindings present unchanged")
