#!/usr/bin/env python3
"""Record 0120: the audit, per pin, from committed surfaces.

  audit.py <inventory.json> <release.toml> <0119 surface.json> <0120 surface.json> <out.json>

Sections:
  owners       every row on a listed owner (the allowlist's owners), one
               disposition: generated (its bindings, per listed identity,
               each reached by that identity's typed downcast from
               ArrayRef, or static), or refused (with its reason)
  guards       every usize parameter of a generated binding with a receiver
               on a listed owner, with the guard that checks it (none may
               be missing)
  deferred     every other row under a deferred prefix: its 0120 status must
               equal its 0119 status (changed must be empty); a refusal whose
               detail now names the new wrappers is listed as refined. A
               trait's method generated only on listed identities is this
               record's scope, and counted with the owners.
  grown        rows generated in both records that gained bindings
  cross_crate  every other row whose status moved, with its review verdict
  unreviewed   any moved row the review table does not name (must be empty)
"""
import json
import sys
import tomllib
from collections import Counter

inv_p, rel_p, old_p, new_p, out_p = sys.argv[1:6]
inv = {c["key"]: c for c in json.load(open(inv_p))["callables"]}
rel = tomllib.load(open(rel_p, "rb"))
old = {e["key"]: e for e in json.load(open(old_p))["entries"]}
new_s = json.load(open(new_p))
new = {e["key"]: e for e in new_s["entries"]}

rows = rel.get("concrete_arrays", [])
owners = sorted({r["owner"] for r in rows})
identities = {r["owner"] + (f"<{r['native']}>" if r.get("native") else ""): r for r in rows}
downcasts = {x["identity"]: x for x in new_s.get("concrete_arrays", [])}
assert set(downcasts) == set(identities), "the surface's downcasts are not the release's rows"
deferred_prefixes = tuple(d["prefix"] for d in rel.get("deferred_type_prefixes", []))
guards = {g["path"]: g for g in rel.get("receiver_guards", [])}

# the cross-crate review (record 0120): every moved row off the listed owners
REVIEW = {
    "polars_core::chunked_array::ChunkedArray::get_row_encoded_array": "pass: returns an owned LargeBinaryArray (BinaryArray<i64>, a listed identity); its readers are guarded and bounded",
}


def owner_of(path):
    base = path.split(" as ")[0]
    return base.rsplit("::", 1)[0] if " as " not in path else base


def on_listed_owner(path):
    return any(path.startswith(o + "::") or path.startswith(o + " as ") for o in owners)


owner_rows, guard_rows = [], []
for k, e in sorted(new.items(), key=lambda x: (x[1]["canonical_path"], x[0])):
    p = e["canonical_path"]
    if not on_listed_owner(p):
        continue
    c = inv.get(k, {})
    if e["status"] in ("generated", "adapted"):
        recvs = sorted({b.get("receiver") or "(static)" for b in e.get("bindings", [])})
        static = c.get("receiver") == "none"
        bad = [r for r in recvs if r != "(static)" and r not in identities and not static]
        reach = "static" if static else ("by downcast: " + ", ".join(downcasts[r]["downcast"] for r in recvs if r in downcasts))
        owner_rows.append({"key": k, "path": p, "disposition": e["status"], "bindings": len(e.get("bindings", [])),
                           "reach": reach if not bad else f"NOT REACHABLE: {bad}"})
        if c.get("receiver", "none") != "none":
            for q in c.get("params", []):
                if q["ty_canonical"] == "usize":
                    g = guards.get(p)
                    named = g and q["name"] in (g["param"], g.get("param2"))
                    guard_rows.append({"path": p, "param": q["name"], "guard": g["check"] if named else "MISSING"})
    else:
        ex = sorted({x["reason"] for x in e.get("exceptions", [])})
        owner_rows.append({"key": k, "path": p, "disposition": "refused",
                           "reason": e.get("reason") or "", "exceptions": ex[:4]})

deferred, changed, refined = [], [], []
for k, e in sorted(new.items(), key=lambda x: (x[1]["canonical_path"], x[0])):
    p = e["canonical_path"]
    if on_listed_owner(p) or not p.startswith(deferred_prefixes):
        continue
    o = old.get(k)
    recvs = {b.get("receiver") for b in e.get("bindings", [])}
    if e["status"] in ("generated", "adapted") and recvs and recvs <= set(identities):
        # a trait's method served on listed identities only: this record's scope
        owner_rows.append({"key": k, "path": p, "disposition": e["status"], "bindings": len(e.get("bindings", [])),
                           "reach": "trait method, by downcast: " + ", ".join(sorted(downcasts[r]["downcast"] for r in recvs))})
        continue
    row = {"key": k, "path": p, "status": e["status"], "reason": (e.get("reason") or "")[:160]}
    deferred.append(row)
    # the disposition must be 0119's; a refusal's detail may name the new wrappers
    if o is None or o["status"] != e["status"]:
        changed.append({**row, "was": None if o is None else o["status"]})
    elif o.get("reason") != e.get("reason"):
        refined.append({"key": k, "path": p, "was": (o.get("reason") or "")[:300], "now": (e.get("reason") or "")[:300]})

cross, unreviewed = [], []
for k, e in sorted(new.items(), key=lambda x: (x[1]["canonical_path"], x[0])):
    o = old.get(k)
    p = e["canonical_path"]
    if on_listed_owner(p) or p.startswith(deferred_prefixes) or o is None or o["status"] == e["status"]:
        continue
    row = {"key": k, "path": p, "from": o["status"], "to": e["status"], "review": REVIEW.get(p)}
    (cross if row["review"] else unreviewed).append(row)

# rows generated in both records that gained bindings (new receivers or
# instantiations whose types are now wrapped); the freeze keeps the old ones
grown = []
for k, e in sorted(new.items(), key=lambda x: (x[1]["canonical_path"], x[0])):
    o = old.get(k)
    if o is None or o["status"] != "generated" or e["status"] != "generated":
        continue
    before = {b["id"] for b in o.get("bindings", [])}
    added = sorted(b["rune"] for b in e.get("bindings", []) if b["id"] not in before)
    if added:
        grown.append({"key": k, "path": e["canonical_path"], "added": added})

counts = {
    "owners": dict(sorted(Counter(r["disposition"] for r in owner_rows).items())),
    "owner_bindings": sum(r.get("bindings", 0) for r in owner_rows),
    "not_reachable": sum(1 for r in owner_rows if "NOT REACHABLE" in r.get("reach", "")),
    "guards": dict(sorted(Counter(r["guard"] for r in guard_rows).items())),
    "deferred_kept": len(deferred) - len(changed),
    "deferred_changed": len(changed),
    "deferred_reason_refined": len(refined),
    "grown_rows": len(grown),
    "grown_bindings": sum(len(g["added"]) for g in grown),
    "cross_crate": len(cross),
    "unreviewed": len(unreviewed),
    "identities": len(identities),
}
doc = {"counts": counts, "owners": owner_rows, "guards": guard_rows, "deferred_changed": changed,
       "deferred_reason_refined": refined,
       "grown": grown, "cross_crate": cross, "unreviewed": unreviewed, "identities": sorted(identities)}
json.dump(doc, open(out_p, "w"), indent=1, sort_keys=True)
open(out_p, "a").write("\n")
print(json.dumps(counts))
