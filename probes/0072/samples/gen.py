#!/usr/bin/env python3
"""Record 0072 sample generator.

  gen.py <inventory.json> [--build] [--release 0.55.2]

--release pins the harness's polars crates to another release (the frozen
generator against the adjacent release is the maintenance measurement).

Selects samples deterministically from the classified inventory, writes
harness/src/generated.rs and harness/tests/samples.rs, and with --build
compiles, disables samples that fail to compile (recording the error),
regenerates until the crate builds, runs the tests and writes results.json.
"""
import json, os, re, subprocess, sys, collections

HERE = os.path.dirname(os.path.abspath(__file__))
HARNESS = os.path.join(HERE, "harness")
QUOTA = {"mechanical": 20, "conversion": 10, "option_struct": 10, "callback": 10}
API_CRATES = {"polars_core", "polars_plan", "polars_lazy", "polars_io", "polars_ops", "polars_time", "polars_dtype", "polars_schema", "polars_error"}
MAX_ROUNDS = 60

# ---- fixtures: type name -> (rust path, fixture fn, show fn) ----------------
WRAPPED = {
    "Expr": ("p::Expr", "expr", "expr"),
    "DataFrame": ("p::DataFrame", "df", "df"),
    "LazyFrame": ("p::LazyFrame", "lf", "lf"),
    "Series": ("p::Series", "series", "series"),
    "Column": ("p::Column", "column", "column"),
    "DataType": ("p::DataType", "dtype", "dtype"),
    "Schema": ("p::Schema", "schema", "schema"),
    "Field": ("p::Field", "field", "field"),
    "LazyGroupBy": ("p::LazyGroupBy", "group_by", "group_by"),
}
INTS = {"i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "usize", "isize", "IdxSize"}
FLOATS = {"f32", "f64"}
SCALAR_FIX = {"int": "2", "float": "1.5", "bool": "true", "str": '"x"'}

# ---- tiny type parser over the extractor's rendered strings ----------------
def split_top(s):
    out, depth, cur = [], 0, ""
    for ch in s:
        if ch in "<([": depth += 1
        elif ch in ">)]": depth -= 1
        if ch == "," and depth == 0:
            out.append(cur.strip()); cur = ""
        else:
            cur += ch
    if cur.strip(): out.append(cur.strip())
    return out

def parse(s):
    s = s.strip()
    if s.startswith("&mut "): return ("refmut", parse(s[5:]))
    if s.startswith("&"): return ("ref", parse(s[1:]))
    if s.startswith("[") and s.endswith("]"): return ("slice", parse(s[1:-1]))
    if s.startswith("(") and s.endswith(")"):
        inner = s[1:-1]
        return ("tuple", [parse(x) for x in split_top(inner)] if inner else [])
    if s.startswith("impl "): return ("impl", [parse_bound(b) for b in s[5:].split(" + ")])
    if s.startswith("dyn "): return ("dyn", s)
    if s.startswith("fn("): return ("fnptr", s)
    m = re.match(r"^([A-Za-z_][A-Za-z0-9_:]*)<(.*)>$", s)
    if m: return ("path", m.group(1).split("::")[-1], [parse(a) for a in split_top(m.group(2))])
    if re.match(r"^[A-Za-z_][A-Za-z0-9_:]*$", s): return ("path", s.split("::")[-1], [])
    return ("other", s)

def parse_bound(b):
    b = b.strip()
    m = re.match(r"^(Fn|FnMut|FnOnce)\((.*?)\)(?: -> (.*))?$", b)
    if m:
        return ("fn", m.group(1), [parse(a) for a in split_top(m.group(2))] if m.group(2) else [], parse(m.group(3)) if m.group(3) else ("tuple", []))
    m = re.match(r"^([A-Za-z_][A-Za-z0-9_:]*)<(.*)>$", b)
    if m:
        name = m.group(1).split("::")[-1]; inner = m.group(2)
        if name == "IntoIterator":
            mm = re.match(r"Item = (.*)", inner)
            return ("bound", name, [parse(mm.group(1))] if mm else [])
        return ("bound", name, [parse(a) for a in split_top(inner)])
    return ("bound", b.split("::")[-1], [])

class Unsupported(Exception): pass

class Gen:
    """Per-sample code generation. Raises Unsupported with a reason."""
    def __init__(self, c, types):
        self.c = c; self.types = types
        self.generics = {g["name"]: [parse_bound(b) for b in g["bounds"].split(" + ") if b.strip()] for g in c["generics"]}
        for w in c["where_clause"]:
            if ": " in w:
                lhs, rhs = w.split(": ", 1)
                if lhs in self.generics or re.match(r"^[A-Z][A-Za-z0-9]*$", lhs):
                    self.generics.setdefault(lhs, []).extend(parse_bound(b) for b in rhs.split(" + "))
        self.owner = c["owner"].split("::")[-1] if c["kind"] != "free_fn" else None
        self.wrapped_used = set(); self.opts_used = set(); self.notes = []
        self.callbacks = []  # parameter names converted to SyncFunction inside the wrapper

    def resolve_generic(self, t):
        if t[0] == "path" and not t[2] and t[1] in self.generics:
            return ("impl", self.generics[t[1]])
        return t

    # -- argument: returns (rust param type, rune-side script literal, rust conversion expr from param name)
    def arg(self, t, name, depth=0):
        t = self.resolve_generic(t)
        k = t[0]
        if k == "ref":
            inner = t[1]
            if inner == ("path", "str", []): return ("&str", SCALAR_FIX["str"], name)
            if inner[0] == "slice":
                rt, lit, conv = self.arg(("path", "Vec", [inner[1]]), name, depth + 1)
                return (rt, lit, f"&{conv}")
            if inner[0] == "path" and inner[1] in WRAPPED:
                self.wrapped_used.add(inner[1])
                return (f"&W_{inner[1]}", f"s::fx_{inner[1]}()", f"&{name}.0")
            if inner[0] == "path" and inner[1] in ("PlSmallStr", "String"): return ("&str", SCALAR_FIX["str"], f"&{name}.into()" if inner[1] == "PlSmallStr" else f"&{name}.to_string()")
            if inner[0] == "path" and inner[1] in self.types and self.is_opts(inner[1]):
                self.opts_used.add(inner[1])
                return (f"&W_{inner[1]}", f"s::fx_{inner[1]}()", f"&{name}.0")
            rt, lit, conv = self.arg(inner, name, depth + 1)
            return (rt, lit, f"&{conv}")
        if k == "refmut":
            inner = t[1]
            if inner[0] == "path" and inner[1] in WRAPPED:
                self.wrapped_used.add(inner[1])
                return (f"&mut W_{inner[1]}", f"s::fx_{inner[1]}()", f"&mut {name}.0")
            raise Unsupported(f"&mut of {inner}")
        if k == "path":
            n, args = t[1], t[2]
            if n == "bool": return ("bool", SCALAR_FIX["bool"], name)
            if n in INTS: return ("i64", SCALAR_FIX["int"], name if n == "i64" else f"({name} as {n if n != 'IdxSize' else 'p::IdxSize'})")
            if n in FLOATS: return ("f64", SCALAR_FIX["float"], name if n == "f64" else f"({name} as f32)")
            if n == "String": return ("String", SCALAR_FIX["str"], name)
            if n in ("PlSmallStr", "SmartString"): return ("String", SCALAR_FIX["str"], f"p::PlSmallStr::from({name})")
            if n == "str": return ("&str", SCALAR_FIX["str"], name)
            if n in WRAPPED:
                self.wrapped_used.add(n)
                return (f"&W_{n}", f"s::fx_{n}()", f"{name}.0.clone()")
            if n == "Option" and args:
                rt, lit, conv = self.arg(args[0], "v", depth + 1)
                rt = rt.replace("&mut ", "").replace("&", "") if rt.startswith("&W_") else rt
                if rt == "&str": rt = "String"; conv = conv.replace("v", "v.as_str()", 1) if "v" in conv else conv
                return (f"Option<{rt}>", f"Some({lit})", f"{name}.map(|v| {conv})")
            if n in ("Vec", "IntoVec") and args:
                rt, lit, conv = self.arg(args[0], "v", depth + 1)
                rt = rt.replace("&", "") if rt.startswith("&W_") else rt
                if rt == "&str": rt = "String"; conv = "v.as_str()" if conv == "v" else conv.replace("v", "v.as_str()", 1)
                return (f"Vec<{rt}>", f"[{lit}]", f"{name}.into_iter().map(|v| {conv}).collect::<Vec<_>>()")
            if n == "Self" and self.owner:
                return self.arg(("path", self.owner, []), name, depth)
            if n in self.types and self.is_opts(n):
                self.opts_used.add(n)
                return (f"&W_{n}", f"s::fx_{n}()", f"{name}.0.clone()")
            if n in self.types and self.types[n]["kind"] == "enum" and self.unit_enum(n):
                self.opts_used.add(n)
                return (f"&W_{n}", f"s::fx_{n}()", f"{name}.0.clone()")
            raise Unsupported(f"no fixture for type {n}")
        if k == "tuple":
            if not t[1]: return ("()", "()", name)
            parts = [self.arg(x, f"{name}.{i}", depth + 1) for i, x in enumerate(t[1])]
            rts = ", ".join(p[0].replace("&", "") if p[0].startswith("&W_") else p[0] for p in parts)
            lits = ", ".join(p[1] for p in parts)
            convs = ", ".join(p[2].replace(f"{name}.{i}.0.clone()", f"{name}.{i}.0.clone()") for i, p in enumerate(parts))
            return (f"({rts})", f"({lits})", f"({convs})")
        if k == "slice":
            return self.arg(("path", "Vec", [t[1]]), name, depth)
        if k == "impl":
            bounds = t[1]
            for b in bounds:
                if b[0] == "fn": return self.callback(b, name)
                if b[0] == "bound" and b[1] in ("Into", "TryInto") and b[2]:
                    rt, lit, conv = self.arg(b[2][0], name, depth + 1)
                    return (rt, lit, conv)
                if b[0] == "bound" and b[1] == "AsRef" and b[2] and b[2][0] == ("path", "str", []):
                    return ("&str", SCALAR_FIX["str"], name)
                if b[0] == "bound" and b[1] in ("IntoVec", "IntoIterator") and b[2]:
                    return self.arg(("path", "Vec", [b[2][0]]), name, depth + 1)
                if b[0] == "bound" and b[1] == "AsRef" and b[2] and b[2][0] == ("slice", ("path", "u8", [])):
                    return ("&str", SCALAR_FIX["str"], f"{name}.as_bytes()")
            raise Unsupported(f"generic bound {t[1]}")
        raise Unsupported(f"type shape {t}")

    def is_opts(self, n):
        s = self.types.get(n)
        if s and s.get("lifetime"): raise Unsupported(f"type {n} has a lifetime parameter")
        return bool(s) and s["kind"] == "struct" and s["public_fields"] > 0

    def unit_enum(self, n):
        s = self.types.get(n)
        if not s or s["kind"] != "enum": return False
        if s.get("lifetime"): raise Unsupported(f"type {n} has a lifetime parameter")
        if not any(not d for _, d in s["variant_shapes"]): raise Unsupported(f"enum {n} has no unit variant for a fixture")
        return True

    def callback(self, b, name):
        _, kind, args, ret = b
        if not args: raise Unsupported("closure arity 0")
        norm = []
        for a in args:
            by_ref = a[0] == "ref"
            if by_ref: a = a[1]
            if a == ("path", "Self", []) and self.owner: a = ("path", self.owner, [])
            if not (a[0] == "path" and a[1] in WRAPPED): raise Unsupported(f"closure arg {a}")
            norm.append((a[1], by_ref))
        def unwrap_ret(r):
            layers = []
            while r[0] == "path" and r[1] in ("PolarsResult", "Result", "Option") and r[2]:
                layers.append(r[1]); r = r[2][0]
            return layers, r
        layers, core = unwrap_ret(ret)
        if core == ("path", "Self", []) and self.owner: core = ("path", self.owner, [])
        if not (core[0] == "path" and core[1] in WRAPPED): raise Unsupported(f"closure return {ret}")
        # the script closure returns its last argument of the return type
        pick = None
        for i, (n, _) in enumerate(norm):
            if n == core[1]: pick = i
        if pick is None: raise Unsupported(f"closure returns {core[1]} which is not among its arguments")
        self.wrapped_used.update([n for n, _ in norm] + [core[1]])
        inner = "out.0"
        for l in reversed(layers):
            if l == "Option": inner = f"Some({inner})"
            elif l in ("PolarsResult", "Result"): inner = f"Ok({inner})"
        rust_args = ", ".join(f"arg{i}: {'&' if r else ''}{WRAPPED[n][0]}" for i, (n, r) in enumerate(norm))
        rune_args = ", ".join(f"W_{n}(arg{i}.clone())" for i, (n, _) in enumerate(norm))
        conv = (f"move |{rust_args}| -> {self.rust_ty(ret)} {{ "
                f"let out: W_{core[1]} = match {name}_sync.call(({rune_args},)) {{ "
                f"rune::runtime::VmResult::Ok(v) => v, rune::runtime::VmResult::Err(e) => {self.err_expr(layers, 'e')} }}; {inner} }}")
        self.callbacks.append(name)
        self.notes.append(f"callback {kind}({', '.join(n for n, _ in norm)}) -> {self.rust_ty(ret)}")
        bound_text = " ".join(self.c["where_clause"]) + " " + " ".join(g["bounds"] for g in self.c["generics"])
        if "Send" in bound_text or "Sync" in bound_text:
            self.notes.append("closure must be Send + Sync: a rune::runtime::Function is neither in Rune 0.14.2")
        names = ", ".join(f"c{i}" for i in range(len(norm)))
        return ("rune::runtime::Function", f"|{names}| {{ c{pick} }}", conv)

    def err_expr(self, layers, e):
        if layers and layers[0] in ("PolarsResult", "Result"):
            return f"return Err(p::PolarsError::ComputeError(format!(\"rune callback: {{{e}}}\").into()))"
        return f"panic!(\"rune callback: {{{e}}}\")"

    def tpath(self, n):
        if n in WRAPPED: return WRAPPED[n][0]
        s = self.types.get(n)
        if s: return best_path(s)
        return f"p::{n}"

    def rust_ty(self, t):
        t = self.resolve_generic(t)
        k = t[0]
        if k == "ref": return "&" + self.rust_ty(t[1])
        if k == "refmut": return "&mut " + self.rust_ty(t[1])
        if k == "slice": return "[" + self.rust_ty(t[1]) + "]"
        if k == "tuple": return "(" + ", ".join(self.rust_ty(x) for x in t[1]) + ")"
        if k == "path":
            n = t[1]
            if n in WRAPPED: base = WRAPPED[n][0]
            elif n == "Self": base = self.tpath(self.owner)
            elif n in ("PolarsResult", "PolarsError", "PlSmallStr", "IdxSize"): base = f"p::{n}"
            elif n in self.types: base = self.tpath(n)
            else: base = n
            return base + ("<" + ", ".join(self.rust_ty(x) for x in t[2]) + ">" if t[2] else "")
        raise Unsupported(f"cannot spell type {t}")

    # -- return: (rust return type of wrapper, conversion from `r`, script formatting expression over `r`)
    def ret(self, t, expr):
        t = self.resolve_generic(t)
        k = t[0]
        if k == "ref":
            rt, conv, fmt = self.ret(t[1], f"({expr}).clone()")
            self.notes.append("borrowed return cloned")
            return rt, conv, fmt
        if k == "refmut":
            rt, conv, fmt = self.ret(t[1], f"({expr}).clone()")
            self.notes.append("&mut return cloned")
            return rt, conv, fmt
        if k == "tuple" and not t[1]: return ("()", expr, "`()`")
        if k == "tuple":
            parts = [self.ret(x, f"t.{i}") for i, x in enumerate(t[1])]
            return ("(" + ", ".join(p[0] for p in parts) + ")", f"{{ let t = {expr}; (" + ", ".join(p[1] for p in parts) + ") }", "`(tuple)`")
        if k == "path":
            n, args = t[1], t[2]
            if n == "Self": n = self.owner; args = []
            if n == "bool": return ("bool", expr, "`${r}`")
            if n in INTS: return ("i64", f"({expr}) as i64", "`${r}`")
            if n in FLOATS: return ("f64", f"({expr}) as f64", "`${r}`")
            if n in ("String", "str"): return ("String", f"({expr}).to_string()", "`${r}`")
            if n in ("PlSmallStr", "SmartString"): return ("String", f"({expr}).to_string()", "`${r}`")
            if n in WRAPPED:
                self.wrapped_used.add(n)
                return (f"W_{n}", f"W_{n}({expr})", "s::dbg(r)")
            if n in ("PolarsResult", "Result") and args:
                rt, conv, fmt = self.ret(args[0], "v")
                return (f"Result<{rt}, String>", f"({expr}).map(|v| {conv}).map_err(|e| e.to_string())", "match r { Ok(r) => `Ok(${" + fmt + "})`, Err(e) => `Err(${e})` }")
            if n == "Option" and args:
                rt, conv, fmt = self.ret(args[0], "v")
                return (f"Option<{rt}>", f"({expr}).map(|v| {conv})", "match r { Some(r) => `Some(${" + fmt + "})`, None => `None` }")
            if n == "Vec" and args:
                rt, conv, fmt = self.ret(args[0], "v")
                return (f"Vec<{rt}>", f"({expr}).into_iter().map(|v| {conv}).collect::<Vec<_>>()", "`[${r.iter().map(|r| " + fmt + ").collect::<Vec>().iter().fold(\"\", |a, b| a + \",\" + b)}]`")
            if n in self.types and (self.is_opts(n) or self.types[n]["kind"] == "enum"):
                if self.types[n]["kind"] == "enum": self.unit_enum(n)
                self.opts_used.add(n)
                return (f"W_{n}", f"W_{n}({expr})", "s::dbg(r)")
            raise Unsupported(f"no fixture for return type {n}")
        raise Unsupported(f"return shape {t}")

    def oracle_ret(self, t, expr):
        """Rust-side string for the oracle value, matching the script formatting."""
        t = self.resolve_generic(t)
        k = t[0]
        if k in ("ref", "refmut"): return self.oracle_ret(t[1], f"({expr}).clone()")
        if k == "tuple" and not t[1]: return f'{{ let _ = {expr}; "()".to_string() }}'
        if k == "tuple": return f'{{ let _ = {expr}; "(tuple)".to_string() }}'
        n, args = t[1], t[2]
        if n == "Self": n = self.owner; args = []
        if n in ("bool",) or n in INTS or n in FLOATS: return f"format!(\"{{}}\", ({expr}) as {'bool' if n=='bool' else ('i64' if n in INTS else 'f64')})"
        if n in ("String", "str", "PlSmallStr", "SmartString"): return f"format!(\"{{}}\", ({expr}).to_string())"
        if n in WRAPPED: return f"show::{WRAPPED[n][2]}(&({expr}))"
        if n in ("PolarsResult", "Result") and args:
            return f"match {expr} {{ Ok(v) => format!(\"Ok({{}})\", {self.oracle_ret(args[0], 'v')}), Err(e) => format!(\"Err({{}})\", e) }}"
        if n == "Option" and args:
            return f"match {expr} {{ Some(v) => format!(\"Some({{}})\", {self.oracle_ret(args[0], 'v')}), None => \"None\".to_string() }}"
        if n == "Vec" and args:
            return f"format!(\"[{{}}]\", ({expr}).into_iter().map(|v| {self.oracle_ret(args[0], 'v')}).fold(String::new(), |a, b| a + \",\" + &b))"
        if n in self.types: return f"format!(\"{{:?}}\", {expr})"
        raise Unsupported(f"oracle for {n}")

def load(inv_path):
    inv = json.load(open(inv_path))
    # names that two different items export at polars::prelude::<name> are
    # ambiguous globs, which rustc drops
    owners = collections.defaultdict(set)
    for c in inv["callables"]:
        for fp in c["found_paths"]:
            if fp.startswith("polars::prelude::") and fp.count("::") == 2:
                owners[fp.split("::")[-1]].add(c["key"])
    for s in inv["supporting"]:
        for fp in s["found_paths"]:
            if fp.startswith("polars::prelude::") and fp.count("::") == 2:
                owners[fp.split("::")[-1]].add(s["key"])
    AMBIGUOUS_PRELUDE.update(n for n, ks in owners.items() if len(ks) > 1)
    types = {}
    for s in inv["supporting"]:
        n = s["canonical_path"].split("::")[-1]
        types.setdefault(n, s)
    # Default availability: derived or hand-written foreign impl
    has_default = set()
    for s in inv["supporting"]:
        if "Default" in s["derived"]: has_default.add(s["canonical_path"])
    for c in inv["callables"]:
        if c["kind"] == "foreign_trait_impl" and c["name"].startswith("Default"): has_default.add(c["owner"])
    return inv, types, has_default

def select(inv, types, has_default, disabled):
    cands = [c for c in inv["callables"] if c["bucket"] in QUOTA and c["krate"] in API_CRATES]
    cands.sort(key=lambda c: (c["canonical_path"], c["key"]))
    by_shape = collections.OrderedDict()
    for c in cands:
        by_shape.setdefault((c["bucket"], c["shape"]), []).append(c)
    selected, skipped, used = [], [], set()
    for bucket, quota in QUOTA.items():
        shapes = [k for k in by_shape if k[0] == bucket]
        # most populated shapes first, then by name: deterministic, and the
        # quota lands on the shapes that carry the most entries
        shapes.sort(key=lambda k: (-len(by_shape[k]), k[1]))
        pos = {k: 0 for k in shapes}
        count = 0
        progress = True
        while count < quota and progress:
            progress = False
            for k in shapes:
                if count >= quota: break
                lst = by_shape[k]
                while pos[k] < len(lst):
                    c = lst[pos[k]]; pos[k] += 1
                    if c["key"] in used: continue
                    used.add(c["key"])
                    reason = feasible(c, types, has_default)
                    if reason:
                        skipped.append({"key": c["key"], "path": c["canonical_path"], "bucket": bucket, "shape": c["shape"], "reason": reason}); continue
                    selected.append(c); count += 1; progress = True
                    break
        if count < quota:
            print(f"note: bucket {bucket} has only {count} feasible samples (quota {quota})")
    # every conversion rule at least once
    for rule in ("P3", "P4", "T2", "F3"):
        if not any(rule in c["rules"] for c in selected if c["bucket"] == "conversion"):
            for c in cands:
                if c["bucket"] == "conversion" and rule in c["rules"] and c["key"] not in used:
                    used.add(c["key"])
                    if not feasible(c, types, has_default):
                        selected.append(c); break
    return selected, skipped

def feasible(c, types, has_default):
    try:
        g = Gen(c, types)
        build_sample(g, c, "s", has_default)
    except Unsupported as e:
        return str(e)
    return None

def build_sample(g, c, sid, has_default):
    """Return (rust wrapper code, registration code, script, oracle expr, meta)."""
    kind = c["kind"]
    owner = g.owner
    if kind == "foreign_trait_impl":
        return build_foreign(g, c, sid, has_default)
    recv = c["receiver"]
    if recv != "none":
        if owner in WRAPPED: pass
        elif g.is_opts(owner):
            if g.types[owner]["canonical_path"] not in has_default: raise Unsupported(f"owner {owner} has no Default")
        elif owner in g.types and g.unit_enum(owner): pass
        else: raise Unsupported(f"owner {owner} not wrapped")
    params = []
    for i, prm in enumerate(c["params"]):
        t = parse(prm["ty"])
        rt, lit, conv = g.arg(t, f"a{i}")
        params.append((f"a{i}", rt, lit, conv))
    for n in list(g.opts_used):
        if n in types_needing_default(g, n) and g.types[n]["canonical_path"] not in has_default and g.types[n]["kind"] == "struct":
            raise Unsupported(f"option struct {n} has no Default")
    ret_t = parse(c["ret"]) if c["ret"] else ("tuple", [])
    rt, conv, fmt = g.ret(ret_t, "__r")
    name = c["name"]
    args_rust = ", ".join(p[3] for p in params)
    if recv == "none" and kind in ("inherent", "trait_method") and owner:
        if owner in WRAPPED: g.wrapped_used.add(owner)
        elif g.is_opts(owner) or g.unit_enum(owner):
            if g.is_opts(owner) and g.types[owner]["canonical_path"] not in has_default: raise Unsupported(f"owner {owner} has no Default")
            g.opts_used.add(owner)
        else: raise Unsupported(f"owner {owner} not wrapped")
        call = f"<{g.tpath(owner)}>::{name}({args_rust})"
        sig_recv = ""
    elif recv == "none":
        call = f"{fn_path(c)}({args_rust})"
        sig_recv = ""
    elif recv == "self":
        if owner not in WRAPPED and not g.is_opts(owner): raise Unsupported(f"owner {owner} not wrapped")
        g.wrapped_used.add(owner) if owner in WRAPPED else g.opts_used.add(owner)
        call = f"this.0.clone().{name}({args_rust})"
        sig_recv = f"this: &W_{owner}"
        g.notes.append("receiver consumed in Rust; wrapper clones so the Rune value stays usable")
    elif recv == "&self":
        if owner not in WRAPPED and not g.is_opts(owner): raise Unsupported(f"owner {owner} not wrapped")
        g.wrapped_used.add(owner) if owner in WRAPPED else g.opts_used.add(owner)
        call = f"this.0.{name}({args_rust})"
        sig_recv = f"this: &W_{owner}"
    elif recv == "&mut self":
        if owner not in WRAPPED and not g.is_opts(owner): raise Unsupported(f"owner {owner} not wrapped")
        g.wrapped_used.add(owner) if owner in WRAPPED else g.opts_used.add(owner)
        call = f"this.0.{name}({args_rust})"
        sig_recv = f"this: &mut W_{owner}"
        g.notes.append("&mut self: mutates the Rune value in place")
    else:
        raise Unsupported(f"receiver {recv}")
    if kind == "trait_method":
        trait = c["owner"]
        call = call  # trait must be in scope: the harness imports polars::prelude::*
        g.notes.append(f"trait method of {trait}")
    sig = ", ".join(x for x in [sig_recv] + [f"{p[0]}: {p[1]}" for p in params] if x)
    body = f"let __r = {call}; {conv}"
    if g.callbacks:
        # Rune values are not Send; a Function converts to a SyncFunction when
        # it captures nothing but constants. Refusal is a returned error, a
        # recorded boundary, never a panic.
        pre = " ".join(f"let {n}_sync = match {n}.into_sync() {{ Ok(f) => f, Err(e) => return Err(format!(\"callback capture refused: {{e}}\")) }};" for n in g.callbacks)
        body = f"{pre} let __r = {call}; Ok({conv})"
        rt = f"Result<{rt}, String>"
        fmt = "match r { Ok(r) => `Ok(${" + fmt + "})`, Err(e) => `Err(${e})` }"
        g.notes.append("closure converted with Function::into_sync inside the wrapper; captures other than constants are refused with an error")
    if recv == "none" and kind in ("inherent", "trait_method") and owner:
        rust = f"#[rune::function(free, path = W_{owner}::{name})]\nfn {sid}({sig}) -> {rt} {{ {body} }}"
    else:
        rust = f"fn {sid}({sig}) -> {rt} {{ {body} }}"
    if recv == "none" and kind in ("inherent", "trait_method") and owner:
        reg = f'm.function_meta({sid})?;'
        script_call = f"s::{owner}::{name}({', '.join(p[2] for p in params)})"
        script_setup = ""
        reuse = None
    elif recv == "none":
        reg = f'm.function("{sid}", {sid}).build()?;'
        script_call = f"s::{sid}({', '.join(p[2] for p in params)})"
        script_setup = ""
        reuse = None
    else:
        reg = f'm.function("{name}", {sid}).build_associated::<W_{owner}>()?;'
        script_setup = f"let a = s::fx_{owner}();"
        script_call = f"a.{name}({', '.join(p[2] for p in params)})"
        reuse = f"a.{name}({', '.join(p[2] for p in params)})"
    # oracle
    rust_args = []
    for i, prm in enumerate(c["params"]):
        rust_args.append(oracle_arg(g, parse(prm["ty"])))
    if recv == "none" and kind in ("inherent", "trait_method") and owner:
        ocall = f"<{g.tpath(owner)}>::{name}({', '.join(rust_args)})"
    elif recv == "none":
        ocall = f"{fn_path(c)}({', '.join(rust_args)})"
    elif recv == "&mut self":
        ocall = f"{{ let mut o = {owner_fixture(g, owner)}; let r = o.{name}({', '.join(rust_args)}); r }}"
    else:
        ocall = f"{owner_fixture(g, owner)}.{name}({', '.join(rust_args)})"
    if kind == "trait_method":
        ocall = ocall  # prelude in scope
    oracle = g.oracle_ret(ret_t, ocall)
    if g.callbacks:
        oracle = f"format!(\"Ok({{}})\", {oracle})"
    # &mut self returns are often &mut Self / (); oracle above handles via clone in ret
    meta = {"recv": recv, "notes": g.notes, "wrapped": sorted(g.wrapped_used), "opts": sorted(g.opts_used)}
    return rust, reg, (script_setup, script_call, fmt, reuse), oracle, meta

def types_needing_default(g, n):
    return {n} if g.is_opts(n) else set()

def fixture_name(g, owner):
    return WRAPPED[owner][1] if owner in WRAPPED else None

def owner_fixture(g, owner):
    if owner in WRAPPED: return f"fx::{WRAPPED[owner][1]}()"
    if g.is_opts(owner): return f"{g.tpath(owner)}::default()"
    return f"{g.tpath(owner)}::{[v for v, d in g.types[owner]['variant_shapes'] if not d][0]}"

AMBIGUOUS_PRELUDE = set()

def fn_path(c):
    prelude = [fp for fp in c["found_paths"] if fp.startswith("polars::prelude::") and fp.count("::") == 2]
    if prelude and c["name"] not in AMBIGUOUS_PRELUDE:
        return prelude[0]
    return best_path(c, "fn")

def best_path(item, kind="type"):
    """A spellable Rust path: public re-export paths are always valid, but a
    name reached through `prelude` may be an ambiguous glob (dropped by rustc),
    so prefer paths that avoid `prelude`, then the shortest."""
    public = [fp for fp in item["found_paths"] if " as " not in fp]
    direct = sorted((fp for fp in public if "::prelude::" not in fp), key=lambda fp: (fp.count("::"), fp))
    via_prelude = sorted((fp for fp in public if "::prelude::" in fp), key=lambda fp: (fp.count("::"), fp))
    if direct: return direct[0]
    # Free functions in the prelude are often ambiguous glob names; their
    # canonical path is usually public. Types are the other way round.
    order = [item["canonical_path"]] + via_prelude if kind == "fn" else via_prelude + [item["canonical_path"]]
    return order[0]

def oracle_arg(g, t):
    t = g.resolve_generic(t)
    k = t[0]
    if k == "ref":
        inner = t[1]
        if inner == ("path", "str", []): return '"x"'
        if inner[0] == "slice": return "&" + oracle_arg(g, ("path", "Vec", [inner[1]]))
        if inner[0] == "path" and inner[1] in WRAPPED: return f"&fx::{WRAPPED[inner[1]][1]}()"
        if inner[0] == "path" and inner[1] in ("PlSmallStr",): return '&p::PlSmallStr::from("x")'
        if inner[0] == "path" and inner[1] == "String": return '&"x".to_string()'
        return "&" + oracle_arg(g, inner)
    if k == "refmut":
        inner = t[1]
        if inner[0] == "path" and inner[1] in WRAPPED: return f"&mut fx::{WRAPPED[inner[1]][1]}()"
    if k == "path":
        n, args = t[1], t[2]
        if n == "bool": return "true"
        if n in INTS: return f"2{n if n != 'IdxSize' else 'u32'}"
        if n in FLOATS: return f"1.5{n}"
        if n == "String": return '"x".to_string()'
        if n in ("PlSmallStr", "SmartString"): return 'p::PlSmallStr::from("x")'
        if n in WRAPPED: return f"fx::{WRAPPED[n][1]}()"
        if n == "Option" and args: return f"Some({oracle_arg(g, args[0])})"
        if n in ("Vec", "IntoVec") and args: return f"vec![{oracle_arg(g, args[0])}]"
        if n == "Self": return oracle_arg(g, ("path", g.owner, []))
        if n in g.types and g.is_opts(n): return f"{g.tpath(n)}::default()"
        if n in g.types and g.unit_enum(n): return f"{g.tpath(n)}::{[v for v, d in g.types[n]['variant_shapes'] if not d][0]}"
    if k == "tuple": return "(" + ", ".join(oracle_arg(g, x) for x in t[1]) + ")"
    if k == "slice": return oracle_arg(g, ("path", "Vec", [t[1]]))
    if k == "impl":
        for b in t[1]:
            if b[0] == "fn":
                params, body = closure_identity(g, b)
                return "|" + params + "| { " + body + " }"
            if b[0] == "bound" and b[1] in ("Into", "TryInto") and b[2]: return oracle_arg(g, b[2][0])
            if b[0] == "bound" and b[1] == "AsRef" and b[2] and b[2][0] == ("path", "str", []): return '"x"'
            if b[0] == "bound" and b[1] in ("IntoVec", "IntoIterator") and b[2]: return oracle_arg(g, ("path", "Vec", [b[2][0]]))
    raise Unsupported(f"oracle arg {t}")

def closure_identity(g, b):
    """Rust closure equal to the script closure: returns its last argument of the return type."""
    _, kind, args, ret = b
    layers = []
    r = ret
    while r[0] == "path" and r[1] in ("PolarsResult", "Result", "Option") and r[2]:
        layers.append(r[1]); r = r[2][0]
    if r == ("path", "Self", []) and g.owner: r = ("path", g.owner, [])
    pick = None
    for i, a in enumerate(args):
        aa = a[1] if a[0] == "ref" else a
        if aa == ("path", "Self", []) and g.owner: aa = ("path", g.owner, [])
        if aa == r: pick = i
    inner = f"arg{pick}.clone()"
    for l in reversed(layers):
        inner = f"Some({inner})" if l == "Option" else f"Ok({inner})"
    params = ", ".join(f"arg{i}" for i in range(len(args)))
    return params, inner

OPS = {"Add": ("ADD", "+"), "Sub": ("SUB", "-"), "Mul": ("MUL", "*"), "Div": ("DIV", "/"), "Rem": ("REM", "%")}
def build_foreign(g, c, sid, has_default):
    owner = g.owner
    if owner not in WRAPPED: raise Unsupported(f"owner {owner} not wrapped")
    g.wrapped_used.add(owner)
    tname = c["name"].split("<")[0]
    if tname in OPS:
        proto, op = OPS[tname]
        rhs_t = parse(c["params"][0]["ty"]) if c["params"] else ("path", owner, [])
        rt, lit, conv = g.arg(rhs_t, "rhs")
        ret_t = parse(c["ret"]) if c["ret"] else ("path", owner, [])
        rrt, rconv, fmt = g.ret(ret_t, "__r")
        rust = f"#[rune::function(instance, protocol = {proto})]\nfn {sid}(this: &W_{owner}, rhs: {rt}) -> {rrt} {{ let __r = this.0.clone() {op} {conv}; {rconv} }}"
        reg = f"m.function_meta({sid})?;"
        script = (f"let a = s::fx_{owner}();", f"a {op} {lit}", fmt, f"a {op} {lit}")
        oracle = g.oracle_ret(ret_t, f"fx::{WRAPPED[owner][1]}() {op} {oracle_arg(g, rhs_t)}")
        g.notes.append(f"operator {tname} via Rune protocol {proto}")
        return rust, reg, script, oracle, {"recv": "self", "notes": g.notes, "wrapped": sorted(g.wrapped_used), "opts": sorted(g.opts_used)}
    if tname == "Display":
        rust = (f"#[rune::function(instance, protocol = DISPLAY_FMT)]\nfn {sid}(this: &W_{owner}, f: &mut rune::runtime::Formatter) -> rune::runtime::VmResult<()> "
                f"{{ use rune::alloc::fmt::TryWrite; let s = format!(\"{{}}\", this.0); rune::vm_write!(f, \"{{s}}\") }}")
        reg = f"m.function_meta({sid})?;"
        script = (f"let a = s::fx_{owner}();", "`${a}`", "`${r}`", "`${a}`")
        oracle = f"format!(\"{{}}\", fx::{WRAPPED[owner][1]}())"
        g.notes.append("Display via Rune protocol DISPLAY_FMT")
        return rust, reg, script, oracle, {"recv": "&self", "notes": g.notes, "wrapped": sorted(g.wrapped_used), "opts": []}
    if tname == "PartialEq":
        rust = (f"#[rune::function(instance, protocol = PARTIAL_EQ)]\nfn {sid}(this: &W_{owner}, other: &W_{owner}) -> bool {{ this.0 == other.0 }}")
        reg = f"m.function_meta({sid})?;"
        script = (f"let a = s::fx_{owner}();", f"a == s::fx_{owner}()", "`${r}`", f"a == s::fx_{owner}()")
        oracle = f"format!(\"{{}}\", fx::{WRAPPED[owner][1]}() == fx::{WRAPPED[owner][1]}())"
        g.notes.append("PartialEq via Rune protocol PARTIAL_EQ")
        return rust, reg, script, oracle, {"recv": "&self", "notes": g.notes, "wrapped": sorted(g.wrapped_used), "opts": []}
    raise Unsupported(f"foreign impl {tname} not generated")

# ---- emission ---------------------------------------------------------------
def emit(selected, types, has_default, disabled):
    rust_parts, regs, tests, wrapped, opts, samples = [], [], [], set(), set(), []
    line_map = []  # (sid, start marker text)
    for i, c in enumerate(selected):
        sid = f"s{i:03d}"
        if sid in disabled:
            samples.append({"id": sid, **base_meta(c), "status": "compile_failed", "error": disabled[sid]})
            continue
        g = Gen(c, types)
        try:
            rust, reg, script, oracle, meta = build_sample(g, c, sid, has_default)
        except Unsupported as e:
            samples.append({"id": sid, **base_meta(c), "status": "generator_unsupported", "error": str(e)}); continue
        wrapped |= set(meta["wrapped"]); opts |= set(meta["opts"])
        if g.owner in WRAPPED: wrapped.add(g.owner)
        rust_parts.append(f"// >>> {sid} {c['canonical_path']}\n{rust}\n// <<< {sid}")
        regs.append(f"    // {sid}\n    {reg}")
        setup, call, fmt, reuse = script
        if sid in INJECT:
            oracle = '"INJECTED WRONG ORACLE".to_string()'
        fmt_fn = "|r| " + fmt
        # script returns [formatted result, formatted reuse-of-receiver result or "n/a"]
        body = f"{setup} let r = {call}; let f = {fmt_fn}; let first = f(r);"
        if reuse: body += f" let second = f({reuse});"
        else: body += ' let second = "n/a";'
        if sid in INJECT_REUSE: body += ' let second = "BROKEN REUSE";'
        body += " [first, second]"
        script_src = f"pub fn main() {{ {body} }}"
        tests.append(f'''
#[test]
fn {sid}() {{
    let script = {json.dumps(script_src)};
    let got = samples::run(script);
    let oracle: String = match std::panic::catch_unwind(|| {{ {oracle} }}) {{
        Ok(o) => o,
        Err(e) => {{
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            // Polars panics with "activate '<feature>' feature" when an
            // operation is compiled out: a finding about the configuration,
            // recorded as such, not a harness failure.
            if msg.contains("feature") && msg.contains("activate") {{
                samples::record("{sid}", serde_json::json!({{"status": "feature_gated", "detail": msg, "receiver_reused": false}}));
                return;
            }}
            samples::record("{sid}", serde_json::json!({{"status": "oracle_panicked", "detail": msg, "receiver_reused": false}}));
            panic!("oracle panicked: {{msg}}");
        }}
    }};
    let (status, detail) = match &got {{
        Ok(v) if v[0] == oracle && (v[1] == oracle || v[1] == "n/a") => ("executed_match", if v[1] == "n/a" {{ "no receiver".to_string() }} else {{ "receiver reused".to_string() }}),
        Ok(v) if v[0] == oracle => ("receiver_reuse_failed", format!("second call on the same receiver: {{}}", v[1])),
        Ok(v) => ("executed_mismatch", format!("rune: {{}}\noracle: {{}}", v[0], oracle)),
        Err(e) => ("runtime_error", e.clone()),
    }};
    samples::record("{sid}", serde_json::json!({{"status": status, "detail": detail, "receiver_reused": got.as_ref().map(|v| v[1] == oracle || v[1] == "n/a").unwrap_or(false)}}));
    assert_eq!(status, "executed_match", "{{detail}}");
}}''')
        samples.append({"id": sid, **base_meta(c), "status": "generated", "notes": meta["notes"], "script": script_src, "_oracle": oracle})
    # callback controls: constant capture (allowed), native capture (refused), error propagation
    for smp in samples:
        if smp["status"] == "generated" and smp["bucket"] == "callback":
            sid = smp["id"]
            m = re.search(r"\|(c\d+(?:, c\d+)*)\| \{ (c\d+) \}", smp["script"])
            oracle = smp.pop("_oracle", '"".to_string()')
            if m:
                # a runtime scalar captured by the closure decides the result:
                # the right branch only runs if the captured value survived
                const_script = smp["script"].replace(m.group(0), f"|{m.group(1)}| {{ if n == 1 {{ {m.group(2)} }} else {{ panic(\"captured scalar lost\") }} }}", 1).replace("pub fn main() {", "pub fn main() { let n = s::runtime_one();", 1)
                native_script = smp["script"].replace(m.group(0), f"|{m.group(1)}| {{ let _d = d; {m.group(2)} }}", 1).replace("pub fn main() {", "pub fn main() { let d = s::fx_DataType();", 1)
                tests.append(f'''
#[test]
fn {sid}_const_capture() {{
    let got = samples::run({json.dumps(const_script)});
    let oracle: String = {{ {oracle} }};
    let ok = matches!(&got, Ok(v) if v[0] == oracle);
    samples::record("{sid}_const_capture", serde_json::json!({{"status": if ok {{"scalar_capture_carried"}} else {{"scalar_capture_failed"}}, "detail": format!("{{got:?}}")}}));
    assert!(ok, "{{got:?}}");
}}

#[test]
fn {sid}_native_capture() {{
    let got = samples::run({json.dumps(native_script)});
    let refused = matches!(&got, Ok(v) if v[0].contains("callback capture refused"));
    samples::record("{sid}_native_capture", serde_json::json!({{"status": if refused {{"native_capture_refused"}} else {{"native_capture_not_refused"}}, "detail": format!("{{got:?}}")}}));
    assert!(refused, "{{got:?}}");
}}''')
            err_script = re.sub(r"\|(c\d+(?:, c\d+)*)\| \{ c\d+ \}", r'|\1| { panic("boom from rune") }', smp["script"], count=1)
            tests.append(f'''
#[test]
fn {sid}_error() {{
    let script = {json.dumps(err_script)};
    let got = samples::run(script);
    let text = match &got {{ Ok(v) => v[0].clone(), Err(e) => e.clone() }};
    let propagated = text.contains("boom from rune");
    samples::record("{sid}_error", serde_json::json!({{"status": if propagated {{"error_propagated"}} else {{"error_lost"}}, "detail": text}}));
    assert!(propagated, "{{text}}");
}}''')
    # wrappers and fixtures
    decl = []
    for n in sorted(wrapped):
        rp, fxn, shown = WRAPPED[n]
        decl.append(f"#[derive(rune::Any, Clone)]\n#[rune(item = ::s, name = {n})]\npub struct W_{n}(pub {rp});\nfn fx_{n}() -> W_{n} {{ W_{n}(fx::{fxn}()) }}")
    for n in sorted(opts):
        s = types[n]
        tp = Gen({"generics": [], "where_clause": [], "owner": "", "kind": "free_fn"}, types).tpath(n)
        if s["kind"] == "enum":
            first = [v for v, d in s["variant_shapes"] if not d][0]
            decl.append(f"#[derive(rune::Any, Clone)]\n#[rune(item = ::s, name = {n})]\npub struct W_{n}(pub {tp});\nfn fx_{n}() -> W_{n} {{ W_{n}({tp}::{first}) }}")
        else:
            decl.append(f"#[derive(rune::Any, Clone)]\n#[rune(item = ::s, name = {n})]\npub struct W_{n}(pub {tp});\nfn fx_{n}() -> W_{n} {{ W_{n}({tp}::default()) }}")
            # setters for scalar public fields, proving the option-struct constructor rule
            for fname, fty in s["fields"]:
                t = parse(fty)
                if t == ("path", "bool", []):
                    decl.append(f"fn set_{n}_{fname}(this: &mut W_{n}, v: bool) {{ this.0.{fname} = v; }}")
    dbg_arms = "\n".join(f"    if let Ok(w) = v.borrow_ref::<W_{n}>() {{ return show::{WRAPPED[n][2]}(&w.0); }}" for n in sorted(wrapped))
    dbg_arms += "\n" + "\n".join(f"    if let Ok(w) = v.borrow_ref::<W_{n}>() {{ return format!(\"{{:?}}\", w.0); }}" for n in sorted(opts))
    reg_types = "\n".join(f"    m.ty::<W_{n}>()?;\n    m.function(\"fx_{n}\", fx_{n}).build()?;" for n in sorted(wrapped | opts))
    reg_setters = "\n".join(
        f"    m.function(\"set_{fname}\", set_{n}_{fname}).build_associated::<W_{n}>()?;"
        for n in sorted(opts) if types[n]["kind"] != "enum" for fname, fty in types[n]["fields"] if parse(fty) == ("path", "bool", []))
    generated = f'''//! GENERATED by gen.py: do not edit.
use crate::{{fx, show, p}};
use polars::prelude::*;
use rune::{{ContextError, Module}};

{chr(10).join(decl)}

fn dbg(v: rune::Value) -> String {{
{dbg_arms}
    format!("{{v:?}}")
}}

{chr(10).join(rust_parts)}

pub fn install(m: &mut Module) -> Result<(), ContextError> {{
{reg_types}
{reg_setters}
    m.function("dbg", dbg).build()?;
    m.function("runtime_one", || -> i64 {{ std::hint::black_box(1) }}).build()?;
{chr(10).join(regs)}
    Ok(())
}}
'''
    test_file = "use samples::{fx, show, p};\nuse polars::prelude::*;\n" + "\n".join(tests) + "\n"
    open(os.path.join(HARNESS, "src", "generated.rs"), "w").write(generated)
    open(os.path.join(HARNESS, "tests", "samples.rs"), "w").write(test_file)
    return samples

def base_meta(c):
    return {"key": c["key"], "path": c["canonical_path"], "bucket": c["bucket"], "rules": c["rules"], "shape": c["shape"], "kind": c["kind"], "receiver": c["receiver"], "params": [p["ty"] for p in c["params"]], "ret": c["ret"]}

def build_loop(selected, types, has_default):
    disabled = {}
    had_lock = install_lock()
    locked = ["--locked"] if had_lock else []
    for rnd in range(MAX_ROUNDS):
        samples = emit(selected, types, has_default, disabled)
        r = subprocess.run(["cargo", "build", "--tests", "--message-format=json", "-q", *locked], cwd=HARNESS, capture_output=True, text=True,
                           env={**os.environ, "CARGO_TARGET_DIR": target_dir()})
        errors = []
        for line in r.stdout.splitlines():
            try: m = json.loads(line)
            except Exception: continue
            if m.get("reason") == "compiler-message" and m["message"]["level"] == "error":
                errors.append(m["message"])
        if not errors:
            print(f"build ok after {rnd + 1} round(s)")
            check_lock(had_lock)
            return samples, disabled, None
        gen_src = open(os.path.join(HARNESS, "src", "generated.rs")).read().splitlines()
        test_src = open(os.path.join(HARNESS, "tests", "samples.rs")).read().splitlines()
        def sample_at(lines, ln, marker):
            for i in range(ln - 1, -1, -1):
                if lines[i].startswith("// <<< "):
                    return None  # past the end of an earlier block: shared code
                m = re.match(marker, lines[i])
                if m: return m.group(1)
            return None
        newly = 0
        other = []
        for e in errors:
            hit = None
            for sp in e.get("spans", []):
                if sp["file_name"].endswith("generated.rs"):
                    hit = sample_at(gen_src, sp["line_start"], r"^// >>> (s\d+) ")
                elif sp["file_name"].endswith("samples.rs"):
                    hit = sample_at(test_src, sp["line_start"], r"^fn (s\d+)\(\)")
                    if hit is None:
                        hit = sample_at(test_src, sp["line_start"], r"^fn (s\d+)_error\(\)")
                if hit: break
            if hit:
                if hit not in disabled:
                    disabled[hit] = e["message"] + "\n" + (e.get("rendered") or "")[:600]; newly += 1
            else:
                other.append(e.get("rendered") or e["message"])
        json.dump(disabled, open(os.path.join(HERE, "compile_failures.json"), "w"), indent=1)
        if other:
            return samples, disabled, "\n".join(other)[:4000]
        print(f"round {rnd + 1}: {newly} sample(s) failed to compile, {len(disabled)} disabled")
    return samples, disabled, "too many rounds"

RELEASE = "0.55.2"
INJECT = set()  # sample ids whose oracle is replaced by a wrong value: the self-check
INJECT_REUSE = set()  # sample ids whose script returns a broken second result: the reuse self-check

def pin_release(release):
    p = os.path.join(HARNESS, "Cargo.toml")
    t = open(p).read()
    t = re.sub(r'version = "=0\.\d+\.\d+"', f'version = "={release}"', t)
    t = t.replace('version = "=0.14.2"', 'version = "=0.14.2"')  # rune stays
    # rune's line was rewritten too if it matched; restore it explicitly
    t = re.sub(r'rune = \{ version = "=[^"]+"', 'rune = { version = "=0.14.2"', t)
    open(p, "w").write(t)

def identity(inv_path):
    import hashlib
    def h(p): return hashlib.sha256(open(p, "rb").read()).hexdigest()[:16]
    return {"gen.py": h(os.path.join(HERE, "gen.py")), "harness/src/lib.rs": h(os.path.join(HARNESS, "src", "lib.rs")),
            "inventory": os.path.relpath(inv_path, HERE), "inventory_sha256": h(inv_path)}

def lock_path():
    return os.path.join(HERE, "locks", f"{RELEASE}.lock")

def install_lock():
    """Use the saved lock for this release; a missing lock is created once."""
    os.makedirs(os.path.dirname(lock_path()), exist_ok=True)
    dst = os.path.join(HARNESS, "Cargo.lock")
    if os.path.exists(lock_path()):
        open(dst, "wb").write(open(lock_path(), "rb").read())
        return True
    if os.path.exists(dst): os.remove(dst)
    return False

def check_lock(had_lock):
    dst = os.path.join(HARNESS, "Cargo.lock")
    if had_lock:
        if open(dst, "rb").read() != open(lock_path(), "rb").read():
            print(f"RUN FAILED: Cargo.lock changed during a --locked replay of {RELEASE}"); sys.exit(1)
    else:
        open(lock_path(), "wb").write(open(dst, "rb").read())
        print(f"new configuration: lock saved to {os.path.relpath(lock_path(), HERE)}")

def target_dir():
    return os.path.join(HARNESS, "..", "..", "..", "..", "target", "0072", f"samples-{RELEASE}")

def main():
    global RELEASE
    if "--release" in sys.argv:
        RELEASE = sys.argv[sys.argv.index("--release") + 1]
    pin_release(RELEASE)
    inv_path = sys.argv[1]
    if "--inject-failure" in sys.argv:
        INJECT.add(sys.argv[sys.argv.index("--inject-failure") + 1])
    if "--inject-broken-reuse" in sys.argv:
        INJECT_REUSE.add(sys.argv[sys.argv.index("--inject-broken-reuse") + 1])
    inv, types, has_default = load(inv_path)
    selected, skipped = select(inv, types, has_default, {})
    print(f"selected {len(selected)} samples; skipped {len(skipped)} (no fixture / generator limits)")
    if "--build" not in sys.argv:
        samples = emit(selected, types, has_default, {})
        json.dump({"samples": samples, "skipped": skipped}, open(os.path.join(HERE, "selection.json"), "w"), indent=1)
        return
    samples, disabled, fatal = build_loop(selected, types, has_default)
    if fatal:
        print("FATAL (not a sample error):\n" + fatal)
        json.dump({"release": RELEASE, "fatal": fatal, "samples": samples, "skipped": skipped}, open(os.path.join(HERE, f"results-{RELEASE}.json"), "w"), indent=1)
        sys.exit(1)
    results_dir = os.path.join(HARNESS, "results")
    if os.path.isdir(results_dir):
        for f in os.listdir(results_dir): os.remove(os.path.join(results_dir, f))
    test = subprocess.run(["cargo", "test", "-q", "--locked", "--", "--test-threads=1"], cwd=HARNESS, capture_output=True, text=True,
                   env={**os.environ, "CARGO_TARGET_DIR": target_dir()})
    for smp in samples:
        if smp["status"] != "generated": continue
        p = os.path.join(results_dir, smp["id"] + ".json")
        if os.path.exists(p):
            r = json.load(open(p)); smp["status"] = r["status"]; smp["detail"] = r["detail"]; smp["receiver_reused"] = r.get("receiver_reused")
        else:
            smp["status"] = "no_result (test panicked before recording)"
        for suffix, key in (("_error", "error_propagation"), ("_const_capture", "const_capture"), ("_native_capture", "native_capture")):
            pe = os.path.join(results_dir, smp["id"] + suffix + ".json")
            if os.path.exists(pe):
                r = json.load(open(pe)); smp[key] = r["status"]; smp[key + "_detail"] = r["detail"]
            elif smp["bucket"] == "callback":
                smp[key] = "no_result"
    out = {"release": RELEASE, "identity": identity(inv_path), "cargo_test_exit": test.returncode, "samples": samples, "skipped": skipped}
    json.dump(out, open(os.path.join(HERE, f"results-{RELEASE}.json"), "w"), indent=1)
    if RELEASE == "0.55.2":
        json.dump(out, open(os.path.join(HERE, "results.json"), "w"), indent=1)
    tally = collections.Counter((s["bucket"], s["status"]) for s in samples)
    for k, v in sorted(tally.items()): print(f"{k[0]:14} {k[1]:24} {v}")
    # The run is valid only when every generated sample executed and matched,
    # every callback control had its expected outcome, and nothing failed to
    # compile. Anything else is a failed run, whatever the tallies say.
    bad = [s["id"] for s in samples if s["status"] not in ("executed_match", "feature_gated")]
    bad += [s["id"] + ":" + k for s in samples if s["bucket"] == "callback" and s["status"] == "executed_match"
            for k, want in (("error_propagation", "error_propagated"), ("const_capture", "scalar_capture_carried"), ("native_capture", "native_capture_refused"))
            if s.get(k) != want]
    if test.returncode != 0 or bad:
        print(f"RUN FAILED: cargo test exit {test.returncode}; not as required: {bad}")
        print(test.stdout[-3000:]); print(test.stderr[-2000:])
        sys.exit(1)
    print("RUN OK")

if __name__ == "__main__":
    main()
