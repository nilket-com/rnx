//! The subset of the record 0072 inventory the generator reads.
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Inventory {
    pub callables: Vec<Callable>,
    pub supporting: Vec<Supporting>,
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
}
