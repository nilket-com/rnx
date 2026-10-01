#!/usr/bin/env python3
"""Record 0128 (review): classify the Rune bindings shipped at 0.55.2 that v2
does not bind, reproducibly, from retained inputs.

  classify_dropped.py <0.55.2 surface.json> <v2 surface.json> <v2 inventory.json> <v2 polars crates dir> <out.json>

For each Rune path bound at 0.55.2 and not at v2, by its 0.55.2 canonical path:
- in the v2 inventory and bound there for other receivers only
  -> "lost for this type (bound for other types at v2)"
- in the v2 inventory, not bound
  -> "lost in rnx (in the v2 inventory, not bound)", with v2's recorded reason
- not in the v2 inventory: the item's definition is searched for in the v2
  source of its own crate (`fn name`, `struct|enum|trait name`, or the impl)
  -> "confirmed removed (no definition in the v2 source)" when absent
  -> "absent from the v2 inventory, present in its source" (moved,
     feature-gated or unreachable: to be followed up) when found; a method
     whose owning type has no definition counts as removed with its owner.
     The name search is conservative: a common name found anywhere in the
     crate counts as present, so nothing is claimed removed that might not be.
A replacement is never inferred: it needs a named equivalent, which this
script does not guess. The output lists every row with its evidence.
"""
import json
import os
import re
import subprocess
import sys


def bound(surface):
    out = {}
    for e in surface["entries"]:
        if e["status"] == "generated":
            for b in e.get("bindings", []):
                out[b["rune"]] = e
    return out


def defined_in_source(crates, canonical):
    """Whether the item behind a canonical path is defined in its crate's v2 source."""
    head = canonical.split(" as ")[0]
    krate = head.split("::")[0].replace("_", "-")
    root = os.path.join(crates, krate, "src")
    if not os.path.isdir(root):
        return False, f"no crate {krate} at v2"
    if " as " in canonical:
        # a trait impl: the implementing type and the trait's last segment
        ty = head.split("::")[-1]
        tr = canonical.split(" as ")[1].split("::")[-1].split("<")[0]
        pat = rf"impl(<[^>]*>)?\s+[\w:]*{re.escape(tr)}[^ ]*\s+for\s+[\w:]*{re.escape(ty)}\b|derive\([^)]*\b{re.escape(tr)}\b"
        where = ty
    else:
        name = head.split("::")[-1]
        parts = head.split("::")
        # a method: its owning type must still be defined, or the method went with it
        if len(parts) >= 3 and parts[-2][:1].isupper():
            owner = parts[-2]
            r = subprocess.run(["grep", "-rEl", rf"\b(struct|enum|trait|type)\s+{re.escape(owner)}\b", root],
                               capture_output=True, text=True)
            if not r.stdout.split():
                return False, f"owner {owner} has no definition in {krate}/src"
        pat = rf"\bfn\s+{re.escape(name)}\b|\b(struct|enum|trait|type)\s+{re.escape(name)}\b"
        where = name
    r = subprocess.run(["grep", "-rEl", pat, root], capture_output=True, text=True)
    files = [f for f in r.stdout.split() if f]
    if " as " in canonical and files:
        # the impl or derive must sit with the type's own definition or impls
        files = [f for f in files if re.search(rf"\b{re.escape(where)}\b", open(f, errors="replace").read())]
    return bool(files), (os.path.relpath(files[0], crates) if files else f"no definition of {where} in {krate}/src")


def main():
    s55, sv2, inv, crates, out = sys.argv[1:6]
    b55 = bound(json.load(open(s55)))
    v2 = json.load(open(sv2))
    bv2 = bound(v2)
    inv_paths = {c["canonical_path"] for c in json.load(open(inv))["callables"]}
    entries = {}
    for e in v2["entries"]:
        entries.setdefault(e["canonical_path"], []).append(e)
    rows = []
    for rune in sorted(set(b55) - set(bv2)):
        cp = b55[rune]["canonical_path"]
        if cp in inv_paths:
            es = entries.get(cp, [])
            if any(x["status"] == "generated" for x in es):
                kind, evidence = "lost for this type (bound for other types at v2)", [b["rune"] for x in es for b in x.get("bindings", [])][:3]
            else:
                kind, evidence = "lost in rnx (in the v2 inventory, not bound)", (es[0].get("reason") if es else "no surface entry")
        else:
            found, where = defined_in_source(crates, cp)
            kind = ("absent from the v2 inventory, present in its source" if found
                    else "confirmed removed (no definition in the v2 source)")
            evidence = where
        rows.append({"rune": rune, "canonical_path": cp, "kind": kind, "evidence": evidence})
    counts = {}
    for r in rows:
        counts[r["kind"]] = counts.get(r["kind"], 0) + 1
    json.dump({"counts": counts, "rows": rows}, open(out, "w"), indent=1)
    open(out, "a").write("\n")
    print(json.dumps(counts, indent=1))


if __name__ == "__main__":
    main()
