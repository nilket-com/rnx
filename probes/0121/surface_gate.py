#!/usr/bin/env python3
"""Record 0121: the surface gate. Only an explicit allowlist of oracle
fields may differ from the baseline; everything else in both surfaces is
compared recursively (review of 0121: a field-selecting gate let a
binding's `reentry` change pass).

  surface_gate.py <baseline surface.json> <new surface.json>
  surface_gate.py --self-test <surface.json>

Allowed to move:
  top level   source (the inventory file name), oracle_cases, fixtures,
              oracle_skipped
  entry       execution
  binding     case_id, disposition
"""
import copy
import json
import sys

TOP = {"source", "oracle_cases", "fixtures", "oracle_skipped"}
ENTRY = {"execution"}
BINDING = {"case_id", "disposition"}


def normalize(s):
    s = {k: v for k, v in s.items() if k not in TOP}
    entries = []
    for e in s.get("entries", []):
        e = {k: v for k, v in e.items() if k not in ENTRY}
        if "bindings" in e:
            e["bindings"] = [{k: v for k, v in b.items() if k not in BINDING} for b in e["bindings"]]
        entries.append(e)
    s["entries"] = entries
    return s


def first_difference(a, b, path="$"):
    if type(a) is not type(b):
        return path
    if isinstance(a, dict):
        for k in sorted(set(a) | set(b)):
            if k not in a or k not in b:
                return f"{path}.{k}"
            d = first_difference(a[k], b[k], f"{path}.{k}")
            if d:
                return d
        return None
    if isinstance(a, list):
        if len(a) != len(b):
            return f"{path}[len {len(a)} != {len(b)}]"
        for i, (x, y) in enumerate(zip(a, b)):
            d = first_difference(x, y, f"{path}[{i}]")
            if d:
                return d
        return None
    return None if a == b else path


def gate(old, new):
    d = first_difference(normalize(old), normalize(new))
    if d:
        # name the entry, when the difference is inside one
        if d.startswith("$.entries["):
            i = int(d[len("$.entries["):].split("]")[0])
            d += f" ({new['entries'][i].get('canonical_path')})"
        return d
    return None


if sys.argv[1] == "--self-test":
    base = json.load(open(sys.argv[2]))
    assert gate(base, base) is None
    # an allowed annotation moves: passes
    m = copy.deepcopy(base)
    e = next(e for e in m["entries"] if e.get("bindings"))
    e["execution"] = "changed"
    e["bindings"][0]["case_id"] = "changed"
    m["oracle_cases"] = -1
    assert gate(base, m) is None, "an allowed annotation was refused"
    # a binding contract field moves: refused (the reviewer's mutation)
    for field, value in [("reentry", "mutated"), ("route_reason", "mutated"), ("route", "mutated")]:
        m = copy.deepcopy(base)
        e = next(e for e in m["entries"] if e.get("bindings"))
        e["bindings"][0][field] = value
        assert gate(base, m), f"a moved binding {field} passed"
    # entry fields and top-level fields outside the allowlist: refused
    for field in ("bucket", "kind", "exceptions", "note", "status"):
        m = copy.deepcopy(base)
        m["entries"][0][field] = "mutated"
        assert gate(base, m), f"a moved entry {field} passed"
    m = copy.deepcopy(base)
    m["materialize_limit"] = -1
    assert gate(base, m), "a moved top-level field passed"
    print("surface gate self-test: ok")
    sys.exit(0)

old, new = (json.load(open(p)) for p in sys.argv[1:3])
d = gate(old, new)
if d:
    sys.exit(f"surface moved at {d}")
changed = sum(1 for a, b in zip(old["entries"], new["entries"]) if a != b)
print(json.dumps({"entries": len(new["entries"]), "moved": 0, "oracle_annotations_changed": changed}))
