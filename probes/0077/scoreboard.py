#!/usr/bin/env python3
"""The three-number scoreboard, from surface.json and oracle-results.json:
distinct operations available to a script, operations value-tested on at
least one receiver, and operations remaining.

  scoreboard.py <surface.json> <oracle-results.json>

Available = generated operations plus the nine hand-written equivalents
the surface lists as adapted for that reason. Value-tested = generated
operations with at least one oracle case whose outcome is a value match
(`match` or an approved row-order difference), error-only and panic-only
verifications excluded; the nine hand-written equivalents are evidenced
by the hand-written suites (`adapters/polars/tests/`), not by the oracle
file, and are reported apart. Remaining = the unsupported operations,
each with a reason in the surface; the marker impls (`StructuralPartialEq`,
`Eq`, `Copy`) add no operation of their own and are reported apart, not
as missing functionality.
"""
import json, sys
surface, results = json.load(open(sys.argv[1])), json.load(open(sys.argv[2]))
status = {r["id"]: r["status"] for r in results["results"]}
entries = [e for e in surface["entries"] if e["status"] != "out_of_scope"]  # the 710 internal-crate callables are counted apart
total = len(entries)
available = value = markers = unsupported = duplicates = 0
hand: list[str] = []
for e in entries:
    if e["status"] == "generated":
        available += 1
        if any(status.get(b.get("case_id")) in ("match", "row_order_differs") for b in e.get("bindings", []) if b.get("case_id")):
            value += 1
    elif e["status"] == "adapted":
        if e["reason"].startswith("hand-written"):
            available += 1
            hand.append(e["canonical_path"])
        elif e.get("counterpart"):
            duplicates += 1  # the same rustdoc impl listed on a second type's page, its retained listing generated (record 0078)
        else:
            markers += 1
    elif e["status"] == "unsupported":
        unsupported += 1
assert available + markers + duplicates + unsupported == total
print(f"| operations counted (API crates; {len(surface['entries']) - total} internal-crate callables apart) | {total} |")
print(f"| available to a script | {available} ({available * 100 // total}%) |")
print(f"| value-tested on at least one receiver, by the oracle | {value} ({value * 100 // total}%); plus {len(hand)} hand-written equivalents evidenced by the hand-written suites |")
print(f"| remaining: unsupported operations, each with a reason | {unsupported} ({unsupported * 100 // total}%) |")
print(f"| marker entries with no operation of their own, reported apart | {markers} |")
print(f"| duplicate listings of one impl on a second type's page, reported apart | {duplicates} |")
print("hand-written equivalents: " + ", ".join(p.split("::")[-2] + "::" + p.split("::")[-1] for p in hand))
