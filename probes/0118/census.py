#!/usr/bin/env python3
"""Record 0118 stage 1: the census, before any emission.

For every row 0116/0117 stopped as `io_generic`, decide whether a script
path would exist with the plan's closed std-facts table: the reader or writer
is instantiated at `Cursor<Vec<u8>>` (a source, `R`) or `support::Sink` (a
sink, `W`), every bound on that parameter is proven from the table or from a
recorded impl whose own bounds reduce to the table, and the owner has both a
construction and a finishing step. Each row gets exactly one disposition:

  reachable
  refused: fact      a bound the table and the inventory cannot prove (named)
  refused: path      no construction or no finishing step (named)
  out of scope       a file handle, a non-in-memory source, async or cloud

  census.py <inventory.json> <surface.json> <out.json> [--reconcile <0116 final.json>]

The rows are selected by one rule at both pins: a callable whose impl head
is generic over `R`/`W` with an I/O bound, or that takes `&mut dyn Read` /
`&mut dyn Write`, or has a function-level `R`/`W` with an I/O bound. At v2,
`--reconcile` adds 0116's `io_generic` rows and reports both differences.
Only rows the pin's surface lists as not generated are counted.
"""
import json
import re
import sys
from collections import Counter, defaultdict

inv_p, surf_p, out_p = sys.argv[1:4]
final_p = sys.argv[sys.argv.index("--reconcile") + 1] if "--reconcile" in sys.argv else None
inv = json.load(open(inv_p))
C = {c["key"]: c for c in inv["callables"]}
T = {s["canonical_path"]: s for s in inv["supporting"]}

CURSOR = "core::io::cursor::Cursor<alloc::vec::Vec<u8>>"
VEC = "alloc::vec::Vec<u8>"
SINK = "support::Sink"
READ, WRITE = "alloc::io::read::Read", "core::io::write::Write"
SEEK, BUFREAD = "core::io::seek::Seek", "alloc::io::buf_read::BufRead"
SEND, SYNC = "core::marker::Send", "core::marker::Sync"
# the plan's closed table (stage 3 ships exactly these rows, cited)
FACTS = {
    (VEC, "core::convert::AsRef<[u8]>"), (VEC, SEND), (VEC, SYNC),
    (CURSOR, READ), (CURSOR, SEEK), (CURSOR, BUFREAD), (CURSOR, SEND), (CURSOR, SYNC),
    (SINK, WRITE), (SINK, SEND), (SINK, SYNC),
}
MMAP = "polars_io::mmap::MmapBytesReader"


def terms(bound):
    out, depth, cur = [], 0, ""
    for ch in bound:
        if ch in "<([":
            depth += 1
        elif ch in ">)]":
            depth -= 1
        if ch == "+" and depth == 0:
            out.append(cur.strip())
            cur = ""
            continue
        cur += ch
    if cur.strip():
        out.append(cur.strip())
    return [t for t in out if t not in ("?core::marker::Sized", "core::marker::Sized", "?Sized")]


def holds(ty, trait):
    """Proven from the table, or from one recorded impl whose bounds reduce to it."""
    if (ty, trait) in FACTS:
        return None
    if trait == MMAP and ty == CURSOR:
        impls = [i for i in T[MMAP]["impls"] if i["for_type"] == "core::io::cursor::Cursor<T>"]
        if len(impls) != 1:
            return f"{len(impls)} recorded impls of `{MMAP}` for `Cursor<T>`"
        for n, b in impls[0]["bounds"]:
            if n == "T":
                for t in terms(b):
                    why = holds(VEC, t)
                    if why:
                        return why
        return None
    return f"`{ty}: {trait}` is not in the table"


def bounds_of(var, c):
    """Every bound the row itself puts on `var` (impl bounds, where clauses, fn generics)."""
    out = []
    for n, b in c["impl_bounds"]:
        if n == var and b:
            out += terms(b)
    for w in c["impl_where"]:
        if w.startswith(f"{var}: "):
            out += terms(w[len(var) + 2:])
    for n, b in c["generics_canonical"]:
        if n == var and b:
            out += terms(b)
    return sorted(set(out))


def owner_var(c):
    head = c.get("impl_head") or ""
    m = re.fullmatch(r"(.+)<([RW])>", head)
    return (m.group(1), m.group(2)) if m else (None, None)


def inst(var):
    return CURSOR if var == "R" else SINK


IO_TRAITS = {READ, WRITE, SEEK, BUFREAD, MMAP}


def selected(c):
    if c["kind"] not in ("inherent", "trait_method", "free_fn", "foreign_trait_impl"):
        return False
    base, var = owner_var(c)
    if base and (not c["owner"].endswith("::SerReader") and not c["owner"].endswith("::SerWriter")):
        io = set(bounds_of(var, c))
        if io & IO_TRAITS or any(i["for_type"] == f"{base}<{var}>" for s in ("polars_io::shared::SerReader", "polars_io::shared::SerWriter") for i in T.get(s, {}).get("impls", [])):
            return True
    if any(p["ty_canonical"] in (f"&mut dyn {READ}", f"&mut dyn {WRITE}") for p in c["params"]):
        return True
    if any(n in ("R", "W") and set(terms(b)) & IO_TRAITS for n, b in c["generics_canonical"]):
        return True
    if any("ByteSourceReader<" in p["ty_canonical"] for p in c["params"]):
        return True
    return False


open_rows = {e["key"] for e in json.load(open(surf_p))["entries"] if e["status"] != "generated"}
got = {c["key"] for c in inv["callables"] if selected(c)} & open_rows
reconcile = None
if final_p:
    want = {r["key"] for r in json.load(open(final_p))["rows"] if r["family"] == "io_generic"}
    reconcile = {"only_0116": sorted(want - got), "only_rule": sorted(got - want)}
    got |= want & open_rows
rows = [{"key": k} for k in sorted(got)]

# owners generic over a source or sink, and the trait impls that construct/finish them
SER = {"R": "polars_io::shared::SerReader", "W": "polars_io::shared::SerWriter"}
owners = {}
for r in rows:
    base, var = owner_var(C[r["key"]])
    if base:
        owners[base] = var


def ser_impl(base, var):
    tr = T.get(SER[var])
    hits = [i for i in (tr or {}).get("impls", []) if i["for_type"] == f"{base}<{var}>"]
    return hits[0] if hits else None


def owner_path(base, var):
    """(ok, detail): a construction and a finishing step with every bound proven."""
    ty = inst(var)
    if T.get(base, {}).get("lifetime"):
        return False, "path: the owner carries a lifetime parameter, which a wrapper cannot own"
    ser = ser_impl(base, var)
    if ser:
        need = [t for n, b in ser["bounds"] if n == var for t in terms(b)]
        for w in ser["where_predicates"]:
            lhs, _, rhs = w.partition(": ")
            if lhs == var:
                need += terms(rhs)
            elif rhs:
                # a predicate on anything but the parameter is outside this census
                return False, f"path: {SER[var].rsplit('::', 1)[1]} impl has the predicate `{w}`, which this census does not decide"
        for t in need:
            why = holds(ty, t)
            if why:
                return False, f"fact: {SER[var].rsplit('::', 1)[1]} impl needs {why}"
        return True, f"{SER[var].rsplit('::', 1)[1]}::new then finish"
    own = [c for c in inv["callables"] if c["owner"] == base]
    ctor = [c for c in own if c["receiver"] == "none" and re.search(rf"\bSelf\b|{re.escape(base)}<{var}>", c["ret_canonical"] or "")
            and any(p["ty_canonical"] == var for p in c["params"])]
    ext = [c for c in inv["callables"] if (c["ret_canonical"] or "").startswith(f"{base}<") and c["owner"] != base]
    fin = [c for c in own if c["receiver"] in ("self", "&mut self", "&self")
           and c["ret_canonical"] and "Self" not in c["ret_canonical"] and base not in c["ret_canonical"]]
    if not (ctor or ext):
        borrowing = [c["name"] for c in own if c["receiver"] == "none" and any(p["ty_canonical"] == f"&mut {var}" for p in c["params"])]
        if borrowing:
            return False, f"path: the constructors ({', '.join(sorted(borrowing))}) borrow the sink as `&mut {var}`; a writer holding a borrow into the script's handle has no script contract"
        return False, "path: no constructor"
    if not fin:
        return False, "path: no finishing step"
    for c in ctor + fin:
        for t in bounds_of(var, c):
            why = holds(ty, t)
            if why:
                return False, f"fact: {c['name']} needs {why}"
    via = sorted({c["name"] for c in ctor} | {f"{c['owner'].rsplit('::', 1)[-1]}::{c['name']}" for c in ext})
    return True, f"constructed by {', '.join(via[:4])}; finished by {', '.join(sorted({c['name'] for c in fin})[:4])}"


opath = {b: owner_path(b, v) for b, v in owners.items()}
# a batched writer is reached through its parent writer's `batched`
for b in list(opath):
    if b.endswith("BatchedWriter") and not opath[b][0]:
        parents = [c for c in inv["callables"] if (c["ret_canonical"] or "").startswith(f"polars_error::PolarsResult<{b}<") and c["name"] == "batched"]
        ok = [p for p in parents if owner_var(p)[0] and opath.get(owner_var(p)[0], (False,))[0]]
        if ok:
            opath[b] = (True, f"constructed by {owner_var(ok[0])[0].rsplit('::', 1)[1]}::batched")

out = []
for r in rows:
    c = C[r["key"]]
    base, var = owner_var(c)
    sig = [p["ty_canonical"] for p in c["params"]]
    if base:
        ok, detail = opath[base]
        disp = "reachable" if ok else ("refused: fact" if detail.startswith("fact") else "refused: path")
        if ok:
            for t in bounds_of(var, c):
                why = holds(inst(var), t)
                if why:
                    disp, detail = "refused: fact", why
                    break
        if "file_handle" in c["name"]:
            disp, detail = "out of scope", "a file-handle constructor"
    else:
        dyn = [s for s in sig if s.startswith("&mut dyn ")]
        gen = dict((n, b) for n, b in c["generics_canonical"])
        if dyn:
            tr = dyn[0][len("&mut dyn "):]
            ty = CURSOR if tr == READ else SINK if tr == WRITE else None
            why = holds(ty, tr) if ty else f"`dyn {tr}` has no in-memory instantiation"
            disp, detail = ("reachable", f"`{dyn[0]}` taken as {'Cursor' if ty == CURSOR else 'Sink'}") if not why else ("refused: fact", why)
        elif gen and set(gen) <= {"R", "W"}:
            why = None
            for n, b in gen.items():
                for t in terms(b):
                    why = why or holds(inst(n), t)
            disp, detail = ("reachable", "function generic taken as " + ", ".join(f"{n} = {inst(n)}" for n in gen)) if not why else ("refused: fact", why)
            if c["name"] == "into_reader_with_file_handle":
                disp, detail = "out of scope", "a file-handle constructor"
        else:
            src = c.get("impl_head") or ", ".join(sig)
            disp, detail = "refused: path", f"a source this record does not instantiate ({src}; parameters {', '.join(sig)})"[:300]
    out.append({"key": r["key"], "path": c["canonical_path"], "owner": base or c["owner"], "disposition": disp, "detail": detail})

out.sort(key=lambda x: (x["owner"], x["path"], x["key"]))
counts = Counter(x["disposition"] for x in out)
owners_out = {b: {"reachable": v[0], "detail": v[1]} for b, v in sorted(opath.items())}
json.dump({"counts": dict(sorted(counts.items())), "owners": owners_out, "reconcile": reconcile, "rows": out}, open(out_p, "w"), indent=1, sort_keys=True)
open(out_p, "a").write("\n")
print(len(out), "rows", dict(sorted(counts.items())))
if reconcile:
    print("reconcile with 0116:", {k: len(v) for k, v in reconcile.items()})
for b, v in sorted(opath.items()):
    print(f"  {b.rsplit('::', 1)[1]:18} {'ok ' if v[0] else 'NO '} {v[1]}")
