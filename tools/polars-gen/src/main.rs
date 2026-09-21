//! Record 0073: generate Rune bindings for the Polars adapter from the
//! record 0072 inventory.
//!
//!   polars-gen <inventory.json> <adapter-dir> [--check] [--buckets mechanical,conversion,option_struct]
//!
//! Writes <adapter-dir>/src/generated/{types.rs,functions.rs,mod.rs,catalogue.rs}
//! and <adapter-dir>/surface.json. With --check, writes to a temporary
//! directory and exits nonzero if anything differs from what is committed.
mod model;
mod ty;

use model::{Callable, Inventory, Param, Supporting};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use ty::{last, Bound, Ty};

const API_CRATES: &[&str] = &["polars_core", "polars_plan", "polars_lazy", "polars_io", "polars_ops", "polars_time", "polars_dtype", "polars_schema", "polars_error"];

/// Hand-written wrappers in the adapter, by canonical type path, with the
/// Rust path of the wrapper and the Rune method names they already bind.
const HAND_WRAPPERS: &[(&str, &str)] = &[
    ("polars_core::frame::dataframe::DataFrame", "crate::DataFrame"),
    ("polars_lazy::frame::LazyFrame", "crate::LazyFrame"),
    ("polars_lazy::frame::LazyGroupBy", "crate::LazyGroupBy"),
    ("polars_plan::dsl::expr::Expr", "crate::Expr"),
];
const HAND_METHODS: &[(&str, &str)] = &[
    ("polars_core::frame::dataframe::DataFrame", "lazy"),
    ("polars_core::frame::dataframe::DataFrame", "preview"),
    ("polars_core::frame::dataframe::DataFrame", "write_parquet_new"),
    ("polars_lazy::frame::LazyFrame", "filter"),
    ("polars_lazy::frame::LazyFrame", "group_by"),
    ("polars_lazy::frame::LazyFrame", "sort"),
    ("polars_lazy::frame::LazyFrame", "collect"),
    ("polars_lazy::frame::LazyGroupBy", "agg"),
    ("polars_plan::dsl::expr::Expr", "gt"),
    ("polars_plan::dsl::expr::Expr", "add"),
    ("polars_plan::dsl::expr::Expr", "sum"),
    ("polars_plan::dsl::expr::Expr", "alias"),
];
/// Rune keywords, from rune 0.14.2 `ast::generated`; a binding whose name
/// is one gets a trailing underscore.
const RUNE_KEYWORDS: &[&str] = &["abstract","alignof","as","async","await","become","break","const","continue","crate","default","do","else","enum","extern","false","final","fn","for","if","impl","in","is","let","loop","macro","match","mod","move","mut","not","offsetof","override","priv","proc","pub","pure","ref","return","select","self","sizeof","static","struct","super","true","typeof","unsafe","use","virtual","while","yield"];

fn rune_name(name: &str) -> String {
    if RUNE_KEYWORDS.contains(&name) { format!("{name}_") } else { name.to_string() }
}

const HAND_FREE: &[&str] = &["col", "lit", "version", "read_csv", "read_parquet"];
const HAND_PROTOCOLS: &[(&str, &str)] = &[("polars_core::frame::dataframe::DataFrame", "Display"), ("polars_plan::dsl::expr::Expr", "Add")];

/// One wrapped Polars type.
#[derive(Clone)]
struct Wrapper {
    /// Rust path of the wrapper struct as seen from `crate::generated::functions`.
    rust: String,
    /// Rust spelling of the wrapped Polars type.
    spell: String,
    /// Rune item (module path) and name.
    rune_item: String,
    rune_name: String,
    hand: bool,
}

struct World {
    /// Counter for per-binding temporaries.
    tmp: std::cell::Cell<usize>,
    types: BTreeMap<String, Supporting>,
    wrappers: BTreeMap<String, Wrapper>,
    ambiguous_prelude: BTreeSet<String>,
    /// Types with a `Clone` impl, derived or hand-written.
    clonable: BTreeSet<String>,
}

/// A spellable Rust path for an inventoried item, in order: a public
/// re-export path from the facade that avoids `prelude`; a public path
/// inside the defining crate (the adapter depends on those crates
/// directly); an unambiguous name directly under the facade prelude.
fn spell(found: &[String], own: &[String], name: &str, ambiguous: &BTreeSet<String>) -> Option<String> {
    let mut direct: Vec<&String> = found.iter().filter(|f| !f.contains(" as ") && !f.contains("::prelude::")).collect();
    direct.sort_by_key(|f| (f.matches("::").count(), f.as_str()));
    if let Some(d) = direct.first() {
        return Some((*d).clone());
    }
    let mut own_direct: Vec<&String> = own.iter().filter(|f| !f.contains(" as ") && !f.contains("::prelude::")).collect();
    own_direct.sort_by_key(|f| (f.matches("::").count(), f.as_str()));
    if let Some(d) = own_direct.first() {
        return Some((*d).clone());
    }
    let mut own_prelude: Vec<&String> = own.iter().filter(|f| !f.contains(" as ") && f.contains("::prelude::") && f.matches("::").count() == 2).collect();
    own_prelude.sort();
    if let Some(o) = own_prelude.first() {
        return Some((*o).clone());
    }
    let mut via: Vec<&String> = found.iter().filter(|f| !f.contains(" as ") && f.starts_with("polars::prelude::") && f.matches("::").count() == 2).collect();
    via.sort_by_key(|f| (f.matches("::").count(), f.as_str()));
    if let Some(v) = via.first() {
        if !ambiguous.contains(name) {
            return Some((*v).clone());
        }
    }
    None
}

/// Whether `ident` occurs in `text` as a whole identifier.
fn mentions(text: &str, ident: &str) -> bool {
    let mut from = 0;
    while let Some(i) = text[from..].find(ident) {
        let start = from + i;
        let end = start + ident.len();
        let before_ok = start == 0 || !text.as_bytes()[start - 1].is_ascii_alphanumeric() && text.as_bytes()[start - 1] != b'_';
        let after_ok = end == text.len() || !text.as_bytes()[end].is_ascii_alphanumeric() && text.as_bytes()[end] != b'_';
        if before_ok && after_ok {
            return true;
        }
        from = end;
    }
    false
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect()
}

impl World {
    fn new(inv: &Inventory) -> World {
        let mut types = BTreeMap::new();
        let mut owners: HashMap<String, BTreeSet<String>> = HashMap::new();
        for s in &inv.supporting {
            types.insert(s.canonical_path.clone(), s.clone());
            for fp in &s.found_paths {
                if fp.starts_with("polars::prelude::") && fp.matches("::").count() == 2 {
                    owners.entry(last(fp).to_string()).or_default().insert(s.key.clone());
                }
            }
        }
        for c in &inv.callables {
            for fp in &c.found_paths {
                if fp.starts_with("polars::prelude::") && fp.matches("::").count() == 2 {
                    owners.entry(last(fp).to_string()).or_default().insert(c.key.clone());
                }
            }
        }
        let ambiguous_prelude: BTreeSet<String> = owners.into_iter().filter(|(_, v)| v.len() > 1).map(|(k, _)| k).collect();
        let mut clonable: BTreeSet<String> = types.values().filter(|s| s.derived.iter().any(|d| d == "Clone")).map(|s| s.canonical_path.clone()).collect();
        for c in &inv.callables {
            if c.kind == "foreign_trait_impl" && c.name.starts_with("Clone") {
                clonable.insert(c.owner.clone());
            }
        }
        let mut w = World { tmp: std::cell::Cell::new(0), types, wrappers: BTreeMap::new(), ambiguous_prelude, clonable };
        w.assign_wrappers();
        w
    }

    /// Every concrete, reachable, unhidden struct/enum/union in the API
    /// crates gets a wrapper; hand-written wrappers are reused.
    fn assign_wrappers(&mut self) {
        let mut by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (path, s) in &self.types {
            if !matches!(s.kind.as_str(), "struct" | "enum" | "union") || s.generic || s.lifetime || s.hidden {
                continue;
            }
            if !API_CRATES.iter().any(|c| path.starts_with(&format!("{c}::"))) {
                continue;
            }
            if spell(&s.found_paths, &s.crate_paths, last(path), &self.ambiguous_prelude).is_none() {
                continue;
            }
            by_name.entry(last(path).to_string()).or_default().push(path.clone());
        }
        for (name, paths) in by_name {
            let collision = paths.len() > 1;
            for path in paths {
                let s = &self.types[&path];
                let spelled = spell(&s.found_paths, &s.crate_paths, &name, &self.ambiguous_prelude).unwrap();
                let hand = HAND_WRAPPERS.iter().find(|(c, _)| *c == path);
                let krate_short = path.split("::").next().unwrap().trim_start_matches("polars_").to_string();
                let rune_item = if collision && hand.is_none() { format!("::polars::{krate_short}") } else { "::polars".to_string() };
                self.wrappers.insert(
                    path.clone(),
                    Wrapper {
                        rust: hand.map(|(_, r)| r.rsplit("::").next().unwrap().to_string()).unwrap_or_else(|| format!("W_{}", sanitize(&path))),
                        spell: spelled,
                        rune_item,
                        rune_name: name.clone(),
                        hand: hand.is_some(),
                    },
                );
            }
        }
    }
}

// ---------------------------------------------------------------- mapping

/// How one argument crosses from Rune into Rust.
struct Arg {
    /// Parameter type of the generated wrapper function.
    rust_ty: String,
    /// Expression converting the parameter `name` to the Polars argument.
    /// `?` inside means the wrapper is fallible.
    conv: String,
    fallible: bool,
    /// What the catalogue says the script passes.
    doc: String,
    /// Statements to run before the call (owned temporaries a borrow needs).
    pre: Vec<String>,
    /// 0 by value, 1 a reference, 2 a slice.
    borrow: u8,
    /// For a borrow: the expression producing the owned value it refers to.
    owned: Option<String>,
    /// What the script passes, for the oracle generator: `bool`, `int`,
    /// `float`, `string`, `W:<canonical>`, `opt(..)`, `vec(..)`, `tuple(..;..)`, `unit`.
    shape: String,
}

struct Ret {
    rust_ty: String,
    /// Expression converting `__r` to the wrapper's return.
    conv: String,
    fallible: bool,
    doc: String,
}

#[derive(Debug)]
struct Unsupported(&'static str, String);

fn ok_arg(rust_ty: &str, conv: String, doc: &str) -> Result<Arg, Unsupported> {
    let shape = match rust_ty {
        "bool" => "bool".to_string(),
        "i64" => "int".to_string(),
        "f64" => "float".to_string(),
        "String" | "&str" => "string".to_string(),
        "()" => "unit".to_string(),
        _ => String::new(),
    };
    Ok(Arg { rust_ty: rust_ty.to_string(), fallible: conv.contains('?'), conv, doc: doc.to_string(), pre: vec![], borrow: 0, owned: None, shape })
}

fn shaped(mut a: Arg, shape: String) -> Arg {
    a.shape = shape;
    a
}

fn borrowed(mut a: Arg, borrow: u8, owned: Option<String>) -> Arg {
    a.borrow = borrow;
    a.owned = owned;
    a
}

const INT_NARROW: &[&str] = &["i8", "i16", "i32", "u8", "u16", "u32", "u64", "usize", "isize", "i128", "u128"];

impl World {
    fn wrapper_for(&self, canonical: &str) -> Option<&Wrapper> {
        self.wrappers.get(canonical)
    }

    fn tmp(&self) -> String {
        let n = self.tmp.get();
        self.tmp.set(n + 1);
        format!("__t{n}")
    }

    /// Argument mapping. `generics` resolves a generic parameter's bounds.
    fn arg(&self, t: &Ty, name: &str, generics: &BTreeMap<String, String>, owner: Option<&str>, depth: u8) -> Result<Arg, Unsupported> {
        if depth > 6 {
            return Err(Unsupported("nesting", t.render()));
        }
        match t {
            Ty::Path { path, args } => match path.as_str() {
                "bool" => ok_arg("bool", name.into(), "bool"),
                "i64" => ok_arg("i64", name.into(), "int"),
                "f64" => ok_arg("f64", name.into(), "float"),
                "f32" => ok_arg("f64", format!("({name} as f32)"), "float"),
                p if INT_NARROW.contains(&p) => ok_arg("i64", format!("support::narrow::<{p}>({name}, \"{name}\")?"), "int"),
                "polars_utils::index::IdxSize" => ok_arg("i64", format!("support::narrow::<p::IdxSize>({name}, \"{name}\")?"), "int"),
                "char" => ok_arg("&str", format!("support::one_char({name}, \"{name}\")?"), "one-character string"),
                "alloc::string::String" => ok_arg("String", name.into(), "string"),
                "polars_utils::pl_str::PlSmallStr" => ok_arg("&str", format!("p::PlSmallStr::from({name})"), "string"),
                "core::option::Option" if args.len() == 1 => {
                    let inner = self.arg(&args[0], "v", generics, owner, depth + 1)?;
                    let (ity, conv) = by_value(&inner, "v");
                    let doc = format!("option of {}", inner.doc);
                    if inner.borrow == 0 {
                        let mut a = ok_arg(&format!("Option<{ity}>"), format!("match {name} {{ Some(v) => Some({conv}), None => None }}"), &doc)?;
                        a.pre = inner.pre.clone();
                        a.shape = format!("opt({})", inner.shape);
                        return Ok(a);
                    }
                    // A borrow inside an Option needs an owned temporary that
                    // outlives the call, so it is hoisted into a statement.
                    let t = self.tmp();
                    let (pre, expr) = if inner.borrow == 2 {
                        let owned = inner.owned.clone().unwrap();
                        (format!("let {t}: Option<{}> = match {name} {{ Some(v) => Some({owned}), None => None }};", inner.rust_ty), format!("{t}.as_deref()"))
                    } else if inner.rust_ty == "&str" {
                        (format!("let {t}: Option<String> = {name};"), format!("{t}.as_deref()"))
                    } else if let Some(w) = inner.rust_ty.strip_prefix('&').filter(|w| self.wrappers.values().any(|x| x.rust == *w)) {
                        (format!("let {t} = match {name} {{ Some(v) => Some(support::take::<{w}>(&v, \"{name}\")?), None => None }};"), format!("{t}.as_ref().map(|w| &w.0)"))
                    } else if let Some(owned) = &inner.owned {
                        (format!("let {t} = match {name} {{ Some(v) => Some({owned}), None => None }};"), format!("{t}.as_ref()"))
                    } else {
                        return Err(Unsupported("option of borrow", t));
                    };
                    let mut a = ok_arg(&format!("Option<{ity}>"), expr, &doc)?;
                    a.fallible = pre.contains('?');
                    a.pre = vec![pre];
                    a.shape = format!("opt({})", inner.shape);
                    Ok(a)
                }
                "alloc::vec::Vec" if args.len() == 1 => {
                    let inner = self.arg(&args[0], "v", generics, owner, depth + 1)?;
                    if inner.borrow != 0 {
                        return Err(Unsupported("vector of borrows", t.render()));
                    }
                    let (ity, conv) = by_value(&inner, "v");
                    Ok(shaped(ok_arg(&format!("Vec<{ity}>"), format!("{name}.into_iter().map(|v| Ok::<_, Error>({conv})).collect::<Result<Vec<_>, Error>>()?"), &format!("vector of {}", inner.doc))?, format!("vec({})", inner.shape)))
                }
                "core::result::Result" => Err(Unsupported("result argument", t.render())),
                "polars_error::PolarsResult" => Err(Unsupported("result argument", t.render())),
                "Self" => match owner {
                    Some(o) => self.arg(&Ty::Path { path: o.to_string(), args: vec![] }, name, generics, owner, depth + 1),
                    None => Err(Unsupported("Self without owner", t.render())),
                },
                _ => {
                    if let Some(w) = self.wrapper_for(path) {
                        if !args.is_empty() {
                            return Err(Unsupported("generic instantiation", t.render()));
                        }
                        if !self.clonable.contains(path) {
                            return Err(Unsupported("by-value argument of a non-Clone type", t.render()));
                        }
                        let doc = w.rune_name.clone();
                        return Ok(shaped(ok_arg(&format!("&{}", w.rust), format!("{name}.0.clone()"), &doc)?, format!("W:{path}")));
                    }
                    if let Some(s) = self.types.get(path) {
                        let why = if s.generic { "generic type" } else if s.lifetime { "lifetime type" } else if s.hidden { "hidden type" } else if s.kind == "trait" { "trait object" } else { "unwrapped type" };
                        return Err(Unsupported(why, t.render()));
                    }
                    if path.starts_with("polars") {
                        return Err(Unsupported("unreachable polars type", t.render()));
                    }
                    Err(Unsupported("foreign type", t.render()))
                }
            },
            Ty::Ref { mutable, inner } => match &**inner {
                Ty::Path { path, args } if path == "str" => Ok(borrowed(ok_arg("&str", name.into(), "string")?, 1, None)),
                Ty::Path { path, .. } if path == "alloc::string::String" => Ok(borrowed(ok_arg("&str", name.into(), "string")?, 1, None)),
                Ty::Path { path, .. } if path == "polars_utils::pl_str::PlSmallStr" => Ok(borrowed(ok_arg("&str", format!("&p::PlSmallStr::from({name})"), "string")?, 1, Some(format!("p::PlSmallStr::from({name})")))),
                Ty::Slice(elem) => {
                    if *mutable {
                        return Err(Unsupported("mutable slice", t.render()));
                    }
                    let inner = self.arg(elem, "v", generics, owner, depth + 1)?;
                    if inner.borrow != 0 {
                        return Err(Unsupported("slice of borrows", t.render()));
                    }
                    let (ity, conv) = by_value(&inner, "v");
                    let owned = format!("{name}.into_iter().map(|v| Ok::<_, Error>({conv})).collect::<Result<Vec<_>, Error>>()?");
                    let tmp = self.tmp();
                    let mut a = ok_arg(&format!("Vec<{ity}>"), format!("&{tmp}[..]"), &format!("vector of {}", inner.doc))?;
                    a.fallible = true;
                    a.pre = vec![format!("let {tmp} = {owned};")];
                    a.shape = format!("vec({})", inner.shape);
                    Ok(borrowed(a, 2, Some(owned)))
                }
                Ty::Path { path, args } if args.is_empty() => {
                    let path = if path == "Self" { owner.ok_or_else(|| Unsupported("Self without owner", t.render()))?.to_string() } else { path.clone() };
                    match self.wrapper_for(&path) {
                        Some(w) => {
                            if *mutable {
                                Ok(shaped(borrowed(ok_arg(&format!("&mut {}", w.rust), format!("&mut {name}.0"), &w.rune_name)?, 1, None), format!("W:{path}")))
                            } else {
                                Ok(shaped(borrowed(ok_arg(&format!("&{}", w.rust), format!("&{name}.0"), &w.rune_name)?, 1, None), format!("W:{path}")))
                            }
                        }
                        None => {
                            let inner = self.arg(inner, name, generics, owner, depth + 1)?;
                            if *mutable {
                                return Err(Unsupported("mutable reference to non-wrapped", t.render()));
                            }
                            let owned = inner.conv.clone();
                            let mut a = ok_arg(&inner.rust_ty, format!("&{}", inner.conv), &inner.doc)?;
                            a.pre = inner.pre.clone();
                            Ok(borrowed(a, 1, Some(owned)))
                        }
                    }
                }
                other => {
                    if *mutable {
                        return Err(Unsupported("mutable reference", t.render()));
                    }
                    let inner = self.arg(other, name, generics, owner, depth + 1)?;
                    if inner.borrow != 0 {
                        return Err(Unsupported("reference to a borrow", t.render()));
                    }
                    let owned = inner.conv.clone();
                    let mut a = ok_arg(&inner.rust_ty, format!("&{}", inner.conv), &inner.doc)?;
                    a.pre = inner.pre.clone();
                    Ok(borrowed(a, 1, Some(owned)))
                }
            },
            Ty::Slice(_) => Err(Unsupported("bare slice", t.render())),
            Ty::Tuple(ts) => {
                if ts.is_empty() {
                    return ok_arg("()", name.into(), "unit");
                }
                let mut tys = Vec::new();
                let mut convs = Vec::new();
                let mut docs = Vec::new();
                let mut pre = Vec::new();
                let mut shapes = Vec::new();
                for (i, e) in ts.iter().enumerate() {
                    let a = self.arg(e, &format!("{name}.{i}"), generics, owner, depth + 1)?;
                    shapes.push(a.shape.clone());
                    if a.borrow != 0 {
                        return Err(Unsupported("tuple with a borrow", t.render()));
                    }
                    let (ity, conv) = by_value(&a, &format!("{name}.{i}"));
                    tys.push(ity);
                    convs.push(conv);
                    docs.push(a.doc);
                    pre.extend(a.pre);
                }
                let mut a = ok_arg(&format!("({})", tys.join(", ")), format!("({})", convs.join(", ")), &format!("tuple of {}", docs.join(", ")))?;
                a.pre = pre;
                a.shape = format!("tuple({})", shapes.join(";"));
                Ok(a)
            }
            Ty::Impl(bounds) => self.bounds(bounds, name, generics, owner, depth, t),
            Ty::Generic(g) => {
                if g == "Self" {
                    return self.arg(&Ty::Path { path: "Self".into(), args: vec![] }, name, generics, owner, depth + 1);
                }
                match generics.get(g) {
                    Some(b) => {
                        let bounds: Vec<Bound> = match ty::parse(&format!("impl {b}")) {
                            Ty::Impl(bs) => bs,
                            _ => vec![],
                        };
                        self.bounds(&bounds, name, generics, owner, depth, t)
                    }
                    None => Err(Unsupported("unbounded generic", t.render())),
                }
            }
            Ty::Other(s) => Err(Unsupported(if s.starts_with("dyn ") { "trait object" } else if s.starts_with("fn(") { "function pointer" } else if s.starts_with("&'static") { "static borrow" } else { "shape" }, s.clone())),
        }
    }

    fn bounds(&self, bounds: &[Bound], name: &str, generics: &BTreeMap<String, String>, owner: Option<&str>, depth: u8, t: &Ty) -> Result<Arg, Unsupported> {
        let neutral = ["core::marker::Send", "core::marker::Sync", "core::marker::Sized", "core::clone::Clone", "core::marker::Copy", "core::fmt::Debug", "core::marker::Unpin", "'static"];
        let mut target: Option<Ty> = None;
        for b in bounds {
            if neutral.contains(&b.path.as_str()) || b.path.starts_with('\'') {
                continue;
            }
            let l = last(&b.path);
            if l.starts_with("Fn") {
                return Err(Unsupported("callback", t.render()));
            }
            if target.is_some() {
                return Err(Unsupported("extra bound", t.render()));
            }
            target = match l {
                "Into" | "AsRef" | "IntoVec" | "Borrow" if b.args.len() == 1 => {
                    let inner = &b.args[0];
                    match inner {
                        Ty::Path { path, .. } if l == "AsRef" && path == "str" => Some(Ty::Ref { mutable: false, inner: Box::new(inner.clone()) }),
                        Ty::Slice(e) if l == "AsRef" => Some(Ty::Path { path: "alloc::vec::Vec".into(), args: vec![(**e).clone()] }),
                        _ if l == "IntoVec" => Some(Ty::Path { path: "alloc::vec::Vec".into(), args: vec![inner.clone()] }),
                        _ if l == "AsRef" => Some(Ty::Ref { mutable: false, inner: Box::new(inner.clone()) }),
                        _ => Some(inner.clone()),
                    }
                }
                "IntoIterator" => match &b.item {
                    Some(item) => Some(Ty::Path { path: "alloc::vec::Vec".into(), args: vec![(**item).clone()] }),
                    None => return Err(Unsupported("iterator without item", t.render())),
                },
                _ => return Err(Unsupported("bound", t.render())),
            };
        }
        match target {
            Some(tt) => {
                let a = self.arg(&tt, name, generics, owner, depth + 1)?;
                // `&str` for AsRef<str> must stay a reference; Into<T> takes T by value
                Ok(a)
            }
            None => Err(Unsupported("unbounded generic", t.render())),
        }
    }

    fn ret(&self, t: &Ty, owner: Option<&str>, depth: u8) -> Result<Ret, Unsupported> {
        if depth > 6 {
            return Err(Unsupported("nesting", t.render()));
        }
        let r = |rust_ty: &str, conv: String, doc: &str| Ok(Ret { rust_ty: rust_ty.to_string(), fallible: false, conv, doc: doc.to_string() });
        match t {
            Ty::Tuple(ts) if ts.is_empty() => r("()", "__r".into(), "unit"),
            Ty::Tuple(ts) => {
                let mut tys = Vec::new();
                let mut convs = Vec::new();
                let mut docs = Vec::new();
                let mut fallible = false;
                for (i, e) in ts.iter().enumerate() {
                    let x = self.ret(e, owner, depth + 1)?;
                    tys.push(x.rust_ty);
                    convs.push(format!("{{ let __r = __t.{i}; {} }}", x.conv));
                    docs.push(x.doc);
                    fallible |= x.fallible;
                }
                Ok(Ret { rust_ty: format!("({})", tys.join(", ")), fallible, conv: format!("{{ let __t = __r; ({}) }}", convs.join(", ")), doc: format!("tuple of {}", docs.join(", ")) })
            }
            Ty::Ref { inner, .. } => {
                if let Ty::Path { path, .. } = &**inner {
                    let p = if path == "Self" { owner.unwrap_or("") } else { path.as_str() };
                    if self.wrappers.contains_key(p) && !self.clonable.contains(p) {
                        return Err(Unsupported("borrowed return of a non-Clone type", t.render()));
                    }
                }
                let x = self.ret(inner, owner, depth + 1)?;
                Ok(Ret { rust_ty: x.rust_ty, fallible: x.fallible, conv: format!("{{ let __r = (__r).clone(); {} }}", x.conv), doc: x.doc })
            }
            Ty::Path { path, args } => match path.as_str() {
                "bool" => r("bool", "__r".into(), "bool"),
                "i64" => r("i64", "__r".into(), "int"),
                "f64" => r("f64", "__r".into(), "float"),
                "f32" => r("f64", "(__r as f64)".into(), "float"),
                p if INT_NARROW.contains(&p) => r("i64", "(__r as i64)".into(), "int"),
                "polars_utils::index::IdxSize" => r("i64", "(__r as i64)".into(), "int"),
                "char" => r("String", "__r.to_string()".into(), "string"),
                "str" | "alloc::string::String" | "polars_utils::pl_str::PlSmallStr" => r("String", "__r.to_string()".into(), "string"),
                "Self" => self.ret(&Ty::Path { path: owner.ok_or_else(|| Unsupported("Self without owner", t.render()))?.to_string(), args: vec![] }, owner, depth + 1),
                "core::option::Option" if args.len() == 1 => {
                    let x = self.ret(&args[0], owner, depth + 1)?;
                    Ok(Ret { rust_ty: format!("Option<{}>", x.rust_ty), fallible: x.fallible, conv: format!("match __r {{ Some(__r) => Some({}), None => None }}", x.conv), doc: format!("option of {}", x.doc) })
                }
                "alloc::vec::Vec" if args.len() == 1 => {
                    let x = self.ret(&args[0], owner, depth + 1)?;
                    Ok(Ret { rust_ty: format!("Vec<{}>", x.rust_ty), fallible: x.fallible, conv: format!("{{ let mut __v = Vec::new(); for __r in __r {{ __v.push({}); }} __v }}", x.conv), doc: format!("vector of {}", x.doc) })
                }
                "polars_error::PolarsResult" if args.len() == 1 => {
                    let x = self.ret(&args[0], owner, depth + 1)?;
                    Ok(Ret { rust_ty: x.rust_ty, fallible: true, conv: format!("{{ let __r = __r.map_err(Error::from)?; {} }}", x.conv), doc: format!("result of {}", x.doc) })
                }
                "core::result::Result" if args.len() == 2 && args[1] == Ty::Path { path: "polars_error::PolarsError".into(), args: vec![] } => {
                    let x = self.ret(&args[0], owner, depth + 1)?;
                    Ok(Ret { rust_ty: x.rust_ty, fallible: true, conv: format!("{{ let __r = __r.map_err(Error::from)?; {} }}", x.conv), doc: format!("result of {}", x.doc) })
                }
                "core::result::Result" => Err(Unsupported("result with foreign error", t.render())),
                _ => {
                    if let Some(w) = self.wrapper_for(path) {
                        if !args.is_empty() {
                            return Err(Unsupported("generic instantiation", t.render()));
                        }
                        return r(&w.rust, format!("{}(__r)", w.rust), &w.rune_name);
                    }
                    if let Some(s) = self.types.get(path) {
                        let why = if s.generic { "generic type" } else if s.lifetime { "lifetime type" } else if s.hidden { "hidden type" } else if s.kind == "trait" { "trait object" } else { "unwrapped type" };
                        return Err(Unsupported(why, t.render()));
                    }
                    if path.starts_with("polars") {
                        return Err(Unsupported("unreachable polars type", t.render()));
                    }
                    Err(Unsupported("foreign type", t.render()))
                }
            },
            Ty::Generic(g) if g == "Self" => self.ret(&Ty::Path { path: "Self".into(), args: vec![] }, owner, depth + 1),
            Ty::Generic(_) => Err(Unsupported("generic return", t.render())),
            Ty::Impl(_) => Err(Unsupported("impl return", t.render())),
            Ty::Slice(_) => Err(Unsupported("bare slice", t.render())),
            Ty::Other(s) => Err(Unsupported("shape", s.clone())),
        }
    }
}

/// By-value form of an argument inside a container: wrapper references
/// become owned `rune::Value`s taken by `support::take`.
fn by_value(a: &Arg, name: &str) -> (String, String) {
    if let Some(w) = a.rust_ty.strip_prefix("&mut ") {
        return ("rune::Value".into(), format!("support::take::<{w}>(&{name}, \"{name}\")?.0"));
    }
    if let Some(w) = a.rust_ty.strip_prefix('&') {
        if w == "str" {
            // owned string; the borrow form (`&str`) is handled by the caller
            return ("String".into(), a.conv.replace(name, &format!("{name}.as_str()")));
        }
        return ("rune::Value".into(), format!("support::take::<{w}>(&{name}, \"{name}\")?.0"));
    }
    (a.rust_ty.clone(), a.conv.clone())
}

// ---------------------------------------------------------------- emission

#[derive(serde::Serialize, Clone)]
struct Entry {
    key: String,
    canonical_path: String,
    kind: String,
    bucket: String,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    fallible: Option<bool>,
    /// Parameter and return types, canonical, so a release diff sees reshapes.
    signature: String,
    /// For generated entries: `case` (an oracle case exists), or why not.
    #[serde(skip_serializing_if = "Option::is_none")]
    execution: Option<String>,
    /// For the oracle generator: how to call the binding and the Polars function.
    #[serde(skip)]
    oracle: Option<OracleInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rune: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

/// Everything the oracle test generator needs about one generated binding.
#[derive(Clone)]
struct OracleInfo {
    /// Rune-side call target: `polars::DataFrame::height` style path and the method name.
    rune_owner: Option<String>,
    rune_name: String,
    receiver: String,
    /// Owner canonical path (methods) and wrapper rust ident.
    owner: Option<(String, String)>,
    /// Rust callee expression prefix, e.g. `<polars::frame::DataFrame>::height` or `polars_ops::prelude::f`.
    callee: String,
    /// (rune shape, canonical type) per parameter.
    params: Vec<(String, String)>,
    ret_canonical: Option<String>,
    /// Wrapper return type as emitted.
    ret_rust: String,
    fallible: bool,
    generics: BTreeMap<String, String>,
}

struct Emitted {
    functions: String,
    registrations: Vec<String>,
    catalogue: Vec<(String, String)>,
    entries: Vec<Entry>,
    /// per (wrapper rust path, rune method name) -> canonical path that took it
    taken: BTreeMap<(String, String), String>,
    fn_index: usize,
}

/// A stable Rust identifier for a binding: a short hash of the canonical
/// path plus its tail, so regenerating after an upstream change only
/// touches the lines that changed.
fn rust_ident(prefix: &str, canonical: &str, _idx: usize) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in canonical.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    let mut s = sanitize(canonical);
    if s.len() > 48 {
        s = s[s.len() - 48..].to_string();
    }
    format!("{prefix}_{:08x}_{s}", (h >> 32) as u32 ^ h as u32).to_lowercase()
}

fn doc_line(c: &Callable) -> String {
    let mut d = c.docs_first.clone().unwrap_or_default().replace('\n', " ");
    if d.len() > 160 {
        d.truncate(157);
        d.push_str("...");
    }
    d
}

impl Emitted {
    fn unsupported(&mut self, c: &Callable, reason: &str, detail: &str) {
        self.entries.push(Entry { key: c.key.clone(), canonical_path: c.canonical_path.clone(), kind: c.kind.clone(), bucket: c.bucket.clone(), status: "unsupported", fallible: None, signature: signature_of(c), execution: None, oracle: None, reason: Some(format!("{reason}: {detail}")), rune: None, note: None });
    }
    fn adapted(&mut self, c: &Callable, reason: &str, rune: &str) {
        self.entries.push(Entry { key: c.key.clone(), canonical_path: c.canonical_path.clone(), kind: c.kind.clone(), bucket: c.bucket.clone(), status: "adapted", fallible: None, signature: signature_of(c), execution: None, oracle: None, reason: Some(reason.into()), rune: Some(rune.into()), note: None });
    }
    fn generated(&mut self, c: &Callable, rune: &str, note: Option<String>) {
        self.entries.push(Entry { key: c.key.clone(), canonical_path: c.canonical_path.clone(), kind: c.kind.clone(), bucket: c.bucket.clone(), status: "generated", fallible: None, signature: signature_of(c), execution: None, oracle: None, reason: None, rune: Some(rune.into()), note });
    }
    fn generated_with(&mut self, c: &Callable, rune: &str, note: Option<String>, info: OracleInfo) {
        let fallible = info.fallible;
        self.entries.push(Entry { key: c.key.clone(), canonical_path: c.canonical_path.clone(), kind: c.kind.clone(), bucket: c.bucket.clone(), status: "generated", fallible: Some(fallible), signature: signature_of(c), execution: None, oracle: Some(info), reason: None, rune: Some(rune.into()), note });
    }
}

fn signature_of(c: &Callable) -> String {
    format!("({}) -> {}", c.params.iter().map(|p| p.ty_canonical.clone()).collect::<Vec<_>>().join(", "), c.ret_canonical.clone().unwrap_or_else(|| "()".into()))
}

fn generics_map(c: &Callable) -> BTreeMap<String, String> {
    c.generics_canonical.iter().cloned().collect()
}

/// A generic parameter that no parameter type mentions cannot be inferred
/// from the arguments a binding passes.
fn unused_generic(c: &Callable) -> Option<String> {
    for (g, _) in &c.generics_canonical {
        if g.starts_with("impl ") {
            continue; // rustdoc's synthetic parameter for an `impl Trait` argument
        }
        let used = c.params.iter().any(|p| mentions(&p.ty_canonical, g));
        if !used {
            return Some(g.clone());
        }
    }
    None
}

/// Types whose operations do eager work on data: any binding whose owner,
/// parameter or return is one of these runs on the engine thread, like the
/// hand-written `collect`, so no Polars work runs on the session's runtime
/// thread. Plan construction (Expr, options, dtypes) does not.
const DATA_TYPES: &[&str] = &[
    "polars_core::frame::dataframe::DataFrame",
    "polars_core::series::Series",
    "polars_core::frame::column::Column",
    "polars_core::frame::column::scalar::ScalarColumn",
    "polars_core::frame::group_by::GroupBy",
    "polars_lazy::frame::LazyFrame",
    "polars_lazy::frame::LazyGroupBy",
    "polars_core::series::implementations::null::NullChunked",
];
/// Explicit additions by name prefix: I/O and query entry points that do
/// not mention a data type in their signature.
const ROUTED_PREFIXES: &[&str] = &["collect", "fetch", "sink", "scan", "read", "write", "execute", "concat", "sort", "rechunk"];

/// Types that are not `Send`, so their bindings cannot cross to the engine
/// thread; they run on the caller and are listed here on purpose.
const NOT_ROUTED_TYPES: &[&str] = &["polars_core::series::amortized_iter::AmortSeries"];

fn routed(name: &str, owner: Option<&str>, params: &[Param], ret: Option<&str>) -> bool {
    let unsendable = |t: &str| NOT_ROUTED_TYPES.iter().any(|d| mentions(t, d));
    if owner.is_some_and(unsendable) || params.iter().any(|p| unsendable(&p.ty_canonical)) || ret.is_some_and(unsendable) {
        return false;
    }
    if ROUTED_PREFIXES.iter().any(|p| name.starts_with(p)) {
        return true;
    }
    let touches = |t: &str| DATA_TYPES.iter().any(|d| mentions(t, d));
    owner.is_some_and(touches) || params.iter().any(|p| touches(&p.ty_canonical)) || ret.is_some_and(touches)
}

fn rune_path(w: &Wrapper) -> String {
    format!("{}::{}", w.rune_item.trim_start_matches("::"), w.rune_name)
}

/// Generate one method/function binding. `owner` is the canonical owner
/// type (for methods) and `trait_spell` the trait for UFCS calls.
fn emit_callable(world: &World, out: &mut Emitted, c: &Callable, buckets: &[&str]) {
    if !buckets.contains(&c.bucket.as_str()) {
        out.unsupported(c, "bucket", c.bucket.clone().as_str());
        return;
    }
    match c.kind.as_str() {
        "inherent" => emit_method(world, out, c, &c.owner, None),
        "trait_method" => {
            let trait_ = &c.owner;
            let tspell = match world.types.get(trait_).and_then(|s| spell(&s.found_paths, &s.crate_paths, last(trait_), &world.ambiguous_prelude)) {
                Some(s) => s,
                None => {
                    out.unsupported(c, "trait has no public path", trait_);
                    return;
                }
            };
            let impls: Vec<&String> = c.implementors.iter().filter(|i| world.wrappers.contains_key(i.as_str())).collect();
            if impls.is_empty() {
                out.unsupported(c, "no wrapped implementor", &c.implementors.join(", "));
                return;
            }
            let mut done = Vec::new();
            let mut first_err: Option<(String, String)> = None;
            let mut first_info: Option<OracleInfo> = None;
            for owner in impls {
                let before = out.entries.len();
                emit_method(world, out, c, owner, Some(&tspell));
                let e = out.entries.pop().unwrap();
                debug_assert_eq!(before, out.entries.len());
                if e.status == "generated" {
                    done.push(e.rune.clone().unwrap());
                    if first_info.is_none() {
                        first_info = e.oracle.clone();
                    }
                } else if first_err.is_none() {
                    first_err = Some((e.status.to_string(), e.reason.unwrap_or_default()));
                }
            }
            if done.is_empty() {
                let (st, why) = first_err.unwrap();
                if st == "adapted" { out.adapted(c, &why, "") } else { out.unsupported(c, "on every implementor", &why) }
            } else if let Some(info) = first_info {
                out.generated_with(c, &done.join(" "), None, info);
            } else {
                out.generated(c, &done.join(" "), None);
            }
        }
        "free_fn" => emit_free(world, out, c),
        "foreign_trait_impl" => emit_foreign(world, out, c),
        _ => out.unsupported(c, "kind", c.kind.clone().as_str()),
    }
}

fn emit_method(world: &World, out: &mut Emitted, c: &Callable, owner: &str, trait_spell: Option<&str>) {
    world.tmp.set(0);
    if c.is_async {
        out.unsupported(c, "async", &c.name);
        return;
    }
    let Some(w) = world.wrapper_for(owner) else {
        let s = world.types.get(owner);
        let why = match s {
            Some(s) if s.generic => "generic owner",
            Some(s) if s.lifetime => "lifetime owner",
            Some(s) if s.hidden => "hidden owner",
            Some(_) => "owner has no public path",
            None => "unknown owner",
        };
        out.unsupported(c, why, owner.to_string().as_str());
        return;
    };
    let rust_name = c.name.clone();
    let name = rune_name(&rust_name);
    if let Some(g) = unused_generic(c) {
        out.unsupported(c, "generic parameter not inferable from arguments", &g);
        return;
    }
    if HAND_METHODS.iter().any(|(o, n)| *o == owner && *n == name) {
        out.adapted(c, "hand-written binding of the same name", &format!("{}::{name}", rune_path(w)));
        return;
    }
    let key = (w.rust.clone(), name.clone());
    if let Some(prev) = out.taken.get(&key) {
        out.unsupported(c, "name taken on this type by", prev.clone().as_str());
        return;
    }
    let generics = generics_map(c);
    let mut params = Vec::new();
    for p in &c.params {
        match world.arg(&ty::parse(&p.ty_canonical), &sanitize(&p.name), &generics, Some(owner), 0) {
            Ok(a) => params.push((sanitize(&p.name), a)),
            Err(Unsupported(why, what)) => {
                out.unsupported(c, why, &format!("{} ({what})", p.name));
                return;
            }
        }
    }
    let ret = match c.ret_canonical.as_deref() {
        None => Ret { rust_ty: "()".into(), fallible: false, conv: "__r".into(), doc: "unit".into() },
        Some(rc) => {
            let t = ty::parse(rc);
            // `&mut Self` chains return unit: the receiver was mutated in place
            if matches!(&t, Ty::Ref { mutable: true, .. }) {
                Ret { rust_ty: "()".into(), fallible: false, conv: "{ let _ = __r; }".into(), doc: "unit (receiver mutated in place)".into() }
            } else if let Ty::Path { path, args } = &t {
                if (path == "polars_error::PolarsResult" || path == "core::result::Result") && matches!(args.first(), Some(Ty::Ref { mutable: true, .. })) {
                    Ret { rust_ty: "()".into(), fallible: true, conv: "{ let _ = __r.map_err(Error::from)?; }".into(), doc: "result of unit (receiver mutated in place)".into() }
                } else {
                    match world.ret(&t, Some(owner), 0) {
                        Ok(r) => r,
                        Err(Unsupported(why, what)) => {
                            out.unsupported(c, why, &format!("return ({what})"));
                            return;
                        }
                    }
                }
            } else {
                match world.ret(&t, Some(owner), 0) {
                    Ok(r) => r,
                    Err(Unsupported(why, what)) => {
                        out.unsupported(c, why, &format!("return ({what})"));
                        return;
                    }
                }
            }
        }
    };
    let fallible = ret.fallible || params.iter().any(|(_, a)| a.fallible);
    let (recv_sig, recv_expr, recv_note) = match c.receiver.as_str() {
        "none" => ("".to_string(), None, None),
        "self" if !world.clonable.contains(owner) => {
            out.unsupported(c, "receiver consumes a non-Clone type", owner);
            return;
        }
        "self" => (format!("this: &{}", w.rust), Some("this.0.clone()".to_string()), Some("consumes in Rust; the Rune value is cloned and stays usable")),
        "&self" => (format!("this: &{}", w.rust), Some("&this.0".to_string()), None),
        "&mut self" => (format!("this: &mut {}", w.rust), Some("&mut this.0".to_string()), Some("mutates the Rune value in place")),
        other => {
            out.unsupported(c, "receiver", other);
            return;
        }
    };
    let idx = out.fn_index;
    out.fn_index += 1;
    // trait methods are emitted once per implementor: the owner is part of the identity
    let ident = rust_ident("f", &format!("{}#{owner}", c.canonical_path), idx);
    let callee = match trait_spell {
        Some(t) => format!("<{} as {t}>::{rust_name}", w.spell),
        None => format!("<{}>::{rust_name}", w.spell),
    };
    let mut args: Vec<String> = Vec::new();
    if let Some(r) = &recv_expr {
        args.push(r.clone());
    }
    args.extend(params.iter().map(|(_, a)| a.conv.clone()));
    let sig: Vec<String> = std::iter::once(recv_sig).filter(|s| !s.is_empty()).chain(params.iter().map(|(n, a)| format!("{n}: {}", a.rust_ty))).collect();
    let route = routed(&rust_name, Some(owner), &c.params, c.ret_canonical.as_deref());
    let fallible = fallible || params.iter().any(|(_, a)| a.pre.iter().any(|p| p.contains('?')));
    let pre: String = params.iter().flat_map(|(_, a)| a.pre.iter()).map(|p| format!("{p} ")).collect();
    let ret_ty = if fallible { format!("Result<{}, Error>", ret.rust_ty) } else { ret.rust_ty.clone() };
    let body_conv = if fallible { format!("Ok({})", ret.conv) } else { ret.conv.clone() };
    let attr = if c.receiver == "none" { format!("#[rune::function(free, path = {}::{name})]", w.rust) } else { format!("#[rune::function(instance, path = {name})]") };
    let doc = doc_line(c);
    let rune = format!("{}::{name}", rune_path(w));
    let arg_docs: Vec<String> = params.iter().map(|(n, a)| format!("{n}: {}", a.doc)).collect();
    let summary = format!("{name}({}) -> {}{}", arg_docs.join(", "), ret.doc, if fallible { " (fallible)" } else { "" });
    let (pre, call) = if route {
        // conversions may fail with `?`, so they run before the closure
        let mut pre = pre.clone();
        let mut hoisted = Vec::new();
        for (i, a) in args.iter().enumerate() {
            pre.push_str(&format!("let __arg{i} = {a}; "));
            hoisted.push(format!("__arg{i}"));
        }
        let join = if fallible { ".map_err(Error::engine)?" } else { ".unwrap_or_else(|e| panic!(\"polars engine thread: {e}\"))" };
        (pre, format!("crate::engine::run(move || {callee}({})){join}", hoisted.join(", ")))
    } else {
        (pre, format!("{callee}({})", args.join(", ")))
    };
    writeln!(out.functions, "/// {doc}\n/// Polars: `{}`. {}\n{attr}\nfn {ident}({}) -> {ret_ty} {{ {pre}let __r = {call}; {body_conv} }}", c.canonical_path, summary, sig.join(", ")).unwrap();
    out.registrations.push(format!("m.function_meta({ident})?;"));
    out.catalogue.push((rune.clone(), if doc.is_empty() { summary.clone() } else { format!("{summary}: {doc}") }));
    out.taken.insert(key, c.canonical_path.clone());
    let mut note = recv_note.map(|s| s.to_string());
    if route {
        note = Some(format!("{}{}", note.map(|n| n + "; ").unwrap_or_default(), "routed through the engine thread"));
    }
    if name != rust_name {
        note = Some(format!("{}renamed: `{rust_name}` is a Rune keyword", note.map(|n| n + "; ").unwrap_or_default()));
    }
    if fallible && !ret.fallible {
        note = Some(format!("{}{}", note.map(|n| n + "; ").unwrap_or_default(), "fallible in Rune because an argument conversion can fail"));
    }
    let info = OracleInfo {
        rune_owner: Some(rune_path(w)),
        rune_name: name.clone(),
        receiver: c.receiver.clone(),
        owner: Some((owner.to_string(), w.rust.clone())),
        callee,
        params: c.params.iter().zip(params.iter()).map(|(p, (_, a))| (a.shape.clone(), p.ty_canonical.clone())).collect(),
        ret_canonical: c.ret_canonical.clone(),
        ret_rust: ret.rust_ty.clone(),
        fallible,
        generics: generics.clone(),
    };
    out.generated_with(c, &rune, note, info);
}

fn emit_free(world: &World, out: &mut Emitted, c: &Callable) {
    world.tmp.set(0);
    let rust_name = c.name.clone();
    let name = rune_name(&rust_name);
    if c.is_async {
        out.unsupported(c, "async", &name);
        return;
    }
    if HAND_FREE.contains(&name.as_str()) {
        out.adapted(c, "hand-written binding of the same name", &format!("polars::{name}"));
        return;
    }
    if let Some(g) = unused_generic(c) {
        out.unsupported(c, "generic parameter not inferable from arguments", &g);
        return;
    }
    let Some(spelled) = spell(&c.found_paths, &c.crate_paths, &name, &world.ambiguous_prelude) else {
        out.unsupported(c, "no public path", c.canonical_path.clone().as_str());
        return;
    };
    let key = ("polars".to_string(), name.clone());
    if let Some(prev) = out.taken.get(&key) {
        out.unsupported(c, "name taken in polars:: by", prev.clone().as_str());
        return;
    }
    let generics = generics_map(c);
    let mut params = Vec::new();
    for p in &c.params {
        match world.arg(&ty::parse(&p.ty_canonical), &sanitize(&p.name), &generics, None, 0) {
            Ok(a) => params.push((sanitize(&p.name), a)),
            Err(Unsupported(why, what)) => {
                out.unsupported(c, why, &format!("{} ({what})", p.name));
                return;
            }
        }
    }
    let ret = match c.ret_canonical.as_deref() {
        None => Ret { rust_ty: "()".into(), fallible: false, conv: "__r".into(), doc: "unit".into() },
        Some(rc) => match world.ret(&ty::parse(rc), None, 0) {
            Ok(r) => r,
            Err(Unsupported(why, what)) => {
                out.unsupported(c, why, &format!("return ({what})"));
                return;
            }
        },
    };
    let route = routed(&rust_name, None, &c.params, c.ret_canonical.as_deref());
    let fallible = ret.fallible || params.iter().any(|(_, a)| a.fallible || a.pre.iter().any(|p| p.contains('?')));
    let pre: String = params.iter().flat_map(|(_, a)| a.pre.iter()).map(|p| format!("{p} ")).collect();
    let idx = out.fn_index;
    out.fn_index += 1;
    let ident = rust_ident("g", &c.canonical_path, idx);
    let sig: Vec<String> = params.iter().map(|(n, a)| format!("{n}: {}", a.rust_ty)).collect();
    let args: Vec<String> = params.iter().map(|(_, a)| a.conv.clone()).collect();
    let ret_ty = if fallible { format!("Result<{}, Error>", ret.rust_ty) } else { ret.rust_ty.clone() };
    let body_conv = if fallible { format!("Ok({})", ret.conv) } else { ret.conv.clone() };
    let doc = doc_line(c);
    let arg_docs: Vec<String> = params.iter().map(|(n, a)| format!("{n}: {}", a.doc)).collect();
    let summary = format!("{name}({}) -> {}{}", arg_docs.join(", "), ret.doc, if fallible { " (fallible)" } else { "" });
    let (pre, call) = if route {
        let mut pre = pre.clone();
        let mut hoisted = Vec::new();
        for (i, a) in args.iter().enumerate() {
            pre.push_str(&format!("let __arg{i} = {a}; "));
            hoisted.push(format!("__arg{i}"));
        }
        let join = if fallible { ".map_err(Error::engine)?" } else { ".unwrap_or_else(|e| panic!(\"polars engine thread: {e}\"))" };
        (pre, format!("crate::engine::run(move || {spelled}({})){join}", hoisted.join(", ")))
    } else {
        (pre, format!("{spelled}({})", args.join(", ")))
    };
    writeln!(out.functions, "/// {doc}\n/// Polars: `{}`. {}\n#[rune::function(path = {name})]\nfn {ident}({}) -> {ret_ty} {{ {pre}let __r = {call}; {body_conv} }}", c.canonical_path, summary, sig.join(", ")).unwrap();
    out.registrations.push(format!("m.function_meta({ident})?;"));
    let rune = format!("polars::{name}");
    out.catalogue.push((rune.clone(), if doc.is_empty() { summary.clone() } else { format!("{summary}: {doc}") }));
    out.taken.insert(key, c.canonical_path.clone());
    let mut notes = Vec::new();
    if route { notes.push("routed through the engine thread".to_string()); }
    if name != rust_name { notes.push(format!("renamed: `{rust_name}` is a Rune keyword")); }
    let info = OracleInfo {
        rune_owner: None,
        rune_name: name.clone(),
        receiver: "none".into(),
        owner: None,
        callee: spelled.clone(),
        params: c.params.iter().zip(params.iter()).map(|(p, (_, a))| (a.shape.clone(), p.ty_canonical.clone())).collect(),
        ret_canonical: c.ret_canonical.clone(),
        ret_rust: ret.rust_ty.clone(),
        fallible,
        generics: generics.clone(),
    };
    out.generated_with(c, &rune, if notes.is_empty() { None } else { Some(notes.join("; ")) }, info);
}

const OPS: &[(&str, &str, &str)] = &[("Add", "ADD", "+"), ("Sub", "SUB", "-"), ("Mul", "MUL", "*"), ("Div", "DIV", "/"), ("Rem", "REM", "%"), ("BitAnd", "BIT_AND", "&"), ("BitOr", "BIT_OR", "|"), ("BitXor", "BIT_XOR", "^")];

fn emit_foreign(world: &World, out: &mut Emitted, c: &Callable) {
    let owner = &c.owner;
    let Some(w) = world.wrapper_for(owner) else {
        out.unsupported(c, "owner not wrapped", owner.clone().as_str());
        return;
    };
    let tname = c.name.split('<').next().unwrap_or("").to_string();
    if matches!(tname.as_str(), "Eq" | "StructuralPartialEq" | "Copy") {
        out.adapted(c, "implied by the PartialEq or Clone protocol", &format!("{} {tname}", rune_path(w)));
        return;
    }
    if HAND_PROTOCOLS.iter().any(|(o, t)| *o == owner && *t == tname) {
        out.adapted(c, "hand-written protocol", &format!("{} {tname}", rune_path(w)));
        return;
    }
    let key = (w.rust.clone(), format!("<{tname}>"));
    if let Some(prev) = out.taken.get(&key) {
        out.unsupported(c, "protocol taken on this type by", prev.clone().as_str());
        return;
    }
    let idx = out.fn_index;
    let ident = rust_ident("p", &c.canonical_path, idx);
    let (code, note) = match tname.as_str() {
        "Display" => (format!("#[rune::function(instance, protocol = DISPLAY_FMT)]\nfn {ident}(this: &{0}, f: &mut rune::runtime::Formatter) -> rune::runtime::VmResult<()> {{ use rune::alloc::fmt::TryWrite; let s = format!(\"{{}}\", this.0); rune::vm_write!(f, \"{{s}}\") }}", w.rust), "DISPLAY_FMT"),
        "Debug" => (format!("#[rune::function(instance, protocol = DEBUG_FMT)]\nfn {ident}(this: &{0}, f: &mut rune::runtime::Formatter) -> rune::runtime::VmResult<()> {{ use rune::alloc::fmt::TryWrite; let s = format!(\"{{:?}}\", this.0); rune::vm_write!(f, \"{{s}}\") }}", w.rust), "DEBUG_FMT"),
        "PartialEq" if c.params.len() == 1 && c.params[0].ty_canonical == format!("&{owner}") => (format!("#[rune::function(instance, protocol = PARTIAL_EQ)]\nfn {ident}(this: &{0}, other: &{0}) -> bool {{ this.0 == other.0 }}", w.rust), "PARTIAL_EQ"),
        "Clone" => (format!("#[rune::function(instance, protocol = CLONE)]\nfn {ident}(this: &{0}) -> {0} {{ {0}(this.0.clone()) }}", w.rust), "CLONE"),
        "Default" => (format!("#[rune::function(free, path = {0}::default_)]\nfn {ident}() -> {0} {{ {0}(<{1}>::default()) }}", w.rust, w.spell), "default_() (renamed: `default` is a Rune keyword)"),
        t if OPS.iter().any(|(n, _, _)| *n == t) => {
            let (_, proto, op) = OPS.iter().find(|(n, _, _)| *n == t).unwrap();
            let Some(rhs) = c.params.first() else {
                out.unsupported(c, "operator without rhs", tname.as_str());
                return;
            };
            let a = match world.arg(&ty::parse(&rhs.ty_canonical), "rhs", &BTreeMap::new(), Some(owner), 0).and_then(|a| if a.pre.is_empty() { Ok(a) } else { Err(Unsupported("operand needs a temporary", String::new())) }) {
                Ok(a) => a,
                Err(Unsupported(why, what)) => {
                    out.unsupported(c, why, &format!("rhs ({what})"));
                    return;
                }
            };
            let ret = match c.ret_canonical.as_deref().map(|r| world.ret(&ty::parse(r), Some(owner), 0)) {
                Some(Ok(r)) => r,
                Some(Err(Unsupported(why, what))) => {
                    out.unsupported(c, why, &format!("return ({what})"));
                    return;
                }
                None => {
                    out.unsupported(c, "operator without output", tname.as_str());
                    return;
                }
            };
            let fallible = ret.fallible || a.fallible;
            let ret_ty = if fallible { format!("Result<{}, Error>", ret.rust_ty) } else { ret.rust_ty.clone() };
            let body = if fallible { format!("Ok({})", ret.conv) } else { ret.conv.clone() };
            (format!("#[rune::function(instance, protocol = {proto})]\nfn {ident}(this: &{0}, rhs: {1}) -> {ret_ty} {{ let __r = this.0.clone() {op} {2}; {body} }}", w.rust, a.rust_ty, a.conv), *proto)
        }
        _ if !world.clonable.contains(owner) && OPS.iter().any(|(n, _, _)| *n == tname) => {
            out.unsupported(c, "operator on a non-Clone type", tname.as_str());
            return;
        }
        "Neg" => (format!("#[rune::function(instance, protocol = NEG)]\nfn {ident}(this: &{0}) -> {0} {{ {0}(-this.0.clone()) }}", w.rust), "NEG"),
        _ => {
            out.unsupported(c, "trait impl not mapped to a Rune protocol", tname.as_str());
            return;
        }
    };
    out.fn_index += 1;
    writeln!(out.functions, "/// Polars: `{}`.\n{code}", c.canonical_path).unwrap();
    out.registrations.push(format!("m.function_meta({ident})?;"));
    out.taken.insert(key, c.canonical_path.clone());
    let rune = format!("{} {note}", rune_path(w));
    let info = OracleInfo {
        rune_owner: Some(rune_path(w)),
        rune_name: format!("<{tname}>"),
        receiver: "protocol".into(),
        owner: Some((owner.to_string(), w.rust.clone())),
        callee: tname.clone(),
        params: c.params.iter().map(|p| (String::new(), p.ty_canonical.clone())).collect(),
        ret_canonical: c.ret_canonical.clone(),
        ret_rust: String::new(),
        fallible: false,
        generics: BTreeMap::new(),
    };
    out.generated_with(c, &rune, Some(format!("derived: {}", c.derived)), info);
}

/// Wrapper type declarations, plus constructors/setters/getters for option
/// structs and variant constants/constructors for enums.
fn emit_types(world: &World, out: &mut Emitted) -> String {
    let mut s = String::new();
    s.push_str("//! GENERATED by tools/polars-gen from the record 0072 inventory: do not edit.\n//! Wrapper types for every concrete, reachable Polars type in the API crates.\n#![allow(non_camel_case_types, unused_imports, dead_code, clippy::all)]\nuse polars::prelude as p;\nuse rnx::rune;\n\n");
    for (canonical, w) in &world.wrappers {
        if w.hand {
            continue;
        }
        let ident = w.rust.rsplit("::").next().unwrap();
        let derive = if world.clonable.contains(canonical) { "#[derive(rune::Any, Clone)]" } else { "#[derive(rune::Any)]" };
        writeln!(s, "/// `{canonical}`\n{derive}\n#[rune(item = {}, name = {})]\npub struct {ident}(pub(crate) {});", w.rune_item, w.rune_name, w.spell).unwrap();
    }
    s.push_str("\npub fn install(m: &mut rune::Module) -> Result<(), rune::ContextError> {\n");
    for w in world.wrappers.values() {
        if !w.hand {
            let ident = w.rust.rsplit("::").next().unwrap();
            writeln!(s, "    m.ty::<{ident}>()?;").unwrap();
        }
    }
    s.push_str("    Ok(())\n}\n");
    let _ = out;
    s
}

fn emit_struct_extras(world: &World, out: &mut Emitted, buckets: &[&str]) {
    if !buckets.contains(&"option_struct") {
        return;
    }
    for (canonical, w) in &world.wrappers {
        let s = &world.types[canonical];
        if s.kind == "struct" && s.public_fields > 0 && world.clonable.contains(canonical) {
            for (fname, fty) in &s.fields_canonical {
                let t = ty::parse(fty);
                let fid = rune_name(&sanitize(fname));
                // getter, unless a method already has the field's name
                if out.taken.contains_key(&(w.rust.clone(), fid.clone())) {
                    // the method wins; the field stays readable through it
                } else if let Ok(r) = world.ret(&t, Some(canonical), 0) {
                    out.taken.insert((w.rust.clone(), fid.clone()), format!("{canonical}::{fname} (field getter)"));
                    let idx = out.fn_index;
                    out.fn_index += 1;
                    let ident = rust_ident("s", &format!("{canonical}::{fname}"), idx);
                    let ret_ty = if r.fallible { format!("Result<{}, Error>", r.rust_ty) } else { r.rust_ty.clone() };
                    let body = if r.fallible { format!("Ok({})", r.conv) } else { r.conv.clone() };
                    writeln!(out.functions, "/// Field `{fname}` of `{canonical}`.\n#[rune::function(instance, path = {fid})]\nfn {ident}(this: &{}) -> {ret_ty} {{ let __r = this.0.{fname}.clone(); {body} }}", w.rust).unwrap();
                    out.registrations.push(format!("m.function_meta({ident})?;"));
                    out.catalogue.push((format!("{}::{fid}", rune_path(w)), format!("{fid}() -> {}: field", r.doc)));
                }
                // setter, chainable, unless a method already has the name
                if out.taken.contains_key(&(w.rust.clone(), format!("with_{fid}"))) {
                    // the method wins
                } else if let Ok(a) = world.arg(&t, "v", &BTreeMap::new(), Some(canonical), 0).and_then(|a| if a.pre.is_empty() && a.borrow == 0 { Ok(a) } else { Err(Unsupported("setter of a borrow", String::new())) }) {
                    out.taken.insert((w.rust.clone(), format!("with_{fid}")), format!("{canonical}::{fname} (field setter)"));
                    let idx = out.fn_index;
                    out.fn_index += 1;
                    let ident = rust_ident("w", &format!("{canonical}::{fname}"), idx);
                    let (ret_ty, body) = if a.fallible { (format!("Result<{}, Error>", w.rust), format!("Ok({}(__o))", w.rust)) } else { (w.rust.clone(), format!("{}(__o)", w.rust)) };
                    writeln!(out.functions, "/// Set field `{fname}` of `{canonical}`, returning the updated value.\n#[rune::function(instance, path = with_{fid})]\nfn {ident}(this: &{}, v: {}) -> {ret_ty} {{ let mut __o = this.0.clone(); __o.{fname} = {}; {body} }}", w.rust, a.rust_ty, a.conv).unwrap();
                    out.registrations.push(format!("m.function_meta({ident})?;"));
                    out.catalogue.push((format!("{}::with_{fid}", rune_path(w)), format!("with_{fid}(v: {}) -> {}: field setter", a.doc, w.rune_name)));
                }
            }
        }
        if s.kind == "enum" {
            let payloads: BTreeMap<&str, &Vec<String>> = s.variant_payloads.iter().map(|(n, p)| (n.as_str(), p)).collect();
            for (vname, data) in &s.variant_shapes {
                let vid = rune_name(&sanitize(vname));
                if out.taken.contains_key(&(w.rust.clone(), vid.clone())) {
                    continue; // an associated function already has the variant's name
                }
                out.taken.insert((w.rust.clone(), vid.clone()), format!("{canonical}::{vname} (variant)"));
                if !*data {
                    let idx = out.fn_index;
                    out.fn_index += 1;
                    let ident = rust_ident("v", &format!("{canonical}::{vname}"), idx);
                    writeln!(out.functions, "/// Variant `{vname}` of `{canonical}`.\n#[rune::function(free, path = {0}::{vid})]\nfn {ident}() -> {0} {{ {0}(<{1}>::{vname}) }}", w.rust, w.spell).unwrap();
                    out.registrations.push(format!("m.function_meta({ident})?;"));
                    out.catalogue.push((format!("{}::{vid}", rune_path(w)), format!("{vid}() -> {}: variant", w.rune_name)));
                } else if let Some(p) = payloads.get(vname.as_str()) {
                    if p.iter().any(|x| x.contains(": ")) {
                        continue; // struct variants: next stage
                    }
                    let mut args = Vec::new();
                    let mut ok = true;
                    for (i, pt) in p.iter().enumerate() {
                        match world.arg(&ty::parse(pt), &format!("a{i}"), &BTreeMap::new(), Some(canonical), 0) {
                            Ok(a) if a.pre.is_empty() && a.borrow == 0 => args.push((format!("a{i}"), a)),
                            _ => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if !ok {
                        continue;
                    }
                    let idx = out.fn_index;
                    out.fn_index += 1;
                    let ident = rust_ident("v", &format!("{canonical}::{vname}"), idx);
                    let fallible = args.iter().any(|(_, a)| a.fallible);
                    let sig: Vec<String> = args.iter().map(|(n, a)| format!("{n}: {}", a.rust_ty)).collect();
                    let convs: Vec<String> = args.iter().map(|(_, a)| a.conv.clone()).collect();
                    let (ret_ty, body) = if fallible { (format!("Result<{}, Error>", w.rust), format!("Ok({}(<{}>::{vname}({})))", w.rust, w.spell, convs.join(", "))) } else { (w.rust.clone(), format!("{}(<{}>::{vname}({}))", w.rust, w.spell, convs.join(", "))) };
                    writeln!(out.functions, "/// Variant `{vname}` of `{canonical}`.\n#[rune::function(free, path = {0}::{vid})]\nfn {ident}({1}) -> {ret_ty} {{ {body} }}", w.rust, sig.join(", ")).unwrap();
                    out.registrations.push(format!("m.function_meta({ident})?;"));
                    out.catalogue.push((format!("{}::{vid}", rune_path(w)), format!("{vid}({}) -> {}: variant constructor", args.iter().map(|(n, a)| format!("{n}: {}", a.doc)).collect::<Vec<_>>().join(", "), w.rune_name)));
                }
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: polars-gen <inventory.json> <adapter-dir> [--check] [--buckets a,b,c]");
        std::process::exit(2);
    }
    let check = args.iter().any(|a| a == "--check");
    let buckets: Vec<String> = args.iter().position(|a| a == "--buckets").map(|i| args[i + 1].split(',').map(|s| s.to_string()).collect()).unwrap_or_else(|| vec!["mechanical".into(), "conversion".into(), "option_struct".into()]);
    let buckets: Vec<&str> = buckets.iter().map(|s| s.as_str()).collect();
    let inv: Inventory = serde_json::from_str(&std::fs::read_to_string(&args[1]).expect("inventory")).expect("inventory json");
    let world = World::new(&inv);
    let mut out = Emitted { functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    let mut callables: Vec<&Callable> = inv.callables.iter().collect();
    callables.sort_by(|a, b| a.canonical_path.cmp(&b.canonical_path).then(a.key.cmp(&b.key)));
    // inherent methods take names before trait methods do
    callables.sort_by_key(|c| match c.kind.as_str() { "inherent" => 0, "free_fn" => 1, "foreign_trait_impl" => 2, _ => 3 });
    for c in callables {
        let api = API_CRATES.contains(&c.krate.as_str());
        if c.bucket == "unsupported" || c.bucket == "unknown" {
            continue; // not eligible in 0072's terms
        }
        if !api {
            out.entries.push(Entry { key: c.key.clone(), canonical_path: c.canonical_path.clone(), kind: c.kind.clone(), bucket: c.bucket.clone(), status: "out_of_scope", fallible: None, signature: signature_of(c), execution: None, oracle: None, reason: Some("internal crate reachable through the prelude".into()), rune: None, note: None });
            continue;
        }
        emit_callable(&world, &mut out, c, &buckets);
    }
    emit_struct_extras(&world, &mut out, &buckets);
    // Only wrapper types some generated binding mentions are emitted; the
    // rest would be registered for nothing. Hand-written wrappers always exist.
    let mut used: BTreeSet<String> = BTreeSet::new();
    for (canonical, w) in &world.wrappers {
        if w.hand || mentions(&out.functions, &w.rust) {
            used.insert(canonical.clone());
        }
    }
    let mut world = world;
    world.wrappers.retain(|c, _| used.contains(c));
    let types = emit_types(&world, &mut out);
    let (fixtures, oracle_tests, oracle_skipped) = emit_oracle(&world, &mut out.entries, &inv);
    let mut functions = String::from("//! GENERATED by tools/polars-gen from the record 0072 inventory: do not edit.\n//! One binding per accounted callable; see ../../surface.json.\n#![allow(non_snake_case, unused_variables, unused_imports, unused_parens, clippy::all)]\nuse super::support::{self, Error};\nuse super::types::*;\nuse crate::{DataFrame, Expr, LazyFrame, LazyGroupBy};\nuse polars::prelude as p;\nuse polars::prelude::*;\nuse rnx::rune;\n\n");
    functions.push_str(&out.functions);
    functions.push_str("\npub fn install(m: &mut rune::Module) -> Result<(), rune::ContextError> {\n");
    for r in &out.registrations {
        functions.push_str("    ");
        functions.push_str(r);
        functions.push('\n');
    }
    functions.push_str("    Ok(())\n}\n");
    let mut catalogue = String::from("//! GENERATED by tools/polars-gen: catalogue entries for the session's help.\npub const CATALOGUE: &[(&str, &str)] = &[\n");
    for (k, v) in &out.catalogue {
        writeln!(catalogue, "    ({:?}, {:?}),", k, v).unwrap();
    }
    catalogue.push_str("];\n");
    let modrs = "//! GENERATED by tools/polars-gen: do not edit. Regenerate with\n//! `cargo run --manifest-path tools/polars-gen/Cargo.toml -- <inventory> adapters/polars`.\npub mod catalogue;\n#[cfg(feature = \"test-support\")]\npub mod fixtures;\npub mod functions;\npub mod support;\npub mod types;\n";
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for e in &out.entries {
        *counts.entry(e.status).or_insert(0) += 1;
    }
    let surface = serde_json::json!({
        "source": Path::new(&args[1]).file_name().unwrap().to_str().unwrap(),
        "buckets": buckets,
        "counts": counts,
        "wrappers": world.wrappers.iter().map(|(c, w)| serde_json::json!({"type": c, "rune": rune_path(w), "hand_written": w.hand})).collect::<Vec<_>>(),
        "oracle_cases": oracle_tests.matches("    Case {").count(),
        "oracle_skipped": oracle_skipped.iter().map(|(p, r)| serde_json::json!({"path": p, "reason": r})).collect::<Vec<_>>(),
        "entries": out.entries,
    });
    let surface = serde_json::to_string_pretty(&surface).unwrap() + "\n";
    let adapter = PathBuf::from(&args[2]);
    let files: Vec<(PathBuf, String)> = vec![
        (adapter.join("src/generated/types.rs"), types),
        (adapter.join("src/generated/functions.rs"), functions),
        (adapter.join("src/generated/catalogue.rs"), catalogue),
        (adapter.join("src/generated/mod.rs"), modrs.to_string()),
        (adapter.join("src/generated/fixtures.rs"), fixtures),
        (adapter.join("tests/generated_oracle.rs"), oracle_tests),
        (adapter.join("surface.json"), surface),
    ];
    let mut drift = false;
    for (path, content) in &files {
        if check {
            let existing = std::fs::read_to_string(path).unwrap_or_default();
            if existing != *content {
                eprintln!("drift: {} differs from the generator's output", path.display());
                drift = true;
            }
        } else {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
    }
    for (k, v) in &counts {
        println!("{k}: {v}");
    }
    println!("wrappers: {} ({} hand-written)", world.wrappers.len(), world.wrappers.values().filter(|w| w.hand).count());
    if drift {
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------- oracle tests

/// Core fixtures: canonical type -> (rune fx function, Rust value, how to
/// show it structurally). Frames, series and columns are shown cell by cell
/// through `rnx_polars::oracle`, independent of Polars's `fmt` feature.
const FIXTURES: &[(&str, &str, &str, &str)] = &[
    ("polars_core::frame::dataframe::DataFrame", "df", "polars::df!(\"x\" => [1i64, 2, 3], \"y\" => [\"a\", \"b\", \"c\"], \"z\" => [1.5f64, 2.5, 3.5]).unwrap()", "crate_oracle::frame_repr(v)"),
    ("polars_lazy::frame::LazyFrame", "lf", "df().lazy()", "match v.clone().collect() { Ok(d) => crate_oracle::frame_repr(&d).prefixed(\"collected \"), Err(e) => crate_oracle::Repr::Text(format!(\"collect error: {}\", crate_oracle::error_kind(&e))) }"),
    ("polars_plan::dsl::expr::Expr", "expr", "p::col(\"x\")", "match df().lazy().select([v.clone()]).collect() { Ok(d) => crate_oracle::frame_repr(&d).prefixed(\"selected \"), Err(e) => crate_oracle::Repr::Text(format!(\"select error: {}\", crate_oracle::error_kind(&e))) }"),
    ("polars_core::series::Series", "series", "p::Series::new(\"x\".into(), [1i64, 2, 3])", "crate_oracle::series_repr(v)"),
    ("polars_core::frame::column::Column", "column", "series().into_column()", "crate_oracle::column_repr(v)"),
    ("polars_core::datatypes::dtype::DataType", "dtype", "p::DataType::Int64", "crate_oracle::Repr::Text(format!(\"{:?}\", v))"),
    ("polars_core::datatypes::field::Field", "field", "p::Field::new(\"x\".into(), p::DataType::Int64)", "crate_oracle::Repr::Text(format!(\"{:?}\", v))"),
    ("polars_lazy::frame::LazyGroupBy", "group_by", "lf().group_by_stable([p::col(\"y\")])", "match v.clone().agg([p::col(\"x\").sum()]).collect() { Ok(d) => crate_oracle::frame_repr(&d).prefixed(\"agg \"), Err(e) => crate_oracle::Repr::Text(format!(\"agg error: {}\", crate_oracle::error_kind(&e))) }"),
];

/// Operations whose Rust contract leaves row order unspecified; their
/// cases compare rows as a set. Everything else compares in order.
const UNORDERED_OPS: &[&str] = &["unique", "unique_generic"];

/// Oracles that cannot be compared and are excluded by identity, with the
/// reason recorded as the entry's disposition.
const EXCLUDED_ORACLE: &[(&str, &str)] = &[
    ("polars_core::series::Series::as_single_ptr", "returns a memory address"),
    ("polars_io::cloud::concurrency::ConcurrencyController::new", "Debug output shows runtime state"),
];

struct Oracle<'a> {
    world: &'a World,
    debuggable: BTreeSet<String>,
    defaultable: BTreeSet<String>,
    /// Wrapper types the tests show through an in-crate helper (canonical paths).
    shown: std::cell::RefCell<BTreeSet<String>>,
}

impl<'a> Oracle<'a> {
    fn fixture(&self, canonical: &str) -> Option<&'static (&'static str, &'static str, &'static str, &'static str)> {
        FIXTURES.iter().find(|f| f.0 == canonical)
    }

    /// Rune expression producing a value of this shape, if fixtures allow.
    fn rune_value(&self, shape: &str) -> Option<String> {
        match shape {
            "bool" => Some("true".into()),
            "int" => Some("2".into()),
            "float" => Some("1.5".into()),
            "string" => Some("\"x\"".into()),
            "unit" => Some("()".into()),
            _ => {
                if let Some(c) = shape.strip_prefix("W:") {
                    return self.rune_wrapped(c);
                }
                if let Some(inner) = shape.strip_prefix("opt(").and_then(|s| s.strip_suffix(')')) {
                    return self.rune_value(inner).map(|v| format!("Some({v})"));
                }
                if let Some(inner) = shape.strip_prefix("vec(").and_then(|s| s.strip_suffix(')')) {
                    return self.rune_value(inner).map(|v| format!("[{v}]"));
                }
                if let Some(inner) = shape.strip_prefix("tuple(").and_then(|s| s.strip_suffix(')')) {
                    let parts: Option<Vec<String>> = inner.split(';').map(|p| self.rune_value(p)).collect();
                    return parts.map(|p| format!("({})", p.join(", ")));
                }
                None
            }
        }
    }

    fn rune_wrapped(&self, canonical: &str) -> Option<String> {
        if let Some(f) = self.fixture(canonical) {
            return Some(format!("fx::{}()", f.1));
        }
        let w = self.world.wrappers.get(canonical)?;
        let s = self.world.types.get(canonical)?;
        if s.kind == "enum" {
            let v = s.variant_shapes.iter().find(|(_, data)| !*data)?;
            return Some(format!("{}::{}()", rune_path(w), rune_name(&sanitize(&v.0))));
        }
        if self.defaultable.contains(canonical) {
            return Some(format!("{}::default_()", rune_path(w)));
        }
        None
    }

    /// Rust expression producing the Polars value for a canonical parameter
    /// type. References are leaked so they outlive any call and container:
    /// this is test fixture code.
    fn rust_value(&self, t: &Ty, generics: &BTreeMap<String, String>, owner: Option<&str>, depth: u8) -> Option<String> {
        if depth > 6 {
            return None;
        }
        match t {
            Ty::Path { path, args } => match path.as_str() {
                "bool" => Some("true".into()),
                "i64" => Some("2i64".into()),
                "f64" => Some("1.5f64".into()),
                "f32" => Some("1.5f32".into()),
                p if INT_NARROW.contains(&p) => Some(format!("2{p}")),
                "polars_utils::index::IdxSize" => Some("2 as p::IdxSize".into()),
                "char" => Some("'x'".into()),
                "alloc::string::String" => Some("\"x\".to_string()".into()),
                "polars_utils::pl_str::PlSmallStr" => Some("p::PlSmallStr::from(\"x\")".into()),
                "core::option::Option" if args.len() == 1 => self.rust_value(&args[0], generics, owner, depth + 1).map(|v| format!("Some({v})")),
                "alloc::vec::Vec" if args.len() == 1 => self.rust_value(&args[0], generics, owner, depth + 1).map(|v| format!("vec![{v}]")),
                "Self" => self.rust_value(&Ty::Path { path: owner?.to_string(), args: vec![] }, generics, owner, depth + 1),
                _ => self.rust_wrapped(path),
            },
            Ty::Ref { mutable, inner } => match &**inner {
                Ty::Path { path, .. } if path == "str" => Some("\"x\"".into()),
                Ty::Slice(e) => self.rust_value(e, generics, owner, depth + 1).map(|v| if *mutable { format!("vec![{v}].leak()") } else { format!("&*vec![{v}].leak()") }),
                other => {
                    let v = self.rust_value(other, generics, owner, depth + 1)?;
                    Some(if *mutable { format!("Box::leak(Box::new({v}))") } else { format!("&*Box::leak(Box::new({v}))") })
                }
            },
            Ty::Tuple(ts) => {
                let parts: Option<Vec<String>> = ts.iter().map(|e| self.rust_value(e, generics, owner, depth + 1)).collect();
                parts.map(|p| format!("({})", p.join(", ")))
            }
            Ty::Impl(bounds) => self.rust_bounds(bounds, generics, owner, depth),
            Ty::Generic(g) => {
                if g == "Self" {
                    return self.rust_value(&Ty::Path { path: "Self".into(), args: vec![] }, generics, owner, depth + 1);
                }
                let b = generics.get(g)?;
                match ty::parse(&format!("impl {b}")) {
                    Ty::Impl(bs) => self.rust_bounds(&bs, generics, owner, depth),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn rust_bounds(&self, bounds: &[Bound], generics: &BTreeMap<String, String>, owner: Option<&str>, depth: u8) -> Option<String> {
        for b in bounds {
            let l = last(&b.path);
            match l {
                "Into" | "Borrow" if b.args.len() == 1 => return self.rust_value(&b.args[0], generics, owner, depth + 1),
                "AsRef" if b.args.len() == 1 => {
                    return match &b.args[0] {
                        Ty::Path { path, .. } if path == "str" => Some("\"x\"".into()),
                        Ty::Slice(e) => self.rust_value(e, generics, owner, depth + 1).map(|v| format!("vec![{v}]")),
                        other => self.rust_value(other, generics, owner, depth + 1),
                    };
                }
                "IntoVec" if b.args.len() == 1 => return self.rust_value(&b.args[0], generics, owner, depth + 1).map(|v| format!("vec![{v}]")),
                "IntoIterator" => return self.rust_value(b.item.as_ref()?, generics, owner, depth + 1).map(|v| format!("vec![{v}]")),
                _ => {}
            }
        }
        None
    }

    fn rust_wrapped(&self, canonical: &str) -> Option<String> {
        if let Some(f) = self.fixture(canonical) {
            return Some(format!("{}()", f.1));
        }
        let w = self.world.wrappers.get(canonical)?;
        let s = self.world.types.get(canonical)?;
        if s.kind == "enum" {
            let v = s.variant_shapes.iter().find(|(_, data)| !*data)?;
            return Some(format!("<{}>::{}", w.spell, v.0));
        }
        if self.defaultable.contains(canonical) {
            return Some(format!("<{}>::default()", w.spell));
        }
        None
    }

    /// How to show a wrapped value of this type, as an expression over `v: &T`.
    fn show(&self, canonical: &str) -> Option<String> {
        if let Some(f) = self.fixture(canonical) {
            return Some(f.3.to_string());
        }
        if self.debuggable.contains(canonical) {
            return Some("crate_oracle::Repr::Text(format!(\"{:?}\", v))".into());
        }
        None
    }

    /// The wrapped type a return produces at the top level, through a
    /// `PolarsResult`, if any: such cases compare the value under the policy.
    fn top_wrapped(&self, t: &Ty, owner: Option<&str>) -> Option<(String, bool)> {
        let (inner, fallible) = match t {
            Ty::Path { path, args } if (path == "polars_error::PolarsResult" || path == "core::result::Result") && !args.is_empty() => (&args[0], true),
            other => (other, false),
        };
        let path = match inner {
            Ty::Path { path, args } if args.is_empty() => if path == "Self" { owner?.to_string() } else { path.clone() },
            Ty::Generic(g) if g == "Self" => owner?.to_string(),
            _ => return None,
        };
        if self.world.wrappers.contains_key(&path) && self.show(&path).is_some() { Some((path, fallible)) } else { None }
    }

    /// Rust: format the Polars value `__r` of canonical type `t` as a `Side`.
    /// A nested result that fails is `<<ERR:kind>>`, collapsed by the caller.
    fn oracle_fmt(&self, t: &Ty, owner: Option<&str>, depth: u8) -> Option<String> {
        if depth > 6 {
            return None;
        }
        match t {
            Ty::Tuple(ts) if ts.is_empty() => Some("\"()\".to_string()".into()),
            Ty::Tuple(ts) => {
                let parts: Option<Vec<String>> = ts.iter().enumerate().map(|(i, e)| self.oracle_fmt(e, owner, depth + 1).map(|f| format!("{{ let __r = __t.{i}; {f} }}"))).collect();
                parts.map(|p| format!("{{ let __t = __r; format!(\"({{}})\", [{}].join(\", \")) }}", p.join(", ")))
            }
            Ty::Ref { inner, .. } => self.oracle_fmt(inner, owner, depth + 1).map(|f| format!("{{ let __r = (__r).clone(); {f} }}")),
            Ty::Path { path, args } => match path.as_str() {
                "bool" | "i64" | "f64" => Some("format!(\"{}\", __r)".into()),
                "f32" => Some("format!(\"{}\", __r as f64)".into()),
                p if INT_NARROW.contains(&p) => Some("format!(\"{}\", __r as i64)".into()),
                "polars_utils::index::IdxSize" => Some("format!(\"{}\", __r as i64)".into()),
                "char" | "str" | "alloc::string::String" | "polars_utils::pl_str::PlSmallStr" => Some("format!(\"{}\", __r.to_string())".into()),
                "Self" => self.oracle_fmt(&Ty::Path { path: owner?.to_string(), args: vec![] }, owner, depth + 1),
                "core::option::Option" if args.len() == 1 => self.oracle_fmt(&args[0], owner, depth + 1).map(|f| format!("match __r {{ Some(__r) => format!(\"Some({{}})\", {f}), None => \"None\".to_string() }}")),
                "alloc::vec::Vec" if args.len() == 1 => self.oracle_fmt(&args[0], owner, depth + 1).map(|f| format!("format!(\"[{{}}]\", __r.into_iter().map(|__r| {f}).collect::<Vec<_>>().join(\", \"))")),
                "polars_error::PolarsResult" | "core::result::Result" if !args.is_empty() => self.oracle_fmt(&args[0], owner, depth + 1).map(|f| format!("match __r {{ Ok(__r) => {f}, Err(e) => format!(\"<<ERR:{{}}>>\", crate_oracle::error_kind(&e)) }}")),
                _ => {
                    let show = self.show(path)?;
                    Some(format!("{{ let v = &__r; ({show}).to_text() }}"))
                }
            },
            Ty::Generic(g) if g == "Self" => self.oracle_fmt(&Ty::Path { path: "Self".into(), args: vec![] }, owner, depth + 1),
            _ => None,
        }
    }

    /// Rust: format the Rune value `v` of the wrapper's return type as a string.
    fn script_fmt(&self, ret_rust: &str, ret_canonical: Option<&Ty>, owner: Option<&str>, depth: u8) -> Option<String> {
        if depth > 6 {
            return None;
        }
        let r = ret_rust.trim();
        match r {
            "()" => Some("Ok(\"()\".to_string())".into()),
            "bool" => Some("rune::from_value::<bool>(v).map(|x| format!(\"{x}\")).map_err(|e| e.to_string())".into()),
            "i64" => Some("rune::from_value::<i64>(v).map(|x| format!(\"{x}\")).map_err(|e| e.to_string())".into()),
            "f64" => Some("rune::from_value::<f64>(v).map(|x| format!(\"{x}\")).map_err(|e| e.to_string())".into()),
            "String" => Some("rune::from_value::<String>(v).map(|x| format!(\"{x}\")).map_err(|e| e.to_string())".into()),
            _ => {
                if let Some(inner) = r.strip_prefix("Option<").and_then(|s| s.strip_suffix('>')) {
                    let ic = match ret_canonical { Some(Ty::Path { path, args }) if path == "core::option::Option" && args.len() == 1 => Some(&args[0]), Some(Ty::Ref { inner, .. }) => match &**inner { Ty::Path { path, args } if path == "core::option::Option" && args.len() == 1 => Some(&args[0]), _ => None }, _ => None };
                    let f = self.script_fmt(inner, ic, owner, depth + 1)?;
                    return Some(format!("match rune::from_value::<Option<rune::Value>>(v) {{ Ok(Some(v)) => ({f}).map(|s| format!(\"Some({{s}})\")), Ok(None) => Ok(\"None\".to_string()), Err(e) => Err(e.to_string()) }}"));
                }
                if let Some(inner) = r.strip_prefix("Vec<").and_then(|s| s.strip_suffix('>')) {
                    let ic = match ret_canonical { Some(Ty::Path { path, args }) if path == "alloc::vec::Vec" && args.len() == 1 => Some(&args[0]), _ => None };
                    let f = self.script_fmt(inner, ic, owner, depth + 1)?;
                    return Some(format!("match rune::from_value::<Vec<rune::Value>>(v) {{ Ok(items) => items.into_iter().map(|v| {f}).collect::<Result<Vec<_>, _>>().map(|s| format!(\"[{{}}]\", s.join(\", \"))), Err(e) => Err(e.to_string()) }}"));
                }
                if r.starts_with('(') {
                    return None; // tuples: not compared in this stage
                }
                let (canonical, w) = self.world.wrappers.iter().find(|(_, x)| x.rust == r)?;
                self.show(canonical)?;
                self.shown.borrow_mut().insert(canonical.clone());
                Some(format!("rnx_polars::generated::fixtures::show_{}(&v).map(|r| r.to_text())", w.rust.to_lowercase()))
            }
        }
    }
}

struct OracleCase {
    id: String,
    path: String,
    script: String,
    has_receiver: bool,
    /// Rows compared as a set: only for `UNORDERED_OPS`.
    unordered: bool,
    /// Rust: turn the script's first (and second) value into a `Side`.
    fmt: String,
    /// Rust: run the Polars side and produce a `Side`.
    oracle: String,
}

fn emit_oracle(world: &World, entries: &mut [Entry], inv: &Inventory) -> (String, String, Vec<(String, String)>) {
    let mut debuggable: BTreeSet<String> = inv.supporting.iter().filter(|s| s.derived.iter().any(|d| d == "Debug")).map(|s| s.canonical_path.clone()).collect();
    let mut defaultable: BTreeSet<String> = inv.supporting.iter().filter(|s| s.derived.iter().any(|d| d == "Default")).map(|s| s.canonical_path.clone()).collect();
    for c in &inv.callables {
        if c.kind == "foreign_trait_impl" {
            if c.name.starts_with("Debug") { debuggable.insert(c.owner.clone()); }
            if c.name.starts_with("Default") { defaultable.insert(c.owner.clone()); }
        }
    }
    let o = Oracle { world, debuggable, defaultable, shown: std::cell::RefCell::new(BTreeSet::new()) };
    let mut cases = Vec::new();
    let mut skipped: Vec<(String, String)> = Vec::new();
    // Every generated entry gets exactly one disposition.
    for e in entries.iter_mut() {
        if e.status != "generated" {
            continue;
        }
        let mut skip = |e: &mut Entry, why: String| {
            e.execution = Some(why.clone());
            skipped.push((e.canonical_path.clone(), why));
        };
        let Some(info) = e.oracle.clone() else {
            skip(e, "no call information".into());
            continue;
        };
        let id = sanitize(&e.canonical_path).to_lowercase();
        let owner = info.owner.as_ref().map(|(c, _)| c.as_str());
        if let Some((_, why)) = EXCLUDED_ORACLE.iter().find(|(p, _)| *p == e.canonical_path) {
            skip(e, format!("excluded: nondeterministic oracle ({why})"));
            continue;
        }
        let unordered = UNORDERED_OPS.contains(&info.rune_name.as_str()) || UNORDERED_OPS.contains(&info.rune_name.trim_end_matches('_'));
        // fallible script value: Ok(v) -> value side (text), Err(e) -> error kind
        let value_side = |inner: &str| format!("match rune::from_value::<Result<rune::Value, rune::Value>>(v) {{ Ok(Ok(v)) => ({inner}).map(|s| crate_oracle::Side::Value(crate_oracle::Repr::Text(s))).unwrap_or_else(crate_oracle::Side::Broken), Ok(Err(e)) => crate_oracle::rune_error_kind(&e).map(crate_oracle::Side::Error).unwrap_or_else(crate_oracle::Side::Broken), Err(e) => crate_oracle::Side::Broken(e.to_string()) }}");
        let plain_side = |inner: &str| format!("({inner}).map(|s| crate_oracle::Side::Value(crate_oracle::Repr::Text(s))).unwrap_or_else(crate_oracle::Side::Broken)");
        // a wrapped value at the top level keeps its structure for the policy
        let wrapped_side = |helper: &str, fallible: bool| if fallible {
            format!("match rune::from_value::<Result<rune::Value, rune::Value>>(v) {{ Ok(Ok(v)) => {helper}(&v).map(crate_oracle::Side::Value).unwrap_or_else(crate_oracle::Side::Broken), Ok(Err(e)) => crate_oracle::rune_error_kind(&e).map(crate_oracle::Side::Error).unwrap_or_else(crate_oracle::Side::Broken), Err(e) => crate_oracle::Side::Broken(e.to_string()) }}")
        } else {
            format!("{helper}(&v).map(crate_oracle::Side::Value).unwrap_or_else(crate_oracle::Side::Broken)")
        };
        // protocols
        if info.receiver == "protocol" {
            let tname = info.callee.as_str();
            let Some(recv_rune) = o.rune_wrapped(owner.unwrap()) else { skip(e, "no fixture for the receiver type".into()); continue };
            let Some(recv_rust) = o.rust_wrapped(owner.unwrap()) else { skip(e, "no Rust fixture for the receiver type".into()); continue };
            let (script, fmt, oracle) = match tname {
                "Display" => (
                    format!("pub fn main() {{ let a = {recv_rune}; (`${{a}}`, ()) }}"),
                    plain_side("rune::from_value::<String>(v).map_err(|e| e.to_string())"),
                    format!("crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"{{}}\", {recv_rust})))"),
                ),
                "PartialEq" => (
                    format!("pub fn main() {{ let a = {recv_rune}; let b = {recv_rune}; (a == b, ()) }}"),
                    plain_side("rune::from_value::<bool>(v).map(|x| format!(\"{x}\")).map_err(|e| e.to_string())"),
                    format!("crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"{{}}\", {recv_rust} == {recv_rust})))"),
                ),
                "Default" => {
                    let c = owner.unwrap();
                    let Some(show) = o.show(c) else { skip(e, "receiver type has no comparison".into()); continue };
                    let w = &world.wrappers[c];
                    o.shown.borrow_mut().insert(c.to_string());
                    (
                        format!("pub fn main() {{ let a = {}::default_(); (a, ()) }}", rune_path(w)),
                        wrapped_side(&format!("rnx_polars::generated::fixtures::show_{}", w.rust.to_lowercase()), false),
                        format!("crate_oracle::Side::Value({{ let __o = <{}>::default(); let v = &__o; {show} }})", w.spell),
                    )
                }
                t if OPS.iter().any(|(n, _, _)| *n == t) || t == "Neg" => {
                    let op = if t == "Neg" { "-".to_string() } else { OPS.iter().find(|(n, _, _)| *n == t).unwrap().2.to_string() };
                    let ret_t = info.ret_canonical.as_deref().map(ty::parse);
                    let Some(ret_t) = ret_t else { skip(e, "operator without output".into()); continue };
                    let Some(of) = o.oracle_fmt(&ret_t, owner, 0) else { skip(e, "return type has no Rust comparison".into()); continue };
                    let ret_rust = match &ret_t { Ty::Path { path, .. } => world.wrappers.get(path).map(|w| w.rust.clone()), _ => None };
                    let Some(ret_rust) = ret_rust else { skip(e, "operator result type not wrapped".into()); continue };
                    let Some(sf) = o.script_fmt(&ret_rust, Some(&ret_t), owner, 0) else { skip(e, "return type has no comparison".into()); continue };
                    let of = format!("crate_oracle::Repr::Text({of})");
                    if t == "Neg" {
                        (format!("pub fn main() {{ let a = {recv_rune}; (-a, ()) }}"), plain_side(&sf), format!("{{ let __r = -{recv_rust}; crate_oracle::Side::Value({of}) }}"))
                    } else {
                        let Some((shape, canonical)) = info.params.first() else { skip(e, "operator without rhs".into()); continue };
                        let _ = shape;
                        let rhs_t = ty::parse(canonical);
                        let rhs_rune = match &rhs_t { Ty::Path { path, .. } => o.rune_wrapped(path), Ty::Ref { inner, .. } => match &**inner { Ty::Path { path, .. } => o.rune_wrapped(path), _ => None }, _ => None };
                        let Some(rhs_rune) = rhs_rune else { skip(e, "no fixture for the operand".into()); continue };
                        let Some(rhs_rust) = o.rust_value(&rhs_t, &BTreeMap::new(), owner, 0) else { skip(e, "no Rust fixture for the operand".into()); continue };
                        (format!("pub fn main() {{ let a = {recv_rune}; let b = {rhs_rune}; (a {op} b, ()) }}"), plain_side(&sf), format!("{{ let __r = {recv_rust} {op} {rhs_rust}; crate_oracle::Side::Value({of}) }}"))
                    }
                }
                _ => { skip(e, format!("protocol {tname} has no script-level trigger")); continue }
            };
            e.execution = Some("case".into());
            cases.push(OracleCase { id, path: e.canonical_path.clone(), script, has_receiver: false, fmt, oracle, unordered: false });
            continue;
        }
        // receiver
        let recv_rune = match owner { Some(c) => o.rune_wrapped(c), None => None };
        if info.receiver != "none" && recv_rune.is_none() {
            skip(e, "no fixture for the receiver type".into());
            continue;
        }
        let recv_rust = match owner { Some(c) => o.rust_wrapped(c), None => None };
        if info.receiver != "none" && recv_rust.is_none() {
            skip(e, "no Rust fixture for the receiver type".into());
            continue;
        }
        // arguments
        let mut rune_args = Vec::new();
        let mut rust_args = Vec::new();
        let mut missing = None;
        for (shape, canonical) in &info.params {
            match (o.rune_value(shape), o.rust_value(&ty::parse(canonical), &info.generics, owner, 0)) {
                (Some(a), Some(b)) => {
                    rune_args.push(a);
                    rust_args.push(b);
                }
                _ => {
                    missing = Some(format!("no fixture for a parameter ({canonical})"));
                    break;
                }
            }
        }
        if let Some(why) = missing {
            skip(e, why);
            continue;
        }
        let mutating = info.receiver == "&mut self";
        let ret_ty = info.ret_canonical.as_deref().map(ty::parse);
        let rust_is_result = matches!(&ret_ty, Some(Ty::Path { path, .. }) if path == "polars_error::PolarsResult" || path == "core::result::Result");
        // return formatting on both sides
        // top-level wrapped returns are compared as structured values under the
        // case's policy; everything else as ordered text
        let top = if mutating { None } else { ret_ty.as_ref().and_then(|t| o.top_wrapped(t, owner)) };
        let (ret_fmt_script, ret_fmt_rust) = if info.ret_rust == "()" {
            // unit, or a `&mut Self` chain reduced to unit; Rust may still be a Result
            let of = if rust_is_result { "match __r { Ok(_) => \"()\".to_string(), Err(e) => format!(\"<<ERR:{}>>\", crate_oracle::error_kind(&e)) }".to_string() } else { "{ let _ = __r; \"()\".to_string() }".to_string() };
            ("Ok(\"()\".to_string())".to_string(), of)
        } else if let Some((canonical, _)) = &top {
            let w = &world.wrappers[canonical];
            o.shown.borrow_mut().insert(canonical.clone());
            (format!("rnx_polars::generated::fixtures::show_{}", w.rust.to_lowercase()), o.show(canonical).unwrap())
        } else {
            let Some(sf) = o.script_fmt(&info.ret_rust, ret_ty.as_ref(), owner, 0) else { skip(e, format!("return type has no comparison ({})", info.ret_rust)); continue };
            let of = match &ret_ty { None => Some("\"()\".to_string()".to_string()), Some(t) => o.oracle_fmt(t, owner, 0) };
            let Some(of) = of else { skip(e, "return type has no Rust comparison".into()); continue };
            (sf, of)
        };
        let args_r = rune_args.join(", ");
        let mut binds = String::new();
        let mut passed = Vec::new();
        for (i, a) in rust_args.iter().enumerate() {
            binds.push_str(&format!("let __a{i} = {a}; "));
            passed.push(format!("__a{i}"));
        }
        let args_s = passed.join(", ");
        let (script, fmt, oracle) = if mutating {
            let c = owner.unwrap();
            let Some(show) = o.show(c) else { skip(e, "receiver type has no comparison".into()); continue };
            let w = &world.wrappers[c];
            o.shown.borrow_mut().insert(c.to_string());
            // script: (receiver after the call, return); both compared
            let script = format!("pub fn main() {{ let a = {}; let r = a.{}({args_r}); ((a, r), ()) }}", recv_rune.as_ref().unwrap(), info.rune_name);
            // receiver state after the call and the return are compared together;
            // an error return keeps the receiver in the comparison as `ret=ERR:kind`
            let fmt = format!("{{ let (a, r) = match rune::from_value::<(rune::Value, rune::Value)>(v) {{ Ok(x) => x, Err(e) => return crate_oracle::Side::Broken(e.to_string()) }}; let recv = match rnx_polars::generated::fixtures::show_{}(&a) {{ Ok(s) => s.to_text(), Err(e) => return crate_oracle::Side::Broken(e) }}; let ret = {{ let v = r; {} }}; match ret {{ crate_oracle::Side::Value(s) => crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"recv={{recv}};ret={{}}\", s.to_text()))), crate_oracle::Side::Error(k) => crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"recv={{recv}};ret=ERR:{{k}}\"))), other => other }} }}", w.rust.to_lowercase(), if info.fallible { value_side(&ret_fmt_script) } else { plain_side(&ret_fmt_script) });
            let oracle = format!("{{ let mut __o = {}; {binds}let __r = {}(&mut __o, {args_s}); let ret = {}; let recv = ({{ let v = &__o; {show} }}).to_text(); let ret = ret.replace(\"<<ERR:\", \"ERR:\").replace(\">>\", \"\"); crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"recv={{recv}};ret={{ret}}\"))) }}", recv_rust.as_ref().unwrap(), info.callee, ret_fmt_rust).replace(", )", ")");
            (script, fmt, oracle)
        } else {
            let script = match (&info.rune_owner, info.receiver.as_str()) {
                (Some(_), "none") => format!("pub fn main() {{ let r = {}::{}({args_r}); (r, ()) }}", info.rune_owner.as_ref().unwrap(), info.rune_name),
                (Some(_), _) => format!("pub fn main() {{ let a = {}; let r = a.{}({args_r}); let r2 = a.{}({args_r}); (r, r2) }}", recv_rune.as_ref().unwrap(), info.rune_name, info.rune_name),
                (None, _) => format!("pub fn main() {{ let r = polars::{}({args_r}); (r, ()) }}", info.rune_name),
            };
            let fmt = match &top {
                Some((_, _)) => wrapped_side(&ret_fmt_script, info.fallible),
                None => if info.fallible { value_side(&ret_fmt_script) } else { plain_side(&ret_fmt_script) },
            };
            let call = match info.receiver.as_str() {
                "none" => format!("{binds}let __r = {}({args_s});", info.callee),
                "self" => format!("let __recv = {}; {binds}let __r = {}(__recv, {args_s});", recv_rust.as_ref().unwrap(), info.callee),
                "&self" => format!("let __recv = {}; {binds}let __r = {}(&__recv, {args_s});", recv_rust.as_ref().unwrap(), info.callee),
                _ => { skip(e, "receiver form".into()); continue }
            }.replace(", )", ")");
            let oracle_body = match &top {
                Some((_, true)) => format!("match __r {{ Ok(__r) => crate_oracle::Side::Value({{ let v = &__r; {ret_fmt_rust} }}), Err(e) => crate_oracle::Side::Error(crate_oracle::error_kind(&e)) }}"),
                Some((_, false)) => format!("crate_oracle::Side::Value({{ let v = &__r; {ret_fmt_rust} }})"),
                None => format!("crate_oracle::collapse({ret_fmt_rust})"),
            };
            (script, fmt, format!("{{ {call} {oracle_body} }}"))
        };
        e.execution = Some("case".into());
        cases.push(OracleCase { id, path: e.canonical_path.clone(), script, has_receiver: !mutating && (info.receiver == "self" || info.receiver == "&self"), fmt, oracle, unordered });
    }
    // fixtures module inside the adapter
    let mut fixtures = String::from("//! GENERATED by tools/polars-gen: fixtures for the generated oracle tests.\n//! Only built with the `test-support` feature.\n#![allow(dead_code, unused_imports, clippy::all)]\nuse super::types::*;\nuse crate::oracle as crate_oracle;\nuse crate::{DataFrame, Expr, LazyFrame, LazyGroupBy};\nuse polars::prelude as p;\nuse polars::prelude::{IntoColumn, IntoLazy};\nuse rnx::rune;\nuse values::*;\n\n/// The Polars values the oracle tests use on both sides.\npub mod values {\n    use polars::prelude as p;\n    use polars::prelude::*;\n");
    for (_, name, expr, _) in FIXTURES {
        let ret = match *name { "df" => "p::DataFrame", "lf" => "p::LazyFrame", "expr" => "p::Expr", "series" => "p::Series", "column" => "p::Column", "dtype" => "p::DataType", "field" => "p::Field", "group_by" => "p::LazyGroupBy", _ => unreachable!() };
        writeln!(fixtures, "    pub fn {name}() -> {ret} {{ {expr} }}").unwrap();
    }
    fixtures.push_str("}\n\n");
    for (canonical, name, _, _) in FIXTURES {
        let w = &world.wrappers[*canonical];
        writeln!(fixtures, "#[rune::function(path = {name})]\nfn fx_{name}() -> {} {{ {}(values::{name}()) }}", w.rust, w.rust).unwrap();
    }
    for canonical in o.shown.borrow().iter() {
        let w = &world.wrappers[canonical];
        let show = o.show(canonical).unwrap();
        writeln!(fixtures, "/// Show a `{}` held in a Rune value, for the oracle tests.\npub fn show_{}(v: &rune::Value) -> Result<crate_oracle::Repr, String> {{ v.borrow_ref::<{}>().map_err(|e| e.to_string()).map(|w| {{ let v = &w.0; {show} }}) }}", canonical, w.rust.to_lowercase(), w.rust).unwrap();
    }
    fixtures.push_str("\npub fn install(m: &mut rune::Module) -> Result<(), rune::ContextError> {\n");
    for (_, name, _, _) in FIXTURES {
        writeln!(fixtures, "    m.function_meta(fx_{name})?;").unwrap();
    }
    fixtures.push_str("    Ok(())\n}\n");
    // the test file
    let mut t = String::from(r#"//! GENERATED by tools/polars-gen: every generated binding with fixtures,
//! called in Rune and in Rust on the same values and compared through
//! `rnx_polars::oracle`, which fails every unapproved outcome. Outcomes are
//! written to oracle-results.json.
#![cfg(feature = "test-support")]
#![allow(unused_imports, unused_variables, unused_mut, clippy::all)]
use polars::prelude as p;
use polars::prelude::*;
use rnx::rune::{self, Context, Module, Source, Sources, Value, Vm};
use rnx_polars::generated::fixtures::values::*;
use rnx_polars::oracle as crate_oracle;
use rnx_polars::oracle::{Outcome, Side};
use std::sync::Arc;

struct Case {
    id: &'static str,
    path: &'static str,
    script: &'static str,
    has_receiver: bool,
    /// Rows compared as a set: only for operations whose Rust contract
    /// leaves row order unspecified.
    unordered: bool,
    fmt: fn(Value) -> Side,
    oracle: fn() -> Side,
}

"#);
    for c in &cases {
        writeln!(t, "fn fmt_{}(v: Value) -> Side {{ {} }}\nfn oracle_{}() -> Side {{ {} }}", c.id, c.fmt, c.id, c.oracle).unwrap();
    }
    t.push_str("\nstatic CASES: &[Case] = &[\n");
    for c in &cases {
        writeln!(t, "    Case {{ id: {:?}, path: {:?}, script: {:?}, has_receiver: {}, unordered: {}, fmt: fmt_{}, oracle: oracle_{} }},", c.id, c.path, c.script, c.has_receiver, c.unordered, c.id, c.id).unwrap();
    }
    t.push_str("];\n");
    t.push_str(r#"
fn run_side(f: impl FnOnce() -> Side + std::panic::UnwindSafe) -> Side {
    match std::panic::catch_unwind(f) {
        Ok(s) => s,
        Err(e) => Side::Panic(crate_oracle::panic_text(e)),
    }
}

struct Harness {
    context: Context,
    runtime: Arc<rune::runtime::RuntimeContext>,
}

impl Harness {
    fn new() -> Harness {
        let mut m = Module::with_crate("polars").unwrap();
        rnx_polars::build(&mut m).unwrap();
        let mut f = Module::with_crate("fx").unwrap();
        rnx_polars::generated::fixtures::install(&mut f).unwrap();
        let mut context = Context::with_default_modules().unwrap();
        context.install(m).unwrap();
        context.install(f).unwrap();
        let runtime = Arc::new(context.runtime().unwrap());
        Harness { context, runtime }
    }

    /// Run one case the way the main test does: the script, the oracle
    /// twice, then the classifier under the case's policy.
    fn run(&self, case: &Case) -> (Outcome, String) {
        let (first, second) = {
            let context = &self.context;
            let runtime = self.runtime.clone();
            let run = move || -> (Side, Option<Side>) {
                let mut sources = Sources::new();
                sources.insert(Source::memory(case.script).unwrap()).unwrap();
                let unit = match rune::prepare(&mut sources).with_context(context).build() { Ok(u) => u, Err(e) => return (Side::Broken(format!("compile: {e}")), None) };
                let mut vm = Vm::new(runtime, Arc::new(unit));
                let out = match vm.call(["main"], ()) { Ok(o) => o, Err(e) => return (Side::Broken(format!("vm: {e}")), None) };
                let (r, r2) = match rune::from_value::<(Value, Value)>(out) { Ok(x) => x, Err(e) => return (Side::Broken(e.to_string()), None) };
                let first = (case.fmt)(r);
                let second = if case.has_receiver { Some((case.fmt)(r2)) } else { None };
                (first, second)
            };
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)) {
                Ok(x) => x,
                Err(e) => (Side::Panic(crate_oracle::panic_text(e)), None),
            }
        };
        let oracle = run_side(case.oracle);
        let again = run_side(case.oracle);
        let policy = if case.unordered { crate_oracle::UNORDERED } else { crate_oracle::ORDERED };
        crate_oracle::classify(policy, &first, second.as_ref(), &oracle, &again)
    }
}

#[test]
fn generated_bindings_match_polars() {
    let h = Harness::new();
    let mut results = Vec::new();
    let mut tally: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    let mut bad = Vec::new();
    // Polars panics are expected outcomes here; keep the log readable.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    for case in CASES {
        let (outcome, detail) = h.run(case);
        *tally.entry(outcome.name()).or_insert(0) += 1;
        if !outcome.approved() {
            bad.push(format!("{}: {}: {}", case.path, outcome.name(), detail.chars().take(300).collect::<String>()));
        }
        results.push(serde_json::json!({"id": case.id, "path": case.path, "status": outcome.name(), "unordered": case.unordered, "detail": detail.chars().take(400).collect::<String>()}));
    }
    std::panic::set_hook(previous);
    let out = serde_json::json!({"cases": CASES.len(), "tally": tally, "results": results});
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("oracle-results.json");
    std::fs::write(&path, serde_json::to_string_pretty(&out).unwrap() + "\n").unwrap();
    eprintln!("oracle tally: {tally:?}");
    assert!(bad.is_empty(), "{} case(s) failed:\n{}", bad.len(), bad.join("\n"));
}

/// Integrated negative controls through the same runner: each injected
/// wrong binding must come out as a failing outcome.
#[test]
fn oracle_runner_fails_closed() {
    use rnx_polars::generated::fixtures::show_dataframe as show_df;
    let h = Harness::new();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    fn df_side(v: Value) -> Side { show_df(&v).map(Side::Value).unwrap_or_else(Side::Broken) }
    fn reversed() -> Side { Side::Value(crate_oracle::frame_repr(&df().reverse())) }
    fn same() -> Side { Side::Value(crate_oracle::frame_repr(&df())) }
    fn alternating() -> Side {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        Side::Value(crate_oracle::Repr::Text(format!("run{}", N.fetch_add(1, std::sync::atomic::Ordering::SeqCst))))
    }
    fn panics() -> Side { panic!("polars panic") }
    fn nth() -> usize {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        N.fetch_add(1, std::sync::atomic::Ordering::SeqCst) % 2
    }
    fn first_then_reversed() -> Side { if nth() == 0 { same() } else { reversed() } }
    fn first_then_error() -> Side { if nth() == 0 { same() } else { Side::Error("ComputeError".into()) } }
    fn first_then_panic() -> Side { if nth() == 0 { same() } else { panic!("second run") } }
    let controls: &[(&str, Case, &[Outcome])] = &[
        ("unreversed frame against Rust reverse", Case { id: "c1", path: "control", script: "pub fn main() { let a = fx::df(); (a, ()) }", has_receiver: false, unordered: false, fmt: df_side, oracle: reversed }, &[Outcome::Mismatch]),
        ("unreversed frame is still wrong under an unordered policy when a cell differs", Case { id: "c1b", path: "control", script: "pub fn main() { let a = fx::df(); (a, ()) }", has_receiver: false, unordered: true, fmt: df_side, oracle: || Side::Value(crate_oracle::frame_repr(&polars::df!("x" => [1i64, 2, 4], "y" => ["a", "b", "c"], "z" => [1.5f64, 2.5, 3.5]).unwrap())) }, &[Outcome::Mismatch]),
        ("wrong second call on the same receiver", Case { id: "c2", path: "control", script: "pub fn main() { let a = fx::df(); (a.reverse(), a) }", has_receiver: true, unordered: false, fmt: df_side, oracle: reversed }, &[Outcome::ReuseFailed]),
        ("wrong second order under the default policy", Case { id: "c2b", path: "control", script: "pub fn main() { let a = fx::df(); (a, a.reverse()) }", has_receiver: true, unordered: false, fmt: df_side, oracle: same }, &[Outcome::ReuseFailed]),
        ("compile error against a nondeterministic oracle", Case { id: "c3", path: "control", script: "pub fn main() { let a = ; }", has_receiver: false, unordered: false, fmt: df_side, oracle: alternating }, &[Outcome::Broken]),
        ("a script panic (a VM error) against a Rust panic", Case { id: "c4", path: "control", script: "pub fn main() { panic(\"unrelated\") }", has_receiver: false, unordered: false, fmt: df_side, oracle: panics }, &[Outcome::Broken]),
        ("a binding that panics in Polars against a Rust panic with another message", Case { id: "c4b", path: "control", script: "pub fn main() { let a = fx::column(); (a.product(), ()) }", has_receiver: false, unordered: false, fmt: df_side, oracle: || panic!("a different panic") }, &[Outcome::PanicMismatch]),
        ("a value against a panicking oracle", Case { id: "c5", path: "control", script: "pub fn main() { let a = fx::df(); (a, ()) }", has_receiver: false, unordered: false, fmt: df_side, oracle: panics }, &[Outcome::OraclePanicked]),
        ("a wrong value against a nondeterministic oracle", Case { id: "c6", path: "control", script: "pub fn main() { let a = fx::df(); (a, ()) }", has_receiver: false, unordered: false, fmt: df_side, oracle: alternating }, &[Outcome::Nondeterministic]),
        ("a matching first run whose second run changes value", Case { id: "c7", path: "control", script: "pub fn main() { let a = fx::df(); (a, ()) }", has_receiver: false, unordered: false, fmt: df_side, oracle: first_then_reversed }, &[Outcome::Nondeterministic]),
        ("a matching first run whose second run errors", Case { id: "c7b", path: "control", script: "pub fn main() { let a = fx::df(); (a, ()) }", has_receiver: false, unordered: false, fmt: df_side, oracle: first_then_error }, &[Outcome::Nondeterministic]),
        ("a matching first run whose second run panics", Case { id: "c7c", path: "control", script: "pub fn main() { let a = fx::df(); (a, ()) }", has_receiver: false, unordered: false, fmt: df_side, oracle: first_then_panic }, &[Outcome::Nondeterministic]),
    ];
    let mut failures = Vec::new();
    for (label, case, expected) in controls {
        let (outcome, detail) = h.run(case);
        if outcome.approved() || !expected.contains(&outcome) {
            failures.push(format!("{label}: got {} ({detail})", outcome.name()));
        }
    }
    std::panic::set_hook(previous);
    assert!(failures.is_empty(), "controls that did not fail closed:\n{}", failures.join("\n"));
}
"#);
    (fixtures, t, skipped)
}
