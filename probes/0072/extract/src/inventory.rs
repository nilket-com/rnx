//! Reachability walk over a set of rustdoc JSON documents.
use rustdoc_types::{Crate, Id, Item, ItemEnum, ItemKind, StructKind, Type, Visibility};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use crate::render;

/// Every loaded document, keyed by crate name (underscored, as rustc names it).
pub struct Docs {
    pub crates: HashMap<String, Crate>,
    /// Per crate: local item path -> id, for cross-document resolution.
    by_path: HashMap<String, HashMap<Vec<String>, Id>>,
}

impl Docs {
    pub fn load(dir: &Path) -> Docs {
        let mut crates = HashMap::new();
        let mut by_path = HashMap::new();
        for entry in std::fs::read_dir(dir).expect("docs dir") {
            let p = entry.unwrap().path();
            if p.extension().map(|e| e != "json").unwrap_or(true) || p.file_name().unwrap() == "pins.json" {
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            let c: Crate = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {}", p.display(), e));
            let name = c.index[&c.root].name.clone().expect("crate root name");
            let mut paths = HashMap::new();
            for (id, s) in &c.paths {
                if s.crate_id == 0 {
                    paths.insert(s.path.clone(), *id);
                }
            }
            by_path.insert(name.clone(), paths);
            crates.insert(name, c);
        }
        Docs { crates, by_path }
    }

    /// Resolve an id seen in `krate` to its defining crate and local id.
    pub fn resolve(&self, krate: &str, id: Id) -> Resolved {
        let c = &self.crates[krate];
        if let Some(item) = c.index.get(&id) {
            if item.crate_id == 0 {
                return Resolved::Local(krate.to_string(), id);
            }
        }
        let Some(summary) = c.paths.get(&id) else {
            return Resolved::Unknown(format!("{krate}: id {} has no path entry", id.0));
        };
        let cname = match c.external_crates.get(&summary.crate_id) {
            Some(e) => e.name.clone(),
            None => return Resolved::Unknown(format!("{krate}: {} crate_id {} unlisted", summary.path.join("::"), summary.crate_id)),
        };
        match self.by_path.get(&cname).and_then(|m| m.get(&summary.path)) {
            Some(local) => Resolved::Local(cname, *local),
            None if self.crates.contains_key(&cname) => Resolved::Unknown(format!("{}: not in {}'s index", summary.path.join("::"), cname)),
            // A polars crate that was not documented is a gap, not an external crate.
            None if cname.starts_with("polars") => Resolved::Unknown(format!("{}: crate {} not documented", summary.path.join("::"), cname)),
            None => Resolved::Foreign(cname, summary.path.join("::"), summary.kind.clone()),
        }
    }
}

pub enum Resolved {
    Local(String, Id),
    /// Belongs to a crate that was not documented (std, core, arrow2, ...).
    Foreign(String, String, ItemKind),
    Unknown(String),
}

#[derive(Serialize, Clone, Debug)]
pub struct Param {
    pub name: String,
    pub ty: String,
    #[serde(skip)]
    pub raw: Type,
}

#[derive(Serialize, Clone, Debug)]
pub struct GenericParam {
    pub name: String,
    pub bounds: String,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CallableKind {
    Inherent,
    TraitMethod,
    FreeFn,
    /// An impl of a foreign (std/core) trait for a reachable type: operators,
    /// conversions, Display, Iterator ...
    ForeignTraitImpl,
}

#[derive(Serialize, Clone, Debug)]
pub struct Callable {
    pub key: String,
    pub kind: CallableKind,
    pub krate: String,
    /// Canonical owner: the type (inherent), trait (trait method) or module path.
    pub owner: String,
    pub name: String,
    pub canonical_path: String,
    pub found_paths: Vec<String>,
    pub receiver: String,
    pub params: Vec<Param>,
    pub ret: Option<String>,
    #[serde(skip)]
    pub ret_raw: Option<Type>,
    pub generics: Vec<GenericParam>,
    pub where_clause: Vec<String>,
    #[serde(skip)]
    pub bounds_raw: Vec<(String, Vec<rustdoc_types::GenericBound>)>,
    pub owner_generic: bool,
    /// Owner is generic but was reached through a concrete type alias
    /// (Int64Chunked = ChunkedArray<Int64Type>); lists the aliases.
    pub owner_aliases: Vec<String>,
    pub is_unsafe: bool,
    pub is_async: bool,
    pub is_provided: bool,
    pub deprecated: bool,
    pub hidden: bool,
    pub unstable: bool,
    /// For trait methods: reachable types implementing the trait; "blanket"
    /// marks a blanket impl.
    pub implementors: Vec<String>,
    /// For trait methods: whether the trait itself is reachable by a public path.
    pub trait_reachable: bool,
    /// For foreign trait impls: the impl is `#[derive]`d.
    pub derived: bool,
    #[serde(skip)]
    pub inputs_raw: Vec<(String, Type)>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Supporting {
    pub key: String,
    pub kind: String,
    pub canonical_path: String,
    pub found_paths: Vec<String>,
    pub public_fields: usize,
    /// (name, rendered type) of public fields.
    pub fields: Vec<(String, String)>,
    pub variants: usize,
    /// Variant names with whether they carry data.
    pub variant_shapes: Vec<(String, bool)>,
    pub generic: bool,
    /// Has lifetime parameters (AnyValue<'a>).
    pub lifetime: bool,
    pub hidden: bool,
    /// Traits whose impls are `#[derive]`d (Clone, Default, Debug, ...).
    pub derived: Vec<String>,
}

#[derive(Serialize, Default)]
pub struct Inventory {
    pub root: String,
    pub callables: Vec<Callable>,
    pub supporting: Vec<Supporting>,
    pub unknown: Vec<String>,
    /// Foreign items re-exported by path (std types etc.), listed, not counted.
    pub foreign_reexports: Vec<String>,
    /// `#[derive]`d impls on reachable types, skipped as callables.
    pub derived_impls: usize,
    /// Gross public inventory: every public item in every loaded crate, by crate.
    pub gross: BTreeMap<String, GrossCounts>,
}

#[derive(Serialize, Default, Clone)]
pub struct GrossCounts {
    pub functions: usize,
    pub methods: usize,
    pub types: usize,
    pub traits: usize,
    pub constants: usize,
    pub macros: usize,
}

struct Walker<'a> {
    docs: &'a Docs,
    inv: Inventory,
    /// (crate, id) -> index into callables/supporting.
    seen_callable: HashMap<(String, Id), usize>,
    seen_support: HashMap<(String, Id), usize>,
    visited_types: BTreeSet<(String, Id)>,
    visited_modules: BTreeSet<(String, Id, bool)>,
    /// trait key -> implementors, filled while visiting types.
    implementors: BTreeMap<(String, Id), Vec<String>>,
    reachable_traits: BTreeSet<(String, Id)>,
    /// traits implemented by reachable types but not reached by path yet.
    pending_traits: BTreeSet<(String, Id)>,
    /// generic type -> concrete aliases that reach it.
    aliases: BTreeMap<(String, Id), Vec<String>>,
}

fn is_hidden(item: &Item) -> bool {
    item.attrs.iter().any(|a| matches!(a, rustdoc_types::Attribute::Other(s) if s.contains("doc(hidden)")))
}

fn item_path(c: &Crate, id: Id) -> Option<String> {
    c.paths.get(&id).map(|s| s.path.join("::"))
}

pub fn extract(docs: &Docs, root: &str) -> Inventory {
    let mut w = Walker {
        docs,
        inv: Inventory { root: root.to_string(), ..Default::default() },
        seen_callable: HashMap::new(),
        seen_support: HashMap::new(),
        visited_types: BTreeSet::new(),
        visited_modules: BTreeSet::new(),
        implementors: BTreeMap::new(),
        reachable_traits: BTreeSet::new(),
        pending_traits: BTreeSet::new(),
        aliases: BTreeMap::new(),
    };
    let Some(root_crate) = docs.crates.get(root) else {
        eprintln!("root crate {root} is not among the documented crates");
        std::process::exit(3);
    };
    let root_id = root_crate.root;
    w.walk_module(root, root_id, root, false);
    // Traits implemented by reachable types but not reachable by path: their
    // methods are still callable when the trait is in scope. Count them,
    // flagged.
    let pending: Vec<_> = w.pending_traits.iter().cloned().collect();
    for (krate, id) in pending {
        if !w.reachable_traits.contains(&(krate.clone(), id)) {
            let path = item_path(&docs.crates[&krate], id).unwrap_or_default();
            w.visit_trait(&krate, id, &path, false, false);
        }
    }
    // Concrete aliases of generic owners, onto their inherent methods.
    let aliases = w.aliases.clone();
    for ((krate, id), names) in aliases {
        let okey = format!("{krate}:{}", id.0);
        let mut names = names;
        names.sort();
        names.dedup();
        for c in &mut w.inv.callables {
            if c.kind == CallableKind::Inherent && c.owner == okey {
                c.owner_aliases = names.clone();
            }
        }
    }
    // Fill implementors into trait methods.
    let impls = w.implementors.clone();
    for c in &mut w.inv.callables {
        if c.kind == CallableKind::TraitMethod {
            let tk: Vec<&str> = c.key.splitn(2, ':').collect();
            let _ = tk;
        }
    }
    for ((krate, id), types) in impls {
        let tkey = format!("{krate}:{}", id.0);
        for c in &mut w.inv.callables {
            if c.kind == CallableKind::TraitMethod && c.owner == tkey {
                c.implementors = types.clone();
            }
        }
    }
    // Owner keys were internal; replace with canonical paths for output.
    let mut owner_paths: HashMap<String, String> = HashMap::new();
    for s in &w.inv.supporting {
        owner_paths.insert(s.key.clone(), s.canonical_path.clone());
    }
    for c in &mut w.inv.callables {
        if let Some(p) = owner_paths.get(&c.owner) {
            c.owner = p.clone();
        }
        c.found_paths.sort();
        c.found_paths.dedup();
    }
    for s in &mut w.inv.supporting {
        s.found_paths.sort();
        s.found_paths.dedup();
    }
    w.inv.unknown.sort();
    w.inv.unknown.dedup();
    w.inv.foreign_reexports.sort();
    w.inv.foreign_reexports.dedup();
    w.inv.callables.sort_by(|a, b| a.canonical_path.cmp(&b.canonical_path).then(a.key.cmp(&b.key)));
    w.inv.supporting.sort_by(|a, b| a.canonical_path.cmp(&b.canonical_path));
    w.inv.gross = gross(docs);
    w.inv
}

fn gross(docs: &Docs) -> BTreeMap<String, GrossCounts> {
    let mut out = BTreeMap::new();
    for (name, c) in &docs.crates {
        let mut g = GrossCounts::default();
        for item in c.index.values() {
            if item.crate_id != 0 || !matches!(item.visibility, Visibility::Public | Visibility::Default) {
                continue;
            }
            match &item.inner {
                ItemEnum::Function(_) => {
                    if c.paths.contains_key(&item.id) { g.functions += 1 } else { g.methods += 1 }
                }
                ItemEnum::Struct(_) | ItemEnum::Enum(_) | ItemEnum::Union(_) | ItemEnum::TypeAlias(_) => g.types += 1,
                ItemEnum::Trait(_) | ItemEnum::TraitAlias(_) => g.traits += 1,
                ItemEnum::Constant { .. } | ItemEnum::Static(_) => g.constants += 1,
                ItemEnum::Macro(_) | ItemEnum::ProcMacro(_) => g.macros += 1,
                _ => {}
            }
        }
        out.insert(name.clone(), g);
    }
    out
}

impl<'a> Walker<'a> {
    fn crate_(&self, k: &str) -> &'a Crate {
        &self.docs.crates[k]
    }

    fn walk_module(&mut self, krate: &str, id: Id, prefix: &str, hidden: bool) {
        if !self.visited_modules.insert((krate.to_string(), id, hidden)) {
            return;
        }
        let c = self.crate_(krate);
        let ItemEnum::Module(m) = &c.index[&id].inner else { return };
        for child in &m.items {
            let item = &c.index[child];
            let hidden = hidden || is_hidden(item);
            if !matches!(item.visibility, Visibility::Public | Visibility::Default) {
                continue;
            }
            match &item.inner {
                ItemEnum::Module(_) => {
                    let name = item.name.clone().unwrap();
                    self.walk_module(krate, *child, &format!("{prefix}::{name}"), hidden);
                }
                ItemEnum::Use(u) => {
                    let Some(target) = u.id else {
                        self.inv.unknown.push(format!("{prefix}::{} (use {} unresolved by rustdoc)", u.name, u.source));
                        continue;
                    };
                    match self.docs.resolve(krate, target) {
                        Resolved::Unknown(why) => self.inv.unknown.push(format!("{prefix}::{} <- {}", u.name, why)),
                        Resolved::Foreign(cname, path, kind) => {
                            self.inv.foreign_reexports.push(format!("{prefix}::{} = {cname}::{path} ({:?})", u.name, kind));
                        }
                        Resolved::Local(home, hid) => {
                            let h = self.crate_(&home);
                            let titem = &h.index[&hid];
                            if u.is_glob {
                                match &titem.inner {
                                    ItemEnum::Module(_) => self.walk_module(&home, hid, prefix, hidden),
                                    ItemEnum::Enum(_) => self.visit(&home, hid, prefix, hidden),
                                    _ => self.inv.unknown.push(format!("{prefix}::* <- glob of non-module {}", u.source)),
                                }
                            } else {
                                let name = &u.name;
                                self.visit(&home, hid, &format!("{prefix}::{name}"), hidden);
                            }
                        }
                    }
                }
                _ => {
                    let name = item.name.clone().unwrap_or_default();
                    self.visit(krate, *child, &format!("{prefix}::{name}"), hidden);
                }
            }
        }
    }

    fn visit(&mut self, krate: &str, id: Id, found: &str, hidden: bool) {
        let c = self.crate_(krate);
        let item = &c.index[&id];
        let hidden = hidden || is_hidden(item);
        match &item.inner {
            ItemEnum::Module(_) => self.walk_module(krate, id, found, hidden),
            ItemEnum::Struct(_) | ItemEnum::Enum(_) | ItemEnum::Union(_) => self.visit_type(krate, id, found, hidden),
            ItemEnum::Trait(_) => self.visit_trait(krate, id, found, hidden, true),
            ItemEnum::Function(_) => {
                self.visit_function(krate, id, found, hidden, CallableKind::FreeFn, String::new(), false, true);
            }
            ItemEnum::TypeAlias(ta) => {
                let alias_generic = ta.generics.params.iter().any(|p| !matches!(p.kind, rustdoc_types::GenericParamDefKind::Lifetime { .. }));
                let _ = self.support(krate, id, "type_alias", found, 0, 0, alias_generic, hidden);
                if let Type::ResolvedPath(p) = &ta.type_ {
                    if let Resolved::Local(home, hid) = self.docs.resolve(krate, p.id) {
                        let h = self.crate_(&home);
                        if matches!(h.index[&hid].inner, ItemEnum::Struct(_) | ItemEnum::Enum(_) | ItemEnum::Union(_)) {
                            self.visit_type(&home, hid, found, hidden);
                            if !alias_generic && p.args.is_some() {
                                let alias = item_path(c, id).unwrap_or_else(|| found.to_string());
                                self.aliases.entry((home, hid)).or_default().push(alias);
                            }
                        }
                    }
                }
            }
            ItemEnum::TraitAlias(_) => {
                self.support(krate, id, "trait_alias", found, 0, 0, false, hidden);
            }
            ItemEnum::Constant { .. } => {
                self.support(krate, id, "constant", found, 0, 0, false, hidden);
            }
            ItemEnum::Static(_) => {
                self.support(krate, id, "static", found, 0, 0, false, hidden);
            }
            ItemEnum::Macro(_) | ItemEnum::ProcMacro(_) => {
                self.support(krate, id, "macro", found, 0, 0, false, hidden);
            }
            ItemEnum::ExternType => {
                self.support(krate, id, "extern_type", found, 0, 0, false, hidden);
            }
            ItemEnum::Use(_) => {} // nested use reached through a module walk only
            _ => {}
        }
    }

    fn support(&mut self, krate: &str, id: Id, kind: &str, found: &str, fields: usize, variants: usize, generic: bool, hidden: bool) -> usize {
        let key = (krate.to_string(), id);
        if let Some(&i) = self.seen_support.get(&key) {
            self.inv.supporting[i].found_paths.push(found.to_string());
            self.inv.supporting[i].hidden &= hidden;
            return i;
        }
        let c = self.crate_(krate);
        let s = Supporting {
            key: format!("{krate}:{}", id.0),
            kind: kind.to_string(),
            canonical_path: item_path(c, id).unwrap_or_else(|| found.to_string()),
            found_paths: vec![found.to_string()],
            public_fields: fields,
            fields: vec![],
            variants,
            variant_shapes: vec![],
            generic,
            lifetime: false,
            hidden,
            derived: vec![],
        };
        self.inv.supporting.push(s);
        let i = self.inv.supporting.len() - 1;
        self.seen_support.insert(key, i);
        i
    }

    fn visit_type(&mut self, krate: &str, id: Id, found: &str, hidden: bool) {
        let c = self.crate_(krate);
        let item = &c.index[&id];
        let (kind, impls, fields, variants, generic) = match &item.inner {
            ItemEnum::Struct(s) => {
                let fields = match &s.kind {
                    StructKind::Plain { fields, .. } => fields.len(),
                    StructKind::Tuple(f) => f.iter().filter(|f| f.is_some()).count(),
                    StructKind::Unit => 0,
                };
                ("struct", s.impls.clone(), fields, 0, s.generics.params.iter().any(|p| !matches!(p.kind, rustdoc_types::GenericParamDefKind::Lifetime { .. })))
            }
            ItemEnum::Enum(e) => ("enum", e.impls.clone(), 0, e.variants.len(), e.generics.params.iter().any(|p| !matches!(p.kind, rustdoc_types::GenericParamDefKind::Lifetime { .. }))),
            ItemEnum::Union(u) => ("union", u.impls.clone(), 0, 0, !u.generics.params.is_empty()),
            _ => return,
        };
        let idx = self.support(krate, id, kind, found, fields, variants, generic, hidden);
        let owner_key = self.inv.supporting[idx].key.clone();
        let owner_path = self.inv.supporting[idx].canonical_path.clone();
        if !self.visited_types.insert((krate.to_string(), id)) {
            return;
        }
        let lifetime = match &item.inner {
            ItemEnum::Struct(st) => st.generics.params.iter().any(|p| matches!(p.kind, rustdoc_types::GenericParamDefKind::Lifetime { .. })),
            ItemEnum::Enum(en) => en.generics.params.iter().any(|p| matches!(p.kind, rustdoc_types::GenericParamDefKind::Lifetime { .. })),
            _ => false,
        };
        self.inv.supporting[idx].lifetime = lifetime;
        match &item.inner {
            ItemEnum::Struct(st) => {
                if let StructKind::Plain { fields, .. } = &st.kind {
                    for f in fields {
                        if let Some(fi) = c.index.get(f) {
                            if let ItemEnum::StructField(t) = &fi.inner {
                                self.inv.supporting[idx].fields.push((fi.name.clone().unwrap_or_default(), render::ty(t)));
                            }
                        }
                    }
                }
            }
            ItemEnum::Enum(en) => {
                for v in &en.variants {
                    if let Some(vi) = c.index.get(v) {
                        if let ItemEnum::Variant(var) = &vi.inner {
                            let data = !matches!(var.kind, rustdoc_types::VariantKind::Plain);
                            self.inv.supporting[idx].variant_shapes.push((vi.name.clone().unwrap_or_default(), data));
                        }
                    }
                }
            }
            _ => {}
        }
        for impl_id in impls {
            let imp = match &c.index.get(&impl_id).map(|i| &i.inner) {
                Some(ItemEnum::Impl(i)) => i,
                _ => continue,
            };
            if imp.is_negative || imp.is_synthetic {
                continue;
            }
            // `#[derive]`d impls (Clone, Debug, PartialEq, ...) are public
            // operations like hand-written ones; they are inventoried on the
            // same terms and flagged, and the count is also reported.
            let derived = c.index[&impl_id].attrs.iter().any(|a| matches!(a, rustdoc_types::Attribute::AutomaticallyDerived));
            if derived {
                self.inv.derived_impls += 1;
                if let Some(tr) = &imp.trait_ {
                    self.inv.supporting[idx].derived.push(render::last(&tr.path).to_string());
                }
            }
            match &imp.trait_ {
                None => {
                    for m in &imp.items {
                        if matches!(c.index.get(m).map(|i| &i.inner), Some(ItemEnum::Function(_))) {
                            let name = c.index[m].name.clone().unwrap();
                            self.visit_function(krate, *m, &format!("{owner_path}::{name}"), hidden, CallableKind::Inherent, owner_key.clone(), generic, true);
                        }
                    }
                }
                Some(tr) => {
                    let blanket = imp.blanket_impl.is_some();
                    match self.docs.resolve(krate, tr.id) {
                        Resolved::Local(home, tid) => {
                            let label = if blanket { "blanket".to_string() } else { owner_path.clone() };
                            self.implementors.entry((home.clone(), tid)).or_default().push(label);
                            self.pending_traits.insert((home, tid));
                        }
                        Resolved::Foreign(_cname, tpath, _) => {
                            // Operators and std conversions on a reachable type: one
                            // callable per (type, trait), not per method.
                            if blanket {
                                continue;
                            }
                            let tname = render::path(&tr.path, tr.args.as_deref());
                            let key = format!("{krate}:{}:{}", id.0, impl_id.0);
                            if self.seen_callable.contains_key(&(key.clone(), Id(0))) {
                                continue;
                            }
                            self.seen_callable.insert((key.clone(), Id(0)), self.inv.callables.len());
                            let (recv, params, ret, ret_raw, inputs_raw) = first_method_sig(c, imp);
                            self.inv.callables.push(Callable {
                                key: key.clone(),
                                kind: CallableKind::ForeignTraitImpl,
                                krate: krate.to_string(),
                                owner: owner_key.clone(),
                                name: tname.clone(),
                                canonical_path: format!("{owner_path} as {tpath}"),
                                found_paths: vec![format!("{found} as {tname}")],
                                receiver: recv,
                                params,
                                ret,
                                ret_raw,
                                generics: vec![],
                                where_clause: vec![],
                                bounds_raw: vec![],
                                owner_generic: generic,
                                owner_aliases: vec![],
                                is_unsafe: imp.is_unsafe,
                                is_async: false,
                                is_provided: false,
                                deprecated: false,
                                hidden,
                                unstable: false,
                                implementors: vec![],
                                trait_reachable: false,
                                derived,
                                inputs_raw,
                            });
                        }
                        Resolved::Unknown(why) => self.inv.unknown.push(format!("impl for {owner_path}: {why}")),
                    }
                }
            }
        }
    }

    fn visit_trait(&mut self, krate: &str, id: Id, found: &str, hidden: bool, by_path: bool) {
        let c = self.crate_(krate);
        let ItemEnum::Trait(t) = &c.index[&id].inner else { return };
        let generic = t.generics.params.iter().any(|p| !matches!(p.kind, rustdoc_types::GenericParamDefKind::Lifetime { .. }));
        let idx = self.support(krate, id, "trait", found, 0, 0, generic, hidden);
        let owner_key = self.inv.supporting[idx].key.clone();
        let owner_path = self.inv.supporting[idx].canonical_path.clone();
        if by_path {
            self.reachable_traits.insert((krate.to_string(), id));
        }
        for m in &t.items {
            if let Some(ItemEnum::Function(f)) = c.index.get(m).map(|i| &i.inner) {
                let name = c.index[m].name.clone().unwrap();
                let provided = f.has_body;
                let i = self.visit_function(krate, *m, &format!("{owner_path}::{name}"), hidden, CallableKind::TraitMethod, owner_key.clone(), generic, by_path);
                if let Some(i) = i {
                    self.inv.callables[i].is_provided = provided;
                    self.inv.callables[i].trait_reachable |= by_path;
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn visit_function(&mut self, krate: &str, id: Id, found: &str, hidden: bool, kind: CallableKind, owner: String, owner_generic: bool, trait_reachable: bool) -> Option<usize> {
        let key = (krate.to_string(), id);
        if let Some(&i) = self.seen_callable.get(&key) {
            self.inv.callables[i].found_paths.push(found.to_string());
            self.inv.callables[i].hidden &= hidden;
            self.inv.callables[i].trait_reachable |= trait_reachable;
            return Some(i);
        }
        let c = self.crate_(krate);
        let item = &c.index[&id];
        let ItemEnum::Function(f) = &item.inner else { return None };
        let (receiver, params) = split_receiver(&f.sig.inputs);
        let generics = f
            .generics
            .params
            .iter()
            .filter_map(|p| match &p.kind {
                rustdoc_types::GenericParamDefKind::Type { bounds, .. } => Some(GenericParam { name: p.name.clone(), bounds: render::bounds_str(bounds) }),
                rustdoc_types::GenericParamDefKind::Const { type_, .. } => Some(GenericParam { name: p.name.clone(), bounds: format!("const {}", render::ty(type_)) }),
                rustdoc_types::GenericParamDefKind::Lifetime { .. } => None,
            })
            .collect();
        let mut bounds_raw: Vec<(String, Vec<rustdoc_types::GenericBound>)> = f
            .generics
            .params
            .iter()
            .filter_map(|p| match &p.kind {
                rustdoc_types::GenericParamDefKind::Type { bounds, .. } => Some((p.name.clone(), bounds.clone())),
                _ => None,
            })
            .collect();
        for w in &f.generics.where_predicates {
            if let rustdoc_types::WherePredicate::BoundPredicate { type_: Type::Generic(g), bounds, .. } = w {
                match bounds_raw.iter_mut().find(|(n, _)| n == g) {
                    Some((_, b)) => b.extend(bounds.iter().cloned()),
                    None => bounds_raw.push((g.clone(), bounds.clone())),
                }
            }
        }
        let where_clause = f
            .generics
            .where_predicates
            .iter()
            .filter_map(|w| match w {
                rustdoc_types::WherePredicate::BoundPredicate { type_, bounds, .. } => Some(format!("{}: {}", render::ty(type_), render::bounds_str(bounds))),
                rustdoc_types::WherePredicate::EqPredicate { lhs, .. } => Some(format!("{} = ..", render::ty(lhs))),
                rustdoc_types::WherePredicate::LifetimePredicate { .. } => None,
            })
            .collect();
        let canonical_path = match kind {
            CallableKind::FreeFn => item_path(c, id).unwrap_or_else(|| found.to_string()),
            _ => found.to_string(),
        };
        let call = Callable {
            key: format!("{krate}:{}", id.0),
            kind,
            krate: krate.to_string(),
            owner,
            name: item.name.clone().unwrap_or_default(),
            canonical_path,
            found_paths: vec![found.to_string()],
            receiver,
            params,
            ret: f.sig.output.as_ref().map(render::ty),
            ret_raw: f.sig.output.clone(),
            generics,
            where_clause,
            bounds_raw,
            owner_generic,
            owner_aliases: vec![],
            is_unsafe: f.header.is_unsafe,
            is_async: f.header.is_async,
            is_provided: false,
            deprecated: item.deprecation.is_some(),
            hidden,
            unstable: item.stability.as_ref().map(|s| matches!(s.level, rustdoc_types::StabilityLevel::Unstable { .. })).unwrap_or(false),
            implementors: vec![],
            trait_reachable,
            derived: false,
            inputs_raw: f.sig.inputs.clone(),
        };
        self.inv.callables.push(call);
        let i = self.inv.callables.len() - 1;
        self.seen_callable.insert(key, i);
        Some(i)
    }
}

fn split_receiver(inputs: &[(String, Type)]) -> (String, Vec<Param>) {
    let mut params = Vec::new();
    let mut receiver = "none".to_string();
    for (i, (name, ty)) in inputs.iter().enumerate() {
        if i == 0 && name == "self" {
            receiver = match ty {
                Type::Generic(g) if g == "Self" => "self".into(),
                Type::BorrowedRef { is_mutable: false, type_, .. } if matches!(&**type_, Type::Generic(g) if g == "Self") => "&self".into(),
                Type::BorrowedRef { is_mutable: true, type_, .. } if matches!(&**type_, Type::Generic(g) if g == "Self") => "&mut self".into(),
                other => format!("self: {}", render::ty(other)),
            };
            continue;
        }
        params.push(Param { name: name.clone(), ty: render::ty(ty), raw: ty.clone() });
    }
    (receiver, params)
}

/// For a foreign trait impl, take the signature of its first method (e.g.
/// `add(self, rhs)`) so the classifier can judge the argument shapes.
fn first_method_sig(c: &Crate, imp: &rustdoc_types::Impl) -> (String, Vec<Param>, Option<String>, Option<Type>, Vec<(String, Type)>) {
    for m in &imp.items {
        if let Some(ItemEnum::Function(f)) = c.index.get(m).map(|i| &i.inner) {
            let (r, p) = split_receiver(&f.sig.inputs);
            return (r, p, f.sig.output.as_ref().map(render::ty), f.sig.output.clone(), f.sig.inputs.clone());
        }
    }
    ("none".into(), vec![], None, None, vec![])
}
