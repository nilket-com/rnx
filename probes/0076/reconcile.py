#!/usr/bin/env python3
"""Record 0076: the two-level reconciliation, re-runnable.

  reconcile.py <baseline surface.json> <surface.json> [<baseline oracle-results.json> <oracle-results.json>]

Callable coverage counts entries by status; bindings count emitted
bindings by route. A surface without `bindings` (records before 0076)
lists a generated entry's bindings space-separated in `rune`, one per
receiver, with the route inferred from the callable kind.
"""
import collections, json, sys

def load(p): return json.load(open(p))

def bindings(surface):
    out = collections.Counter()
    for e in surface["entries"]:
        if e["status"] != "generated":
            continue
        if "bindings" in e:
            for b in e["bindings"]:
                out[b["route"]] += 1
        else:
            # one binding per receiver: a trait method lists one Rune path per
            # implementor; other kinds have one path (a note may follow it)
            paths = [t for t in e["rune"].split(" ") if t.startswith("polars::")]
            n = max(1, len(paths)) if e["kind"] == "trait_method" else 1
            route = {"inherent": "inherent", "free_fn": "free", "foreign_trait_impl": "protocol"}.get(e["kind"], "implementor")
            out[route] += n
    return out

def coverage(surface):
    return collections.Counter(e["status"] for e in surface["entries"])

def tally(results):
    return results["tally"], results.get("verified"), results["cases"]

a, b = load(sys.argv[1]), load(sys.argv[2])
ca, cb = coverage(a), coverage(b)
ba, bb = bindings(a), bindings(b)
print("| level | baseline | now | delta |\n|---|---:|---:|---:|")
for k in ("generated", "adapted", "unsupported", "out_of_scope"):
    print(f"| callables {k} | {ca[k]} | {cb[k]} | {cb[k] - ca[k]:+} |")
for k in sorted(set(ba) | set(bb)):
    print(f"| bindings {k} | {ba[k]} | {bb[k]} | {bb[k] - ba[k]:+} |")
print(f"| bindings total | {sum(ba.values())} | {sum(bb.values())} | {sum(bb.values()) - sum(ba.values()):+} |")
if len(sys.argv) > 4:
    ta, tb = load(sys.argv[3]), load(sys.argv[4])
    print(f"| oracle cases | {ta['cases']} | {tb['cases']} | {tb['cases'] - ta['cases']:+} |")
    print(f"| verified | {ta.get('verified', ta['cases'])} | {tb.get('verified')} | |")
    keys = sorted(set(ta["tally"]) | set(tb["tally"]))
    for k in keys:
        print(f"| {k} | {ta['tally'].get(k, 0)} | {tb['tally'].get(k, 0)} | {tb['tally'].get(k, 0) - ta['tally'].get(k, 0):+} |")
