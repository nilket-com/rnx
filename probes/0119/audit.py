#!/usr/bin/env python3
"""Record 0119: the audit, per pin, from committed surfaces.

  audit.py <inventory.json> <0118 surface.json> <0119 surface.json> <out.json>

Sections:
  arrow        every polars_arrow row in the 0119 surface, one disposition:
               available (generated or adapted, with script reachability),
               refused (with its reason), deferred (stage 3's generic arrays,
               to record 0120)
  cross_crate  every other row whose status moved, with its review verdict
  unreviewed   any moved row the review table does not name (must be empty)

Reachability: an Arrow type is reachable when a generated static binding
(the inventory's receiver `none`) returns it, or a generated binding on a non-Arrow or reachable
receiver returns it (to a fixpoint). An available Arrow row is reachable
when one of its bindings has no receiver or a reachable receiver.
"""
import json
import re
import sys
from collections import Counter

inv_p, old_p, new_p, out_p = sys.argv[1:5]
static = {c["key"] for c in json.load(open(inv_p))["callables"] if c["receiver"] == "none"}
old = {e["key"]: e for e in json.load(open(old_p))["entries"]}
new = {e["key"]: e for e in json.load(open(new_p))["entries"]}

ARRAY_REF = {"support::ArrayRef", "polars_arrow::array::ArrayRef", "alloc::boxed::Box<dyn polars_arrow::array::Array>"}
# the release's deferred type prefixes (record 0119 stage 2: no concrete
# array wrapper here; the concrete arrays move to record 0120)
DEFERRED_PREFIXES = ("polars_arrow::array::", "polars_arrow::legacy::array::")
STAGE3 = ("PrimitiveArray", "Utf8Array", "BinaryArray", "ListArray", "BinaryViewArrayGeneric", "MutablePrimitiveArray", "MutableBinaryViewArray", "FixedSizeListArray", "StructArray", "BooleanArray")

# the cross-crate review (record 0119, Codex gate): every moved non-Arrow row
REVIEW = {
    "polars_core::datatypes::dtype::DataType::from_arrow_field": "pass: borrows a wrapped Arrow Field, returns an owned DataType",
    "polars_core::datatypes::dtype::DataType::to_arrow_field": "pass: returns an owned Arrow Field",
    "polars_core::datatypes::field::Field::to_arrow": "pass: returns an owned Arrow Field",
    "polars_core::datatypes::field::Field as core::convert::From": "pass: an owned Field from a borrowed Arrow Field",
    "polars_core::frame::column::Column::rechunk_to_arrow": "pass: consumes a Column clone (Arc-shared), returns an owned ArrayRef",
    "polars_core::frame::dataframe::DataFrame::rechunk_into_arrow": "pass: consumes a DataFrame clone (Arc-shared), returns owned ArrayRefs, one per column",
    "polars_core::frame::dataframe::DataFrame::rechunk_to_arrow": "pass: borrows the frame, returns owned ArrayRefs, one per column",
    "polars_core::series::Series::from_arrow": "pass: moves the ArrayRef in; a dtype mismatch is a PolarsError",
    "polars_core::series::Series::from_chunk_and_dtype": "pass: moves the ArrayRef in; the dtype is checked first (series/from.rs: InvalidOperation on mismatch)",
    "polars_core::series::Series::into_chunks": "pass: consumes a Series clone (Arc-shared), returns owned ArrayRefs",
    "polars_core::series::Series::to_arrow": "pass: the chunk index is guarded (receiver_guards below_n_chunks) before the call",
    "polars_core::series::arrow_export::categorical::CategoricalArrayToArrowConverter::build_values_array": "pass: returns an owned ArrayRef",
    "polars_dtype::categorical::mapping::CategoricalMapping::to_arrow": "pass: returns an owned ArrayRef",
}


def available(e):
    return e["status"] in ("generated", "adapted")


def ret_of(e, receiver):
    sig = e.get("signature") or ""
    r = sig.rsplit("-> ", 1)[-1] if "-> " in sig else ""
    return r.replace("Self", receiver or "Self")


def arrow_type(path):
    return path.startswith("polars_arrow::") or path in ARRAY_REF or "polars_arrow::" in path


reachable = set()
changed = True
while changed:
    changed = False
    for e in new.values():
        if e["status"] != "generated":
            continue
        for b in e.get("bindings", []):
            recv = b.get("receiver")
            if recv and arrow_type(recv) and recv not in reachable and e["key"] not in static:
                continue
            ret = ret_of(e, recv)
            for t in re.findall(r"polars_arrow::[\w:]+(?:<[^>]*>)?|alloc::boxed::Box<dyn polars_arrow::array::Array>", ret):
                key = "support::ArrayRef" if ("ArrayRef" in t or "dyn polars_arrow::array::Array" in t) else t.split("<")[0]
                if key not in reachable:
                    reachable.add(key)
                    changed = True

arrow = []
for k, e in sorted(new.items(), key=lambda x: (x[1]["canonical_path"], x[0])):
    if not e["canonical_path"].startswith("polars_arrow::"):
        continue
    reason = e.get("reason") or ""
    if available(e):
        recvs = [b.get("receiver") for b in e.get("bindings", [])]
        if recvs:
            reach = k in static or any((r is None) or (not arrow_type(r)) or (r in reachable) or (r.split("<")[0] in reachable) for r in recvs)
        else:
            # a marker entry (for example Eq installing Rune EQ) is its owner's
            reach = e["canonical_path"].split(" as ")[0] in reachable
        disp, detail = e["status"], f"{len(e.get('bindings', []))} bindings; {'script-reachable' if reach else 'available, not script-reachable'}"
    elif e["canonical_path"].startswith(DEFERRED_PREFIXES) and not e["canonical_path"].startswith("polars_arrow::array::Array::"):
        disp, detail = "deferred", f"record 0120 (concrete arrays, deferred by the release's deferred_type_prefixes): {reason[:200]}"
    elif any(f"::{s}" in reason or f"{s}<" in reason for s in STAGE3) and (
        "generic owner" in reason or "bucket: generic" in reason or "generic type" in reason
        or "no wrapped implementor" in reason
    ):
        disp, detail = "deferred", f"record 0120 (stage 3, concrete generic arrays): {reason[:200]}"
    else:
        ex = [x["reason"] for x in e.get("exceptions", [])]
        disp, detail = "refused", (f"{reason} | {ex[0]}" if ex and "[" not in reason and ex[0] not in reason else reason)
    arrow.append({"key": k, "path": e["canonical_path"], "disposition": disp, "detail": detail})

cross, unreviewed = [], []
for k, e in sorted(new.items(), key=lambda x: (x[1]["canonical_path"], x[0])):
    o = old.get(k)
    if e["canonical_path"].startswith("polars_arrow::") or o is None or o["status"] == e["status"]:
        continue
    row = {"key": k, "path": e["canonical_path"], "from": o["status"], "to": e["status"],
           "review": REVIEW.get(e["canonical_path"])}
    (cross if row["review"] else unreviewed).append(row)

counts = {
    "arrow": dict(sorted(Counter(r["disposition"] for r in arrow).items())),
    "arrow_available_reachable": sum(1 for r in arrow if r["disposition"] in ("generated", "adapted") and "script-reachable" in r["detail"] and "not script" not in r["detail"]),
    "arrow_available_not_reachable": sum(1 for r in arrow if "not script-reachable" in r["detail"]),
    "cross_crate": len(cross),
    "unreviewed": len(unreviewed),
}
doc = {"counts": counts, "arrow": arrow, "cross_crate": cross, "unreviewed": unreviewed,
       "reachable_arrow_types": sorted(reachable)}
json.dump(doc, open(out_p, "w"), indent=1, sort_keys=True)
open(out_p, "a").write("\n")
print(json.dumps(counts))
