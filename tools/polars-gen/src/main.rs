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

/// Record 0075: the release-specific inputs, read from
/// `releases/<release>.toml` and recorded (name, source, digest) in
/// `surface.json`. Nothing about a release is a constant in this file.
#[derive(serde::Deserialize, Clone)]
struct Release {
    name: String,
    source: String,
    api_crates: Vec<String>,
    /// The inventory this policy belongs to: `release` for a crates.io
    /// release, `rev` for Git sources. Checked against the inventory's
    /// recorded provenance before anything is generated.
    #[serde(default)]
    provenance: ReleaseProvenance,
    #[serde(default)]
    unordered: Vec<Unordered>,
    #[serde(default)]
    excluded_oracle: Vec<ExcludedOracle>,
}
#[derive(serde::Deserialize, Clone, Default)]
struct ReleaseProvenance {
    #[serde(default)]
    release: Option<String>,
    #[serde(default)]
    rev: Option<String>,
}
#[derive(serde::Deserialize, Clone)]
struct Unordered {
    path: String,
    /// The recognized configuration, one entry per parameter of the
    /// operation: the exact Rune fixture expression the case must pass, or
    /// `*` for a parameter that cannot affect row order. A case whose
    /// parameters or expressions differ stays ordered.
    #[serde(default)]
    args: BTreeMap<String, String>,
    options: String,
    cite: String,
}
#[derive(serde::Deserialize, Clone)]
struct ExcludedOracle {
    path: String,
    reason: String,
}

impl Release {
    fn load(path: &Path) -> (Release, String) {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("release file {}: {e}", path.display()));
        let r: Release = toml::from_str(&text).unwrap_or_else(|e| panic!("release file {}: {e}", path.display()));
        use sha2::Digest;
        let digest = format!("{:x}", sha2::Sha256::digest(text.as_bytes()));
        (r, digest)
    }
    fn is_api(&self, krate: &str) -> bool {
        self.api_crates.iter().any(|c| c == krate)
    }
    /// The order policy for a case: unordered only when the canonical
    /// operation is listed and the case's parameters are exactly the
    /// listed ones, each passing the recorded expression (or anything,
    /// for a `*` parameter). Anything unrecognized is ordered.
    fn policy(&self, canonical_path: &str, params: &[(String, String)]) -> (bool, String) {
        let Some(u) = self.unordered.iter().find(|u| u.path == canonical_path) else {
            return (false, "ordered (operation not listed as unordered)".into());
        };
        let names: BTreeSet<&str> = params.iter().map(|(n, _)| n.as_str()).collect();
        let listed: BTreeSet<&str> = u.args.keys().map(|k| k.as_str()).collect();
        if names != listed {
            return (false, format!("ordered (listed, but the case's parameters [{}] are not the recorded configuration's [{}])", names.into_iter().collect::<Vec<_>>().join(", "), listed.into_iter().collect::<Vec<_>>().join(", ")));
        }
        for (n, expr) in params {
            let want = &u.args[n];
            if want != "*" && want != expr {
                return (false, format!("ordered (listed, but `{n}` is `{expr}`, not the recorded `{want}`)"));
            }
        }
        (true, format!("unordered: {} [{}]", u.options, u.cite))
    }

    /// The release file must name the inventory it belongs to, and the
    /// inventory must record the same provenance.
    fn check_provenance(&self, inv: &Inventory) -> Result<String, String> {
        let Some(p) = &inv.provenance else { return Err("the inventory records no provenance (re-extract it with the record 0075 extractor)".into()) };
        let ((Some(_), _) | (_, Some(_))) = (&self.provenance.release, &self.provenance.rev) else { return Err(format!("release file `{}` names no provenance ([provenance] release = … or rev = …)", self.name)) };
        if let Some(want) = &self.provenance.release {
            match &p.release {
                Some(have) if have == want => {}
                other => return Err(format!("release file `{}` is for release {want}; the inventory records {other:?}", self.name)),
            }
        }
        if let Some(want) = &self.provenance.rev {
            match &p.rev {
                Some(have) if have == want => {}
                other => return Err(format!("release file `{}` is for rev {want}; the inventory records {other:?}", self.name)),
            }
        }
        Ok(format!("release {:?} rev {:?} cfg {:?}", p.release, p.rev, p.cfg))
    }
}

/// Record 0075 gate 3 controls: equivalent aliases resolve to one wrapper;
/// same-name distinct types resolve to two Rune paths.
fn wrapper_self_test() {
    fn sup(path: &str, kind: &str, target: Option<&str>) -> Supporting {
        Supporting {
            key: path.to_string(),
            kind: kind.to_string(),
            canonical_path: path.to_string(),
            found_paths: vec![format!("polars::{}", path.split("::").skip(1).collect::<Vec<_>>().join("::"))],
            crate_paths: vec![path.to_string()],
            public_fields: 0,
            fields_canonical: vec![],
            variant_shapes: vec![],
            variant_payloads: vec![],
            generic: false,
            lifetime: false,
            hidden: false,
            derived: vec![],
            alias_target: target.map(|t| t.to_string()),
            implementors: vec![],
        }
    }
    let mut generic = sup("polars_core::chunked_array::ChunkedArray", "struct", None);
    generic.generic = true;
    let inv = Inventory {
        callables: vec![],
        provenance: None,
        supporting: vec![
            generic,
            sup("polars_core::datatypes::UInt32Chunked", "type_alias", Some("polars_core::chunked_array::ChunkedArray<polars_core::datatypes::UInt32Type>")),
            sup("polars_core::datatypes::aliases::IdxCa", "type_alias", Some("polars_core::chunked_array::ChunkedArray<polars_core::datatypes::UInt32Type>")),
            sup("polars_core::datatypes::BooleanChunked", "type_alias", Some("polars_core::chunked_array::ChunkedArray<polars_core::datatypes::BooleanType>")),
            sup("polars_core::schema::Field", "struct", None),
            sup("polars_plan::dsl::Field", "struct", None),
            sup("polars_dtype::categorical::CatSize", "type_alias", Some("u32")),
        ],
    };
    let release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), api_crates: vec!["polars_core".into(), "polars_plan".into()], unordered: vec![], excluded_oracle: vec![] };
    let w = World::new(&inv, &release, &["mechanical"]);
    let idx = &w.wrappers["polars_core::datatypes::aliases::IdxCa"];
    let u32c = &w.wrappers["polars_core::datatypes::UInt32Chunked"];
    assert_eq!(idx.rust, u32c.rust, "equivalent aliases must share one wrapper");
    assert_eq!(rune_path(idx), "polars::IdxCa", "the representative is the alphabetically first alias name");
    assert_eq!(idx.aliases, vec!["polars_core::datatypes::aliases::IdxCa".to_string(), "polars_core::datatypes::UInt32Chunked".to_string()]);
    let boolean = &w.wrappers["polars_core::datatypes::BooleanChunked"];
    assert_ne!(boolean.rust, idx.rust, "a different instantiation is a different wrapper");
    let f1 = rune_path(&w.wrappers["polars_core::schema::Field"]);
    let f2 = rune_path(&w.wrappers["polars_plan::dsl::Field"]);
    assert_ne!(f1, f2, "same-name distinct types must have distinct Rune paths");
    assert_eq!((f1.as_str(), f2.as_str()), ("polars::core::Field", "polars::plan::Field"));
    assert!(!w.wrappers.contains_key("polars_dtype::categorical::CatSize"), "a scalar alias is not wrapped");
    let distinct: BTreeSet<&str> = w.wrappers.values().map(|w| w.rust.as_str()).collect();
    assert_eq!(distinct.len(), 4, "four wrapper structs: one shared alias, BooleanChunked, two Fields");
    println!("wrapper self-test: ok");
}

fn policy_self_test() {
    let r: Release = toml::from_str(r#"
name = "t"
source = "t"
api_crates = ["polars_lazy"]
[provenance]
release = "9.9.9"
[[unordered]]
path = "polars_lazy::frame::LazyFrame::join"
args = { other = "*", left_on = "*", right_on = "*", args = "polars::JoinArgs::default_()" }
options = "JoinArgs::default()"
cite = "t"
[[unordered]]
path = "polars_lazy::frame::LazyFrame::inner_join"
args = { other = "*", left_on = "*", right_on = "*" }
options = "default arguments"
cite = "t"
"#).unwrap();
    let p = |v: &[(&str, &str)]| v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
    let join = "polars_lazy::frame::LazyFrame::join";
    let base = [("other", "fx::lf()"), ("left_on", "[fx::expr()]"), ("right_on", "[fx::expr()]")];
    fn with<'a>(base: &[(&'a str, &'a str)], a: &'a str) -> Vec<(&'a str, &'a str)> { let mut v = base.to_vec(); v.push(("args", a)); v }
    assert!(r.policy(join, &p(&with(&base, "polars::JoinArgs::default_()"))).0, "the recorded recipe is unordered");
    assert!(!r.policy(join, &p(&with(&base, "polars::JoinArgs::default_().with_maintain_order(polars::MaintainOrderJoin::Left())"))).0, "an explicit order stays ordered");
    assert!(!r.policy(join, &p(&with(&base, "polars::JoinArgs::default_().with_maintain_order(polars::MaintainOrderJoin::None()).with_maintain_order(polars::MaintainOrderJoin::Left())"))).0, "a chained explicit order stays ordered");
    assert!(!r.policy(join, &p(&with(&base, "unknown_nondefault_fixture()"))).0, "an unrecognized configuration stays ordered");
    assert!(!r.policy(join, &p(&base)).0, "a case missing a recorded parameter stays ordered");
    assert!(!r.policy(join, &p(&[("other", "fx::lf()"), ("left_on", "[fx::expr()]"), ("right_on", "[fx::expr()]"), ("args", "polars::JoinArgs::default_()"), ("extra", "1")])).0, "a case with an unlisted parameter stays ordered");
    assert!(r.policy("polars_lazy::frame::LazyFrame::inner_join", &p(&base)).0, "a listed operation with only order-irrelevant parameters is unordered");
    assert!(!r.policy("polars_lazy::frame::LazyFrame::cross_join", &p(&base)).0, "an unlisted operation is ordered");
    assert!(!r.policy("other::LazyFrame::join", &p(&with(&base, "polars::JoinArgs::default_()"))).0, "a same-named method elsewhere is not matched");
    // provenance: the release file must belong to the inventory
    let inv = |release: Option<&str>, rev: Option<&str>| Inventory { callables: vec![], supporting: vec![], provenance: Some(model::Provenance { release: release.map(String::from), rev: rev.map(String::from), cfg: None }) };
    assert!(r.check_provenance(&inv(Some("9.9.9"), None)).is_ok());
    assert!(r.check_provenance(&inv(Some("0.55.2"), None)).is_err(), "another release is refused");
    assert!(r.check_provenance(&inv(None, Some("abc"))).is_err(), "a Git inventory is refused by a release-pinned file");
    assert!(r.check_provenance(&Inventory { callables: vec![], supporting: vec![], provenance: None }).is_err(), "no provenance is refused");
    let mut g = r.clone();
    g.provenance = ReleaseProvenance { release: None, rev: Some("abc".into()) };
    assert!(g.check_provenance(&inv(None, Some("abc"))).is_ok());
    assert!(g.check_provenance(&inv(None, Some("abd"))).is_err(), "another revision is refused");
    let mut n = r.clone();
    n.provenance = ReleaseProvenance::default();
    assert!(n.check_provenance(&inv(Some("9.9.9"), None)).is_err(), "a release file without provenance is refused");
    println!("policy self-test: ok");
}

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
    /// Identity: the canonical instantiated type this wrapper holds. For a
    /// struct or enum it is the type's own path; for an alias it is the
    /// expanded alias target, so equivalent aliases share one wrapper.
    identity: String,
    /// The rule that admitted the wrapper: `api type`, `alias`, `internal type`.
    rule: &'static str,
    /// Every canonical path (the type, or each alias) this wrapper serves.
    aliases: Vec<String>,
    /// For an alias: the generic base type of its target (an `Arc<T>`
    /// target reports `T`'s base), whose Clone/Debug/Default impls apply.
    base: Option<String>,
}

struct World {
    release: Release,
    /// Counter for per-binding temporaries.
    tmp: std::cell::Cell<usize>,
    types: BTreeMap<String, Supporting>,
    wrappers: BTreeMap<String, Wrapper>,
    ambiguous_prelude: BTreeSet<String>,
    /// Types with a `Clone` impl, derived or hand-written.
    clonable: BTreeSet<String>,
    /// Foreign trait impls by owner: (trait short name, `for` type, bounds).
    impls: BTreeMap<String, Vec<(String, String, Vec<(String, String)>)>>,
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

/// A spelling through the `polars` facade only, for a type of a crate the
/// adapter does not depend on directly: the shortest non-prelude re-export,
/// then a nested prelude path, then `polars::prelude::Name` when unambiguous.
fn spell_facade(found: &[String], name: &str, ambiguous: &BTreeSet<String>) -> Option<String> {
    let mut direct: Vec<&String> = found.iter().filter(|f| !f.contains(" as ") && f.starts_with("polars::") && !f.contains("::prelude::")).collect();
    direct.sort_by_key(|f| (last(f) != name, f.matches("::").count(), f.as_str()));
    if let Some(d) = direct.first() {
        return Some((*d).clone());
    }
    let mut nested: Vec<&String> = found.iter().filter(|f| !f.contains(" as ") && f.starts_with("polars::prelude::") && f.matches("::").count() > 2).collect();
    nested.sort_by_key(|f| (last(f) != name, f.matches("::").count(), f.as_str()));
    if let Some(n) = nested.first() {
        return Some((*n).clone());
    }
    let short = format!("polars::prelude::{name}");
    if found.iter().any(|f| *f == short) && !ambiguous.contains(name) {
        return Some(short);
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
    fn new(inv: &Inventory, release: &Release, buckets: &[&str]) -> World {
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
        // Types an API-crate signature in the accepted buckets mentions, by
        // canonical path: the admission test for internal-crate wrappers.
        let mut mentioned: BTreeSet<String> = BTreeSet::new();
        for c in &inv.callables {
            if !buckets.contains(&c.bucket.as_str()) || !release.is_api(&c.krate) {
                continue;
            }
            for t in c.params.iter().map(|p| p.ty_canonical.as_str()).chain(c.ret_canonical.as_deref()) {
                for m in type_paths(t) {
                    mentioned.insert(m);
                }
            }
        }
        let mut impls: BTreeMap<String, Vec<(String, String, Vec<(String, String)>)>> = BTreeMap::new();
        for c in &inv.callables {
            if c.kind == "foreign_trait_impl" {
                if let Some(f) = &c.impl_for {
                    let short = c.name.split('<').next().unwrap_or("").to_string();
                    impls.entry(c.owner.clone()).or_default().push((short, f.clone(), c.impl_bounds.clone()));
                }
            }
        }
        let mut w = World { release: release.clone(), tmp: std::cell::Cell::new(0), types, wrappers: BTreeMap::new(), ambiguous_prelude, clonable, impls };
        w.assign_wrappers(&mentioned);
        // an alias wrapper clones when an impl covers its instantiation
        let mut more = Vec::new();
        for (path, wr) in &w.wrappers {
            if wr.base.is_some() && w.trait_holds(&wr.identity, "Clone", 0) {
                more.push(path.clone());
            }
        }
        w.clonable.extend(more);
        w
    }

    /// Whether `trait_short` (Clone, Debug, Default, PartialEq) holds for a
    /// canonical type, from the inventory alone: a derive or impl on a
    /// concrete type; for an instantiation `Base<A, B>`, an impl on `Base`
    /// whose `for` type unifies with it and whose bounds on the unified
    /// parameters hold (a local trait by its recorded implementors, one of
    /// the four by recursion, marker bounds trivially). `Arc<T>` clones
    /// always and otherwise follows `T`; `()` and scalars satisfy all four.
    fn trait_holds(&self, ty: &str, trait_short: &str, depth: usize) -> bool {
        if depth > 8 {
            return false;
        }
        let ty = ty.trim();
        if ty == "()" || SCALARS.contains(&ty) || ty == "alloc::string::String" || ty == "polars_utils::pl_str::PlSmallStr" {
            return true;
        }
        if let Some(inner) = ty.strip_prefix("alloc::sync::Arc<").and_then(|r| r.strip_suffix('>')) {
            return trait_short == "Clone" || self.trait_holds(inner, trait_short, depth + 1);
        }
        let base = ty.split('<').next().unwrap_or(ty);
        if let Some(s) = self.types.get(base) {
            if s.derived.iter().any(|d| d == trait_short) && !ty.contains('<') {
                return true;
            }
            if s.kind == "type_alias" {
                if let Some(t) = &s.alias_target {
                    return self.trait_holds(t, trait_short, depth + 1);
                }
            }
        }
        let Some(impls) = self.impls.get(base) else { return false };
        let args = split_top(ty.strip_prefix(base).and_then(|r| r.strip_prefix('<')).and_then(|r| r.strip_suffix('>')).unwrap_or(""));
        'impls: for (short, for_ty, bounds) in impls {
            if short != trait_short {
                continue;
            }
            if for_ty == ty {
                return true;
            }
            let for_args = split_top(for_ty.strip_prefix(base).and_then(|r| r.strip_prefix('<')).and_then(|r| r.strip_suffix('>')).unwrap_or(""));
            if for_args.len() != args.len() {
                continue;
            }
            for (fa, a) in for_args.iter().zip(&args) {
                if fa == a {
                    continue;
                }
                let Some((_, b)) = bounds.iter().find(|(n, _)| n == fa) else {
                    if fa.contains("::") || !fa.chars().all(|c| c.is_alphanumeric() || c == '_') {
                        continue 'impls;
                    }
                    continue; // unbounded parameter
                };
                for bound in b.split(" + ").map(str::trim).filter(|b| !b.is_empty()) {
                    let tpath = bound.split('<').next().unwrap();
                    let tshort = last(tpath);
                    let ok = match tshort {
                        "Sized" | "Send" | "Sync" | "Unpin" | "Any" => true,
                        "Clone" | "Debug" | "Default" | "PartialEq" if tpath.starts_with("core::") => self.trait_holds(a, tshort, depth + 1),
                        _ => self.types.get(tpath).is_some_and(|t| t.implementors.iter().any(|i| i == a)),
                    };
                    if !ok {
                        continue 'impls;
                    }
                }
            }
            return true;
        }
        false
    }

    /// Every concrete, reachable, unhidden struct/enum/union in the API
    /// crates gets a wrapper; hand-written wrappers are reused. Record
    /// 0075 adds two rules: a concrete alias in an API crate whose expanded
    /// target is an instantiation of a reachable struct or enum (or an
    /// `Arc` of one) is wrapped under the alias's name, equivalent aliases
    /// sharing one wrapper; and a concrete, lifetime-free struct or enum of
    /// an internal crate that a bound API signature mentions is wrapped
    /// under `polars::<crate short name>::<Name>`.
    fn assign_wrappers(&mut self, mentioned: &BTreeSet<String>) {
        struct Cand {
            paths: Vec<String>,
            identity: String,
            rule: &'static str,
            base: Option<String>,
            internal: bool,
        }
        let mut by_name: BTreeMap<String, Vec<Cand>> = BTreeMap::new();
        let mut alias_groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (path, s) in &self.types {
            let api = self.release.api_crates.iter().any(|c| path.starts_with(&format!("{c}::")));
            if s.generic || s.lifetime || s.hidden {
                continue;
            }
            if spell(&s.found_paths, &s.crate_paths, last(path), &self.ambiguous_prelude).is_none() && spell_facade(&s.found_paths, last(path), &self.ambiguous_prelude).is_none() {
                continue;
            }
            match s.kind.as_str() {
                "struct" | "enum" | "union" if api => {
                    by_name.entry(last(path).to_string()).or_default().push(Cand { paths: vec![path.clone()], identity: path.clone(), rule: "api type", base: None, internal: false });
                }
                "struct" | "enum" | "union" if mentioned.contains(path) => {
                    by_name.entry(last(path).to_string()).or_default().push(Cand { paths: vec![path.clone()], identity: path.clone(), rule: "internal type", base: None, internal: true });
                }
                "type_alias" if api => {
                    let Some(target) = &s.alias_target else { continue };
                    let Some(base) = alias_base(target) else { continue };
                    let Some(b) = self.types.get(&base) else { continue };
                    if !matches!(b.kind.as_str(), "struct" | "enum" | "union") || b.lifetime || b.hidden {
                        continue;
                    }
                    if target.contains("&") || target.contains("dyn ") || target.contains("impl ") {
                        continue;
                    }
                    alias_groups.entry(target.clone()).or_default().push(path.clone());
                }
                _ => {}
            }
        }
        for (identity, mut paths) in alias_groups {
            // deterministic representative: the alphabetically first short
            // name, then the alphabetically first path
            paths.sort_by(|a, b| last(a).cmp(last(b)).then(a.cmp(b)));
            let base = alias_base(&identity);
            by_name.entry(last(&paths[0]).to_string()).or_default().push(Cand { paths, identity, rule: "alias", base, internal: false });
        }
        for (name, cands) in by_name {
            let collision = cands.iter().filter(|c| !c.internal).count() > 1;
            for cand in cands {
                let rep = &cand.paths[0];
                let s = &self.types[rep];
                let spelled = if cand.internal { spell_facade(&s.found_paths, &name, &self.ambiguous_prelude) } else { spell(&s.found_paths, &s.crate_paths, &name, &self.ambiguous_prelude) };
                let Some(spelled) = spelled else { continue };
                let hand = HAND_WRAPPERS.iter().find(|(c, _)| *c == rep.as_str());
                let krate_short = rep.split("::").next().unwrap().trim_start_matches("polars_").to_string();
                let rune_item = if (collision || cand.internal) && hand.is_none() { format!("::polars::{krate_short}") } else { "::polars".to_string() };
                let w = Wrapper {
                    rust: hand.map(|(_, r)| r.rsplit("::").next().unwrap().to_string()).unwrap_or_else(|| format!("W_{}", sanitize(rep))),
                    spell: spelled,
                    rune_item,
                    rune_name: name.clone(),
                    hand: hand.is_some(),
                    identity: cand.identity.clone(),
                    rule: cand.rule,
                    aliases: cand.paths.clone(),
                    base: cand.base.clone(),
                };
                for p in &cand.paths {
                    self.wrappers.insert(p.clone(), w.clone());
                }
            }
        }
        // two wrappers must never share a Rune path
        let mut seen: BTreeMap<String, String> = BTreeMap::new();
        for w in self.wrappers.values() {
            let rp = rune_path(w);
            if let Some(prev) = seen.insert(rp.clone(), w.identity.clone()) {
                if prev != w.identity {
                    panic!("two wrappers share the Rune path {rp}: {prev} and {}", w.identity);
                }
            }
        }
    }
}

const SCALARS: &[&str] = &["bool", "i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128", "usize", "isize", "f32", "f64", "char", "str"];

/// Split a generic argument list at its top-level commas.
fn split_top(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in s.chars() {
        match ch {
            '<' | '(' | '[' => { depth += 1; cur.push(ch); }
            '>' | ')' | ']' => { depth -= 1; cur.push(ch); }
            ',' if depth == 0 => { out.push(cur.trim().to_string()); cur.clear(); }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// The generic base of an alias target: `ChunkedArray<BooleanType>` gives
/// `ChunkedArray`; an `Arc<T>` target gives `T`'s base. None for a scalar,
/// tuple, slice or reference target.
fn alias_base(target: &str) -> Option<String> {
    let t = target.strip_prefix("alloc::sync::Arc<").and_then(|r| r.strip_suffix('>')).unwrap_or(target);
    let base = t.split('<').next()?.trim();
    if base.is_empty() || !base.contains("::") || base.starts_with('(') || base.starts_with('[') || base.starts_with('&') {
        return None;
    }
    Some(base.to_string())
}

/// Every `a::b::C` path that occurs in a canonical type rendering.
fn type_paths(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_alphabetic() || b[i] == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || (b[i] == b':' && i + 1 < b.len() && b[i + 1] == b':') || (b[i] == b':' && i > 0 && b[i - 1] == b':')) {
                i += 1;
            }
            let tok = &text[start..i];
            if tok.contains("::") {
                out.push(tok.to_string());
            }
        } else {
            i += 1;
        }
    }
    out
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
                        (format!("let {t} = match {name} {{ Some(v) => Some({owned}), None => None }};"), format!("{t}.as_deref()"))
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
                        // an unwrapped concrete alias stands for its target
                        if s.kind == "type_alias" && args.is_empty() {
                            if let Some(target) = &s.alias_target {
                                return self.arg(&ty::parse(target), name, generics, owner, depth + 1);
                            }
                        }
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
                // `&str` converts through `to_string`; cloning the reference is a no-op
                if matches!(&**inner, Ty::Path { path, .. } if path == "str") {
                    return Ok(x);
                }
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
                        if s.kind == "type_alias" && args.is_empty() {
                            if let Some(target) = &s.alias_target {
                                return self.ret(&ty::parse(target), owner, depth + 1);
                            }
                        }
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
    /// Parameter names, for the order policy's recorded configuration.
    param_names: Vec<String>,
    ret_canonical: Option<String>,
    /// Wrapper return type as emitted.
    ret_rust: String,
    fallible: bool,
    generics: BTreeMap<String, String>,
    /// For trait methods: every generated implementor (canonical, wrapper ident).
    implementors: Vec<(String, String)>,
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
            let mut infos: Vec<OracleInfo> = Vec::new();
            for owner in impls {
                let before = out.entries.len();
                emit_method(world, out, c, owner, Some(&tspell));
                let e = out.entries.pop().unwrap();
                debug_assert_eq!(before, out.entries.len());
                if e.status == "generated" {
                    done.push(e.rune.clone().unwrap());
                    if let Some(i) = e.oracle.clone() {
                        infos.push(i);
                    }
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
            } else if let Some(mut info) = first_info {
                // every generated implementor is a candidate receiver for the
                // oracle; the fixture derivation picks the first with a fixture
                info.implementors = infos.iter().filter_map(|i| i.owner.clone()).collect();
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
        param_names: c.params.iter().map(|p| sanitize(&p.name)).collect(),
        ret_canonical: c.ret_canonical.clone(),
        ret_rust: ret.rust_ty.clone(),
        fallible,
        generics: generics.clone(),
        implementors: vec![],
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
    // Rune implements `Function` for free functions of at most five
    // parameters (rune 0.14.2 `function/macros.rs`, every reference
    // permutation); instance functions go to fifteen.
    const FREE_ARITY: usize = 5;
    if params.len() > FREE_ARITY {
        out.unsupported(c, "arity", &format!("free function with {} parameters; Rune binds at most {FREE_ARITY}", params.len()));
        return;
    }
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
        param_names: c.params.iter().map(|p| sanitize(&p.name)).collect(),
        ret_canonical: c.ret_canonical.clone(),
        ret_rust: ret.rust_ty.clone(),
        fallible,
        generics: generics.clone(),
        implementors: vec![],
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
        param_names: c.params.iter().map(|p| sanitize(&p.name)).collect(),
        ret_canonical: c.ret_canonical.clone(),
        ret_rust: String::new(),
        fallible: false,
        generics: BTreeMap::new(),
        implementors: vec![],
    };
    out.generated_with(c, &rune, Some(format!("derived: {}", c.derived)), info);
}

/// Wrapper type declarations, plus constructors/setters/getters for option
/// structs and variant constants/constructors for enums.
fn emit_types(world: &World, out: &mut Emitted) -> String {
    let mut s = String::new();
    s.push_str("//! GENERATED by tools/polars-gen from the record 0072 inventory: do not edit.\n//! Wrapper types for every concrete, reachable Polars type in the API crates.\n#![allow(non_camel_case_types, unused_imports, dead_code, clippy::all)]\nuse polars::prelude as p;\nuse rnx::rune;\n\n");
    let mut emitted: BTreeSet<String> = BTreeSet::new();
    for (canonical, w) in &world.wrappers {
        if w.hand || !emitted.insert(w.rust.clone()) {
            continue;
        }
        let ident = w.rust.rsplit("::").next().unwrap();
        let derive = if world.clonable.contains(canonical) { "#[derive(rune::Any, Clone)]" } else { "#[derive(rune::Any)]" };
        let doc = if w.aliases.len() > 1 { format!("{} = `{}` (also {})", w.aliases[0], w.identity, w.aliases[1..].iter().map(|a| format!("`{a}`")).collect::<Vec<_>>().join(", ")) } else if w.rule == "alias" { format!("{canonical} = `{}`", w.identity) } else { format!("`{canonical}`") };
        writeln!(s, "/// {doc}\n{derive}\n#[rune(item = {}, name = {})]\npub struct {ident}(pub(crate) {});", w.rune_item, w.rune_name, w.spell).unwrap();
    }
    s.push_str("\npub fn install(m: &mut rune::Module) -> Result<(), rune::ContextError> {\n");
    let mut installed: BTreeSet<String> = BTreeSet::new();
    for w in world.wrappers.values() {
        if !w.hand && installed.insert(w.rust.clone()) {
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
    if args.iter().any(|a| a == "--self-test") {
        wrapper_self_test();
        policy_self_test();
        return;
    }
    if args.len() < 3 {
        eprintln!("usage: polars-gen <inventory.json> <adapter-dir> --release <file> [--check] [--buckets a,b,c]");
        std::process::exit(2);
    }
    let check = args.iter().any(|a| a == "--check");
    let buckets: Vec<String> = args.iter().position(|a| a == "--buckets").map(|i| args[i + 1].split(',').map(|s| s.to_string()).collect()).unwrap_or_else(|| vec!["mechanical".into(), "conversion".into(), "option_struct".into()]);
    let buckets: Vec<&str> = buckets.iter().map(|s| s.as_str()).collect();
    let inv: Inventory = serde_json::from_str(&std::fs::read_to_string(&args[1]).expect("inventory")).expect("inventory json");
    let release_path = args.iter().position(|a| a == "--release").map(|i| PathBuf::from(&args[i + 1])).unwrap_or_else(|| { eprintln!("--release <file> is required"); std::process::exit(2) });
    let (release, release_digest) = Release::load(&release_path);
    match release.check_provenance(&inv) {
        Ok(p) => println!("inventory provenance: {p}; release file: {}", release.name),
        Err(e) => {
            eprintln!("refusing to generate: {e}");
            std::process::exit(2);
        }
    }
    let world = World::new(&inv, &release, &buckets);
    let mut out = Emitted { functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    let mut callables: Vec<&Callable> = inv.callables.iter().collect();
    callables.sort_by(|a, b| a.canonical_path.cmp(&b.canonical_path).then(a.key.cmp(&b.key)));
    // inherent methods take names before trait methods do
    callables.sort_by_key(|c| match c.kind.as_str() { "inherent" => 0, "free_fn" => 1, "foreign_trait_impl" => 2, _ => 3 });
    for c in callables {
        let api = release.is_api(&c.krate);
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
    let (fixtures, oracle_tests, oracle_skipped, recipes) = emit_oracle(&world, &mut out.entries, &inv);
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
        "release": {"name": release.name, "source": release.source, "file": release_path.file_name().unwrap().to_str().unwrap(), "sha256": release_digest},
        "buckets": buckets,
        "counts": counts,
        "wrappers": world.wrappers.iter().map(|(c, w)| serde_json::json!({"type": c, "rune": rune_path(w), "hand_written": w.hand, "rule": w.rule, "identity": w.identity, "shared_with": w.aliases.iter().filter(|a| *a != c).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "oracle_cases": oracle_tests.matches("    Case {").count(),
        "fixtures": recipes,
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
    let distinct: BTreeSet<&str> = world.wrappers.values().map(|w| w.rust.as_str()).collect();
    println!("wrappers: {} ({} hand-written; {} paths served)", distinct.len(), world.wrappers.values().filter(|w| w.hand).count(), world.wrappers.len());
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



/// How a fixture value of a wrapped type is built on both sides.
#[derive(Clone)]
struct Recipe {
    /// Rune expression; a fallible constructor is unwrapped with a `fixture:` panic.
    rune: String,
    /// Rust expression of the Polars value; a fallible constructor is
    /// unwrapped with a `fixture:` panic.
    rust: String,
    /// What the recipe is, for `surface.json`.
    kind: String,
}

struct Oracle<'a> {
    world: &'a World,
    debuggable: BTreeSet<String>,
    defaultable: BTreeSet<String>,
    /// Types whose `default_()` binding was generated: the Rune side of a
    /// `Default` fixture needs the binding, not only the Rust impl.
    default_bound: BTreeSet<String>,
    /// Wrapper types the tests show through an in-crate helper (canonical paths).
    shown: std::cell::RefCell<BTreeSet<String>>,
    /// Derived fixture recipes by canonical type (record 0075 gate 2).
    recipes: BTreeMap<String, Recipe>,
    /// Types that got no recipe, with the reason.
    no_recipe: BTreeMap<String, String>,
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

    /// The disposition note for a case: the derived recipes whose Rune
    /// fixture text the case's script actually contains (receiver or any
    /// argument, including one reached through a generic parameter).
    fn recipe_note(&self, case_path: &str, texts: &[&str]) -> String {
        let mut used: Vec<String> = Vec::new();
        for (c, r) in &self.recipes {
            // a constructor's own case is not "via" itself
            let own = r.kind.split(' ').nth(1).map(|n| format!("{c}::{n}")).as_deref() == Some(case_path);
            if !own && texts.iter().any(|t| t.contains(r.rune.as_str())) && !used.contains(&r.kind) { used.push(r.kind.clone()); }
        }
        used.sort();
        if used.is_empty() { String::new() } else { format!(" (fixture via {})", used.join(", ")) }
    }

    fn rune_wrapped(&self, canonical: &str) -> Option<String> {
        self.base_rune(canonical).or_else(|| self.recipes.get(canonical).map(|r| r.rune.clone()))
    }

    /// The rules of record 0073: a core fixture, `Default`, or a unit variant.
    fn base_rune(&self, canonical: &str) -> Option<String> {
        if let Some(f) = self.fixture(canonical) {
            return Some(format!("fx::{}()", f.1));
        }
        let w = self.world.wrappers.get(canonical)?;
        let s = self.world.types.get(canonical)?;
        if s.kind == "enum" {
            let v = s.variant_shapes.iter().find(|(_, data)| !*data)?;
            return Some(format!("{}::{}()", rune_path(w), rune_name(&sanitize(&v.0))));
        }
        if self.defaultable.contains(canonical) && self.default_bound.contains(canonical) {
            return Some(format!("{}::default_()", rune_path(w)));
        }
        None
    }

    fn base_rust(&self, canonical: &str) -> Option<String> {
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

    /// Record 0075 gate 2: derive recipes for wrapped types the base rules
    /// cannot build, from constructor bindings generated in this run and
    /// from data-carrying variants, as a fixpoint. Deterministic: names in
    /// the fixed order `new`, `from_*`, then alphabetical; the first
    /// variant in declaration order. A recipe is only adopted when every
    /// argument has a fixture already.
    fn derive_recipes(&mut self, entries: &[Entry]) {
        // constructor candidates per owner: receiver none, returns Self or PolarsResult<Self>
        let mut ctors: BTreeMap<String, Vec<(String, OracleInfo, bool)>> = BTreeMap::new();
        for e in entries {
            let Some(info) = &e.oracle else { continue };
            let Some((owner, _)) = &info.owner else { continue };
            if info.receiver != "none" || e.status != "generated" { continue }
            let ret = info.ret_canonical.as_deref().unwrap_or("");
            let (target, fallible) = match ty::parse(ret) {
                Ty::Path { path, args } if (path == "polars_error::PolarsResult" || path == "core::result::Result") && !args.is_empty() => (args[0].clone(), true),
                other => (other, false),
            };
            let returns_self = match &target { Ty::Path { path, args } => args.is_empty() && (path == "Self" || path == owner), Ty::Generic(g) => g == "Self", _ => false };
            if !returns_self { continue }
            ctors.entry(owner.clone()).or_default().push((info.rune_name.clone(), info.clone(), fallible));
        }
        for v in ctors.values_mut() {
            v.sort_by_key(|(n, _, _)| (if n == "new" { 0 } else if n.starts_with("from_") { 1 } else { 2 }, n.clone()));
        }
        loop {
            let mut progress = false;
            let mut wanted: Vec<String> = self.world.wrappers.keys().filter(|c| self.rune_wrapped(c).is_none()).cloned().collect();
            wanted.sort();
            for canonical in wanted {
                let Some(w) = self.world.wrappers.get(&canonical) else { continue };
                let Some(s) = self.world.types.get(&canonical) else { continue };
                // rule 3: a constructor binding whose arguments all have fixtures
                let mut found: Option<Recipe> = None;
                for (name, info, fallible) in ctors.get(&canonical).cloned().unwrap_or_default() {
                    let mut rune_args = Vec::new();
                    let mut rust_args = Vec::new();
                    let mut ok = true;
                    for (shape, ct) in &info.params {
                        match (self.rune_value(shape), self.rust_value(&ty::parse(ct), &info.generics, Some(&canonical), 0)) {
                            (Some(a), Some(b)) => { rune_args.push(a); rust_args.push(b); }
                            _ => { ok = false; break; }
                        }
                    }
                    if !ok { continue }
                    let rune_call = format!("{}::{}({})", rune_path(w), name, rune_args.join(", "));
                    let rust_call = format!("{}({})", info.callee, rust_args.join(", "));
                    // the Rune binding is fallible when the Rust return is a
                    // Result or an argument conversion can fail; each side is
                    // unwrapped where it is fallible, with a `fixture:` panic
                    let rune = if info.fallible { format!("match {rune_call} {{ Ok(v) => v, Err(e) => panic(`fixture: {} failed: ${{e}}`) }}", name) } else { rune_call };
                    let rust = if fallible { format!("match {rust_call} {{ Ok(v) => v, Err(e) => panic!(\"fixture: {} failed: {{e}}\") }}", name) } else { rust_call };
                    found = Some(Recipe { rune, rust, kind: format!("constructor {}{}", name, if fallible || info.fallible { " (fallible)" } else { "" }) });
                    break;
                }
                // rule 4: the first data-carrying variant whose payload has fixtures
                if found.is_none() && s.kind == "enum" {
                    for (vname, payload) in &s.variant_payloads {
                        if payload.iter().any(|x| x.contains(": ")) { continue } // struct variants: not this stage
                        let mut rune_args = Vec::new();
                        let mut rust_args = Vec::new();
                        let mut ok = true;
                        for pt in payload {
                            let t = ty::parse(pt);
                            let shape = match &t { Ty::Path { path, args } if args.is_empty() && self.world.wrappers.contains_key(path) => format!("W:{path}"), Ty::Path { path, .. } if path == "bool" => "bool".into(), Ty::Path { path, .. } if path == "i64" || INT_NARROW.contains(&path.as_str()) => "int".into(), Ty::Path { path, .. } if path == "f64" || path == "f32" => "float".into(), Ty::Path { path, .. } if path == "alloc::string::String" || path == "polars_utils::pl_str::PlSmallStr" => "string".into(), _ => String::new() };
                            match (self.rune_value(&shape), self.rust_value(&t, &BTreeMap::new(), Some(&canonical), 0)) {
                                (Some(a), Some(b)) => { rune_args.push(a); rust_args.push(b); }
                                _ => { ok = false; break; }
                            }
                        }
                        if !ok { continue }
                        found = Some(Recipe { rune: format!("{}::{}({})", rune_path(w), rune_name(&sanitize(vname)), rune_args.join(", ")), rust: format!("<{}>::{}({})", w.spell, vname, rust_args.join(", ")), kind: format!("variant {vname}") });
                        break;
                    }
                }
                if let Some(r) = found {
                    self.recipes.insert(canonical.clone(), r);
                    progress = true;
                }
            }
            if !progress { break }
        }
        for (canonical, w) in &self.world.wrappers {
            if self.rune_wrapped(canonical).is_none() {
                let s = &self.world.types[canonical];
                let why = if s.kind == "enum" { "no unit variant and no variant whose payload has fixtures" } else if ctors.contains_key(canonical) { "constructors exist but none has fixtures for all arguments" } else if self.defaultable.contains(canonical) && !self.default_bound.contains(canonical) { "Default impl without a generated default_ binding (the impl is on the generic base), no constructor binding" } else if s.public_fields > 0 { "public fields, no Default, no constructor binding" } else { "no public fields, no Default, no constructor binding" };
                let _ = w;
                self.no_recipe.insert(canonical.clone(), why.to_string());
            }
        }
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
        self.base_rust(canonical).or_else(|| self.recipes.get(canonical).map(|r| r.rust.clone()))
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
            Ty::Ref { inner, .. } if matches!(&**inner, Ty::Path { path, .. } if path == "str") => self.oracle_fmt(inner, owner, depth + 1),
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
    /// Rows compared as a set: only for a listed operation under its listed options.
    unordered: bool,
    /// The recorded justification, or why the case is ordered.
    policy: String,
    /// Rust: turn the script's first (and second) value into a `Side`.
    fmt: String,
    /// Rust: run the Polars side and produce a `Side`.
    oracle: String,
}

/// The script's setup stage: every fixture the measured call needs, and
/// nothing else, returned as a vector that `main(__fx)` receives; the
/// measured call uses those prepared values and constructs nothing.
fn setup_fn(exprs: &[String]) -> String {
    format!("pub fn setup() {{ [{}] }}", exprs.join(", "))
}

/// The Rust oracle with its own staged setup: the named fixtures are built
/// under `catch_unwind` and a failure there is `Staged::SetupFailed`; only
/// then does the body run with the prepared values.
fn staged(fixtures: &[(String, String)], body: &str) -> String {
    let names: Vec<&str> = fixtures.iter().map(|(n, _)| n.as_str()).collect();
    let exprs: Vec<&str> = fixtures.iter().map(|(_, e)| e.as_str()).collect();
    let pat = if names.is_empty() { "()".to_string() } else { format!("({},)", names.join(", ")) };
    let tup = if exprs.is_empty() { "()".to_string() } else { format!("({},)", exprs.join(", ")) };
    format!("{{ let {pat} = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {{ {tup} }})) {{ Ok(v) => v, Err(e) => return crate_oracle::Staged::SetupFailed(crate_oracle::panic_text(e)) }}; crate_oracle::Staged::Ran({body}) }}")
}

fn emit_oracle(world: &World, entries: &mut [Entry], inv: &Inventory) -> (String, String, Vec<(String, String)>, serde_json::Value) {
    let mut debuggable: BTreeSet<String> = inv.supporting.iter().filter(|s| s.derived.iter().any(|d| d == "Debug")).map(|s| s.canonical_path.clone()).collect();
    let mut defaultable: BTreeSet<String> = inv.supporting.iter().filter(|s| s.derived.iter().any(|d| d == "Default")).map(|s| s.canonical_path.clone()).collect();
    for c in &inv.callables {
        if c.kind == "foreign_trait_impl" {
            if c.name.starts_with("Debug") { debuggable.insert(c.owner.clone()); }
            if c.name.starts_with("Default") { defaultable.insert(c.owner.clone()); }
        }
    }
    for (path, w) in &world.wrappers {
        if w.base.is_some() {
            if world.trait_holds(&w.identity, "Debug", 0) { debuggable.insert(path.clone()); }
            if world.trait_holds(&w.identity, "Default", 0) { defaultable.insert(path.clone()); }
        }
    }
    let default_bound: BTreeSet<String> = entries
        .iter()
        .filter(|e| e.status == "generated")
        .filter_map(|e| e.canonical_path.strip_suffix(" as core::default::Default").map(|s| s.to_string()))
        .collect();
    let mut o = Oracle { world, debuggable, defaultable, default_bound, shown: std::cell::RefCell::new(BTreeSet::new()), recipes: BTreeMap::new(), no_recipe: BTreeMap::new() };
    o.derive_recipes(entries);
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
        if let Some(x) = world.release.excluded_oracle.iter().find(|x| x.path == e.canonical_path) {
            skip(e, format!("excluded: nondeterministic oracle ({})", x.reason));
            continue;
        }
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
            let one = |e: &str| vec![("__recv".to_string(), e.to_string())];
            let (script, fmt, oracle) = match tname {
                "Display" => (
                    format!("{} pub fn main(__fx) {{ let a = __fx[0]; (`${{a}}`, ()) }}", setup_fn(&[recv_rune.clone()])),
                    plain_side("rune::from_value::<String>(v).map_err(|e| e.to_string())"),
                    staged(&one(&recv_rust), "crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"{}\", __recv)))"),
                ),
                "PartialEq" => (
                    format!("{} pub fn main(__fx) {{ let a = __fx[0]; let b = __fx[1]; (a == b, ()) }}", setup_fn(&[recv_rune.clone(), recv_rune.clone()])),
                    plain_side("rune::from_value::<bool>(v).map(|x| format!(\"{x}\")).map_err(|e| e.to_string())"),
                    staged(&[("__recv".to_string(), recv_rust.clone()), ("__recv2".to_string(), recv_rust.clone())], "crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"{}\", __recv == __recv2)))"),
                ),
                "Default" => {
                    let c = owner.unwrap();
                    let Some(show) = o.show(c) else { skip(e, "receiver type has no comparison".into()); continue };
                    let w = &world.wrappers[c];
                    o.shown.borrow_mut().insert(c.to_string());
                    // the constructor under test is not a fixture: setup is empty
                    (
                        format!("{} pub fn main(__fx) {{ let a = {}::default_(); (a, ()) }}", setup_fn(&[]), rune_path(w)),
                        wrapped_side(&format!("rnx_polars::generated::fixtures::show_{}", w.rust.to_lowercase()), false),
                        staged(&[], &format!("crate_oracle::Side::Value({{ let __o = <{}>::default(); let v = &__o; {show} }})", w.spell)),
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
                        (format!("{} pub fn main(__fx) {{ let a = __fx[0]; (-a, ()) }}", setup_fn(&[recv_rune.clone()])), plain_side(&sf), staged(&one(&recv_rust), &format!("{{ let __r = -__recv; crate_oracle::Side::Value({of}) }}")))
                    } else {
                        let Some((shape, canonical)) = info.params.first() else { skip(e, "operator without rhs".into()); continue };
                        let _ = shape;
                        let rhs_t = ty::parse(canonical);
                        let rhs_rune = match &rhs_t { Ty::Path { path, .. } => o.rune_wrapped(path), Ty::Ref { inner, .. } => match &**inner { Ty::Path { path, .. } => o.rune_wrapped(path), _ => None }, _ => None };
                        let Some(rhs_rune) = rhs_rune else { skip(e, "no fixture for the operand".into()); continue };
                        let Some(rhs_rust) = o.rust_value(&rhs_t, &BTreeMap::new(), owner, 0) else { skip(e, "no Rust fixture for the operand".into()); continue };
                        (format!("{} pub fn main(__fx) {{ let a = __fx[0]; let b = __fx[1]; (a {op} b, ()) }}", setup_fn(&[recv_rune.clone(), rhs_rune.clone()])), plain_side(&sf), staged(&[("__recv".to_string(), recv_rust.clone()), ("__rhs".to_string(), rhs_rust.clone())], &format!("{{ let __r = __recv {op} __rhs; crate_oracle::Side::Value({of}) }}")))
                    }
                }
                _ => { skip(e, format!("protocol {tname} has no script-level trigger")); continue }
            };
            e.execution = Some(format!("case{}", o.recipe_note(&e.canonical_path, &[script.as_str()])));
            cases.push(OracleCase { id, path: e.canonical_path.clone(), script, has_receiver: false, fmt, oracle, unordered: false, policy: "ordered (protocol)".into() });
            continue;
        }
        // receiver: for a trait method, the first generated implementor with a fixture
        let mut info = info;
        if !info.implementors.is_empty() {
            if let Some((c, w)) = info.implementors.iter().find(|(c, _)| o.rune_wrapped(c).is_some() && o.rust_wrapped(c).is_some()).cloned() {
                info.callee = info.callee.replacen(&format!("<{}", world.wrappers[&info.owner.as_ref().unwrap().0].spell), &format!("<{}", world.wrappers[&c].spell), 1);
                info.rune_owner = Some(rune_path(&world.wrappers[&c]));
                info.owner = Some((c, w));
            }
        }
        let owner = info.owner.as_ref().map(|(c, _)| c.as_str());
        let recv_rune = match owner { Some(c) => o.rune_wrapped(c), None => None };
        if info.receiver != "none" && recv_rune.is_none() {
            skip(e, format!("no fixture for the receiver type ({})", owner.and_then(|c| o.no_recipe.get(c)).cloned().unwrap_or_else(|| "no wrapper".into())));
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
        let named: Vec<(String, String)> = info.param_names.iter().cloned().zip(rune_args.iter().cloned()).collect();
        let (unordered, policy) = world.release.policy(&e.canonical_path, &named);
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
        // Rune: the prepared fixtures arrive in `__fx`; a two-call case gets
        // a second, separately prepared argument set for the second call
        let n = rune_args.len();
        let recv_in_fx = info.receiver != "none";
        let off = if recv_in_fx { 1 } else { 0 };
        let args_r = (0..n).map(|i| format!("__fx[{}]", off + i)).collect::<Vec<_>>().join(", ");
        let args_r2 = (0..n).map(|i| format!("__fx[{}]", off + n + i)).collect::<Vec<_>>().join(", ");
        let mut setup_rune: Vec<String> = Vec::new();
        if recv_in_fx { setup_rune.push(recv_rune.clone().unwrap()); }
        setup_rune.extend(rune_args.iter().cloned());
        // Rust: the same fixtures, named, built in the staged setup
        let mut fixtures: Vec<(String, String)> = Vec::new();
        let mut passed = Vec::new();
        for (i, a) in rust_args.iter().enumerate() {
            fixtures.push((format!("__a{i}"), a.clone()));
            passed.push(format!("__a{i}"));
        }
        let args_s = passed.join(", ");
        let (script, fmt, oracle) = if mutating {
            let c = owner.unwrap();
            let Some(show) = o.show(c) else { skip(e, "receiver type has no comparison".into()); continue };
            let w = &world.wrappers[c];
            o.shown.borrow_mut().insert(c.to_string());
            // script: (receiver after the call, return); both compared
            let script = format!("{} pub fn main(__fx) {{ let a = __fx[0]; let r = a.{}({args_r}); ((a, r), ()) }}", setup_fn(&setup_rune), info.rune_name);
            // receiver state after the call and the return are compared together;
            // an error return keeps the receiver in the comparison as `ret=ERR:kind`
            let fmt = format!("{{ let (a, r) = match rune::from_value::<(rune::Value, rune::Value)>(v) {{ Ok(x) => x, Err(e) => return crate_oracle::Side::Broken(e.to_string()) }}; let recv = match rnx_polars::generated::fixtures::show_{}(&a) {{ Ok(s) => s.to_text(), Err(e) => return crate_oracle::Side::Broken(e) }}; let ret = {{ let v = r; {} }}; match ret {{ crate_oracle::Side::Value(s) => crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"recv={{recv}};ret={{}}\", s.to_text()))), crate_oracle::Side::Error(k) => crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"recv={{recv}};ret=ERR:{{k}}\"))), other => other }} }}", w.rust.to_lowercase(), if info.fallible { value_side(&ret_fmt_script) } else { plain_side(&ret_fmt_script) });
            let mut fx = vec![("__o".to_string(), recv_rust.clone().unwrap())];
            fx.extend(fixtures.iter().cloned());
            let oracle = staged(&fx, &format!("{{ let mut __o = __o; let __r = {}(&mut __o, {args_s}); let ret = {}; let recv = ({{ let v = &__o; {show} }}).to_text(); let ret = ret.replace(\"<<ERR:\", \"ERR:\").replace(\">>\", \"\"); crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"recv={{recv}};ret={{ret}}\"))) }}", info.callee, ret_fmt_rust).replace(", )", ")"));
            (script, fmt, oracle)
        } else {
            let script = match (&info.rune_owner, info.receiver.as_str()) {
                (Some(_), "none") => format!("{} pub fn main(__fx) {{ let r = {}::{}({args_r}); (r, ()) }}", setup_fn(&setup_rune), info.rune_owner.as_ref().unwrap(), info.rune_name),
                (Some(_), _) => {
                    let mut twice = setup_rune.clone();
                    twice.extend(rune_args.iter().cloned());
                    format!("{} pub fn main(__fx) {{ let a = __fx[0]; let r = a.{}({args_r}); let r2 = a.{}({args_r2}); (r, r2) }}", setup_fn(&twice), info.rune_name, info.rune_name)
                }
                (None, _) => format!("{} pub fn main(__fx) {{ let r = polars::{}({args_r}); (r, ()) }}", setup_fn(&setup_rune), info.rune_name),
            };
            let fmt = match &top {
                Some((_, _)) => wrapped_side(&ret_fmt_script, info.fallible),
                None => if info.fallible { value_side(&ret_fmt_script) } else { plain_side(&ret_fmt_script) },
            };
            let mut fx: Vec<(String, String)> = Vec::new();
            if info.receiver != "none" { fx.push(("__recv".to_string(), recv_rust.clone().unwrap())); }
            fx.extend(fixtures.iter().cloned());
            let call = match info.receiver.as_str() {
                "none" => format!("let __r = {}({args_s});", info.callee),
                "self" => format!("let __r = {}(__recv, {args_s});", info.callee),
                "&self" => format!("let __r = {}(&__recv, {args_s});", info.callee),
                _ => { skip(e, "receiver form".into()); continue }
            }.replace(", )", ")");
            let oracle_body = match &top {
                Some((_, true)) => format!("match __r {{ Ok(__r) => crate_oracle::Side::Value({{ let v = &__r; {ret_fmt_rust} }}), Err(e) => crate_oracle::Side::Error(crate_oracle::error_kind(&e)) }}"),
                Some((_, false)) => format!("crate_oracle::Side::Value({{ let v = &__r; {ret_fmt_rust} }})"),
                None => format!("crate_oracle::collapse({ret_fmt_rust})"),
            };
            (script, fmt, staged(&fx, &format!("{{ {call} {oracle_body} }}")))
        };
        e.execution = Some(format!("case{}", o.recipe_note(&e.canonical_path, &[script.as_str()])));
        cases.push(OracleCase { id, path: e.canonical_path.clone(), script, has_receiver: !mutating && (info.receiver == "self" || info.receiver == "&self"), fmt, oracle, unordered, policy });
    }
    // fixtures module inside the adapter
    let mut fixtures = String::from("//! GENERATED by tools/polars-gen: fixtures for the generated oracle tests.\n//! Only built with the `test-support` feature.\n#![allow(dead_code, non_snake_case, unused_imports, clippy::all)]\nuse super::types::*;\nuse crate::oracle as crate_oracle;\nuse crate::{DataFrame, Expr, LazyFrame, LazyGroupBy};\nuse polars::prelude as p;\nuse polars::prelude::{IntoColumn, IntoLazy};\nuse rnx::rune;\nuse values::*;\n\n/// The Polars values the oracle tests use on both sides.\npub mod values {\n    use polars::prelude as p;\n    use polars::prelude::*;\n");
    for (_, name, expr, _) in FIXTURES {
        let ret = match *name { "df" => "p::DataFrame", "lf" => "p::LazyFrame", "expr" => "p::Expr", "series" => "p::Series", "column" => "p::Column", "dtype" => "p::DataType", "field" => "p::Field", "group_by" => "p::LazyGroupBy", _ => unreachable!() };
        writeln!(fixtures, "    pub fn {name}() -> {ret} {{ {expr} }}").unwrap();
    }
    fixtures.push_str("}\n\n");
    for (canonical, name, _, _) in FIXTURES {
        let w = &world.wrappers[*canonical];
        writeln!(fixtures, "#[rune::function(path = {name})]\nfn fx_{name}() -> {} {{ {}(values::{name}()) }}", w.rust, w.rust).unwrap();
    }
    let mut shown_structs: BTreeSet<String> = BTreeSet::new();
    for canonical in o.shown.borrow().iter() {
        let w = &world.wrappers[canonical];
        if !shown_structs.insert(w.rust.clone()) {
            continue; // one show function per wrapper struct, however many aliases share it
        }
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
#![allow(non_snake_case, unused_imports, unused_variables, unused_mut, clippy::all)]
use polars::prelude as p;
use polars::prelude::*;
use rnx::rune::{self, Context, Module, Source, Sources, Value, Vm};
use rnx_polars::generated::fixtures::values::*;
use rnx_polars::oracle as crate_oracle;
use rnx_polars::oracle::{Outcome, Setup, Side, Staged};
use std::sync::Arc;

struct Case {
    id: &'static str,
    path: &'static str,
    /// `pub fn setup()` builds every fixture the measured call needs and
    /// nothing else, as a vector; `pub fn main(__fx)` makes the measured
    /// call with those prepared values and constructs nothing.
    script: &'static str,
    has_receiver: bool,
    /// Rows compared as a set: only for a listed operation under its
    /// listed options; `policy` records the justification.
    unordered: bool,
    policy: &'static str,
    fmt: fn(Value) -> Side,
    /// One Rust run: its own staged setup, then the measured call.
    oracle: fn() -> Staged,
}

"#);
    for c in &cases {
        writeln!(t, "fn fmt_{}(v: Value) -> Side {{ {} }}\nfn oracle_{}() -> Staged {{ {} }}", c.id, c.fmt, c.id, c.oracle).unwrap();
    }
    t.push_str("\nstatic CASES: &[Case] = &[\n");
    for c in &cases {
        writeln!(t, "    Case {{ id: {:?}, path: {:?}, script: {:?}, has_receiver: {}, unordered: {}, policy: {:?}, fmt: fmt_{}, oracle: oracle_{} }},", c.id, c.path, c.script, c.has_receiver, c.unordered, c.policy, c.id, c.id).unwrap();
    }
    t.push_str("];\n");
    t.push_str(r#"
/// One Rust oracle run: a panic outside the staged setup is a target panic.
fn run_staged(f: fn() -> Staged) -> Staged {
    match std::panic::catch_unwind(f) {
        Ok(s) => s,
        Err(e) => Staged::Ran(Side::Panic(crate_oracle::panic_text(e))),
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

    /// Run one case the way the main test does: compile the script; run
    /// the script's setup and the first Rust run's staged setup; only when
    /// both succeed, pass the prepared values into the measured call, run
    /// the oracle a second time (with its own staged setup), and classify
    /// under the case's policy.
    fn run(&self, case: &Case) -> (Outcome, String) {
        let unrun = || Side::Broken("target not run".to_string());
        let policy = if case.unordered { crate_oracle::UNORDERED } else { crate_oracle::ORDERED };
        let mut sources = Sources::new();
        sources.insert(Source::memory(case.script).unwrap()).unwrap();
        let unit = match rune::prepare(&mut sources).with_context(&self.context).build() {
            Ok(u) => Arc::new(u),
            Err(e) => return crate_oracle::classify(policy, &Setup::NotRun, &Setup::NotRun, &Side::Broken(format!("compile: {e}")), None, &unrun(), &unrun()),
        };
        let mut vm = Vm::new(self.runtime.clone(), unit);
        let (rune_setup, prepared) = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| vm.call(["setup"], ()).map_err(|e| format!("vm: {e}")))) {
            Ok(Ok(v)) => (Setup::Ok, Some(v)),
            Ok(Err(e)) => (Setup::Failed(e), None),
            Err(e) => (Setup::Failed(crate_oracle::panic_text(e)), None),
        };
        let run1 = run_staged(case.oracle);
        let (rust_setup, oracle) = match run1 {
            Staged::SetupFailed(e) => (Setup::Failed(e), unrun()),
            Staged::Ran(side) => (Setup::Ok, side),
        };
        if rune_setup != Setup::Ok || rust_setup != Setup::Ok {
            return crate_oracle::classify(policy, &rune_setup, &rust_setup, &unrun(), None, &unrun(), &unrun());
        }
        let prepared = prepared.unwrap();
        let (first, second) = {
            let run = move || -> (Side, Option<Side>) {
                let out = match vm.call(["main"], (prepared,)) { Ok(o) => o, Err(e) => return (Side::Broken(format!("vm: {e}")), None) };
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
        // the second run has its own staged setup; a setup failure there is
        // a disagreement with the first run, never a setup skip
        let again = match run_staged(case.oracle) {
            Staged::SetupFailed(e) => Side::Broken(format!("second run setup failed: {e}")),
            Staged::Ran(side) => side,
        };
        crate_oracle::classify(policy, &rune_setup, &rust_setup, &first, second.as_ref(), &oracle, &again)
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
        if !outcome.approved() && !outcome.setup_failed() {
            bad.push(format!("{}: {}: {}", case.path, outcome.name(), detail.chars().take(300).collect::<String>()));
        }
        results.push(serde_json::json!({"id": case.id, "path": case.path, "status": outcome.name(), "unordered": case.unordered, "policy": case.policy, "detail": detail.chars().take(400).collect::<String>()}));
    }
    std::panic::set_hook(previous);
    let verified = results.iter().filter(|r| ["match", "row_order_differs", "both_error", "both_panic"].contains(&r["status"].as_str().unwrap())).count();
    let out = serde_json::json!({"cases": CASES.len(), "verified": verified, "tally": tally, "results": results});
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
    fn ran(f: fn() -> Side) -> Staged { Staged::Ran(f()) }
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
        ("unreversed frame against Rust reverse", Case { id: "c1", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { let a = fx::df(); (a, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: df_side, oracle: || ran(reversed) }, &[Outcome::Mismatch]),
        ("unreversed frame is still wrong under an unordered policy when a cell differs", Case { id: "c1b", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { let a = fx::df(); (a, ()) }", has_receiver: false, unordered: true, policy: "control", fmt: df_side, oracle: || Staged::Ran(Side::Value(crate_oracle::frame_repr(&polars::df!("x" => [1i64, 2, 4], "y" => ["a", "b", "c"], "z" => [1.5f64, 2.5, 3.5]).unwrap()))) }, &[Outcome::Mismatch]),
        ("wrong second call on the same receiver", Case { id: "c2", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { let a = fx::df(); (a.reverse(), a) }", has_receiver: true, unordered: false, policy: "control", fmt: df_side, oracle: || ran(reversed) }, &[Outcome::ReuseFailed]),
        ("wrong second order under the default policy", Case { id: "c2b", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { let a = fx::df(); (a, a.reverse()) }", has_receiver: true, unordered: false, policy: "control", fmt: df_side, oracle: || ran(same) }, &[Outcome::ReuseFailed]),
        ("compile error against a nondeterministic oracle", Case { id: "c3", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { let a = ; }", has_receiver: false, unordered: false, policy: "control", fmt: df_side, oracle: || ran(alternating) }, &[Outcome::Broken]),
        ("a script panic (a VM error) against a Rust panic", Case { id: "c4", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { panic(\"unrelated\") }", has_receiver: false, unordered: false, policy: "control", fmt: df_side, oracle: || ran(panics) }, &[Outcome::Broken]),
        ("a binding that panics in Polars against a Rust panic with another message", Case { id: "c4b", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { let a = fx::column(); (a.product(), ()) }", has_receiver: false, unordered: false, policy: "control", fmt: df_side, oracle: || panic!("a different panic") }, &[Outcome::PanicMismatch]),
        ("a value against a panicking oracle", Case { id: "c5", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { let a = fx::df(); (a, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: df_side, oracle: || ran(panics) }, &[Outcome::OraclePanicked]),
        ("a wrong value against a nondeterministic oracle", Case { id: "c6", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { let a = fx::df(); (a, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: df_side, oracle: || ran(alternating) }, &[Outcome::Nondeterministic]),
        ("a matching first run whose second run changes value", Case { id: "c7", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { let a = fx::df(); (a, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: df_side, oracle: || ran(first_then_reversed) }, &[Outcome::Nondeterministic]),
        ("a matching first run whose second run errors", Case { id: "c7b", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { let a = fx::df(); (a, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: df_side, oracle: || ran(first_then_error) }, &[Outcome::Nondeterministic]),
        ("a matching first run whose second run panics", Case { id: "c7c", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { let a = fx::df(); (a, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: df_side, oracle: || ran(first_then_panic) }, &[Outcome::Nondeterministic]),
    ];
    // join order: permuting rows passes only for a justified unordered join
    // case; an explicit order on the same operation, and an unrelated
    // ordered operation, must fail
    fn lf_side(v: Value) -> Side { rnx_polars::generated::fixtures::show_lazyframe(&v).map(Side::Value).unwrap_or_else(Side::Broken) }
    fn joined_reversed() -> Side {
        let d = lf().inner_join(lf(), expr(), expr()).collect().unwrap().reverse();
        Side::Value(crate_oracle::frame_repr(&d).prefixed("collected "))
    }
    // `LazyFrame::join` returns a `LazyFrame` on 0.55.2 and a `PolarsResult`
    // at rc2; the control accepts either so it compiles on every release.
    trait IntoLazy { fn into_lazy(self) -> p::LazyFrame; }
    impl IntoLazy for p::LazyFrame { fn into_lazy(self) -> p::LazyFrame { self } }
    impl IntoLazy for p::PolarsResult<p::LazyFrame> { fn into_lazy(self) -> p::LazyFrame { self.unwrap() } }
    fn ordered_join_reversed() -> Side {
        let mut args = p::JoinArgs::new(p::JoinType::Inner); args.maintain_order = p::MaintainOrderJoin::Left;
        let d = lf().join(lf(), [expr()], [expr()], args).into_lazy().collect().unwrap().reverse();
        Side::Value(crate_oracle::frame_repr(&d).prefixed("collected "))
    }
    // The order policy of each join control is the production decision
    // (`Release::policy` on the control's parameters), substituted by the
    // generator; j2 and j4 are asserted ordered at generation time.
    const J1_UNORDERED: bool = @J1@;
    let (j1_expected, j1_pass): (&[Outcome], bool) = if J1_UNORDERED { (&[Outcome::RowOrderDiffers, Outcome::Match], true) } else { (&[Outcome::Mismatch, Outcome::Nondeterministic], false) };
    fn none_order_join_reversed() -> Side {
        let mut args = p::JoinArgs::new(p::JoinType::Inner); args.maintain_order = p::MaintainOrderJoin::None;
        let d = lf().join(lf(), [expr()], [expr()], args).into_lazy().collect().unwrap().reverse();
        Side::Value(crate_oracle::frame_repr(&d).prefixed("collected "))
    }
    let join_controls: &[(&str, Case, &[Outcome], bool)] = &[
        ("permuted rows pass for a justified unordered join iff the policy says unordered", Case { id: "j1", path: "control", script: "pub fn setup() { [fx::lf(), fx::lf(), fx::expr(), fx::expr()] } pub fn main(__fx) { let a = __fx[0]; (a.inner_join(__fx[1], __fx[2], __fx[3]), ()) }", has_receiver: false, unordered: J1_UNORDERED, policy: "@J1_POLICY@", fmt: lf_side, oracle: || ran(joined_reversed) }, j1_expected, j1_pass),
        ("permuted rows fail for the same join with an explicit order", Case { id: "j2", path: "control", script: "pub fn setup() { [fx::lf(), fx::lf(), polars::JoinArgs::default_().with_maintain_order(polars::MaintainOrderJoin::Left())] } pub fn main(__fx) { let a = __fx[0]; let args = __fx[2]; let j = match a.join(__fx[1], [fx::expr()], [fx::expr()], args) { Ok(v) => v, Err(e) => panic(`join: ${e}`) }; (j, ()) }", has_receiver: false, unordered: @J2@, policy: "@J2_POLICY@", fmt: lf_side, oracle: || ran(ordered_join_reversed) }, &[Outcome::Mismatch], false),
        ("permuted rows fail for an unrelated ordered operation", Case { id: "j3", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { let a = fx::df(); (a, ()) }", has_receiver: false, unordered: false, policy: "ordered", fmt: df_side, oracle: || ran(reversed) }, &[Outcome::Mismatch], false),
        ("permuted rows fail for the same join with an unrecognized configuration", Case { id: "j4", path: "control", script: "pub fn setup() { [fx::lf(), fx::lf(), polars::JoinArgs::default_().with_maintain_order(polars::MaintainOrderJoin::None())] } pub fn main(__fx) { let a = __fx[0]; let args = __fx[2]; let j = match a.join(__fx[1], [fx::expr()], [fx::expr()], args) { Ok(v) => v, Err(e) => panic(`join: ${e}`) }; (j, ()) }", has_receiver: false, unordered: @J4@, policy: "@J4_POLICY@", fmt: lf_side, oracle: || ran(none_order_join_reversed) }, &[Outcome::Mismatch, Outcome::Nondeterministic], false),
    ];
    // setup failures are structural: a failing setup stage on both sides
    // credits nothing and calls the target zero times, whatever the
    // message; on one side it is a failure; a compile error is judged
    // before any setup; a target panic that says "fixture:" is a panic
    let engine_started = |h: &Harness| -> i64 {
        let script = "pub fn main() { (polars::engine_counts().0, ()) }";
        let mut sources = Sources::new();
        sources.insert(Source::memory(script).unwrap()).unwrap();
        let unit = rune::prepare(&mut sources).with_context(&h.context).build().unwrap();
        let mut vm = Vm::new(h.runtime.clone(), Arc::new(unit));
        rune::from_value::<(i64, rune::Value)>(vm.call(["main"], ()).unwrap()).unwrap().0
    };
    fn setup_fails() -> Staged { Staged::SetupFailed("control setup failed".into()) }
    fn setup_refused() -> Staged { match std::panic::catch_unwind(|| { let _: usize = usize::try_from(-1i64).unwrap_or_else(|_| panic!("fixture: narrow failed")); }) { Ok(()) => Staged::Ran(same()), Err(e) => Staged::SetupFailed(crate_oracle::panic_text(e)) } }
    fn later_setup_fails() -> Staged { match std::panic::catch_unwind(|| { let _ = df(); panic!("second construction failed") }) { Ok(()) => Staged::Ran(same()), Err(e) => Staged::SetupFailed(crate_oracle::panic_text(e)) } }
    fn fixture_worded_panic() -> Side { panic!("fixture: a target panic that mentions the word") }
    let zero_call_controls: &[(&str, Case)] = &[
        ("untagged setup panic on both sides", Case { id: "s1", path: "control", script: "pub fn setup() { panic(\"control setup failed\") } pub fn main(__fx) { let a = fx::df(); (a.height(), ()) }", has_receiver: false, unordered: false, policy: "control", fmt: df_side, oracle: setup_fails }),
        ("a constructor returning an error, unwrapped in setup, on both sides", Case { id: "s2", path: "control", script: "pub fn setup() { [match polars::BooleanChunkedBuilder::new(\"x\", -1) { Ok(v) => v, Err(e) => panic(`fixture: ${e}`) }] } pub fn main(__fx) { let a = fx::df(); (a.height(), ()) }", has_receiver: false, unordered: false, policy: "control", fmt: df_side, oracle: setup_refused }),
        ("a first fixture that succeeds, then a later fixture that fails, on both sides", Case { id: "s8", path: "control", script: "pub fn setup() { let a = fx::df(); let b = panic(\"second construction failed\"); [a, b] } pub fn main(__fx) { let a = __fx[0]; (a.height(), ()) }", has_receiver: false, unordered: false, policy: "control", fmt: df_side, oracle: later_setup_fails }),
    ];
    let mut failures = Vec::new();
    for (label, case) in zero_call_controls {
        let before = engine_started(&h);
        let (outcome, detail) = h.run(case);
        let after = engine_started(&h);
        if outcome != Outcome::FixtureFailed || outcome.approved() || after != before {
            failures.push(format!("{label}: got {} ({detail}); engine threads started {before} -> {after}", outcome.name()));
        }
    }
    let one_sided_controls: &[(&str, Case, Outcome)] = &[
        ("rune setup fails, rust setup succeeds", Case { id: "s3", path: "control", script: "pub fn setup() { panic(\"only here\") } pub fn main(__fx) { let a = fx::df(); (a, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: df_side, oracle: || ran(same) }, Outcome::SetupMismatch),
        ("rust setup fails, rune setup succeeds", Case { id: "s4", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { let a = fx::df(); (a, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: df_side, oracle: setup_fails }, Outcome::SetupMismatch),
        ("a script that does not compile while rust setup fails", Case { id: "s5", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { let a = ; }", has_receiver: false, unordered: false, policy: "control", fmt: df_side, oracle: setup_fails }, Outcome::Broken),
        ("a wrong value while the second rust run fails with a fixture-worded panic", Case { id: "s6", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { let a = fx::df(); (a.reverse(), ()) }", has_receiver: false, unordered: false, policy: "control", fmt: df_side, oracle: || if nth() == 0 { Staged::Ran(same()) } else { panic!("fixture: second run") } }, Outcome::Nondeterministic),
        ("a target panic that mentions fixture: is a target panic", Case { id: "s7", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { let a = fx::df(); (a, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: df_side, oracle: || ran(fixture_worded_panic) }, Outcome::OraclePanicked),
        ("a first rust run whose setup succeeds, then a second run whose setup fails", Case { id: "s9", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { let a = fx::df(); (a, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: df_side, oracle: || if nth() == 0 { Staged::Ran(same()) } else { Staged::SetupFailed("second run setup".into()) } }, Outcome::Nondeterministic),
    ];
    for (label, case, expected) in one_sided_controls {
        let (outcome, detail) = h.run(case);
        if outcome != *expected || outcome.approved() || outcome.setup_failed() {
            failures.push(format!("{label}: got {} ({detail}), expected {}", outcome.name(), expected.name()));
        }
    }
    for (label, case, expected, passes) in join_controls {
        let (outcome, detail) = h.run(case);
        let ok = expected.contains(&outcome) && (*passes == outcome.approved());
        if !ok { failures.push(format!("{label}: got {} ({detail})", outcome.name())); }
    }
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
    // the join controls' order policies come from the production rule
    let named = |v: &[(&str, &str)]| v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
    let base = [("other", "fx::lf()"), ("left_on", "[fx::expr()]"), ("right_on", "[fx::expr()]")];
    let j1 = world.release.policy("polars_lazy::frame::LazyFrame::inner_join", &named(&[("other", "fx::lf()"), ("left_on", "fx::expr()"), ("right_on", "fx::expr()")]));
    let mut with_left = base.to_vec(); with_left.push(("args", "polars::JoinArgs::default_().with_maintain_order(polars::MaintainOrderJoin::Left())"));
    let j2 = world.release.policy("polars_lazy::frame::LazyFrame::join", &named(&with_left));
    let mut with_none = base.to_vec(); with_none.push(("args", "polars::JoinArgs::default_().with_maintain_order(polars::MaintainOrderJoin::None())"));
    let j4 = world.release.policy("polars_lazy::frame::LazyFrame::join", &named(&with_none));
    assert!(!j2.0, "the release policy must keep an explicit order ordered: {}", j2.1);
    assert!(!j4.0, "the release policy must keep an unrecognized configuration ordered: {}", j4.1);
    println!("join controls: j1 {} | j2 {} | j4 {}", j1.1, j2.1, j4.1);
    let t = t.replace("@J1@", &j1.0.to_string()).replace("@J1_POLICY@", &j1.1.replace('"', "'")).replace("@J2@", &j2.0.to_string()).replace("@J2_POLICY@", &j2.1.replace('"', "'")).replace("@J4@", &j4.0.to_string()).replace("@J4_POLICY@", &j4.1.replace('"', "'"));
    let recipes = serde_json::json!({
        "derived": o.recipes.iter().map(|(c, r)| serde_json::json!({"type": c, "recipe": r.kind, "rune": r.rune, "rust": r.rust})).collect::<Vec<_>>(),
        "none": o.no_recipe.iter().map(|(c, why)| serde_json::json!({"type": c, "reason": why})).collect::<Vec<_>>(),
    });
    (fixtures, t, skipped, recipes)
}
