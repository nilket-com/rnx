#!/usr/bin/env python3
"""Record 0117: the final audit of the five pools the plan names, at v2.

Every row of each pool gets exactly one disposition from the 0117 v2
surface: generated (with its binding count), refused (with its reason, the
first named exception when the entry reason is only a summary), stopped
(stage E's I/O, with its cause) or deferred (a trait no inventory impl
implements: an extension point, owned by callbacks).

  audit.py <0116 surface-v2.json> <0116 final.json> <0117 surface-v2.json> <out.json>

Pools, taken from the 0116 evidence, first match wins:
  io             0116 final.json family `io_generic`
  builders       PrimitiveChunkedBuilder / CategoricalChunkedBuilder rows
  generic_trait  refused by 0116's wholesale generic-trait rule
  trait_argument a bound whose trait argument 0116 could not model
  blanket        a bare-parameter implementor (`impl<T: B> Trait for T`)
"""
import json
import re
import sys
from collections import Counter

base_p, final_p, new_p, out_p = sys.argv[1:5]
base = {e["key"]: e for e in json.load(open(base_p))["entries"]}
new = {e["key"]: e for e in json.load(open(new_p))["entries"]}
io_keys = {r["key"] for r in json.load(open(final_p))["rows"] if r["family"] == "io_generic"}

STAGE_E = (
    "stopped (stage E): no reader or writer is reachable. `MmapBytesReader for Cursor<T>` needs "
    "`T: AsRef<[u8]> + Send + Sync` (a core trait with an argument, and auto traits) and "
    "`SerWriter<W>` needs `W: core::io::Write` on the Sink; the inventory records none of these, "
    "and the plan names no model for std facts"
)


def implementors(reason):
    m = re.match(r"no wrapped implementor: ([^\[]*)", reason)
    return [x.strip() for x in m.group(1).split(",")] if m else []


def pool(k, e):
    r = e.get("reason") or ""
    if k in io_keys:
        return "io"
    if re.search(r"::(Primitive|Categorical)ChunkedBuilder::", e["canonical_path"]):
        return "builders"
    if e["status"] == "generated":
        return None
    if "generic trait" in r and not r.startswith("0113 family"):
        return "generic_trait"
    if "trait argument" in json.dumps(e):
        return "trait_argument"
    if any(re.fullmatch(r"[A-Z]", i) for i in implementors(r)):
        return "blanket"
    return None


rows = []
for k, e in base.items():
    p = pool(k, e)
    if p is None:
        continue
    n = new[k]
    reason = n.get("reason") or ""
    ex = [x["reason"] for x in n.get("exceptions", [])]
    if n["status"] == "generated":
        disp, why = "generated", f"{len(n.get('bindings', []))} bindings"
    elif p == "io":
        disp, why = "stopped", STAGE_E
    elif "is recorded under this configuration" in reason and "no impl of" in reason and p != "builders":
        disp, why = "deferred", "callbacks: no inventory impl (an extension point a script would implement)"
    else:
        disp = "refused"
        # an entry reason without its bracketed cause names the first exception
        why = f"{reason} | {ex[0]}" if ex and "[" not in reason and ex[0] not in reason else reason
    rows.append(
        {
            "key": k,
            "path": e["canonical_path"],
            "pool": p,
            "before": e["status"],
            "disposition": disp,
            "detail": why,
            "bindings": len(n.get("bindings", [])) if n["status"] == "generated" else 0,
        }
    )

rows.sort(key=lambda r: (r["pool"], r["path"], r["key"]))
counts = {}
for p in ["generic_trait", "trait_argument", "blanket", "io", "builders"]:
    c = Counter(r["disposition"] for r in rows if r["pool"] == p)
    counts[p] = {
        "rows": sum(c.values()),
        **dict(sorted(c.items())),
        "bindings": sum(r["bindings"] for r in rows if r["pool"] == p),
    }
json.dump({"counts": counts, "rows": rows}, open(out_p, "w"), indent=1, sort_keys=True)
open(out_p, "a").write("\n")
for p, c in counts.items():
    print(p, c)
