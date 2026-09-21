//! The subset of the record 0072 inventory the generator reads.
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Inventory {
    pub callables: Vec<Callable>,
    pub supporting: Vec<Supporting>,
    /// Where the inventory came from (record 0075): the crates.io release
    /// or the Git revision of the documented sources. Absent in older
    /// inventories, which the generator refuses.
    #[serde(default)]
    pub provenance: Option<Provenance>,
}

#[derive(Deserialize, Clone, Default, Debug)]
pub struct Provenance {
    #[serde(default)]
    pub release: Option<String>,
    #[serde(default)]
    pub rev: Option<String>,
    #[serde(default)]
    pub cfg: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct Param {
    pub name: String,
    pub ty: String,
    pub ty_canonical: String,
}

#[derive(Deserialize, Clone)]
pub struct Callable {
    pub key: String,
    pub kind: String,
    pub krate: String,
    pub owner: String,
    pub name: String,
    pub canonical_path: String,
    pub found_paths: Vec<String>,
    pub crate_paths: Vec<String>,
    pub receiver: String,
    pub params: Vec<Param>,
    pub ret: Option<String>,
    pub ret_canonical: Option<String>,
    pub generics_canonical: Vec<(String, String)>,
    /// Foreign trait impls (record 0075): the impl's `for` type and its
    /// generic parameters' bounds, canonical. Absent in older inventories.
    #[serde(default)]
    pub impl_for: Option<String>,
    #[serde(default)]
    pub impl_bounds: Vec<(String, String)>,
    pub docs_first: Option<String>,
    pub owner_generic: bool,
    pub is_unsafe: bool,
    pub is_async: bool,
    pub deprecated: bool,
    pub hidden: bool,
    pub implementors: Vec<String>,
    pub trait_reachable: bool,
    pub derived: bool,
    pub bucket: String,
    pub rules: Vec<String>,
}

#[derive(Deserialize, Clone)]
pub struct Supporting {
    pub key: String,
    pub kind: String,
    pub canonical_path: String,
    pub found_paths: Vec<String>,
    pub crate_paths: Vec<String>,
    pub public_fields: usize,
    pub fields_canonical: Vec<(String, String)>,
    pub variant_shapes: Vec<(String, bool)>,
    pub variant_payloads: Vec<(String, Vec<String>)>,
    pub generic: bool,
    pub lifetime: bool,
    pub hidden: bool,
    pub derived: Vec<String>,
    /// For a type alias: the aliased type, canonical, with non-generic
    /// aliases expanded (record 0075). Absent in older inventories.
    #[serde(default)]
    pub alias_target: Option<String>,
    /// For a trait (record 0075): canonical types with a direct impl.
    #[serde(default)]
    pub implementors: Vec<String>,
}
