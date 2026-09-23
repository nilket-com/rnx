#!/usr/bin/env python3
"""Record 0093 read-back census: compares the 0092 generated bindings with the
current ones and classifies every u64/usize/isize/i128/u128 read-back as
checked (support::widen, with its route) or bounded (support::bounded_usize,
the release allowlist). Usage: census.py <0092 functions.rs> <current functions.rs> <out.json>
The 0092 file is `git show db817ca:adapters/polars/src/generated/functions.rs`."""
import json, re, sys, collections

RISKY = ["u64", "usize", "isize", "i128", "u128"]

def bindings(path):
    out, doc = {}, None
    for line in open(path):
        if line.startswith("/// Polars: `"):
            doc = line.split("`")[1]
        m = re.match(r"(?:pub(?:\(crate\))? )?fn ([a-z]_[0-9a-f]{8}_\w+)\((.*?)\)(?: -> (.*?))? \{", line)
        if m:
            out[m.group(1)] = {"path": doc, "ret": (m.group(3) or "()").strip(), "body": line}
    return out

def route(body, at, op):
    before = body[max(0, at - 160):at]
    if "::" in op:
        return "callback input"
    if "copy_slice(" in before[-110:]:
        return "slice element"
    if "materialize" in before or re.search(r"\|__r\| Ok::<_, Error>\((match __r \{ Some\(__r\) => Some\()?$", before):
        return "iterator item"
    if re.search(r"let __r = __t\.\d+; $", before):
        return "tuple field"
    if before.endswith("Some(__r) => Some("):
        return "option"
    return "direct"

old, new = bindings(sys.argv[1]), bindings(sys.argv[2])
rows, bounded, changes = [], [], []
pat = re.compile(r"support::widen::<(%s)>\(__r, \"([^\"]+)\"\)" % "|".join(RISKY))
for name, b in sorted(new.items()):
    before = old.get(name)
    pre = collections.Counter((m.group(1), m.group(2)) for m in pat.finditer(before["body"])) if before else collections.Counter()
    for m in pat.finditer(b["body"]):
        key = (m.group(1), m.group(2))
        if pre[key] > 0:
            pre[key] -= 1  # checked before this record (0087 chunk_lengths, 0089 arg extrema)
            continue
        rows.append({"binding": name, "polars": b["path"], "source": m.group(1), "operation": m.group(2), "route": route(b["body"], m.start(), m.group(2)), "return_before": before["ret"] if before else None, "return_after": b["ret"]})
    for _ in re.finditer(r"support::bounded_usize\(", b["body"]):
        bounded.append({"binding": name, "polars": b["path"], "return_before": before["ret"] if before else None, "return_after": b["ret"]})
    if before and before["ret"] != b["ret"]:
        changes.append({"binding": name, "polars": b["path"], "before": before["ret"], "after": b["ret"]})

risky_casts = sum(len(re.findall(r"\(__r as i64\)", b["body"])) for b in new.values())
result = {
    "checked_readbacks": len(rows),
    "by_source": dict(collections.Counter(r["source"] for r in rows)),
    "by_route": dict(collections.Counter(r["route"] for r in rows)),
    "bindings_with_checked_readback": len({r["binding"] for r in rows}),
    "bounded_readbacks": len(bounded),
    "bounded_identities": sorted({r["polars"] for r in bounded}),
    "bindings_whose_signature_became_fallible": sum(1 for c in changes if c["after"].startswith("Result<") and not c["before"].startswith("Result<")),
    "signature_changes": len(changes),
    "remaining_plain_as_i64_sites": risky_casts,
    "remaining_as_i64_rule": "emitted only by World::ret for i8, i16, i32, u8, u16, u32 (INT_NARROW minus RISKY_INTS) and IdxSize (u32 without bigidx, asserted in support.rs); every range fits i64",
    "preexisting_checked": "0087 chunk_lengths (widen::<usize>), 0089 arg_min/max_numeric (widen::<usize>), 0091 convert_and_bound_idx_ca returns IdxCa (no scalar read-back)",
    "rows": rows,
    "bounded": bounded,
    "changes": changes,
}
open(sys.argv[3], "w").write(json.dumps(result, indent=1) + "\n")
print({k: v for k, v in result.items() if k not in ("rows", "bounded", "changes")})
