//! Rule-based classification of callables into binding buckets.
//!
//! The rules are judgments written down; the program applies them so the
//! result is reproducible. Counts they produce are predictions, never
//! demonstrations.
use rustdoc_types::{GenericArg, GenericArgs, GenericBound, Type};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::inventory::{Callable, CallableKind, Docs, Inventory, Resolved, Supporting};
use rustdoc_types::ItemEnum;
use crate::render;

/// Rule id and the shape it matches. Every classification cites the rules it
/// used, and the evidence lists this table with per-rule hit counts.
pub const RULES: &[(&str, &str)] = &[
    ("X1", "unsafe fn -> unsupported"),
    ("X2", "#[doc(hidden)] on every path -> unsupported"),
    ("X3", "raw pointer, extern type or C-variadic anywhere -> unsupported"),
    ("X5", "name starts with `_` (public by necessity, internal by convention) -> unsupported"),
    ("X4", "trait method whose trait is not reachable by public path -> generic (needs the trait in scope)"),
    ("R1", "receiver none, self, &self or &mut self -> mechanical"),
    ("R2", "receiver of another shape (Box<Self>, Pin<..>) -> generic"),
    ("P1", "scalar: bool, integers, floats, usize/isize, char, String, &str, PlSmallStr -> mechanical"),
    ("P2", "reachable polars struct with no public fields, or enum with only unit variants, by value or reference -> mechanical (wrapped value); a generic type used with concrete arguments is one such value per instantiation"),
    ("P3", "Option<T>, Vec<T>, &[T], (T, U), Arc<T>, Box<T>, Cow<T>, &T of a mechanical T -> conversion"),
    ("P4", "impl Into<T>, impl AsRef<str>, impl IntoVec<T>, impl IntoIterator<Item = T>, or a generic bounded so, with T mechanical or conversion -> conversion"),
    ("P5", "reachable polars struct with public fields, or enum with data-carrying variants -> option struct"),
    ("P6", "closure: impl Fn/FnMut/FnOnce, generic bounded by Fn*, Box<dyn Fn*>, Arc<dyn ..Udf..> or a named *Udf type -> callback"),
    ("P7", "generic parameter or impl Trait with any other bound, dyn Trait, associated type projection -> generic"),
    ("P8", "foreign type from an undocumented crate (chrono, arrow2, bytes, ...) that is not a std scalar or container -> generic"),
    ("P9", "polars type public in its crate but not reachable from the root -> generic"),
    ("L1", "type carries a lifetime parameter (AnyValue<'a>) -> generic"),
    ("U1", "type identity could not be resolved in the documentation set -> unknown (reported, not eligible)"),
    ("O1", "owner type has type parameters and no concrete reachable alias -> generic"),
    ("O2", "owner type has type parameters but is reached through concrete aliases (Int64Chunked = ChunkedArray<Int64Type>) -> generic, instantiable per alias"),
    ("T1", "return Self, unit, scalar, wrapped value, PolarsResult<T>/Result<T, PolarsError>/Option<T> of those -> mechanical"),
    ("T2", "return &T or &mut T of a wrapped value (cloned on the way out), or a container of mechanical returns -> conversion"),
    ("T3", "return impl Trait, generic T, iterator, reference with non-static lifetime into a non-polars type -> generic"),
    ("F0", "#[derive]d impls are inventoried on the same terms as hand-written ones and flagged derived"),
    ("F2", "protocol impls (Display, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default) -> mechanical (Rune protocol)"),
    ("F3", "From/TryFrom impls classified by their source argument -> conversion at least"),
    ("F1", "foreign trait impl (operators, From/Into, Display, Iterator, ...) classified by its first method's signature; Iterator and Deref impls -> generic"),
];

#[derive(Serialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Bucket {
    Mechanical = 1,
    Conversion = 2,
    OptionStruct = 3,
    Callback = 4,
    Generic = 5,
    Unsupported = 6,
    /// A type in the signature could not be resolved: reported, not eligible.
    Unknown = 7,
}

#[derive(Serialize, Clone)]
pub struct Classified {
    #[serde(flatten)]
    pub callable: Callable,
    pub bucket: Bucket,
    pub rules: Vec<String>,
    /// The part that set the bucket: "param x", "return", "receiver", "owner".
    pub decided_by: String,
    /// Shape signature used for deterministic sampling.
    pub shape: String,
}

#[derive(Serialize)]
pub struct ClassifiedInventory {
    pub root: String,
    pub callables: Vec<Classified>,
    pub supporting: Vec<Supporting>,
    pub unknown: Vec<String>,
    pub foreign_reexports: Vec<String>,
    pub derived_impls: usize,
    pub gross: BTreeMap<String, crate::inventory::GrossCounts>,
    pub rules: Vec<(String, String)>,
}

pub struct Ctx<'a> {
    docs: &'a Docs,
    /// "crate:id" -> supporting item, for identity-based lookup.
    by_key: HashMap<String, &'a Supporting>,
}

/// What a `ResolvedPath` in type position refers to, by identity.
enum Res<'a> {
    /// A reachable polars type (struct, enum, union, trait, alias).
    Item(&'a Supporting),
    /// A type alias: its target, the crate the target is spelled in, and the
    /// alias parameters bound to the caller's argument types (each with the
    /// crate its ids belong to).
    Alias(Type, String, Env),
    /// A type from a crate that was not documented on purpose (std, chrono, ...).
    Foreign(String),
    /// A polars type that is public in its crate but not reachable from the root.
    Unreachable(String),
    /// Could not be resolved: a gap in the documentation set.
    Unknown(String),
}

const SCALARS: &[&str] = &[
    "bool", "i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128", "usize", "isize", "f32", "f64", "char", "str",
];

/// Bounds that do not change how a value crosses into a script.
const NEUTRAL_BOUNDS: &[&str] = &["Send", "Sync", "Sized", "Clone", "Copy", "Debug", "Unpin", "Default", "Display"];

impl<'a> Ctx<'a> {
    fn resolve(&self, krate: &str, p: &rustdoc_types::Path) -> Res<'a> {
        match self.docs.resolve(krate, p.id) {
            Resolved::Local(home, hid) => {
                let key = format!("{home}:{}", hid.0);
                if let Some(s) = self.by_key.get(&key) {
                    if s.kind == "type_alias" {
                        if let Some(ItemEnum::TypeAlias(ta)) = self.docs.crates[&home].index.get(&hid).map(|i| &i.inner) {
                            let env = alias_env(&ta.generics, p.args.as_deref(), krate, &home);
                            return Res::Alias(ta.type_.clone(), home, env);
                        }
                    }
                    return Res::Item(s);
                }
                if let Some(ItemEnum::TypeAlias(ta)) = self.docs.crates[&home].index.get(&hid).map(|i| &i.inner) {
                    // an alias that is not itself reachable but is used in a
                    // signature: follow it anyway, the target decides
                    let env = alias_env(&ta.generics, p.args.as_deref(), krate, &home);
                            return Res::Alias(ta.type_.clone(), home, env);
                }
                let path = self.docs.crates[&home].paths.get(&hid).map(|s| s.path.join("::")).unwrap_or_else(|| p.path.clone());
                Res::Unreachable(path)
            }
            Resolved::Foreign(_, path, _) => Res::Foreign(render::last(&path).to_string()),
            Resolved::Unknown(why) => {
                if std::env::var_os("SURFACE_DEBUG_UNKNOWN").is_some() {
                    eprintln!("unknown: {krate}: {} -> {why}", p.path);
                }
                Res::Unknown(why)
            }
        }
    }
}

/// Alias parameter name -> (argument type, crate whose document its ids belong to).
type Env = HashMap<String, (Type, String)>;

fn alias_env(generics: &rustdoc_types::Generics, args: Option<&GenericArgs>, caller: &str, home: &str) -> Env {
    let names: Vec<&str> = generics.params.iter().filter(|p| matches!(p.kind, rustdoc_types::GenericParamDefKind::Type { .. })).map(|p| p.name.as_str()).collect();
    let given: Vec<Type> = match args {
        Some(GenericArgs::AngleBracketed { args, .. }) => args.iter().filter_map(|a| if let GenericArg::Type(t) = a { Some(t.clone()) } else { None }).collect(),
        _ => vec![],
    };
    let mut env = Env::new();
    for (i, n) in names.iter().enumerate() {
        if let Some(t) = given.get(i) {
            env.insert(n.to_string(), (t.clone(), caller.to_string()));
        } else if let Some(rustdoc_types::GenericParamDefKind::Type { default: Some(d), .. }) = generics.params.iter().find(|p| p.name == *n).map(|p| &p.kind) {
            // a defaulted parameter's ids belong to the alias's own crate
            env.insert(n.to_string(), (d.clone(), home.to_string()));
        }
    }
    env
}

fn path_args(p: &rustdoc_types::Path) -> Vec<&Type> {
    match p.args.as_deref() {
        Some(GenericArgs::AngleBracketed { args, .. }) => args.iter().filter_map(|a| if let GenericArg::Type(t) = a { Some(t) } else { None }).collect(),
        _ => vec![],
    }
}

fn is_closure_bound(b: &[GenericBound]) -> bool {
    b.iter().any(|b| match b {
        GenericBound::TraitBound { trait_, .. } => {
            let n = render::last(&trait_.path);
            n == "Fn" || n == "FnMut" || n == "FnOnce"
        }
        _ => false,
    })
}

/// A conversion bound (`Into<T>`, `AsRef<str>`, `IntoVec<T>`, `IntoIterator<Item = T>`)
/// and whether any non-neutral bound accompanies it.
fn bound_conversion_target(b: &[GenericBound]) -> (Option<Type>, bool) {
    let mut target = None;
    let mut extra = false;
    for b in b {
        match b {
            GenericBound::TraitBound { trait_, .. } => {
                let n = render::last(&trait_.path);
                if NEUTRAL_BOUNDS.contains(&n) {
                    continue;
                }
                if let Some(GenericArgs::AngleBracketed { args, constraints }) = trait_.args.as_deref() {
                    if matches!(n, "Into" | "AsRef" | "IntoVec" | "Borrow" | "TryInto") {
                        if let Some(GenericArg::Type(t)) = args.first() {
                            if target.is_none() {
                                target = Some(t.clone());
                                continue;
                            }
                        }
                    }
                    if n == "IntoIterator" {
                        for c in constraints {
                            if c.name == "Item" {
                                if let rustdoc_types::AssocItemConstraintKind::Equality(rustdoc_types::Term::Type(t)) = &c.binding {
                                    if target.is_none() {
                                        target = Some(t.clone());
                                    }
                                }
                            }
                        }
                        if target.is_some() {
                            continue;
                        }
                    }
                }
                extra = true;
            }
            GenericBound::Outlives(_) => {}
            GenericBound::Use(_) => {}
        }
    }
    (target, extra)
}

fn worst(a: (Bucket, &'static str), b: (Bucket, &'static str)) -> (Bucket, &'static str) {
    if b.0 > a.0 { b } else { a }
}

/// Classify one type in argument position. `krate` is the crate whose
/// document the path ids belong to.
fn arg(ctx: &Ctx, krate: &str, t: &Type, generics: &HashMap<String, Vec<GenericBound>>, env: &Env, depth: u8) -> (Bucket, &'static str) {
    if depth > 8 {
        return (Bucket::Generic, "P7");
    }
    match t {
        Type::Primitive(p) => {
            if SCALARS.contains(&p.as_str()) { (Bucket::Mechanical, "P1") } else { (Bucket::Generic, "P8") }
        }
        Type::ResolvedPath(p) => {
            let args = path_args(p);
            match ctx.resolve(krate, p) {
                Res::Alias(target, home, aenv) => {
                    // inside the alias target, generic names are the alias's
                    // parameters; the caller's fn generics do not apply
                    let none: HashMap<String, Vec<GenericBound>> = HashMap::new();
                    arg(ctx, &home, &target, &none, &aenv, depth + 1)
                }
                Res::Unknown(_) => (Bucket::Unknown, "U1"),
                Res::Unreachable(_) => (Bucket::Generic, "P9"),
                Res::Foreign(name) => match name.as_str() {
                    "String" => (Bucket::Mechanical, "P1"),
                    "Option" | "Vec" | "Arc" | "Box" | "Cow" | "Rc" | "VecDeque" | "HashSet" | "BTreeSet" => match args.first() {
                        Some(inner) => {
                            let (b, r) = arg(ctx, krate, inner, generics, env, depth + 1);
                            if b >= Bucket::Callback { (b, r) } else { (b.max(Bucket::Conversion), if b <= Bucket::Conversion { "P3" } else { r }) }
                        }
                        None => (Bucket::Generic, "P7"),
                    },
                    "HashMap" | "BTreeMap" | "IndexMap" => {
                        let mut w = (Bucket::Mechanical, "P3");
                        for a in &args {
                            w = worst(w, arg(ctx, krate, a, generics, env, depth + 1));
                        }
                        if w.0 <= Bucket::Conversion { (Bucket::Conversion, "P3") } else { w }
                    }
                    "Result" => match args.first() {
                        Some(inner) => arg(ctx, krate, inner, generics, env, depth + 1),
                        None => (Bucket::Generic, "P7"),
                    },
                    "Duration" | "SystemTime" | "Instant" | "PathBuf" | "Path" | "OsStr" | "OsString" | "NonZeroUsize" | "NonZeroU32" | "NonZeroU64" | "Range" | "RangeInclusive" => (Bucket::Conversion, "P3"),
                    _ => (Bucket::Generic, "P8"),
                },
                Res::Item(s) => {
                    if s.canonical_path.ends_with("pl_str::PlSmallStr") {
                        return (Bucket::Mechanical, "P1");
                    }
                    if s.kind == "trait" || s.kind == "trait_alias" {
                        return (Bucket::Generic, "P7");
                    }
                    if s.lifetime {
                        return (Bucket::Generic, "L1");
                    }
                    // Any dyn/closure argument inside (SpecialEq<Arc<dyn ColumnsUdf>>)
                    // decides before the wrapper's own shape.
                    let mut inner_worst = (Bucket::Mechanical, "P2");
                    for a in &args {
                        let r = arg(ctx, krate, a, generics, env, depth + 1);
                        inner_worst = worst(inner_worst, r);
                    }
                    if inner_worst.0 >= Bucket::Callback {
                        return inner_worst;
                    }
                    if s.generic && args.is_empty() {
                        return (Bucket::Generic, "O1");
                    }
                    if s.generic && inner_worst.0 == Bucket::Generic {
                        return (Bucket::Generic, "P7");
                    }
                    let own = match s.kind.as_str() {
                        "struct" | "union" => {
                            if s.public_fields == 0 { (Bucket::Mechanical, "P2") } else { (Bucket::OptionStruct, "P5") }
                        }
                        "enum" => {
                            if s.variant_shapes.iter().any(|(_, data)| *data) { (Bucket::OptionStruct, "P5") } else { (Bucket::Mechanical, "P2") }
                        }
                        _ => (Bucket::Generic, "P7"),
                    };
                    worst(own, if inner_worst.0 <= Bucket::Conversion && !args.is_empty() { (inner_worst.0.max(Bucket::Mechanical), inner_worst.1) } else { (Bucket::Mechanical, "P2") })
                }
            }
        }
        Type::BorrowedRef { type_, .. } => match &**type_ {
            Type::Primitive(p) if p == "str" => (Bucket::Mechanical, "P1"),
            Type::Slice(inner) => {
                let (b, r) = arg(ctx, krate, inner, generics, env, depth + 1);
                if b >= Bucket::Callback { (b, r) } else { (b.max(Bucket::Conversion), if b <= Bucket::Conversion { "P3" } else { r }) }
            }
            inner => arg(ctx, krate, inner, generics, env, depth + 1),
        },
        Type::Slice(inner) | Type::Array { type_: inner, .. } => {
            let (b, r) = arg(ctx, krate, inner, generics, env, depth + 1);
            if b >= Bucket::Callback { (b, r) } else { (b.max(Bucket::Conversion), if b <= Bucket::Conversion { "P3" } else { r }) }
        }
        Type::Tuple(ts) => {
            if ts.is_empty() {
                return (Bucket::Mechanical, "P1");
            }
            let mut w = (Bucket::Mechanical, "P3");
            for t in ts {
                w = worst(w, arg(ctx, krate, t, generics, env, depth + 1));
            }
            if w.0 <= Bucket::Conversion { (Bucket::Conversion, "P3") } else { w }
        }
        Type::ImplTrait(bounds) => {
            if is_closure_bound(bounds) {
                return (Bucket::Callback, "P6");
            }
            match bound_conversion_target(bounds) {
                (Some(target), false) => {
                    let (b, r) = arg(ctx, krate, &target, generics, env, depth + 1);
                    if b >= Bucket::Callback { (b, r) } else { (b.max(Bucket::Conversion), if b <= Bucket::Conversion { "P4" } else { r }) }
                }
                _ => (Bucket::Generic, "P7"),
            }
        }
        Type::Generic(g) => {
            if g == "Self" {
                return (Bucket::Mechanical, "P2");
            }
            if let Some((t, home)) = env.get(g) {
                return arg(ctx, home, t, generics, &Env::new(), depth + 1);
            }
            match generics.get(g) {
                Some(bounds) => {
                    if is_closure_bound(bounds) {
                        return (Bucket::Callback, "P6");
                    }
                    match bound_conversion_target(bounds) {
                        (Some(target), false) => {
                            let (b, r) = arg(ctx, krate, &target, generics, env, depth + 1);
                            if b >= Bucket::Callback { (b, r) } else { (b.max(Bucket::Conversion), if b <= Bucket::Conversion { "P4" } else { r }) }
                        }
                        _ => (Bucket::Generic, "P7"),
                    }
                }
                None => (Bucket::Generic, "P7"),
            }
        }
        Type::DynTrait(d) => {
            if d.traits.iter().any(|t| {
                let n = render::last(&t.trait_.path);
                n.starts_with("Fn") || n.ends_with("Udf") || n == "AnonymousScan"
            }) {
                (Bucket::Callback, "P6")
            } else {
                (Bucket::Generic, "P7")
            }
        }
        Type::FunctionPointer(_) => (Bucket::Callback, "P6"),
        Type::QualifiedPath { .. } => (Bucket::Generic, "P7"),
        Type::RawPointer { .. } => (Bucket::Unsupported, "X3"),
        Type::Infer | Type::Pat { .. } => (Bucket::Generic, "P7"),
    }
}

fn ret(ctx: &Ctx, krate: &str, t: &Type, generics: &HashMap<String, Vec<GenericBound>>) -> (Bucket, &'static str) {
    match t {
        Type::BorrowedRef { type_, .. } => {
            let (b, _) = arg(ctx, krate, type_, generics, &Env::new(), 0);
            if b <= Bucket::Conversion { (Bucket::Conversion, "T2") } else if b == Bucket::Unknown { (Bucket::Unknown, "U1") } else { (Bucket::Generic, "T3") }
        }
        Type::ImplTrait(_) | Type::DynTrait(_) | Type::QualifiedPath { .. } => (Bucket::Generic, "T3"),
        Type::Generic(g) if g != "Self" => (Bucket::Generic, "T3"),
        Type::ResolvedPath(p) if matches!(render::last(&p.path), "Iter" | "IntoIter" | "Chunks" | "Ref" | "RefMut" | "MutexGuard" | "RwLockReadGuard" | "RwLockWriteGuard") => (Bucket::Generic, "T3"),
        _ => {
            let (b, r) = arg(ctx, krate, t, generics, &Env::new(), 0);
            match b {
                Bucket::Mechanical => (Bucket::Mechanical, "T1"),
                Bucket::Conversion => (Bucket::Conversion, "T2"),
                Bucket::OptionStruct => (Bucket::Conversion, "T2"),
                Bucket::Callback => (Bucket::Generic, "T3"),
                Bucket::Unknown => (Bucket::Unknown, "U1"),
                _ => (b, if b == Bucket::Generic { "T3" } else { r }),
            }
        }
    }
}

pub fn classify_all(inv: &Inventory, docs: &Docs) -> ClassifiedInventory {
    let by_key: HashMap<String, &Supporting> = inv.supporting.iter().map(|s| (s.key.clone(), s)).collect();
    let ctx = Ctx { docs, by_key };
    let callables = inv.callables.iter().map(|c| classify_one(&ctx, c)).collect();
    ClassifiedInventory {
        root: inv.root.clone(),
        callables,
        supporting: inv.supporting.clone(),
        unknown: inv.unknown.clone(),
        foreign_reexports: inv.foreign_reexports.clone(),
        derived_impls: inv.derived_impls,
        gross: inv.gross.clone(),
        rules: RULES.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
    }
}

fn classify_one(ctx: &Ctx, c: &Callable) -> Classified {
    let mut rules: Vec<String> = Vec::new();
    let mut bucket = Bucket::Mechanical;
    let mut decided_by = "receiver".to_string();
    let mut shape: Vec<String> = Vec::new();
    let raise = |b: Bucket, r: &str, by: &str, rules: &mut Vec<String>, bucket: &mut Bucket, decided: &mut String| {
        rules.push(r.to_string());
        if b > *bucket {
            *bucket = b;
            *decided = by.to_string();
        }
    };
    if c.is_unsafe {
        raise(Bucket::Unsupported, "X1", "unsafe", &mut rules, &mut bucket, &mut decided_by);
    }
    if c.hidden {
        raise(Bucket::Unsupported, "X2", "hidden", &mut rules, &mut bucket, &mut decided_by);
    }
    if c.name.starts_with('_') {
        raise(Bucket::Unsupported, "X5", "internal", &mut rules, &mut bucket, &mut decided_by);
    }
    if c.kind == CallableKind::TraitMethod && !c.trait_reachable {
        raise(Bucket::Generic, "X4", "trait scope", &mut rules, &mut bucket, &mut decided_by);
    }
    if c.owner_generic {
        let r = if c.owner_aliases.is_empty() { "O1" } else { "O2" };
        raise(Bucket::Generic, r, "owner", &mut rules, &mut bucket, &mut decided_by);
    }
    if c.kind == CallableKind::ForeignTraitImpl {
        let n = c.name.split('<').next().unwrap_or("");
        if matches!(n, "Display" | "Debug" | "PartialEq" | "Eq" | "PartialOrd" | "Ord" | "Hash" | "Default") {
            rules.push("F2".into());
            rules.sort();
            rules.dedup();
            let b = if c.owner_generic { Bucket::Generic } else { Bucket::Mechanical };
            return Classified { callable: c.clone(), bucket: b.max(bucket), rules, decided_by: "protocol".into(), shape: format!("protocol {n}") };
        }
    }
    match c.receiver.as_str() {
        "none" | "self" | "&self" | "&mut self" => raise(Bucket::Mechanical, "R1", "receiver", &mut rules, &mut bucket, &mut decided_by),
        _ => raise(Bucket::Generic, "R2", "receiver", &mut rules, &mut bucket, &mut decided_by),
    }
    shape.push(c.receiver.clone());
    // Generic bounds by name, for P4/P6 on `T: Into<Expr>` style parameters.
    let generics: HashMap<String, Vec<GenericBound>> = c.bounds_raw.iter().cloned().collect();
    for (i, (name, t)) in c.inputs_raw.iter().enumerate() {
        if i == 0 && name == "self" {
            continue;
        }
        let (b, r) = arg(ctx, &c.krate, t, &generics, &Env::new(), 0);
        raise(b, r, &format!("param {name}"), &mut rules, &mut bucket, &mut decided_by);
        shape.push(shape_of(t));
    }
    match &c.ret_raw {
        Some(t) => {
            let (b, r) = ret(ctx, &c.krate, t, &generics);
            raise(b, r, "return", &mut rules, &mut bucket, &mut decided_by);
            shape.push(format!("-> {}", shape_of(t)));
        }
        None => shape.push("-> ()".into()),
    }
    if c.kind == CallableKind::ForeignTraitImpl {
        rules.push("F1".into());
        let n = c.name.as_str();
        if n.starts_with("Iterator") || n.starts_with("IntoIterator") || n.starts_with("Deref") || n.starts_with("Index") || n.starts_with("Borrow") || n.starts_with("AsRef") || n.starts_with("AsMut") || n.starts_with("FromIterator") || n.starts_with("Extend") {
            raise(Bucket::Generic, "F1", "foreign trait", &mut rules, &mut bucket, &mut decided_by);
        }
        if n.starts_with("From<") || n.starts_with("TryFrom<") {
            raise(Bucket::Conversion, "F3", "foreign trait", &mut rules, &mut bucket, &mut decided_by);
        }
    }
    rules.sort();
    rules.dedup();
    Classified { callable: c.clone(), bucket, rules, decided_by, shape: shape.join(" ") }
}

/// A coarse shape string for deterministic sampling: ownership and container
/// kind per parameter, type names erased.
fn shape_of(t: &Type) -> String {
    match t {
        Type::Primitive(p) => if p == "str" { "str".into() } else { "scalar".into() },
        Type::ResolvedPath(p) => {
            let n = render::last(&p.path);
            match n {
                "String" | "PlSmallStr" => "string".into(),
                "Option" | "Vec" | "Arc" | "Box" | "Cow" | "Result" | "PolarsResult" => {
                    let inner = match p.args.as_deref() {
                        Some(GenericArgs::AngleBracketed { args, .. }) => args.iter().find_map(|a| if let GenericArg::Type(t) = a { Some(shape_of(t)) } else { None }).unwrap_or_default(),
                        _ => String::new(),
                    };
                    format!("{n}<{inner}>")
                }
                _ => "T".into(),
            }
        }
        Type::BorrowedRef { is_mutable, type_, .. } => format!("&{}{}", if *is_mutable { "mut " } else { "" }, shape_of(type_)),
        Type::Slice(i) | Type::Array { type_: i, .. } => format!("[{}]", shape_of(i)),
        Type::Tuple(ts) => format!("({})", ts.iter().map(shape_of).collect::<Vec<_>>().join(",")),
        Type::ImplTrait(b) => if is_closure_bound(b) { "closure".into() } else { "impl".into() },
        Type::Generic(g) => if g == "Self" { "Self".into() } else { "G".into() },
        Type::DynTrait(_) => "dyn".into(),
        Type::FunctionPointer(_) => "fnptr".into(),
        Type::QualifiedPath { .. } => "assoc".into(),
        Type::RawPointer { .. } => "ptr".into(),
        Type::Infer | Type::Pat { .. } => "?".into(),
    }
}

#[derive(Serialize)]
pub struct Summary {
    pub root: String,
    pub gross: BTreeMap<String, crate::inventory::GrossCounts>,
    pub gross_total_callables: usize,
    pub reachable_callables: usize,
    pub by_kind: BTreeMap<String, usize>,
    pub excluded: usize,
    /// unresolved re-exports (walker) + callables with an unresolvable type (U1)
    pub unknown: usize,
    pub unknown_reexports: usize,
    pub unknown_types: usize,
    pub eligible: usize,
    pub derived_callables: usize,
    pub predicted: BTreeMap<String, usize>,
    /// defining crate -> (eligible, predicted per bucket)
    pub by_crate: BTreeMap<String, BTreeMap<String, usize>>,
    pub predicted_pct_of_eligible: BTreeMap<String, f64>,
    pub rule_hits: BTreeMap<String, usize>,
    pub supporting_by_kind: BTreeMap<String, usize>,
    pub supporting_fields: usize,
    pub supporting_variants: usize,
    pub foreign_reexports: usize,
    pub derived_impls: usize,
    pub trait_methods_trait_unreachable: usize,
    pub deprecated: usize,
}

pub fn summarize(ci: &ClassifiedInventory) -> Summary {
    let mut by_kind = BTreeMap::new();
    let mut predicted = BTreeMap::new();
    let mut rule_hits = BTreeMap::new();
    let mut excluded = 0;
    let mut unknown_types = 0;
    let mut derived_callables = 0;
    let mut deprecated = 0;
    let mut by_crate: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut unreachable_trait = 0;
    for c in &ci.callables {
        *by_kind.entry(format!("{:?}", c.callable.kind).to_lowercase()).or_insert(0) += 1;
        let e = by_crate.entry(c.callable.krate.clone()).or_default();
        if c.callable.derived {
            derived_callables += 1;
        }
        if c.bucket == Bucket::Unsupported {
            excluded += 1;
            *e.entry("excluded".into()).or_insert(0) += 1;
        } else if c.bucket == Bucket::Unknown {
            unknown_types += 1;
            *e.entry("unknown".into()).or_insert(0) += 1;
        } else {
            let b = format!("{}_{:?}", c.bucket as u8, c.bucket).to_lowercase();
            *predicted.entry(b.clone()).or_insert(0) += 1;
            *e.entry("eligible".into()).or_insert(0) += 1;
            *e.entry(b).or_insert(0) += 1;
        }
        for r in &c.rules {
            *rule_hits.entry(r.clone()).or_insert(0) += 1;
        }
        if c.callable.deprecated {
            deprecated += 1;
        }
        if c.callable.kind == CallableKind::TraitMethod && !c.callable.trait_reachable {
            unreachable_trait += 1;
        }
    }
    let eligible = ci.callables.len() - excluded - unknown_types;
    let pct = predicted.iter().map(|(k, v)| (k.clone(), if eligible == 0 { 0.0 } else { (*v as f64) * 100.0 / eligible as f64 })).collect();
    let mut supporting_by_kind = BTreeMap::new();
    let mut fields = 0;
    let mut variants = 0;
    for s in &ci.supporting {
        *supporting_by_kind.entry(s.kind.clone()).or_insert(0) += 1;
        fields += s.public_fields;
        variants += s.variants;
    }
    let gross_total_callables = ci.gross.values().map(|g| g.functions + g.methods).sum();
    let _ = HashSet::<()>::new();
    Summary {
        root: ci.root.clone(),
        gross: ci.gross.clone(),
        gross_total_callables,
        reachable_callables: ci.callables.len(),
        by_kind,
        excluded,
        unknown: ci.unknown.len() + unknown_types,
        unknown_reexports: ci.unknown.len(),
        unknown_types,
        eligible,
        derived_callables,
        predicted,
        by_crate,
        predicted_pct_of_eligible: pct,
        rule_hits,
        supporting_by_kind,
        supporting_fields: fields,
        supporting_variants: variants,
        foreign_reexports: ci.foreign_reexports.len(),
        derived_impls: ci.derived_impls,
        trait_methods_trait_unreachable: unreachable_trait,
        deprecated,
    }
}

pub fn summary_md(s: &Summary) -> String {
    let mut o = String::new();
    o.push_str(&format!("## {} surface\n\n", s.root));
    o.push_str("| total | count |\n|---|---:|\n");
    o.push_str(&format!("| gross public callables, all documented crates | {} |\n", s.gross_total_callables));
    o.push_str(&format!("| reachable callables (deduplicated) | {} |\n", s.reachable_callables));
    for (k, v) in &s.by_kind {
        o.push_str(&format!("| … {k} | {v} |\n"));
    }
    o.push_str(&format!("| excluded (unsafe, hidden, raw pointer) | {} |\n", s.excluded));
    o.push_str(&format!("| unknown: unresolved re-exports | {} |\n", s.unknown_reexports));
    o.push_str(&format!("| unknown: callables with an unresolvable type (U1) | {} |\n", s.unknown_types));
    o.push_str(&format!("| derived impls counted among callables | {} |\n", s.derived_callables));
    o.push_str(&format!("| **eligible denominator** | **{}** |\n", s.eligible));
    o.push_str(&format!("| trait methods whose trait is not path-reachable | {} |\n", s.trait_methods_trait_unreachable));
    o.push_str(&format!("| deprecated (counted, flagged) | {} |\n", s.deprecated));
    o.push_str(&format!("| foreign re-exports (std etc., listed not counted) | {} |\n", s.foreign_reexports));
    o.push_str(&format!("| derived impls on reachable types (skipped) | {} |\n", s.derived_impls));
    o.push_str("\n| predicted bucket | count | % of eligible |\n|---|---:|---:|\n");
    for (k, v) in &s.predicted {
        o.push_str(&format!("| {k} | {v} | {:.1} |\n", s.predicted_pct_of_eligible[k]));
    }
    o.push_str("\n| defining crate | eligible | mech | conv | opt | cb | gen | excl |\n|---|---:|---:|---:|---:|---:|---:|---:|\n");
    for (k, v) in &s.by_crate {
        let g = |n: &str| v.get(n).copied().unwrap_or(0);
        o.push_str(&format!("| {k} | {} | {} | {} | {} | {} | {} | {} |\n", g("eligible"), g("1_mechanical"), g("2_conversion"), g("3_optionstruct"), g("4_callback"), g("5_generic"), g("excluded")));
    }
    o.push_str("\n| supporting | count |\n|---|---:|\n");
    for (k, v) in &s.supporting_by_kind {
        o.push_str(&format!("| {k} | {v} |\n"));
    }
    o.push_str(&format!("| public fields | {} |\n| enum variants | {} |\n", s.supporting_fields, s.supporting_variants));
    o.push_str("\n| rule | hits |\n|---|---:|\n");
    for (k, v) in &s.rule_hits {
        o.push_str(&format!("| {k} | {v} |\n"));
    }
    o
}
