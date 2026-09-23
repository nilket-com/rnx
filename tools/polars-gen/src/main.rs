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
    /// Record 0084: operations the release refuses to emit although the
    /// mapping rules would admit them, each with the source contract that
    /// the binding would have to validate first.
    #[serde(default)]
    refused: Vec<RefusedOperation>,
    /// Record 0085: canonical paths whose validity `Bitmap` return (direct,
    /// optional or as iterator items) is copied into an owned `Vec<bool>`.
    #[serde(default)]
    bitmap_returns: Vec<String>,
    /// Record 0086: canonical paths whose one `Bitmap` parameter is built
    /// from a script `Vec<bool>`, with the length it must equal.
    #[serde(default)]
    bitmap_inputs: Vec<BitmapInput>,
    /// Record 0087: canonical paths whose concrete iterator return (a
    /// `Map` Polars names by alias) is materialized as an exact-size
    /// iterator of the given integer item, converted with a range check.
    #[serde(default)]
    iterator_returns: Vec<IteratorReturn>,
    /// Record 0088: callables (by inventory key and canonical path) whose
    /// `Cow<Wrapped>` return is made owned inside the call.
    #[serde(default)]
    cow_returns: Vec<CowReturn>,
    /// Record 0089: generic free functions instantiated once per listed
    /// concrete type, their `ChunkedArray<T>` argument spelled as the
    /// type's wrapper and the binding placed on that wrapper.
    #[serde(default)]
    free_instantiations: Vec<FreeInstantiation>,
    /// Record 0092: family-census methods whose one scalar function generic
    /// is bound, per proven pair, to that pair's native type.
    #[serde(default)]
    method_scalar_generics: Vec<MethodScalarGeneric>,
    /// Record 0093: callables (by inventory key and canonical path) whose
    /// `usize` result is proven, from the pinned source, to be a length of
    /// or index into a `Vec`/`IndexMap`, so it fits `i64` and keeps a plain
    /// integer return; every other `usize`/`u64` read-back is checked.
    #[serde(default)]
    bounded_readbacks: Vec<BoundedReadback>,
    /// Record 0094: categorical hashes (by inventory key and canonical path)
    /// carried across the script boundary as exact 16-digit lowercase hex
    /// tokens, as a return or as one named `u64` parameter.
    #[serde(default)]
    hash_tokens: Vec<HashToken>,
    /// Record 0096: family methods (by inventory key and canonical path)
    /// whose exact `Either<Vec<T::Native>, Vec<Option<T::Native>>>` return,
    /// on the listed types, becomes one owned `Vec<Option<script value>>`,
    /// bounded before the Polars call.
    #[serde(default)]
    null_aware_returns: Vec<NullAwareReturn>,
    /// Record 0097: `ChunkedArray` methods whose only function-level
    /// generic is `Self: Sized`, admitted on every proven family pair with
    /// the exact listed signature, and guarded by the receiver length.
    #[serde(default)]
    sized_self_methods: Vec<SizedSelfMethod>,
    /// Record 0098: `ChunkedArray` methods whose only unprovable impl bound
    /// is a cited external trait on `T::Native` (`num_traits::float::Float`,
    /// and `Canonical`), discharged for the listed float pairs only.
    #[serde(default)]
    external_bounds: Vec<ExternalBound>,
    /// Record 0099: `ChunkedArray::chunks` on the listed numeric pairs,
    /// copied into owned nested option vectors that keep chunk boundaries.
    #[serde(default)]
    chunk_snapshots: Vec<ChunkSnapshot>,
    /// Record 0101: `ChunkedArray::downcast_get` on the listed pairs, the
    /// selected chunk copied into an owned optional vector of options.
    #[serde(default)]
    indexed_chunk_snapshots: Vec<IndexedChunkSnapshot>,
    /// Record 0102: `ChunkedArray::downcast_as_array` on the listed pairs,
    /// the one array copied into an owned vector of options.
    #[serde(default)]
    array_snapshots: Vec<ArraySnapshot>,
    /// Record 0103: `ChunkedArray::downcast_iter` on the listed pairs, the
    /// typed chunk iterator driven into owned nested option vectors.
    #[serde(default)]
    iter_snapshots: Vec<IterSnapshot>,
    /// Record 0079: mutable closure arguments with an audited read-back contract.
    #[serde(default)]
    callback_mutable: Vec<CallbackMutable>,
    #[serde(default)]
    callback_invocation: Vec<CallbackInvocation>,
    #[serde(default)]
    callback_sink: Vec<CallbackSink>,
    #[serde(default)]
    callback_safe: Vec<CallbackSafe>,
    #[serde(default)]
    callback_recipe: Vec<CallbackRecipe>,
    /// Record 0076: which alias families get instantiated bindings; empty
    /// means every family. The shipped set under the launch budget is
    /// recorded here, as the recipe that selected it.
    #[serde(default)]
    instantiation: InstantiationScope,
}
#[derive(serde::Deserialize, Clone, Default)]
struct InstantiationScope {
    #[serde(default)]
    families: Vec<String>,
    /// Proven pairs the compiler refuses for a reason the inventory cannot
    /// see (a `no_call_const` in the pinned sources), excluded with a
    /// citation; each is a route exception, never a silent skip.
    #[serde(default)]
    exclude: Vec<InstantiationExclude>,
}
#[derive(serde::Deserialize, Clone)]
struct InstantiationExclude {
    alias: String,
    methods: Vec<String>,
    cite: String,
}
#[derive(serde::Deserialize, Clone, Default)]
struct ReleaseProvenance {
    #[serde(default)]
    release: Option<String>,
    #[serde(default)]
    rev: Option<String>,
    /// Record 0081: the documentation configuration (`cfg` in pins.json)
    /// and the Polars features it must have resolved. When named, an
    /// inventory documented under another configuration or without every
    /// listed feature is refused: the inventory is the coverage
    /// denominator, and a feature-only change must not be reported
    /// against an inventory that predates it.
    #[serde(default)]
    cfg: Option<String>,
    #[serde(default)]
    features: Vec<String>,
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
#[derive(serde::Deserialize, Clone)]
struct ChunkSnapshot {
    key: String,
    path: String,
    /// `[owner type, native]` numeric pairs
    pairs: Vec<(String, String)>,
    cite: String,
}
/// Record 0099: the exact canonical return a chunk snapshot maps.
const CHUNKS_RETURN: &str = "&alloc::vec::Vec<polars_arrow::array::ArrayRef>";
/// Record 0100: the non-numeric owners a chunk snapshot admits, with their
/// kind (polars-core datatypes/mod.rs:229-232: Utf8ViewArray,
/// BinaryViewArray, BinaryArray<i64>, BooleanArray), the support copier,
/// the script element type, the Arrow array and the oracle's element type.
const SCALAR_CHUNKS: &[(&str, &str, &str, &str, &str, &str)] = &[
    ("polars_core::datatypes::BooleanType", "bool", "support::chunk_snapshot_bool", "bool", "polars_arrow::array::BooleanArray", "bool"),
    ("polars_core::datatypes::StringType", "str", "support::chunk_snapshot_str", "String", "polars_arrow::array::Utf8ViewArray", "alloc::string::String"),
    ("polars_core::datatypes::BinaryType", "binary", "support::chunk_snapshot_binview", "Vec<i64>", "polars_arrow::array::BinaryViewArray", "alloc::vec::Vec<u8>"),
    ("polars_core::datatypes::BinaryOffsetType", "binary_offset", "support::chunk_snapshot_binary_offset", "Vec<i64>", "polars_arrow::array::BinaryArray<i64>", "alloc::vec::Vec<u8>"),
];
impl ChunkSnapshot {
    /// Fail closed on anything but the cited shape: `&self` with no
    /// parameters or method generics on `ChunkedArray<T>` with exactly
    /// `T: PolarsDataType` and no where-clauses, returning exactly
    /// `&Vec<ArrayRef>`, with numeric pairs from the fixed table.
    fn check(&self, c: &Callable) -> Result<(), String> {
        if self.cite.trim().is_empty() {
            return Err("no citation".into());
        }
        if c.impl_head.as_deref() != Some("polars_core::chunked_array::ChunkedArray<T>") || c.impl_bounds != [("T".to_string(), "polars_core::datatypes::PolarsDataType".to_string())] || !c.impl_where.is_empty() {
            return Err(format!("impl {} with {:?} / {:?} is not ChunkedArray<T: PolarsDataType>", c.impl_head.as_deref().unwrap_or("none"), c.impl_bounds, c.impl_where));
        }
        if c.receiver != "&self" || !c.params.is_empty() || !c.generics_canonical.is_empty() {
            return Err("not a `&self` method without parameters or method generics".into());
        }
        if c.ret_canonical.as_deref() != Some(CHUNKS_RETURN) {
            return Err(format!("return {} is not {CHUNKS_RETURN}", c.ret_canonical.as_deref().unwrap_or("()")));
        }
        if self.pairs.is_empty() {
            return Err("no listed pair".into());
        }
        for (i, (t, n)) in self.pairs.iter().enumerate() {
            if !NUMERIC_NATIVES.iter().any(|(nt, nn)| nt == t && nn == n) && !SCALAR_CHUNKS.iter().any(|(st, sk, ..)| st == t && sk == n) {
                return Err(format!("`{t}` with `{n}` is not a numeric type and its native, nor a listed scalar owner and its kind"));
            }
            if self.pairs[..i].iter().any(|(u, _)| u == t) {
                return Err(format!("`{t}` is listed twice"));
            }
        }
        Ok(())
    }
    fn native_for(&self, identity: &str) -> Option<&str> {
        let t = identity.strip_prefix("polars_core::chunked_array::ChunkedArray<")?.strip_suffix('>')?;
        self.pairs.iter().find(|(x, _)| x == t).map(|(_, n)| n.as_str())
    }
}
#[derive(serde::Deserialize, Clone)]
struct IndexedChunkSnapshot {
    key: String,
    path: String,
    /// `[owner type, native or scalar kind]`, from NUMERIC_NATIVES or SCALAR_CHUNKS
    pairs: Vec<(String, String)>,
    cite: String,
}
/// Record 0101: the exact canonical return an indexed chunk snapshot maps.
const DOWNCAST_GET_RETURN: &str = "core::option::Option<&T::Array>";
/// Record 0101: the concrete Arrow array `T::Array` resolves to, per native
/// or scalar kind, which the substituted return must name exactly.
fn indexed_array(kind: &str) -> Option<String> {
    Some(match kind {
        "bool" => "polars_arrow::array::boolean::BooleanArray".into(),
        "str" => "polars_arrow::array::binview::BinaryViewArrayGeneric<str>".into(),
        "binary" => "polars_arrow::array::binview::BinaryViewArrayGeneric<[u8]>".into(),
        "binary_offset" => "polars_arrow::array::binary::BinaryArray<i64>".into(),
        n if NUMERIC_NATIVES.iter().any(|(_, x)| *x == n) => format!("polars_arrow::array::primitive::PrimitiveArray<{n}>"),
        _ => return None,
    })
}
impl IndexedChunkSnapshot {
    /// Fail closed on anything but the cited shape: `&self` with exactly one
    /// `usize` parameter and no method generics, on `ChunkedArray<T>` with
    /// exactly `T: PolarsDataType` and no where-clauses, returning exactly
    /// `Option<&T::Array>`, with pairs from the fixed tables.
    fn check(&self, c: &Callable) -> Result<(), String> {
        if self.cite.trim().is_empty() {
            return Err("no citation".into());
        }
        if c.impl_head.as_deref() != Some("polars_core::chunked_array::ChunkedArray<T>") || c.impl_bounds != [("T".to_string(), "polars_core::datatypes::PolarsDataType".to_string())] || !c.impl_where.is_empty() {
            return Err(format!("impl {} with {:?} / {:?} is not ChunkedArray<T: PolarsDataType>", c.impl_head.as_deref().unwrap_or("none"), c.impl_bounds, c.impl_where));
        }
        if c.receiver != "&self" || !c.generics_canonical.is_empty() || c.params.len() != 1 || c.params[0].ty_canonical != "usize" {
            return Err("not a `&self` method taking one usize, without method generics".into());
        }
        if c.ret_canonical.as_deref() != Some(DOWNCAST_GET_RETURN) {
            return Err(format!("return {} is not {DOWNCAST_GET_RETURN}", c.ret_canonical.as_deref().unwrap_or("()")));
        }
        if self.pairs.is_empty() {
            return Err("no listed pair".into());
        }
        for (i, (t, n)) in self.pairs.iter().enumerate() {
            if !NUMERIC_NATIVES.iter().any(|(nt, nn)| nt == t && nn == n) && !SCALAR_CHUNKS.iter().any(|(st, sk, ..)| st == t && sk == n) {
                return Err(format!("`{t}` with `{n}` is not a numeric type and its native, nor a listed scalar owner and its kind"));
            }
            if self.pairs[..i].iter().any(|(u, _)| u == t) {
                return Err(format!("`{t}` is listed twice"));
            }
        }
        Ok(())
    }
    fn native_for(&self, identity: &str) -> Option<&str> {
        let t = identity.strip_prefix("polars_core::chunked_array::ChunkedArray<")?.strip_suffix('>')?;
        self.pairs.iter().find(|(x, _)| x == t).map(|(_, n)| n.as_str())
    }
}
#[derive(serde::Deserialize, Clone)]
struct ArraySnapshot {
    key: String,
    path: String,
    /// `[owner type, native or scalar kind]`, from NUMERIC_NATIVES or SCALAR_CHUNKS
    pairs: Vec<(String, String)>,
    cite: String,
}
/// Record 0102: the exact canonical return an array snapshot maps.
const DOWNCAST_AS_ARRAY_RETURN: &str = "&T::Array";
impl ArraySnapshot {
    /// Fail closed on anything but the cited shape: `&self` with no
    /// parameters or method generics, on `ChunkedArray<T>` with exactly
    /// `T: PolarsDataType` and no where-clauses, returning exactly
    /// `&T::Array`, with pairs from the fixed tables.
    fn check(&self, c: &Callable) -> Result<(), String> {
        let as_indexed = IndexedChunkSnapshot { key: self.key.clone(), path: self.path.clone(), pairs: self.pairs.clone(), cite: self.cite.clone() };
        let mut shaped = c.clone();
        // reuse 0101's checks of citation, impl, receiver, generics and pairs,
        // with this method's own parameters and return checked here
        if !c.params.is_empty() {
            return Err("not a `&self` method without parameters or method generics".into());
        }
        if c.ret_canonical.as_deref() != Some(DOWNCAST_AS_ARRAY_RETURN) {
            return Err(format!("return {} is not {DOWNCAST_AS_ARRAY_RETURN}", c.ret_canonical.as_deref().unwrap_or("()")));
        }
        shaped.params = vec![Param { name: "idx".into(), ty: "usize".into(), ty_canonical: "usize".into() }];
        shaped.ret_canonical = Some(DOWNCAST_GET_RETURN.into());
        as_indexed.check(&shaped).map_err(|e| e.replace("taking one usize, without method generics", "without parameters or method generics"))
    }
    fn native_for(&self, identity: &str) -> Option<&str> {
        let t = identity.strip_prefix("polars_core::chunked_array::ChunkedArray<")?.strip_suffix('>')?;
        self.pairs.iter().find(|(x, _)| x == t).map(|(_, n)| n.as_str())
    }
}
#[derive(serde::Deserialize, Clone)]
struct IterSnapshot {
    key: String,
    path: String,
    /// `[owner type, native or scalar kind]`, from NUMERIC_NATIVES or SCALAR_CHUNKS
    pairs: Vec<(String, String)>,
    cite: String,
}
/// Record 0103: the exact canonical return an iterator snapshot maps.
const DOWNCAST_ITER_RETURN: &str = "impl core::iter::traits::double_ended::DoubleEndedIterator<Item = &T::Array>";
impl IterSnapshot {
    /// Fail closed on anything but the cited shape: no parameters and
    /// exactly the borrowed typed-array iterator return, then 0101's checks
    /// of citation, impl, receiver, generics and pairs.
    fn check(&self, c: &Callable) -> Result<(), String> {
        if !c.params.is_empty() {
            return Err("not a `&self` method without parameters or method generics".into());
        }
        // exact text: `Ty::render` prints only an `impl` type's trait paths,
        // so it cannot tell `Item = &T::Array` from `Item = T::Array`
        let squash = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        if c.ret_canonical.as_deref().map(squash) != Some(squash(DOWNCAST_ITER_RETURN)) {
            return Err(format!("return {} is not {DOWNCAST_ITER_RETURN}", c.ret_canonical.as_deref().unwrap_or("()")));
        }
        let mut shaped = c.clone();
        shaped.params = vec![Param { name: "idx".into(), ty: "usize".into(), ty_canonical: "usize".into() }];
        shaped.ret_canonical = Some(DOWNCAST_GET_RETURN.into());
        IndexedChunkSnapshot { key: self.key.clone(), path: self.path.clone(), pairs: self.pairs.clone(), cite: self.cite.clone() }.check(&shaped).map_err(|e| e.replace("taking one usize, without method generics", "without parameters or method generics"))
    }
    fn native_for(&self, identity: &str) -> Option<&str> {
        let t = identity.strip_prefix("polars_core::chunked_array::ChunkedArray<")?.strip_suffix('>')?;
        self.pairs.iter().find(|(x, _)| x == t).map(|(_, n)| n.as_str())
    }
}
fn iter_snapshot_entry<'a>(release: &'a Release, c: &Callable) -> Option<Result<&'a IterSnapshot, String>> {
    let listed: Vec<&IterSnapshot> = release.iter_snapshots.iter().filter(|m| m.key == c.key && m.path == c.canonical_path).collect();
    match listed.as_slice() {
        [] => None,
        [m] => Some(m.check(c).map(|_| *m)),
        _ => Some(Err("listed twice".into())),
    }
}
fn array_snapshot_entry<'a>(release: &'a Release, c: &Callable) -> Option<Result<&'a ArraySnapshot, String>> {
    let listed: Vec<&ArraySnapshot> = release.array_snapshots.iter().filter(|m| m.key == c.key && m.path == c.canonical_path).collect();
    match listed.as_slice() {
        [] => None,
        [m] => Some(m.check(c).map(|_| *m)),
        _ => Some(Err("listed twice".into())),
    }
}
fn indexed_chunk_entry<'a>(release: &'a Release, c: &Callable) -> Option<Result<&'a IndexedChunkSnapshot, String>> {
    let listed: Vec<&IndexedChunkSnapshot> = release.indexed_chunk_snapshots.iter().filter(|m| m.key == c.key && m.path == c.canonical_path).collect();
    match listed.as_slice() {
        [] => None,
        [m] => Some(m.check(c).map(|_| *m)),
        _ => Some(Err("listed twice".into())),
    }
}
fn chunk_snapshot_entry<'a>(release: &'a Release, c: &Callable) -> Option<Result<&'a ChunkSnapshot, String>> {
    let listed: Vec<&ChunkSnapshot> = release.chunk_snapshots.iter().filter(|m| m.key == c.key && m.path == c.canonical_path).collect();
    match listed.as_slice() {
        [] => None,
        [m] => Some(m.check(c).map(|_| *m)),
        _ => Some(Err("listed twice".into())),
    }
}
#[derive(serde::Deserialize, Clone)]
struct ExternalBound {
    key: String,
    path: String,
    /// the method's exact canonical return
    ret: String,
    /// the method's complete, exact `impl_where` list
    #[serde(rename = "where")]
    where_: Vec<String>,
    /// the clauses of `where` this entry discharges
    discharge: Vec<String>,
    /// `[owner type, native]` pairs the discharge holds for
    pairs: Vec<(String, String)>,
    cite: String,
}
/// Record 0098: the float owner types and their natives; an external float
/// bound is discharged only on these.
const FLOAT_NATIVES: &[(&str, &str)] = &[("polars_core::datatypes::Float32Type", "f32"), ("polars_core::datatypes::Float64Type", "f64")];
impl ExternalBound {
    /// Fail closed on anything but the cited shape: the `ChunkedArray<T>`
    /// head with exactly `T: PolarsFloatType`, `&self`, no parameters or
    /// method generics, the listed return, the listed where-clauses exactly,
    /// discharges drawn from them (never the owner bound), and float pairs
    /// from the fixed table without duplicates.
    fn check(&self, c: &Callable) -> Result<(), String> {
        if self.cite.trim().is_empty() {
            return Err("no citation".into());
        }
        if c.impl_head.as_deref() != Some("polars_core::chunked_array::ChunkedArray<T>") {
            return Err(format!("impl head {} is not ChunkedArray<T>", c.impl_head.as_deref().unwrap_or("none")));
        }
        if c.impl_bounds != [("T".to_string(), "polars_core::datatypes::PolarsFloatType".to_string())] {
            return Err(format!("impl bounds {:?} are not exactly T: PolarsFloatType", c.impl_bounds));
        }
        if c.receiver != "&self" || !c.params.is_empty() || !c.generics_canonical.is_empty() {
            return Err("not a `&self` method without parameters or method generics".into());
        }
        if c.ret_canonical.as_deref() != Some(self.ret.as_str()) {
            return Err(format!("return {} is not the listed {}", c.ret_canonical.as_deref().unwrap_or("()"), self.ret));
        }
        if c.impl_where != self.where_ {
            return Err(format!("where-clauses {:?} are not the listed {:?}", c.impl_where, self.where_));
        }
        if self.discharge.is_empty() || self.discharge.iter().any(|d| !self.where_.contains(d) || !d.starts_with("T::Native: ")) {
            return Err(format!("discharged clauses {:?} are not listed `T::Native` where-clauses", self.discharge));
        }
        if self.pairs.is_empty() {
            return Err("no listed pair".into());
        }
        for (i, (t, n)) in self.pairs.iter().enumerate() {
            if !FLOAT_NATIVES.iter().any(|(ft, fnat)| ft == t && fnat == n) {
                return Err(format!("`{t}` with `{n}` is not a float type and its native"));
            }
            if self.pairs[..i].iter().any(|(u, _)| u == t) {
                return Err(format!("`{t}` is listed twice"));
            }
        }
        Ok(())
    }
}
/// Record 0098: the release's external-bound entry for a callable: `None`
/// if unlisted, `Err` naming the fault if malformed or listed twice.
fn external_bound_entry<'a>(release: &'a Release, c: &Callable) -> Option<Result<&'a ExternalBound, String>> {
    let listed: Vec<&ExternalBound> = release.external_bounds.iter().filter(|m| m.key == c.key && m.path == c.canonical_path).collect();
    match listed.as_slice() {
        [] => None,
        [m] => Some(m.check(c).map(|_| *m)),
        _ => Some(Err("listed twice".into())),
    }
}
#[derive(serde::Deserialize, Clone)]
struct SizedSelfMethod {
    key: String,
    path: String,
    /// the exact canonical parameter types, in order
    params: Vec<String>,
    cite: String,
}
impl SizedSelfMethod {
    /// Fail closed on anything but the cited shape: a `ChunkedArray`
    /// method taking `&self`, whose only function-level generic is exactly
    /// `Self: Sized`, with the listed parameter types and a `Self` return.
    fn check(&self, c: &Callable) -> Result<(), String> {
        if self.cite.trim().is_empty() {
            return Err("no citation".into());
        }
        if c.owner != "polars_core::chunked_array::ChunkedArray" {
            return Err(format!("owner {} is not ChunkedArray", c.owner));
        }
        if c.receiver != "&self" {
            return Err(format!("receiver {} is not &self", c.receiver));
        }
        if c.generics_canonical != [("Self".to_string(), "core::marker::Sized".to_string())] {
            return Err(format!("method generics {:?} are not exactly Self: Sized", c.generics_canonical));
        }
        if c.ret_canonical.as_deref() != Some("Self") {
            return Err(format!("return {} is not Self", c.ret_canonical.as_deref().unwrap_or("()")));
        }
        let got: Vec<&str> = c.params.iter().map(|p| p.ty_canonical.as_str()).collect();
        if got != self.params.iter().map(String::as_str).collect::<Vec<_>>() {
            return Err(format!("parameters {got:?} are not the listed {:?}", self.params));
        }
        Ok(())
    }
}
/// Record 0097: the release's sized-self entry for a callable, validated:
/// `None` if unlisted, `Err` naming the fault if listed but malformed
/// (including listed twice).
fn sized_self_entry<'a>(release: &'a Release, c: &Callable) -> Option<Result<&'a SizedSelfMethod, String>> {
    let listed: Vec<&SizedSelfMethod> = release.sized_self_methods.iter().filter(|m| m.key == c.key && m.path == c.canonical_path).collect();
    match listed.as_slice() {
        [] => None,
        [m] => Some(m.check(c).map(|_| *m)),
        _ => Some(Err("listed twice".into())),
    }
}
#[derive(serde::Deserialize, Clone)]
struct NullAwareReturn {
    key: String,
    path: String,
    /// the owner types (`polars_core::datatypes::Int8Type`, ...) it applies to
    types: Vec<String>,
    cite: String,
}
impl NullAwareReturn {
    /// Fail closed before any pair is lifted: a citation, at least one
    /// type, every type numeric with a known native, no duplicates.
    fn check(&self) -> Result<(), String> {
        if self.cite.trim().is_empty() {
            return Err("no citation".into());
        }
        if self.types.is_empty() {
            return Err("no listed type".into());
        }
        for (i, t) in self.types.iter().enumerate() {
            if !NUMERIC_NATIVES.iter().any(|(ty, _)| ty == t) {
                return Err(format!("`{t}` is not a numeric type with a known native"));
            }
            if self.types[..i].contains(t) {
                return Err(format!("`{t}` is listed twice"));
            }
        }
        Ok(())
    }
    /// The native of a pair's `ChunkedArray<T>` identity, if `T` is listed.
    fn native_for(&self, identity: &str) -> Option<&'static str> {
        let t = identity.strip_prefix("polars_core::chunked_array::ChunkedArray<")?.strip_suffix('>')?;
        if !self.types.iter().any(|x| x == t) {
            return None;
        }
        NUMERIC_NATIVES.iter().find(|(ty, _)| *ty == t).map(|(_, n)| *n)
    }
}
/// Record 0096: the one return a listed null-aware pair may have, for its
/// native, compared on the whole substituted type (a nested or different
/// return is refused before any binding text).
fn null_aware_return_matches(ret: Option<&str>, native: &str) -> bool {
    let want = format!("either::Either<alloc::vec::Vec<{native}>, alloc::vec::Vec<core::option::Option<{native}>>>");
    ret.is_some_and(|r| ty::parse(r).render() == ty::parse(&want).render())
}
#[derive(serde::Deserialize, Clone)]
struct HashToken {
    key: String,
    path: String,
    /// `return` or `parameter`
    direction: String,
    /// the parameter's name, for `parameter` only
    #[serde(default)]
    param: String,
    /// `u64` or, for a return, `Option<u64>`
    source: String,
    cite: String,
}

/// Record 0094: the hash-token scope of one callable, validated before any
/// binding text: `(return is a token, the token parameter)`. Every listed
/// entry must match the callable's exact types, or the callable is refused.
fn hash_token_scope(release: &Release, c: &Callable) -> Result<Option<(bool, Option<String>)>, String> {
    let listed: Vec<&HashToken> = release.hash_tokens.iter().filter(|h| h.key == c.key && h.path == c.canonical_path).collect();
    if listed.is_empty() {
        return Ok(None);
    }
    let (mut ret, mut param) = (false, None);
    for h in listed {
        if h.cite.trim().is_empty() {
            return Err(format!("hash token entry for {} has no citation", h.path));
        }
        match h.direction.as_str() {
            "return" => {
                if !h.param.is_empty() {
                    return Err(format!("a return hash token names a parameter ({})", h.param));
                }
                let want = match h.source.as_str() {
                    "u64" => "u64",
                    "Option<u64>" => "core::option::Option<u64>",
                    other => return Err(format!("hash token return source {other} is not u64 or Option<u64>")),
                };
                if c.ret_canonical.as_deref() != Some(want) {
                    return Err(format!("hash token return {} does not match the callable's {}", h.source, c.ret_canonical.as_deref().unwrap_or("()")));
                }
                if ret {
                    return Err("duplicate return hash token".into());
                }
                ret = true;
            }
            "parameter" => {
                if h.source != "u64" {
                    return Err(format!("hash token parameter source {} is not u64", h.source));
                }
                match c.params.iter().find(|p| p.name == h.param) {
                    Some(p) if p.ty_canonical == "u64" => {}
                    Some(p) => return Err(format!("hash token parameter {} is {}, not u64", h.param, p.ty_canonical)),
                    None => return Err(format!("hash token parameter {} is not a parameter", h.param)),
                }
                if param.is_some() {
                    return Err("duplicate parameter hash token".into());
                }
                param = Some(h.param.clone());
            }
            other => return Err(format!("hash token direction {other} is not return or parameter")),
        }
    }
    Ok(Some((ret, param)))
}
#[derive(serde::Deserialize, Clone)]
struct BoundedReadback {
    key: String,
    path: String,
    cite: String,
}
#[derive(serde::Deserialize, Clone)]
struct MethodScalarGeneric {
    key: String,
    path: String,
    /// The method's function generic (`N`), bound to the pair's native.
    generic: String,
    /// Owner type parameters admitted and their natives, in order.
    types: Vec<String>,
    natives: Vec<String>,
    cite: String,
}
/// Record 0092 review: `T::Native` of each Polars physical numeric type
/// (polars-core 0.55.2 `datatypes/mod.rs`, `impl_polars_num_datatype!`).
/// A release entry's native must be exactly this, or it is refused: a
/// wrong native still compiles where the scalar is an independent generic,
/// and Polars's `NumCast::from(..).expect(..)` would panic at run time.
const NUMERIC_NATIVES: &[(&str, &str)] = &[
    ("polars_core::datatypes::Int8Type", "i8"), ("polars_core::datatypes::Int16Type", "i16"),
    ("polars_core::datatypes::Int32Type", "i32"), ("polars_core::datatypes::Int64Type", "i64"),
    ("polars_core::datatypes::UInt8Type", "u8"), ("polars_core::datatypes::UInt16Type", "u16"),
    ("polars_core::datatypes::UInt32Type", "u32"), ("polars_core::datatypes::UInt64Type", "u64"),
    ("polars_core::datatypes::Float32Type", "f32"), ("polars_core::datatypes::Float64Type", "f64"),
];
fn check_natives(path: &str, types: &[String], natives: &[String]) -> Result<(), String> {
    if types.len() != natives.len() { return Err(format!("`{path}`: types and natives must pair up")); }
    for (t, n) in types.iter().zip(natives) {
        match NUMERIC_NATIVES.iter().find(|(ty, _)| ty == t) {
            None => return Err(format!("`{path}`: `{t}` is not a known numeric type")),
            Some((_, want)) if want != n => return Err(format!("`{path}`: `{t}`'s native is `{want}`, not `{n}`")),
            Some(_) => {}
        }
    }
    Ok(())
}
impl MethodScalarGeneric {
    /// Fail closed: the entry must name one function generic of the callable,
    /// that generic may appear only as a whole parameter type, and the type
    /// and native lists must pair up.
    fn check(&self, c: &Callable) -> Result<(), String> {
        if self.types.is_empty() { return Err(format!("`{}`: no types", self.path)); }
        check_natives(&self.path, &self.types, &self.natives)?;
        if c.generics_canonical.len() != 1 || c.generics_canonical[0].0 != self.generic { return Err(format!("`{}`: `{}` is not the callable's only function generic", self.path, self.generic)); }
        if c.ret_canonical.as_deref().is_some_and(|r| mentions(r, &self.generic)) { return Err(format!("`{}`: `{}` appears in the return", self.path, self.generic)); }
        if !c.params.iter().any(|p| p.ty_canonical.trim() == self.generic) { return Err(format!("`{}`: no parameter is exactly `{}`", self.path, self.generic)); }
        if c.params.iter().any(|p| p.ty_canonical.trim() != self.generic && mentions(&p.ty_canonical, &self.generic)) { return Err(format!("`{}`: `{}` appears inside another parameter type", self.path, self.generic)); }
        Ok(())
    }
    fn native_for(&self, identity: &str) -> Option<&str> {
        self.types.iter().position(|t| identity == format!("polars_core::chunked_array::ChunkedArray<{t}>")).map(|i| self.natives[i].as_str())
    }
}
#[derive(serde::Deserialize, Clone)]
struct FreeInstantiation {
    key: String,
    path: String,
    /// The public path the binding calls; Rust infers `T` from the argument.
    callee: String,
    /// The generic parameter and its concrete types, one binding each.
    generic: String,
    types: Vec<String>,
    /// Record 0090: the native scalar of each listed type, in the same
    /// order, for `T::Native` in the parameters (empty when unused).
    #[serde(default)]
    natives: Vec<String>,
    /// Record 0091: a parameter checked before the call, and the check
    /// (`below_idx_max`: the converted `usize` must be `< IdxSize::MAX`).
    #[serde(default)]
    guard_param: Option<String>,
    #[serde(default)]
    guard: Option<String>,
    cite: String,
}
#[derive(serde::Deserialize, Clone)]
struct CowReturn {
    key: String,
    path: String,
    cite: String,
}
#[derive(serde::Deserialize, Clone)]
struct IteratorReturn {
    path: String,
    item: String,
    cite: String,
}
#[derive(serde::Deserialize, Clone)]
struct BitmapInput {
    path: String,
    /// `receiver` (the receiver's total length), `values` (the length of
    /// the parameter named `values`) or `none`.
    length: String,
    cite: String,
}
#[derive(serde::Deserialize, Clone)]
struct RefusedOperation {
    path: String,
    reason: String,
    cite: String,
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
        if let Some(want) = &self.provenance.cfg {
            match &p.cfg {
                Some(have) if have == want => {}
                other => return Err(format!("release file `{}` is for configuration {want}; the inventory records {other:?}", self.name)),
            }
        }
        if !self.provenance.features.is_empty() {
            // The complete resolved set, compared exactly: a missing base
            // feature or an extra one documents a different surface.
            let Some(have) = &p.features else { return Err(format!("release file `{}` pins features {:?}; the inventory records no feature set (re-extract it with the record 0081 extractor)", self.name, self.provenance.features)) };
            let want: std::collections::BTreeSet<&String> = self.provenance.features.iter().collect();
            let have: std::collections::BTreeSet<&String> = have.iter().collect();
            if want != have {
                let missing: Vec<&&String> = want.difference(&have).collect();
                let extra: Vec<&&String> = have.difference(&want).collect();
                return Err(format!("release file `{}` pins the resolved feature set {:?}; the inventory's set lacks {missing:?} and adds {extra:?}", self.name, self.provenance.features));
            }
        }
        Ok(format!("release {:?} rev {:?} cfg {:?} features {}", p.release, p.rev, p.cfg, p.features.as_ref().map_or("unrecorded".to_string(), |f| format!("{} resolved", f.len()))))
    }
}

/// Record 0075 gate 3 controls: equivalent aliases resolve to one wrapper;
/// same-name distinct types resolve to two Rune paths.
// ---------------------------------------------------------------- instantiation (record 0076)

/// The result of checking one impl against one alias instantiation. Only
/// `Proven` yields a binding; the other two are counted exceptions.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "result", content = "reason")]
enum Applicability {
    Proven,
    Rejected(String),
    Unresolved(String),
}

impl Applicability {
    fn and(self, other: Applicability) -> Applicability {
        match (self, other) {
            (Applicability::Proven, o) => o,
            (Applicability::Rejected(r), _) => Applicability::Rejected(r),
            (Applicability::Unresolved(_), Applicability::Rejected(r)) => Applicability::Rejected(r),
            (u @ Applicability::Unresolved(_), _) => u,
        }
    }
}

/// `Base<args>` split at the top level: the base path and its arguments.
fn split_head(t: &str) -> (String, Vec<String>) {
    let t = t.trim();
    match t.find('<') {
        Some(i) if t.ends_with('>') => (t[..i].to_string(), split_top(&t[i + 1..t.len() - 1])),
        _ => (t.to_string(), vec![]),
    }
}

/// A generic parameter name: one identifier, no path.
fn is_param(s: &str) -> bool {
    !s.is_empty() && !s.contains("::") && s.chars().all(|c| c.is_alphanumeric() || c == '_') && s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// Replace whole-identifier generic parameters by their bindings.
fn subst_params(s: &str, subst: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_alphabetic() || b[i] == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let tok = &s[start..i];
            let prev = if start >= 2 { &s[start - 2..start] } else { "" };
            let next = if i + 2 <= b.len() { &s[i..i + 2] } else { "" };
            // a later path segment is not a parameter (`foo::T`); a leading one
            // is (`T::Native` becomes `Int64Type::Native` for the projection step)
            let _ = next;
            if prev != "::" {
                if let Some(v) = subst.get(tok) {
                    out.push_str(v);
                    continue;
                }
            }
            out.push_str(tok);
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

/// A trait bound `path<Assoc = X, …>` split into the trait path and its
/// associated-type constraints. `Err` for any other argument form (a
/// generic trait argument, a lifetime, a parenthesized signature): the
/// grammar this evaluation supports is bare traits and associated-type
/// equalities, and anything else is unresolved, never weakened.
fn split_bound(bound: &str) -> Result<(String, Vec<(String, String)>), String> {
    if bound.contains('(') || bound.starts_with('\'') {
        return Err(format!("bound syntax `{bound}` is not modelled"));
    }
    let (path, args) = split_head(bound);
    let mut cons = Vec::new();
    for a in &args {
        match a.split_once(" = ") {
            Some((k, v)) => cons.push((k.trim().to_string(), v.trim().to_string())),
            None => return Err(format!("trait argument `{a}` in `{bound}` is not modelled")),
        }
    }
    Ok((path, cons))
}

/// The core derivable traits a builtin type implements, by type: floats
/// have no `Eq`, `Ord` or `Hash`; the owned string is not `Copy`; unit
/// has no `Display`; unsized `str` is neither `Clone` nor `Default`, and
/// a `&T` is `Copy` and `Clone` while a `&mut T` is neither. Anything
/// else is not a builtin and gets no answer here.
fn core_trait_on_builtin(ty: &str, short: &str) -> Option<bool> {
    let float = matches!(ty, "f32" | "f64");
    let owned_string = ty == "alloc::string::String";
    let str_slice = ty == "str";
    let unit = ty == "()";
    let shared_ref = ty.starts_with('&') && !ty.starts_with("&mut ");
    let mut_ref = ty.starts_with("&mut ");
    if shared_ref || mut_ref {
        return match short {
            "Copy" | "Clone" => Some(shared_ref),
            _ => None,
        };
    }
    if !(float || owned_string || str_slice || unit || SCALARS.contains(&ty)) {
        return None;
    }
    Some(match short {
        "Debug" | "PartialEq" | "PartialOrd" => true,
        "Clone" | "Default" => !str_slice,
        "Eq" | "Ord" | "Hash" => !float,
        "Copy" => !(owned_string || str_slice),
        "Display" => !unit,
        _ => return None,
    })
}

/// Whether a canonical type is `Sized`: references, scalars, unit, the
/// owned string, `PlSmallStr`, and reachable structs, enums and unions
/// (bare or instantiated) are; `str`, slices and trait objects are not;
/// anything else is unknown.
fn sizedness(world: &World, ty: &str) -> Option<bool> {
    let t = ty.trim();
    if t.starts_with('&') { return Some(true); }
    if t == "str" || t.starts_with('[') || t.starts_with("dyn ") || t.starts_with("impl ") { return Some(false); }
    if t == "()" || SCALARS.contains(&t) || t == "alloc::string::String" || t == "polars_utils::pl_str::PlSmallStr" || t.starts_with('(') { return Some(true); }
    let (base, _) = split_head(t);
    match world.types.get(&base) {
        Some(s) if matches!(s.kind.as_str(), "struct" | "enum" | "union") => Some(true),
        Some(s) if s.kind == "type_alias" => s.alias_target.as_deref().and_then(|target| sizedness(world, target)),
        _ => None,
    }
}

impl World {
    /// The associated type `assoc` that some recorded impl on `ty` binds;
    /// `None` when no impl binds it or two impls disagree.
    fn assoc_of(&self, ty: &str, assoc: &str, trait_hint: Option<&str>) -> Option<String> {
        let mut found: BTreeSet<String> = BTreeSet::new();
        for (tp, t) in &self.types {
            if t.kind != "trait" {
                continue;
            }
            if let Some(h) = trait_hint {
                if tp != h {
                    continue;
                }
            }
            for i in &t.impls {
                if i.for_type == ty {
                    if let Some((_, v)) = i.assoc_types.iter().find(|(n, _)| n == assoc) {
                        found.insert(v.clone());
                    }
                }
            }
        }
        if found.len() == 1 { found.into_iter().next() } else { None }
    }

    /// Resolve `<P as Trait>::Assoc` and `P::Assoc` projections after
    /// parameter substitution; `Err` names the first one left open.
    fn resolve_projections(&self, s: &str) -> Result<String, String> {
        let mut cur = s.to_string();
        for _ in 0..8 {
            let mut changed = false;
            // qualified projection: <X as Trait>::Assoc
            while let Some(i) = cur.find("<") {
                let rest = &cur[i..];
                let Some(as_i) = rest.find(" as ") else { break };
                // the matching '>' for this '<'
                let mut depth = 0;
                let mut close = None;
                for (k, ch) in rest.char_indices() {
                    match ch { '<' => depth += 1, '>' => { depth -= 1; if depth == 0 { close = Some(k); break; } } _ => {} }
                }
                let Some(close) = close else { break };
                if as_i > close { break; }
                if !rest[close + 1..].starts_with("::") { break; }
                let inner_ty = rest[1..as_i].trim().to_string();
                let trait_path = rest[as_i + 4..close].trim().to_string();
                let after = &rest[close + 3..];
                let assoc: String = after.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if assoc.is_empty() { break; }
                if is_param(&inner_ty) { return Err(format!("projection on an unbound parameter: <{inner_ty} as {trait_path}>::{assoc}")); }
                let Some(v) = self.assoc_of(&inner_ty, &assoc, Some(&trait_path)) else { return Err(format!("no recorded `{assoc}` for `{inner_ty}` in `{trait_path}`")) };
                let end = i + close + 3 + assoc.len();
                cur = format!("{}{}{}", &cur[..i], v, &cur[end..]);
                changed = true;
            }
            // short projection: X::Assoc where X is a concrete path known to some impl
            let mut from = 0;
            loop {
                let Some(off) = cur[from..].find("::") else { break };
                let i = from + off;
                let before = &cur[..i];
                let lhs_start = before.rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':')).map(|k| k + 1).unwrap_or(0);
                let lhs = before[lhs_start..].trim_start_matches(':').to_string();
                let assoc: String = cur[i + 2..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                let upper = assoc.chars().next().is_some_and(|c| c.is_ascii_uppercase());
                if !lhs.is_empty() && upper {
                    if let Some(v) = self.assoc_of(&lhs, &assoc, None) {
                        let end = i + 2 + assoc.len();
                        cur = format!("{}{}{}", &cur[..lhs_start], v, &cur[end..]);
                        changed = true;
                        from = lhs_start;
                        continue;
                    }
                    if is_param(&lhs) {
                        return Err(format!("projection on an unbound parameter: {lhs}::{assoc}"));
                    }
                    // a projection on a known type that no recorded impl binds
                    if self.types.get(&lhs).is_some_and(|t| t.kind != "trait") {
                        return Err(format!("no recorded `{assoc}` for `{lhs}`"));
                    }
                }
                from = i + 2;
            }
            let out = cur.clone();
            cur = out;
            if !changed { break; }
        }
        Ok(cur)
    }

    /// Whether `ty` satisfies one bound, from the inventory's recorded impls.
    fn holds(&self, ty: &str, bound: &str, depth: usize) -> Applicability {
        if depth > 4 {
            return Applicability::Unresolved(format!("nesting limit at `{ty}: {bound}`"));
        }
        let (tpath, cons) = match split_bound(bound) { Ok(x) => x, Err(e) => return Applicability::Unresolved(e) };
        let short = last(&tpath);
        if tpath.starts_with("core::") || tpath.starts_with("alloc::") || tpath.starts_with("std::") {
            if !cons.is_empty() {
                return Applicability::Unresolved(format!("core trait `{tpath}` with associated-type constraints is not modelled"));
            }
            return match short {
                "Sized" => match sizedness(self, ty) {
                    Some(true) => Applicability::Proven,
                    Some(false) => Applicability::Rejected(format!("`{ty}` is not `Sized`")),
                    None => Applicability::Unresolved(format!("the sizedness of `{ty}` is not established")),
                },
                "Send" | "Sync" | "Unpin" | "UnwindSafe" | "RefUnwindSafe" => Applicability::Unresolved(format!("auto trait `{short}` is not recorded")),
                "Clone" | "Debug" | "Default" | "PartialEq" | "Eq" | "Hash" | "Copy" | "PartialOrd" | "Ord" | "Display" => {
                    let (base, _) = split_head(ty);
                    if ty == "polars_utils::pl_str::PlSmallStr" {
                        // a polars type: only its recorded derives and impls count
                    } else if let Some(holds) = core_trait_on_builtin(ty, short) {
                        return if holds { Applicability::Proven } else { Applicability::Rejected(format!("`{ty}` does not implement `{short}`")) };
                    }
                    let derived = self.types.get(&base).is_some_and(|t| !ty.contains('<') && t.derived.iter().any(|d| d == short));
                    let implemented = self.impls.get(&base).is_some_and(|v| v.iter().any(|(n, f, _)| n == short && f == ty));
                    if derived || implemented { Applicability::Proven } else { Applicability::Unresolved(format!("no recorded impl of `{short}` for `{ty}`")) }
                }
                _ => Applicability::Unresolved(format!("core trait `{tpath}` is not modelled")),
            };
        }
        let Some(t) = self.types.get(&tpath).filter(|t| t.kind == "trait") else {
            return Applicability::Unresolved(format!("trait `{tpath}` is outside the inventory"));
        };
        // an exact impl head
        if let Some(i) = t.impls.iter().find(|i| !i.blanket && i.for_type == ty) {
            for (a, x) in &cons {
                match i.assoc_types.iter().find(|(n, _)| n == a) {
                    Some((_, v)) if v == x => {}
                    Some((_, v)) => return Applicability::Rejected(format!("`{ty}: {tpath}` binds `{a} = {v}`, not `{x}`")),
                    None => return Applicability::Unresolved(format!("`{ty}: {tpath}` does not record `{a}`")),
                }
            }
            return Applicability::Proven;
        }
        // a generic impl head that unifies with `ty`
        for i in t.impls.iter().filter(|i| !i.blanket && i.for_type.contains('<')) {
            let params: BTreeSet<String> = i.bounds.iter().map(|(n, _)| n.clone()).collect();
            if let Ok(subst) = unify(&i.for_type, ty, &params) {
                let mut r = Applicability::Proven;
                for (p, b) in &i.bounds {
                    let Some(pt) = subst.get(p) else { continue };
                    r = r.and(self.holds_all(pt, b, depth + 1));
                }
                for w in &i.where_predicates {
                    r = r.and(self.predicate(w, &subst, ty, depth + 1));
                }
                for (a, x) in &cons {
                    match i.assoc_types.iter().find(|(n, _)| n == a) {
                        Some((_, v)) => {
                            let v = subst_params(v, &subst);
                            let v = self.resolve_projections(&v).unwrap_or(v);
                            if &v != x { r = r.and(Applicability::Rejected(format!("`{ty}: {tpath}` binds `{a} = {v}`, not `{x}`"))); }
                        }
                        None => r = r.and(Applicability::Unresolved(format!("`{ty}: {tpath}` does not record `{a}`"))),
                    }
                }
                return r;
            }
        }
        if t.impls.iter().any(|i| i.blanket) {
            return Applicability::Unresolved(format!("`{tpath}` has a blanket impl; `{ty}` is not proven by a direct one"));
        }
        if t.impls.is_empty() {
            return Applicability::Unresolved(format!("no impl of `{tpath}` is recorded under this configuration"));
        }
        Applicability::Rejected(format!("no recorded impl of `{tpath}` for `{ty}`"))
    }

    fn holds_all(&self, ty: &str, bounds: &str, depth: usize) -> Applicability {
        let mut r = Applicability::Proven;
        for b in split_top_plus(bounds) {
            if b.is_empty() { continue; }
            r = r.and(self.holds(ty, &b, depth));
        }
        r
    }

    /// One `where` predicate under a substitution: `X: bounds` or `A = B`.
    fn predicate(&self, w: &str, subst: &BTreeMap<String, String>, self_ty: &str, depth: usize) -> Applicability {
        let mut s2 = subst.clone();
        s2.insert("Self".into(), self_ty.to_string());
        if let Some((lhs, bounds)) = w.split_once(": ") {
            let lhs = subst_params(lhs, &s2);
            let lhs = match self.resolve_projections(&lhs) { Ok(x) => x, Err(e) => return Applicability::Unresolved(e) };
            if lhs.split(|c: char| !(c.is_alphanumeric() || c == '_')).any(|tok| is_param(tok) && !s2.contains_key(tok) && tok != "Self") {
                // a parameter the substitution leaves open
                let open: Vec<&str> = lhs.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|tok| is_param(tok) && !s2.contains_key(*tok)).collect();
                if !open.is_empty() && open.iter().any(|o| !lhs.contains(&format!("::{o}"))) {
                    return Applicability::Unresolved(format!("`{w}` involves a parameter the head does not bind"));
                }
            }
            let bounds = subst_params(bounds, &s2);
            self.holds_all(&lhs, &bounds, depth)
        } else if let Some((a, b)) = w.split_once(" = ") {
            let a = subst_params(a, &s2);
            let b = subst_params(b, &s2);
            match (self.resolve_projections(&a), self.resolve_projections(&b)) {
                (Ok(x), Ok(y)) if x == y => Applicability::Proven,
                (Ok(x), Ok(y)) => Applicability::Rejected(format!("`{w}`: `{x}` is not `{y}`")),
                (Err(e), _) | (_, Err(e)) => Applicability::Unresolved(e),
            }
        } else {
            Applicability::Unresolved(format!("predicate form `{w}` is not modelled"))
        }
    }

    /// Whether a method's impl applies to an alias instantiation, with the
    /// substitution of the impl's parameters that makes it apply.
    fn applicability(&self, c: &Callable, identity: &str) -> (Applicability, BTreeMap<String, String>) {
        let Some(head) = &c.impl_head else { return (Applicability::Unresolved("no impl head recorded".into()), BTreeMap::new()) };
        let mut params: BTreeSet<String> = c.impl_bounds.iter().map(|(n, _)| n.clone()).filter(|n| n != "Self").collect();
        // parameters that appear in the head without a bound
        let (_, hargs) = split_head(head);
        for a in &hargs {
            if is_param(a) { params.insert(a.clone()); }
        }
        let subst = match unify_with(self, head, identity, &params) {
            Ok(s) => s,
            Err(e) if e.starts_with("unresolved: ") => return (Applicability::Unresolved(e), BTreeMap::new()),
            Err(e) => return (Applicability::Rejected(e), BTreeMap::new()),
        };
        let mut r = Applicability::Proven;
        for (p, b) in &c.impl_bounds {
            if b.is_empty() { continue; }
            let pt = if p == "Self" { identity.to_string() } else { match subst.get(p) { Some(t) => t.clone(), None => return (Applicability::Unresolved(format!("bound on `{p}`, which the head does not bind")), subst) } };
            let b = subst_params(b, &subst);
            r = r.and(self.holds_all(&pt, &b, 0));
        }
        // record 0098: a listed, well-formed entry discharges its cited clauses
        // on its listed float pairs, once `T::Native` resolves to that native
        let external = external_bound_entry(&self.release, c);
        if let Some(Err(why)) = &external {
            return (Applicability::Unresolved(format!("external bound: {why}")), subst);
        }
        let external = external.and_then(Result::ok);
        for w in &c.impl_where {
            if let Some(e) = external.filter(|e| e.discharge.contains(w)) {
                let owner = subst.get("T").cloned().unwrap_or_default();
                let Some((_, native)) = e.pairs.iter().find(|(t, _)| *t == owner) else {
                    // an unlisted type keeps the ordinary proof (a nonfloat is rejected by its owner bound)
                    r = r.and(match self.predicate(w, &subst, identity, 0) {
                        Applicability::Unresolved(_) => Applicability::Unresolved(format!("external bound: `{identity}` is not a listed pair")),
                        other => other,
                    });
                    continue;
                };
                let lhs = subst_params("T::Native", &subst);
                match self.resolve_projections(&lhs) {
                    Ok(n) if n == *native => {}
                    Ok(n) => { r = r.and(Applicability::Unresolved(format!("external bound: `T::Native` is `{n}`, not the listed `{native}`"))); }
                    Err(e) => { r = r.and(Applicability::Unresolved(format!("external bound: {e}"))); }
                }
                continue;
            }
            r = r.and(self.predicate(w, &subst, identity, 0));
        }
        (r, subst)
    }

    /// The method's signature under the substitution, every projection
    /// resolved; `Err` names what stays open.
    fn substitute_signature(&self, c: &Callable, identity: &str, subst: &BTreeMap<String, String>) -> Result<(Vec<String>, Option<String>), String> {
        let mut s2 = subst.clone();
        s2.insert("Self".into(), identity.to_string());
        let generic_bounds = generics_map(c);
        let one = |t: &str| -> Result<String, String> {
            let t = generic_bounds.get(t).map(String::as_str).unwrap_or(t);
            let t = subst_params(t, &s2);
            let t = self.resolve_projections(&t)?;
            // an associated-type constraint key (`Item = …`) is not a parameter
            let stripped = {
                let mut r = String::new();
                let mut rest = t.as_str();
                while let Some(i) = rest.find(" = ") {
                    let (pre, post) = rest.split_at(i);
                    let k = pre.rfind(|c: char| !(c.is_alphanumeric() || c == '_')).map(|x| x + 1).unwrap_or(0);
                    r.push_str(&pre[..k]);
                    r.push_str(" = ");
                    rest = &post[3..];
                }
                r.push_str(rest);
                r
            };
            let mut left: Vec<&str> = stripped.split(|ch: char| !(ch.is_alphanumeric() || ch == '_' || ch == ':')).filter(|tok| is_param(tok)).collect();
            left.sort();
            left.dedup();
            if !left.is_empty() { return Err(format!("`{}` left open in `{t}`", left.join(", "))); }
            Ok(t)
        };
        let params = c.params.iter().map(|p| one(&p.ty_canonical)).collect::<Result<Vec<_>, _>>()?;
        let ret = match &c.ret_canonical { Some(r) => Some(one(r)?), None => None };
        Ok((params, ret))
    }
}

/// `A + B` split at top level.
fn split_top_plus(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in s.chars() {
        match ch {
            '<' | '(' | '[' => { depth += 1; cur.push(ch); }
            '>' | ')' | ']' => { depth -= 1; cur.push(ch); }
            '+' if depth == 0 => { out.push(cur.trim().to_string()); cur.clear(); }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() { out.push(cur.trim().to_string()); }
    out
}

/// Unify an impl head with a concrete instantiation: `params` are the
/// head's generic names. Repeated parameters must bind consistently; a
/// concrete argument must be equal. Projections in the head are checked
/// after the parameters they mention are bound.
fn unify(head: &str, ty: &str, params: &BTreeSet<String>) -> Result<BTreeMap<String, String>, String> {
    let (hb, ha) = split_head(head);
    let (tb, ta) = split_head(ty);
    if hb != tb { return Err(format!("head `{hb}` is not `{tb}`")); }
    if ha.len() != ta.len() { return Err(format!("head `{head}` has {} arguments, `{ty}` has {}", ha.len(), ta.len())); }
    let mut subst: BTreeMap<String, String> = BTreeMap::new();
    let mut deferred: Vec<(String, String)> = Vec::new();
    for (h, t) in ha.iter().zip(&ta) {
        if params.contains(h) {
            match subst.get(h) {
                Some(prev) if prev != t => return Err(format!("`{h}` would be both `{prev}` and `{t}`")),
                Some(_) => {}
                None => { subst.insert(h.clone(), t.clone()); }
            }
        } else if h.contains("::") && (h.starts_with('<') || h.split("::").next().is_some_and(is_param)) {
            deferred.push((h.clone(), t.clone()));
        } else if h.contains('<') {
            let inner = unify(h, t, params)?;
            for (k, v) in inner {
                match subst.get(&k) {
                    Some(prev) if *prev != v => return Err(format!("`{k}` would be both `{prev}` and `{v}`")),
                    _ => { subst.insert(k, v); }
                }
            }
        } else if h != t {
            return Err(format!("head argument `{h}` is not `{t}`"));
        }
    }
    for (h, t) in deferred {
        // recorded as a check to run with the world's assoc facts
        subst.insert(format!("?{h}"), t);
    }
    Ok(subst)
}

/// `unify`, then the deferred projection checks against recorded
/// associated types.
fn unify_with(world: &World, head: &str, ty: &str, params: &BTreeSet<String>) -> Result<BTreeMap<String, String>, String> {
    let mut subst = unify(head, ty, params)?;
    let deferred: Vec<(String, String)> = subst.iter().filter(|(k, _)| k.starts_with('?')).map(|(k, v)| (k[1..].to_string(), v.clone())).collect();
    subst.retain(|k, _| !k.starts_with('?'));
    for (proj, want) in deferred {
        let p = subst_params(&proj, &subst);
        match world.resolve_projections(&p) {
            Ok(v) if v == want => {}
            Ok(v) => return Err(format!("head projection `{proj}` is `{v}`, not `{want}`")),
            Err(e) => return Err(format!("unresolved: {e}")),
        }
    }
    Ok(subst)
}

/// The family an alias instantiation belongs to, for the shipping order.
fn family(world: &World, identity: &str) -> &'static str {
    let (base, args) = split_head(identity);
    if base.ends_with("::Logical") { return "logical"; }
    let arg = args.first().map(|s| s.as_str()).unwrap_or("");
    let numeric = world.types.get("polars_core::datatypes::PolarsNumericType").is_some_and(|t| t.implementors.iter().any(|i| i == arg));
    if numeric { return "numeric"; }
    match last(arg) {
        "BooleanType" => "boolean",
        "StringType" | "BinaryType" | "BinaryOffsetType" => "string-binary",
        "ListType" | "StructType" | "ArrayType" => "list-struct",
        _ => "other",
    }
}

/// The instantiation census: every (method, alias identity) candidate
/// on generic owners that have alias wrappers, with its applicability.
#[derive(serde::Serialize, Clone)]
struct PairRecord {
    /// The callable's inventory key: one canonical path can carry several
    /// impl blocks (specialized heads), each its own callable.
    key: String,
    method: String,
    identity: String,
    alias: String,
    family: &'static str,
    #[serde(flatten)]
    result: Applicability,
    #[serde(skip_serializing_if = "Option::is_none")]
    signature: Option<String>,
    /// Whether the callable was eligible for emission at all (0072's
    /// terms: not in the `unsupported`/`unknown` bucket); an ineligible
    /// callable's pairs are gross applicability, never emitted.
    eligible: bool,
    /// Filled after emission: `emitted`, `refused: …`, `excluded: …`,
    /// `not eligible: …`, or for an unproven pair its result.
    #[serde(skip_serializing_if = "Option::is_none")]
    disposition: Option<String>,
}

fn instantiation_census(world: &World, inv: &Inventory) -> Vec<PairRecord> {
    // alias identities per generic base, representative alias first
    let mut by_base: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for (path, w) in &world.wrappers {
        if w.rule != "alias" { continue; }
        let Some(base) = &w.base else { continue };
        if !w.identity.starts_with(&format!("{base}<")) { continue; } // an Arc<…> target is not an instantiation of the base
        by_base.entry(base.clone()).or_default().entry(w.identity.clone()).or_insert_with(|| w.aliases.first().cloned().unwrap_or_else(|| path.clone()));
    }
    let mut out = Vec::new();
    // the record's scope: inherent methods of the two generic owners named
    // in the plan; other generic owners with aliases stay in the generic bucket
    const OWNERS: &[&str] = &["polars_core::chunked_array::ChunkedArray", "polars_core::chunked_array::logical::Logical"];
    for c in &inv.callables {
        if c.kind != "inherent" || c.impl_head.is_none() || !OWNERS.contains(&c.owner.as_str()) { continue; }
        let Some(ids) = by_base.get(&c.owner) else { continue };
        for (identity, alias) in ids {
            let closure_generics: BTreeSet<&str> = c.params.iter().filter(|p| closure_signature(c, p).is_some()).map(|p| p.ty_canonical.as_str()).collect();
            let scalar = world.release.method_scalar_generics.iter().find(|m| m.key == c.key && m.path == c.canonical_path && m.check(c).is_ok());
            // record 0097: a listed, well-formed `Self: Sized` method is decided by applicability
            let sized = sized_self_entry(&world.release, c);
            if let Some(Err(why)) = &sized {
                out.push(PairRecord { key: c.key.clone(), method: c.canonical_path.clone(), identity: identity.clone(), alias: alias.clone(), family: family(world, identity), result: Applicability::Unresolved(format!("sized-self method: {why}")), signature: None, eligible: !matches!(c.bucket.as_str(), "unsupported" | "unknown"), disposition: None });
                continue;
            }
            let sized_ok = matches!(sized, Some(Ok(_)));
            if c.generics_canonical.iter().any(|(name, _)| !closure_generics.contains(name.as_str()) && scalar.is_none_or(|m| m.generic != *name) && !(sized_ok && name == "Self")) {
                out.push(PairRecord { key: c.key.clone(), method: c.canonical_path.clone(), identity: identity.clone(), alias: alias.clone(), family: family(world, identity), result: Applicability::Unresolved("function-level generics are out of this record's scope".into()), signature: None, eligible: !matches!(c.bucket.as_str(), "unsupported" | "unknown"), disposition: None });
                continue;
            }
            let (r, subst) = world.applicability(c, identity);
            let (r, sig) = match r {
                Applicability::Proven => match world.substitute_signature(c, identity, &subst) {
                    Ok((ps, ret)) => (Applicability::Proven, Some(format!("({}) -> {}", ps.join(", "), ret.as_deref().unwrap_or("()")))),
                    Err(e) => (Applicability::Unresolved(format!("signature: {e}")), None),
                },
                other => (other, None),
            };
            out.push(PairRecord { key: c.key.clone(), method: c.canonical_path.clone(), identity: identity.clone(), alias: alias.clone(), family: family(world, identity), result: r, signature: sig, eligible: !matches!(c.bucket.as_str(), "unsupported" | "unknown"), disposition: None });
        }
    }
    out
}

/// Record 0077 gate 1: every eligible callable and every instantiation
/// pair whose return is an iterator, with the item and its disposition.
fn iterator_census(entries: &[Entry], inv: &Inventory, pairs: &[PairRecord]) -> serde_json::Value {
    let mut callables = Vec::new();
    let by_key: BTreeMap<&str, &Entry> = entries.iter().map(|e| (e.key.as_str(), e)).collect();
    for c in &inv.callables {
        let Some(r) = &c.ret_canonical else { continue };
        let Some((item, known, wrap)) = iterator_return(&ty::parse(r)) else { continue };
        let disposition = match by_key.get(c.key.as_str()) {
            Some(e) if e.status == "generated" => if e.bindings.iter().any(|b| b.disposition.is_some()) { "materialized".to_string() } else { "generated".to_string() },
            Some(e) => format!("{}: {}", e.status, e.reason.as_deref().unwrap_or("")),
            None => match c.bucket.as_str() { "unsupported" | "unknown" => "not eligible".to_string(), _ => "out of scope".to_string() },
        };
        callables.push(serde_json::json!({"key": c.key, "method": c.canonical_path, "owner_generic": c.owner_generic, "receiver": c.receiver, "item": item.render(), "known_length": format!("{known:?}"), "wrap": format!("{wrap:?}"), "disposition": disposition}));
    }
    let pair_rows: Vec<serde_json::Value> = pairs.iter().filter(|p| p.signature.as_deref().is_some_and(|s| s.contains("impl ")) || p.disposition.as_deref().is_some_and(|d| d.contains("iterator") || d.contains("impl return"))).map(|p| serde_json::json!({"key": p.key, "method": p.method, "alias": p.alias, "disposition": p.disposition})).collect();
    let mut by_disp: BTreeMap<String, usize> = BTreeMap::new();
    for c in &callables { *by_disp.entry(c["disposition"].as_str().unwrap().split(':').next().unwrap().to_string()).or_insert(0) += 1; }
    let mut pair_disp: BTreeMap<String, usize> = BTreeMap::new();
    for p in &pair_rows { *pair_disp.entry(p["disposition"].as_str().unwrap_or("none").split(':').take(2).collect::<Vec<_>>().join(":")).or_insert(0) += 1; }
    serde_json::json!({"callables": callables.len(), "callables_by_disposition": by_disp, "pairs": pair_rows.len(), "pairs_by_disposition": pair_disp, "callable_rows": callables, "pair_rows": pair_rows})
}

/// Record 0078 gate 1: every `From` impl and assignment/unary operator
/// with the name the rule gives and its disposition.
// ---- record 0079 gate 1: the callback census ----

/// A mutable closure argument the release file has audited: what Polars
/// does with the writes, with the citation.
#[derive(Clone, Debug, serde::Deserialize)]
struct CallbackMutable {
    path: String,
    param: String,
    /// `vector` (the slice is never read back: passed as a Rune vector of
    /// clones) or `result buffer` (the buffer's contents are the result:
    /// the Rune callback returns a string written into it).
    contract: String,
    cite: String,
}

/// Where an operation invokes one closure parameter, from the pinned
/// sources: `immediate` (before the call returns) or `stored` (kept by
/// Polars and invoked from the listed sinks), with the citation. A
/// lifetime bound is a signature fact, never the classification.
#[derive(Clone, Debug, serde::Deserialize)]
struct CallbackInvocation {
    path: String,
    param: String,
    invocation: String,
    #[serde(default)]
    sinks: Vec<String>,
    cite: String,
}

/// A method of a plan-holding type whose result is not the plan: an
/// execution sink (`plan execution`, `schema resolution`) or `none`, with
/// the citation. Every such method must be classified, or stored callbacks
/// are unresolved.
#[derive(Clone, Debug, serde::Deserialize)]
struct CallbackSink {
    path: String,
    sink: String,
    cite: String,
}

#[derive(Clone, Debug, serde::Deserialize)]
struct CallbackSafe {
    path: String,
    cite: String,
}

#[derive(Clone, Debug, serde::Deserialize)]
struct CallbackRecipe {
    signature: String,
    rune: String,
    rust: String,
    #[serde(default)]
    uses: Vec<String>,
}

/// One closure parameter as the inventory spells it: the `Fn` kind, the
/// argument and return types, and whether the bound requires `'static`
/// (a signature fact; the invocation comes from the audit).
#[derive(Clone, Debug)]
struct ClosureSig {
    param: String,
    kind: String,
    args: Vec<String>,
    ret: String,
    /// The bound requires `'static`: Polars may keep the closure. A fact,
    /// not the invocation (an operation may invoke a `'static` closure
    /// immediately, or keep a borrowing closure in a lifetime-bound holder).
    static_bound: bool,
    udf: bool,
}

fn split_top_on(s: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in s.chars() {
        match ch {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            _ => {}
        }
        if ch == sep && depth == 0 {
            out.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(ch);
        }
    }
    if !cur.trim().is_empty() { out.push(cur.trim().to_string()); }
    out
}

fn closure_signature(c: &Callable, p: &Param) -> Option<ClosureSig> {
    let generics = generics_map(c);
    let bare = p.ty_canonical.trim().trim_start_matches("&mut ").trim_start_matches('&').trim();
    let text = generics.get(bare).cloned().unwrap_or_else(|| p.ty_canonical.clone());
    if text.contains("dyn ") && text.contains("Udf") {
        return Some(ClosureSig { param: p.name.clone(), kind: "Udf".into(), args: vec![], ret: String::new(), static_bound: true, udf: true });
    }
    let mut t = text.trim();
    for prefix in ["&mut ", "&", "dyn ", "impl ", "core::ops::function::"] {
        t = t.trim_start_matches(prefix).trim();
    }
    // `Fn(A, B) -> R + 'static + Send`
    let kind = ["FnOnce", "FnMut", "Fn"].iter().find(|k| t.starts_with(**k) && t[k.len()..].starts_with('('))?.to_string();
    let rest = &t[kind.len()..];
    let close = {
        let mut depth = 0i32;
        let mut idx = None;
        for (i, ch) in rest.char_indices() {
            match ch { '(' | '<' | '[' => depth += 1, ')' | '>' | ']' => { depth -= 1; if depth == 0 { idx = Some(i); break; } } _ => {} }
        }
        idx?
    };
    let args = split_top_on(&rest[1..close], ',');
    let after = rest[close + 1..].trim();
    let ret = match after.strip_prefix("->") {
        Some(r) => split_top_on(r.trim(), '+').into_iter().next().unwrap_or_default(),
        None => "()".into(),
    };
    Some(ClosureSig { param: p.name.clone(), kind, args, ret, static_bound: text.contains("+ 'static") || text.contains("'static +"), udf: false })
}

/// A plan-holding type: a method of one whose result is not a plan type
/// may execute or resolve the plan, and must be classified by the release
/// file's `[[callback_sink]]` audit (plan execution, schema resolution, or
/// none, each with a citation).
fn plan_holder_non_plan_method(c: &Callable) -> bool {
    const HOLDERS: &[&str] = &["polars_lazy::frame::LazyFrame", "polars_plan::dsl::plan::DslPlan", "polars_plan::dsl::builder_dsl::DslBuilder", "polars_lazy::frame::JoinBuilder", "polars_lazy::frame::LazyGroupBy", "polars_plan::dsl::expr::Expr"];
    const PLAN_TYPES: &[&str] = &["Self", "LazyFrame", "DslPlan", "DslBuilder", "JoinBuilder", "LazyGroupBy", "Expr", "OptFlags", "ExprIR", "Selector"];
    if !HOLDERS.iter().any(|h| c.owner == *h) || c.kind == "foreign_trait_impl" || c.bucket == "unsupported" || c.bucket == "unknown" { return false; }
    let ret = c.ret_canonical.as_deref().unwrap_or("()");
    !PLAN_TYPES.iter().any(|t| mentions(ret, t))
}

fn callback_census(world: &World, inv: &Inventory, entries: &[Entry]) -> serde_json::Value {
    let release = &world.release;
    let by_key: BTreeMap<&str, &Entry> = entries.iter().map(|e| (e.key.as_str(), e)).collect();
    // an empty family list in the release file means every family (as the
    // instantiation census reads it); the pair-level applicability is 0080's
    let families = true;
    let generic_family_owner = |o: &str| o == "polars_core::chunked_array::ChunkedArray" || o == "polars_core::chunked_array::logical::Logical";
    // every non-plan-returning method of a plan-holding type is classified by the audit, or listed as unclassified
    let mut sinks = Vec::new();
    let mut unclassified_sinks: Vec<String> = Vec::new();
    let mut sink_groups: BTreeSet<&str> = BTreeSet::new();
    for c in &inv.callables {
        if !release.is_api(&c.krate) || !plan_holder_non_plan_method(c) { continue; }
        let e = by_key.get(c.key.as_str());
        let routed_today = routed(&c.name, Some(&c.owner), &c.params, c.ret_canonical.as_deref());
        match release.callback_sink.iter().find(|k| k.path == c.canonical_path) {
            Some(k) => {
                if k.sink != "none" { sink_groups.insert(k.sink.as_str()); }
                sinks.push(serde_json::json!({"key": c.key, "path": c.canonical_path, "sink": k.sink, "cite": k.cite, "status": e.map(|e| e.status).unwrap_or("not emitted"), "rune": e.and_then(|e| e.rune.clone()), "routed_today": routed_today}));
            }
            None => unclassified_sinks.push(c.canonical_path.clone()),
        }
    }
    let mut rows = Vec::new();
    let mut disp: BTreeMap<String, usize> = BTreeMap::new();
    for c in &inv.callables {
        let closures: Vec<ClosureSig> = c.params.iter().filter_map(|p| closure_signature(c, p)).collect();
        if closures.is_empty() { continue; }
        let api = release.is_api(&c.krate);
        let mut refusals: Vec<String> = Vec::new();
        let mut contracts: Vec<String> = Vec::new();
        let mut per_family = false;
        let generics = generics_map(c);
        let closure_names: BTreeSet<&str> = c.params.iter().filter(|p| closure_signature(c, p).is_some()).map(|p| p.ty_canonical.trim().trim_start_matches("&mut ").trim_start_matches('&').trim()).collect();
        let free_generics: Vec<&str> = generics.keys().map(|s| s.as_str()).filter(|g| !closure_names.contains(g)).collect();
        let is_projection = |t: &str| t.contains("::Native") || t.contains("::Physical");
        let free_in = |t: &str| free_generics.iter().find(|g| mentions(t, g)).map(|g| g.to_string());
        if !api {
            let d = "out of scope: internal crate".to_string();
            *disp.entry(d.clone()).or_insert(0) += 1;
            rows.push(serde_json::json!({"key": c.key, "owner": c.owner, "name": c.name, "closures": closures.iter().map(|s| format!("{}: {}({}) -> {}", s.param, s.kind, s.args.join(", "), s.ret)).collect::<Vec<_>>(), "disposition": d}));
            continue;
        }
        if c.bucket == "unsupported" || c.bucket == "unknown" {
            let d = format!("not eligible: bucket {}", c.bucket);
            *disp.entry("not eligible".into()).or_insert(0) += 1;
            rows.push(serde_json::json!({"key": c.key, "owner": c.owner, "name": c.name, "disposition": d}));
            continue;
        }
        for s in &closures {
            if s.udf { refusals.push(format!("{}: Udf trait object", s.param)); continue; }
            if c.is_async || mentions(&s.ret, "Fut") { refusals.push(format!("{}: async closure", s.param)); continue; }
            for a in &s.args {
                let ty = ty::parse(a);
                if let Ty::Ref { mutable: true, inner } = &ty {
                    match release.callback_mutable.iter().find(|m| m.path == c.canonical_path && m.param == s.param) {
                        Some(m) => contracts.push(format!("{} `{}`: {} ({})", s.param, a, m.contract, m.cite)),
                        None => refusals.push(format!("{}: mutable argument `{}` the callback cannot write back (no audited contract)", s.param, inner.render())),
                    }
                    continue;
                }
                if is_projection(a) {
                    if generic_family_owner(&c.owner) && families { per_family = true; } else { refusals.push(format!("{}: projection `{a}` on an owner without instantiation families", s.param)); }
                    continue;
                }
                if let Some(g) = free_in(a) { refusals.push(format!("{}: argument `{a}` is the free generic `{g}`", s.param)); continue; }
                // an amortized series is a borrow Polars reuses between calls; it is kept out of callbacks (the plan's census class), a later record may relax it
                if NOT_ROUTED_TYPES.iter().any(|d| mentions(a, d)) { refusals.push(format!("{}: argument `{a}` is an amortized borrow Polars reuses between calls (AmortSeries)", s.param)); continue; }
                // a shared slice or vector of a wrapped type arrives as a Rune vector of clones
                let elem = match &ty { Ty::Ref { mutable: false, inner } => match &**inner { Ty::Slice(e) => Some((**e).clone()), _ => None }, Ty::Path { path, args } if path == "alloc::vec::Vec" && args.len() == 1 => Some(args[0].clone()), _ => None };
                if let Some(e) = elem {
                    match world.ret(&e, Some(&c.owner), 0) { Ok(_) => continue, Err(Unsupported(why, what)) => { refusals.push(format!("{}: argument `{a}`: {why} ({what})", s.param)); continue; } }
                }
                if let Err(Unsupported(why, what)) = world.ret(&ty, Some(&c.owner), 0) {
                    refusals.push(format!("{}: argument `{a}`: {why} ({what})", s.param));
                }
            }
            let ret = s.ret.trim();
            let ret = ret.strip_prefix("polars_error::PolarsResult<").and_then(|r| r.strip_suffix('>')).unwrap_or(ret);
            let rty = ty::parse(ret);
            if ret == "()" {
            } else if matches!(rty, Ty::Ref { .. }) || ret.starts_with("&'") {
                refusals.push(format!("{}: return `{ret}` borrowed from the argument", s.param));
            } else if is_projection(ret) {
                if generic_family_owner(&c.owner) && families { per_family = true; } else { refusals.push(format!("{}: projection `{ret}` on an owner without instantiation families", s.param)); }
            } else if let Some(g) = free_in(ret) {
                refusals.push(format!("{}: return `{ret}` is the free generic `{g}` (a Rune function cannot choose a Rust type parameter)", s.param));
            } else if let Err(Unsupported(why, what)) = world.arg(&rty, "r", &generics, Some(&c.owner), 0) {
                refusals.push(format!("{}: return `{ret}`: {why} ({what})", s.param));
            }
        }
        // the rest of the callable: owner, receiver, other parameters, other generics
        if world.wrapper_for(&c.owner).is_none() && !c.owner.is_empty() {
            if generic_family_owner(&c.owner) && families { per_family = true; } else { refusals.push(format!("owner not wrapped: {}", c.owner)); }
        }
        if !matches!(c.receiver.as_str(), "none" | "self" | "&self" | "&mut self") { refusals.push(format!("receiver form `{}`", c.receiver)); }
        for p in &c.params {
            if closure_signature(c, p).is_some() { continue; }
            let t = ty::parse(&p.ty_canonical);
            if is_projection(&p.ty_canonical) { if !(generic_family_owner(&c.owner) && families) { refusals.push(format!("parameter `{}`: projection without instantiation families", p.name)); } continue; }
            if let Err(Unsupported(why, what)) = world.arg(&t, &p.name, &generics, Some(&c.owner), 0) {
                refusals.push(format!("parameter `{}`: {why} ({what})", p.name));
            }
        }
        if let Some(r) = c.ret_canonical.as_deref() {
            let r = r.strip_prefix("polars_error::PolarsResult<").and_then(|x| x.strip_suffix('>')).unwrap_or(r);
            if r != "Self" && !is_projection(r) {
                if let Err(Unsupported(why, what)) = world.ret(&ty::parse(r), Some(&c.owner), 0) { refusals.push(format!("return: {why} ({what})")); }
            }
        }
        // invocation per closure from the release file's source audit; a
        // closure without an entry leaves the operation unresolved
        let mut unresolved: Vec<String> = Vec::new();
        let mut invocation: Vec<serde_json::Value> = Vec::new();
        for s in &closures {
            match release.callback_invocation.iter().find(|a| a.path == c.canonical_path && a.param == s.param) {
                Some(a) => {
                    if a.invocation == "stored" {
                        for g in &a.sinks {
                            if !sink_groups.contains(g.as_str()) { unresolved.push(format!("{}: sink group `{g}` has no classified member", s.param)); }
                        }
                        if !unclassified_sinks.is_empty() { unresolved.push(format!("{}: unclassified execution path(s): {}", s.param, unclassified_sinks.join(", "))); }
                    }
                    invocation.push(serde_json::json!({"param": s.param, "invocation": a.invocation, "sinks": a.sinks, "cite": a.cite, "static_bound": s.static_bound}));
                }
                None => {
                    unresolved.push(format!("{}: invocation not audited", s.param));
                    invocation.push(serde_json::json!({"param": s.param, "invocation": "unresolved", "static_bound": s.static_bound}));
                }
            }
        }
        let disposition = if !refusals.is_empty() {
            "refused".to_string()
        } else if !unresolved.is_empty() {
            "unresolved".to_string()
        } else {
            let mut q: Vec<&str> = Vec::new();
            if per_family { q.push("per family"); }
            if contracts.iter().any(|x| x.contains("result buffer")) { q.push("result buffer"); }
            if contracts.iter().any(|x| x.contains("vector")) { q.push("vector argument"); }
            if q.is_empty() { "feasible".to_string() } else { format!("feasible ({})", q.join(", ")) }
        };
        *disp.entry(disposition.clone()).or_insert(0) += 1;
        let today = by_key.get(c.key.as_str()).map(|e| e.status.to_string()).unwrap_or_else(|| "not emitted".into());
        let after_emission = match by_key.get(c.key.as_str()) {
            Some(e) if e.status == "generated" => "generated".to_string(),
            Some(e) if per_family && !e.exceptions.is_empty() => format!("pair refused: {}", e.reason.as_deref().unwrap_or("see exceptions")),
            Some(e) => format!("refused: {}", e.reason.as_deref().unwrap_or(e.status)),
            None => "refused: not eligible for emission".into(),
        };
        assert!(disposition.starts_with("feasible") || after_emission != "generated", "callback audit refused {} but it acquired a binding", c.canonical_path);
        rows.push(serde_json::json!({
            "key": c.key, "owner": c.owner, "name": c.name, "canonical_path": c.canonical_path, "bucket": c.bucket,
            "closures": closures.iter().map(|s| format!("{}: {}({}) -> {}", s.param, s.kind, s.args.join(", "), s.ret)).collect::<Vec<_>>(),
            "invocation": invocation, "mutable_contracts": contracts, "disposition": disposition, "refusals": refusals, "unresolved": unresolved, "today": today, "disposition after emission": after_emission,
        }));
    }
    let unrouted = sinks.iter().filter(|s| s["status"] == "generated" && s["routed_today"] == false && s["sink"] != "none").count();
    serde_json::json!({
        "count": rows.len(),
        "by_disposition": disp,
        "rows": rows,
        "rule": "every binding that accepts a closure is routed through engine::run (route reason `callback`); a stored callback is invoked only from a classified sink, and every sink binding is routed (route reason `executes callbacks`)",
        "sinks": {
            "audit": ["polars-expr-0.55.2/src/expressions/apply.rs:114,133,235,295,325,376,395 (ColumnsUdf::call_udf during plan execution)", "polars-plan-0.55.2/src/plans/functions/mod.rs:202 (DataFrame UDF of LazyFrame::map)", "polars-stream-0.55.2/src/nodes/map.rs:50, columnar_function.rs:94, in_memory_map.rs:51 (the streaming engine)", "polars-plan-0.55.2/src/plans/aexpr/schema.rs:287,299,391, plans/ir/unoptimized.rs:48,65 and plans/schema.rs:21 (FunctionOutputField::get_field during schema resolution)"],
            "classified": "every method of LazyFrame, DslPlan, DslBuilder, JoinBuilder, LazyGroupBy and Expr whose result is not a plan type, by the release file's [[callback_sink]] entries",
            "bindings": sinks,
            "unclassified": unclassified_sinks,
            "generated_unrouted_today": unrouted,
        },
    })
}

/// Record 0092 gate 2 controls: a `[[method_scalar_generics]]` entry is
/// checked before it can lift a census pair (fail closed): it must name the
/// callable's only function generic, that generic must be a whole parameter
/// type and nowhere else, and types and natives must pair up. A valid entry
/// yields each listed type's native; an unlisted type has none.
fn method_scalar_generic_self_test() {
    let ca = "polars_core::chunked_array::ChunkedArray";
    let mk = |params: Vec<(&str, &str)>, generics: Vec<(&str, &str)>, ret: &str| Callable {
        key: "k".into(), kind: "inherent".into(), krate: "polars_core".into(), owner: ca.into(), name: "lhs_op".into(), canonical_path: format!("{ca}::lhs_op"),
        found_paths: vec![], crate_paths: vec![], receiver: "&self".into(), params: params.iter().map(|(n, t)| Param { name: n.to_string(), ty: t.to_string(), ty_canonical: t.to_string() }).collect(),
        ret: None, ret_canonical: Some(ret.into()), generics_canonical: generics.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
        impl_for: None, impl_bounds: vec![], impl_head: Some(format!("{ca}<T>")), impl_where: vec![], impl_assoc: vec![], docs_first: None, owner_generic: true, is_unsafe: false, is_async: false,
        deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "generic".into(), rules: vec![],
    };
    let bound = "num_traits::Num + num_traits::cast::NumCast";
    let entry = |types: Vec<&str>, natives: Vec<&str>| MethodScalarGeneric { key: "k".into(), path: format!("{ca}::lhs_op"), generic: "N".into(), types: types.into_iter().map(String::from).collect(), natives: natives.into_iter().map(String::from).collect(), cite: "t".into() };
    let good = entry(vec!["polars_core::datatypes::Int8Type", "polars_core::datatypes::UInt32Type", "polars_core::datatypes::Float32Type"], vec!["i8", "u32", "f32"]);
    let ok = mk(vec![("lhs", "N")], vec![("N", bound)], "Self");
    assert!(good.check(&ok).is_ok());
    assert_eq!(good.native_for(&format!("{ca}<polars_core::datatypes::Int8Type>")), Some("i8"));
    assert_eq!(good.native_for(&format!("{ca}<polars_core::datatypes::UInt32Type>")), Some("u32"), "IdxCa's identity");
    assert_eq!(good.native_for(&format!("{ca}<polars_core::datatypes::Float32Type>")), Some("f32"));
    assert_eq!(good.native_for(&format!("{ca}<polars_core::datatypes::Int64Type>")), None, "an unlisted type has no native and is refused");
    for (label, c, e) in [
        ("uneven lists", ok.clone(), entry(vec!["polars_core::datatypes::Int8Type"], vec![])),
        ("another generic name", mk(vec![("lhs", "M")], vec![("M", bound)], "Self"), good.clone()),
        ("two function generics", mk(vec![("lhs", "N"), ("x", "K")], vec![("N", bound), ("K", "")], "Self"), good.clone()),
        ("generic in the return", mk(vec![("lhs", "N")], vec![("N", bound)], "core::option::Option<N>"), good.clone()),
        ("generic inside another parameter", mk(vec![("lhs", "N"), ("v", "alloc::vec::Vec<N>")], vec![("N", bound)], "Self"), good.clone()),
        ("no whole parameter", mk(vec![("v", "alloc::vec::Vec<N>")], vec![("N", bound)], "Self"), good.clone()),
        ("Int8Type paired with i64", ok.clone(), entry(vec!["polars_core::datatypes::Int8Type"], vec!["i64"])),
        ("Float32Type paired with f64", ok.clone(), entry(vec!["polars_core::datatypes::Float32Type"], vec!["f64"])),
        ("a type with no known native", ok.clone(), entry(vec!["polars_core::datatypes::BooleanType"], vec!["bool"])),
        ("lists out of order", ok.clone(), entry(vec!["polars_core::datatypes::Int8Type", "polars_core::datatypes::UInt8Type"], vec!["u8", "i8"])),
    ] {
        assert!(e.check(&c).is_err(), "{label} must fail closed");
    }
    // record 0095: lhs_div and lhs_rem reuse the fixed table for all ten
    // numeric types; each scalar class resolves to its own native
    let all_types = ["Int8Type", "Int16Type", "Int32Type", "Int64Type", "UInt8Type", "UInt16Type", "UInt32Type", "UInt64Type", "Float32Type", "Float64Type"].map(|t| format!("polars_core::datatypes::{t}"));
    let all_natives = ["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64"];
    for name in ["lhs_div", "lhs_rem"] {
        let e = MethodScalarGeneric { key: "k".into(), path: format!("{ca}::{name}"), generic: "N".into(), types: all_types.to_vec(), natives: all_natives.iter().map(|n| n.to_string()).collect(), cite: "t".into() };
        let mut c = ok.clone();
        c.name = name.into();
        c.canonical_path = format!("{ca}::{name}");
        assert!(e.check(&c).is_ok(), "{name}: the shipped shape passes");
        for (t, n) in [("Int8Type", "i8"), ("UInt32Type", "u32"), ("Int64Type", "i64"), ("UInt64Type", "u64"), ("Float32Type", "f32"), ("Float64Type", "f64")] {
            assert_eq!(e.native_for(&format!("{ca}<polars_core::datatypes::{t}>")), Some(n), "{name}: {t}");
        }
        assert_eq!(e.native_for(&format!("{ca}<polars_core::datatypes::BooleanType>")), None, "{name}: a nonnumeric type has no native");
        let mut swapped = e.clone();
        swapped.natives.swap(0, 3);
        assert!(swapped.check(&c).is_err(), "{name}: Int8Type with i64 fails closed");
    }
    println!("method-scalar-generic self-test: ok");
}

/// Record 0090 gate 2 controls: `T::Native` and `ChunkedArray<T>` are
/// replaced only as whole tokens, per listed type (a narrow signed integer,
/// `IdxCa`'s `u32`, `f32`), through `Option`; a spelling inside another
/// path is left alone and refused as residual; a listed function whose
/// parameter keeps an unresolved associated type is an exception.
fn native_substitution_self_test() {
    assert_eq!(replace_token("core::option::Option<T::Native>", "T::Native", "i8"), "core::option::Option<i8>");
    assert_eq!(replace_token("polars_core::chunked_array::ChunkedArray<T>", "polars_core::chunked_array::ChunkedArray<T>", "polars_core::datatypes::Int8Chunked"), "polars_core::datatypes::Int8Chunked");
    assert_eq!(replace_token("my::XT::Native", "T::Native", "i8"), "my::XT::Native", "a longer identifier is not the token");
    assert_eq!(replace_token("T::NativeExt", "T::Native", "i8"), "T::NativeExt", "a longer name is not the token");
    assert_eq!(replace_token("a::T::Native", "T::Native", "i8"), "a::T::Native", "a path segment is not the generic");
    let ca = "polars_core::chunked_array::ChunkedArray";
    let sup = |path: &str| Supporting {
        key: path.to_string(), kind: "struct".into(), canonical_path: path.to_string(),
        found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())], crate_paths: vec![path.to_string()],
        public_fields: 0, fields_canonical: vec![], variant_shapes: vec![], variant_payloads: vec![], generic: false, lifetime: false, hidden: false,
        derived: vec!["Clone".into(), "Debug".into()], alias_target: None, implementors: vec![], impls: vec![],
    };
    let alias = |path: &str, target: &str| { let mut a = sup(path); a.kind = "type_alias".into(); a.alias_target = Some(target.into()); a };
    let mut generic = sup(ca); generic.generic = true;
    let mk = |key: &str, name: &str, params: Vec<(&str, &str)>| Callable {
        key: key.into(), kind: "free_fn".into(), krate: "polars_ops".into(), owner: String::new(), name: name.into(), canonical_path: format!("polars_ops::m::{name}"),
        found_paths: vec![format!("polars::m::{name}")], crate_paths: vec![], receiver: "none".into(), params: params.iter().map(|(n, t)| Param { name: n.to_string(), ty: t.to_string(), ty_canonical: t.to_string() }).collect(),
        ret: None, ret_canonical: Some("polars_core::datatypes::BooleanChunked".into()), generics_canonical: vec![("T".into(), "polars_core::datatypes::PolarsNumericType".into())],
        impl_for: None, impl_bounds: vec![], impl_head: None, impl_where: vec![], impl_assoc: vec![], docs_first: None, owner_generic: false, is_unsafe: false, is_async: false,
        deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "generic".into(), rules: vec![],
    };
    let arr = format!("&{ca}<T>");
    let inv = Inventory {
        callables: vec![
            mk("peaks", "peak", vec![("ca", &arr), ("start", "core::option::Option<T::Native>"), ("end", "core::option::Option<T::Native>")]),
            mk("residual", "other", vec![("ca", &arr), ("v", "T::Physical")]),
        ],
        supporting: vec![generic, sup("polars_core::datatypes::BooleanChunked"),
            alias("polars_core::datatypes::Int8Chunked", &format!("{ca}<polars_core::datatypes::Int8Type>")),
            alias("polars_core::datatypes::aliases::IdxCa", &format!("{ca}<polars_core::datatypes::UInt32Type>")),
            alias("polars_core::datatypes::Float32Chunked", &format!("{ca}<polars_core::datatypes::Float32Type>"))],
        provenance: None,
    };
    let mut release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), instantiation: InstantiationScope::default(), api_crates: vec!["polars_core".into(), "polars_ops".into()], unordered: vec![], excluded_oracle: vec![], refused: vec![], bitmap_returns: vec![], bitmap_inputs: vec![], iterator_returns: vec![], cow_returns: vec![], free_instantiations: vec![], method_scalar_generics: vec![], bounded_readbacks: vec![], hash_tokens: vec![], null_aware_returns: vec![], sized_self_methods: vec![], external_bounds: vec![], chunk_snapshots: vec![], indexed_chunk_snapshots: vec![], array_snapshots: vec![], iter_snapshots: vec![], callback_mutable: vec![], callback_invocation: vec![], callback_sink: vec![], callback_safe: vec![], callback_recipe: vec![] };
    let types = vec!["polars_core::datatypes::Int8Type".to_string(), "polars_core::datatypes::UInt32Type".into(), "polars_core::datatypes::Float32Type".into()];
    let natives = vec!["i8".to_string(), "u32".into(), "f32".into()];
    for (k, n) in [("peaks", "peak"), ("residual", "other")] { release.free_instantiations.push(FreeInstantiation { key: k.into(), path: format!("polars_ops::m::{n}"), callee: format!("polars::m::{n}"), generic: "T".into(), types: types.clone(), natives: natives.clone(), guard_param: None, guard: None, cite: "t".into() }); }
    let world = World::new(&inv, &release, &["mechanical", "generic_fn"]);
    let empty = || Emitted { from_names: BTreeMap::new(), functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    let emit = |key: &str| { let mut e = empty(); emit_callable(&world, &mut e, inv.callables.iter().find(|c| c.key == key).unwrap(), &["mechanical", "generic_fn"]); (e.entries[0].clone(), e.functions) };
    let (e, f) = emit("peaks");
    assert_eq!(e.status, "generated", "{:?} {:?}", e.reason, e.exceptions.iter().map(|x| &x.reason).collect::<Vec<_>>());
    assert_eq!(e.bindings.len(), 3, "{:?}", e.exceptions.iter().map(|x| &x.reason).collect::<Vec<_>>());
    assert!(f.contains("support::narrow::<i8>") && f.contains("support::narrow::<u32>") && f.contains("(v as f32)") && f.contains("None => None"), "per-type natives through Option: {f}");
    assert!(!f.contains("T::Native") && !f.contains("<T>"), "no generic left in any binding: {f}");
    let (e, _) = emit("residual");
    assert_eq!(e.status, "unsupported", "an unresolved associated type keeps the function refused: {:?}", e.reason);
    assert!(e.exceptions.iter().all(|x| x.reason.contains("remains in a parameter")), "{:?}", e.exceptions.iter().map(|x| &x.reason).collect::<Vec<_>>());
    // record 0091: a guarded usize parameter is checked in `pre`, only for its listed function
    let mut g = release.clone();
    g.free_instantiations.clear();
    let guard_inv = Inventory { callables: vec![
        mk("guarded", "bound", vec![("ca", &arr), ("target_len", "usize"), ("flag", "bool")]),
        mk("plain", "unguarded", vec![("ca", &arr), ("target_len", "usize")]),
    ], supporting: inv.supporting.clone(), provenance: None };
    g.free_instantiations.push(FreeInstantiation { key: "guarded".into(), path: "polars_ops::m::bound".into(), callee: "polars::m::bound".into(), generic: "T".into(), types: vec!["polars_core::datatypes::Int8Type".into()], natives: vec![], guard_param: Some("target_len".into()), guard: Some("below_idx_max".into()), cite: "t".into() });
    g.free_instantiations.push(FreeInstantiation { key: "plain".into(), path: "polars_ops::m::unguarded".into(), callee: "polars::m::unguarded".into(), generic: "T".into(), types: vec!["polars_core::datatypes::Int8Type".into()], natives: vec![], guard_param: None, guard: None, cite: "t".into() });
    let gw = World::new(&guard_inv, &g, &["mechanical", "generic_fn"]);
    let gemit = |key: &str| { let mut e = empty(); emit_callable(&gw, &mut e, guard_inv.callables.iter().find(|c| c.key == key).unwrap(), &["mechanical", "generic_fn"]); (e.entries[0].clone(), e.functions) };
    let (e, f) = gemit("guarded");
    assert_eq!(e.status, "generated", "{:?}", e.reason);
    assert!(f.contains("let __guarded_target_len = support::below_idx_max(support::narrow::<usize>(target_len, \"target_len\")?, \"bound\", \"target_len\")?;") && f.contains("polars::m::bound(&ca.0, __guarded_target_len, flag)"), "the guard runs before the call: {f}");
    let (e, f) = gemit("plain");
    assert_eq!(e.status, "generated", "{:?}", e.reason);
    assert!(!f.contains("below_idx_max"), "no guard on an unlisted parameter: {f}");
    assert!(gw.arg_guard.borrow().is_none(), "the guard scope never outlives its instantiation");
    // fail closed: every malformed guard refuses the whole function, nothing unguarded is emitted
    for (label, param, check) in [
        ("guard without parameter", None, Some("below_idx_max")),
        ("parameter without guard", Some("target_len"), None),
        ("parameter that does not exist", Some("target_length"), Some("below_idx_max")),
        ("parameter that is not usize", Some("flag"), Some("below_idx_max")),
        ("unknown guard", Some("target_len"), Some("below_something")),
    ] {
        let mut bad = g.clone();
        bad.free_instantiations[0].guard_param = param.map(String::from);
        bad.free_instantiations[0].guard = check.map(String::from);
        let bw = World::new(&guard_inv, &bad, &["mechanical", "generic_fn"]);
        let mut e = empty();
        emit_callable(&bw, &mut e, guard_inv.callables.iter().find(|c| c.key == "guarded").unwrap(), &["mechanical", "generic_fn"]);
        assert_eq!(e.entries[0].status, "unsupported", "{label}: must refuse, got {:?}", e.entries[0].reason);
        assert!(e.functions.is_empty(), "{label}: no binding text at all: {}", e.functions);
        assert!(e.entries[0].reason.as_deref().unwrap_or("").contains("guard"), "{label}: {:?}", e.entries[0].reason);
    }
    println!("native-substitution self-test: ok");
}

/// Record 0089 gate 2 controls, from a synthetic inventory: a listed generic
/// free function over `&ChunkedArray<T>` becomes a static function on each
/// listed type's wrapper (including a type held by an alias such as
/// `IdxCa`), borrowing its argument and checking a `usize` result into
/// range; an unlisted generic free function, a listed function under an
/// unlisted type, and a return-only generic stay refused.
fn free_instantiation_self_test() {
    let ca = "polars_core::chunked_array::ChunkedArray";
    let i64c = "polars_core::datatypes::Int64Chunked";
    let idx = "polars_core::datatypes::aliases::IdxCa";
    let sup = |path: &str| Supporting {
        key: path.to_string(), kind: "struct".into(), canonical_path: path.to_string(),
        found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())], crate_paths: vec![path.to_string()],
        public_fields: 0, fields_canonical: vec![], variant_shapes: vec![], variant_payloads: vec![], generic: false, lifetime: false, hidden: false,
        derived: vec!["Clone".into(), "Debug".into()], alias_target: None, implementors: vec![], impls: vec![],
    };
    let alias = |path: &str, target: &str| { let mut a = sup(path); a.kind = "type_alias".into(); a.alias_target = Some(target.into()); a };
    let mut generic = sup(ca); generic.generic = true;
    let mk = |key: &str, name: &str, params: Vec<(&str, &str)>, generics: Vec<(&str, &str)>, ret: &str| Callable {
        key: key.into(), kind: "free_fn".into(), krate: "polars_core".into(), owner: String::new(), name: name.into(), canonical_path: format!("polars_core::m::{name}"),
        found_paths: vec![format!("polars::m::{name}")], crate_paths: vec![], receiver: "none".into(), params: params.iter().map(|(n, t)| Param { name: n.to_string(), ty: t.to_string(), ty_canonical: t.to_string() }).collect(),
        ret: None, ret_canonical: Some(ret.into()), generics_canonical: generics.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
        impl_for: None, impl_bounds: vec![], impl_head: None, impl_where: vec![], impl_assoc: vec![], docs_first: None, owner_generic: false, is_unsafe: false, is_async: false,
        deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "generic".into(), rules: vec![],
    };
    let arg = format!("&{ca}<T>");
    let inv = Inventory {
        callables: vec![
            mk("listed", "arg_lo", vec![("ca", &arg)], vec![("T", "polars_core::datatypes::PolarsNumericType")], "core::option::Option<usize>"),
            mk("unlisted", "arg_hi", vec![("ca", &arg)], vec![("T", "polars_core::datatypes::PolarsNumericType")], "core::option::Option<usize>"),
            mk("ret_only", "zero", vec![], vec![("T", "polars_core::datatypes::PolarsNumericType")], "T"),
        ],
        supporting: vec![generic, alias(i64c, &format!("{ca}<polars_core::datatypes::Int64Type>")), alias(idx, &format!("{ca}<polars_core::datatypes::UInt32Type>"))],
        provenance: None,
    };
    let mut release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), instantiation: InstantiationScope::default(), api_crates: vec!["polars_core".into()], unordered: vec![], excluded_oracle: vec![], refused: vec![], bitmap_returns: vec![], bitmap_inputs: vec![], iterator_returns: vec![], cow_returns: vec![], free_instantiations: vec![], method_scalar_generics: vec![], bounded_readbacks: vec![], hash_tokens: vec![], null_aware_returns: vec![], sized_self_methods: vec![], external_bounds: vec![], chunk_snapshots: vec![], indexed_chunk_snapshots: vec![], array_snapshots: vec![], iter_snapshots: vec![], callback_mutable: vec![], callback_invocation: vec![], callback_sink: vec![], callback_safe: vec![], callback_recipe: vec![] };
    release.free_instantiations.push(FreeInstantiation { key: "listed".into(), path: "polars_core::m::arg_lo".into(), callee: "polars::m::arg_lo".into(), generic: "T".into(), types: vec!["polars_core::datatypes::Int64Type".into(), "polars_core::datatypes::UInt32Type".into(), "polars_core::datatypes::Float32Type".into()], natives: vec![], guard_param: None, guard: None, cite: "t".into() });
    let world = World::new(&inv, &release, &["mechanical", "generic_fn"]);
    let empty = || Emitted { from_names: BTreeMap::new(), functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    let emit = |key: &str| { let mut e = empty(); emit_callable(&world, &mut e, inv.callables.iter().find(|c| c.key == key).unwrap(), &["mechanical", "generic_fn"]); (e.entries[0].clone(), e.functions) };
    let (e, f) = emit("listed");
    assert_eq!(e.status, "generated", "{:?}", e.reason);
    assert_eq!(e.bindings.len(), 2, "one binding per listed type that a wrapper holds: {:?}", e.exceptions.iter().map(|x| &x.reason).collect::<Vec<_>>());
    assert!(e.exceptions.iter().any(|x| x.reason.contains("Float32Type")), "a listed type without a wrapper is a named exception");
    assert!(f.contains("::arg_lo)]") && f.contains("polars::m::arg_lo(&ca.0)") && f.contains("support::widen::<usize>(__r, \"arg_lo\")?") && f.contains("None => None"), "static on the wrapper, borrowed, range-checked: {f}");
    assert!(!f.contains("ca.0.clone()") && !f.contains("as i64"), "no clone of the argument, no unchecked cast: {f}");
    for key in ["unlisted", "ret_only"] {
        let (e, _) = emit(key);
        assert_eq!(e.status, "unsupported", "{key} must stay refused, got {:?}", e.reason);
    }
    println!("free-instantiation self-test: ok");
}

/// Record 0103 controls: an `[[iter_snapshots]]` entry admits only
/// `&self -> impl DoubleEndedIterator<Item = &T::Array>` and fails closed,
/// naming the fault; with the scope set only the pair's own borrowed array
/// item binds, handing the copier the receiver's chunks, while an owned,
/// other-array or plain-Iterator item and an unscoped return stay
/// unsupported with no text.
fn iter_snapshot_self_test() {
    let ca = "polars_core::chunked_array::ChunkedArray";
    let mk = |params: Vec<Param>, ret: &str| Callable {
        key: "k".into(), kind: "inherent".into(), krate: "polars_core".into(), owner: ca.into(), name: "downcast_iter".into(), canonical_path: format!("{ca}::downcast_iter"),
        found_paths: vec![], crate_paths: vec![], receiver: "&self".into(), params,
        ret: None, ret_canonical: Some(ret.into()), generics_canonical: vec![],
        impl_for: None, impl_bounds: vec![("T".into(), "polars_core::datatypes::PolarsDataType".into())], impl_head: Some(format!("{ca}<T>")), impl_where: vec![], impl_assoc: vec![], docs_first: None, owner_generic: true, is_unsafe: false, is_async: false,
        deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "generic".into(), rules: vec![],
    };
    let pairs = |p: Vec<(&str, &str)>| p.into_iter().map(|(a, b)| (format!("polars_core::datatypes::{a}"), b.to_string())).collect::<Vec<_>>();
    let entry = |p: Vec<(String, String)>, cite: &str| IterSnapshot { key: "k".into(), path: format!("{ca}::downcast_iter"), pairs: p, cite: cite.into() };
    let good_c = mk(vec![], DOWNCAST_ITER_RETURN);
    let good = entry(pairs(vec![("Int8Type", "i8"), ("UInt64Type", "u64"), ("StringType", "str")]), "t");
    assert!(good.check(&good_c).is_ok());
    let mut owned = good_c.clone(); owned.receiver = "self".into();
    for (label, c, e, why) in [
        ("blank citation", good_c.clone(), entry(pairs(vec![("Int8Type", "i8")]), " "), "no citation"),
        ("an owned receiver", owned, good.clone(), "without parameters or method generics"),
        ("a parameter", mk(vec![Param { name: "idx".into(), ty: "usize".into(), ty_canonical: "usize".into() }], DOWNCAST_ITER_RETURN), good.clone(), "without parameters or method generics"),
        ("an owned item", mk(vec![], "impl core::iter::traits::double_ended::DoubleEndedIterator<Item = T::Array>"), good.clone(), "is not impl"),
        ("a plain Iterator", mk(vec![], "impl core::iter::traits::iterator::Iterator<Item = &T::Array>"), good.clone(), "is not impl"),
        ("an optional return", mk(vec![], &format!("core::option::Option<{DOWNCAST_ITER_RETURN}>")), good.clone(), "is not impl"),
        ("a mispaired kind", good_c.clone(), entry(pairs(vec![("BooleanType", "u8")]), "t"), "nor a listed scalar owner and its kind"),
        ("List", good_c.clone(), entry(pairs(vec![("ListType", "list")]), "t"), "nor a listed scalar owner and its kind"),
        ("a duplicate pair", good_c.clone(), entry(pairs(vec![("Int8Type", "i8"), ("Int8Type", "i8")]), "t"), "is listed twice"),
    ] {
        let r = e.check(&c);
        assert!(r.as_ref().is_err_and(|m| m.contains(why)), "{label}: {r:?}");
    }
    let mut release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), instantiation: InstantiationScope::default(), api_crates: vec!["polars_core".into()], unordered: vec![], excluded_oracle: vec![], refused: vec![], bitmap_returns: vec![], bitmap_inputs: vec![], iterator_returns: vec![], cow_returns: vec![], free_instantiations: vec![], method_scalar_generics: vec![], bounded_readbacks: vec![], hash_tokens: vec![], null_aware_returns: vec![], sized_self_methods: vec![], external_bounds: vec![], chunk_snapshots: vec![], indexed_chunk_snapshots: vec![], array_snapshots: vec![], iter_snapshots: vec![], callback_mutable: vec![], callback_invocation: vec![], callback_sink: vec![], callback_safe: vec![], callback_recipe: vec![] };
    assert!(iter_snapshot_entry(&release, &good_c).is_none());
    release.iter_snapshots = vec![good.clone(), good];
    assert!(matches!(iter_snapshot_entry(&release, &good_c), Some(Err(ref m)) if m == "listed twice"));
    let owner = "polars_core::datatypes::Int8Chunked";
    let sup = |path: &str| Supporting {
        key: path.to_string(), kind: "struct".into(), canonical_path: path.to_string(),
        found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())], crate_paths: vec![path.to_string()],
        public_fields: 0, fields_canonical: vec![], variant_shapes: vec![], variant_payloads: vec![], generic: false, lifetime: false, hidden: false,
        derived: vec!["Clone".into(), "Debug".into()], alias_target: None, implementors: vec![], impls: vec![],
    };
    let conc = |key: &str, ret: &str| { let mut c = mk(vec![], ret); c.key = key.into(); c.owner = owner.into(); c.canonical_path = format!("{owner}::downcast_iter"); c.impl_head = None; c.impl_bounds.clear(); c.owner_generic = false; c.bucket = "mechanical".into(); c };
    let it = |item: &str| format!("impl core::iter::traits::double_ended::DoubleEndedIterator<Item = {item}>");
    let inv = Inventory { callables: vec![
        conc("i8", &it("&polars_arrow::array::primitive::PrimitiveArray<i8>")), conc("bool", &it("&polars_arrow::array::boolean::BooleanArray")),
        conc("other", &it("&polars_arrow::array::primitive::PrimitiveArray<i16>")), conc("owned_item", &it("polars_arrow::array::primitive::PrimitiveArray<i8>")), conc("unscoped", &it("&polars_arrow::array::primitive::PrimitiveArray<i8>")),
    ], supporting: vec![sup(owner)], provenance: None };
    let world = World::new(&inv, &release, &["mechanical"]);
    let empty = || Emitted { from_names: BTreeMap::new(), functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    let emit = |key: &str, kind: Option<&str>| {
        let mut e = empty();
        *world.iter_snapshot.borrow_mut() = kind.map(|n| ("downcast_iter".to_string(), n.to_string()));
        emit_method(&world, &mut e, inv.callables.iter().find(|c| c.key == key).unwrap(), owner, None, false);
        *world.iter_snapshot.borrow_mut() = None;
        (e.entries[0].status.clone(), e.entries[0].reason.clone().unwrap_or_default(), e.functions)
    };
    for (key, kind, want) in [("i8", "i8", "support::iter_snapshot::<i8, _>(this.0.chunks(), __r, \"downcast_iter\", |__r| Ok::<_, Error>((__r as i64)))?"), ("bool", "bool", "support::iter_snapshot_bool(this.0.chunks(), __r, \"downcast_iter\")?")] {
        let (status, reason, f) = emit(key, Some(kind));
        assert_eq!(status, "generated", "{key}: {reason}");
        assert!(f.contains(want) && f.contains(">::downcast_iter(&this.0)"), "{key}: one Polars call, the copier sees the receiver's chunks: {f}");
    }
    for (key, kind) in [("other", Some("i8")), ("owned_item", Some("i8")), ("unscoped", None), ("bool", Some("i8"))] {
        let (status, reason, f) = emit(key, kind);
        assert_eq!(status, "unsupported", "{key} must be refused, got {reason}");
        assert!(f.is_empty(), "{key}: no binding text: {f}");
    }
    assert!(world.iter_snapshot.borrow().is_none());
    println!("iter-snapshot self-test: ok");
}

/// Record 0102 controls: an `[[array_snapshots]]` entry admits only
/// `&self -> &T::Array` on `ChunkedArray<T: PolarsDataType>` and fails
/// closed, naming the fault; with the scope set only `&Array` of the pair's
/// own array binds (the scalar rule or the kind's copier), while an optional,
/// nested, other-array or unscoped return stays unsupported with no text.
fn array_snapshot_self_test() {
    let ca = "polars_core::chunked_array::ChunkedArray";
    let mk = |params: Vec<Param>, ret: &str| Callable {
        key: "k".into(), kind: "inherent".into(), krate: "polars_core".into(), owner: ca.into(), name: "downcast_as_array".into(), canonical_path: format!("{ca}::downcast_as_array"),
        found_paths: vec![], crate_paths: vec![], receiver: "&self".into(), params,
        ret: None, ret_canonical: Some(ret.into()), generics_canonical: vec![],
        impl_for: None, impl_bounds: vec![("T".into(), "polars_core::datatypes::PolarsDataType".into())], impl_head: Some(format!("{ca}<T>")), impl_where: vec![], impl_assoc: vec![], docs_first: None, owner_generic: true, is_unsafe: false, is_async: false,
        deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "generic".into(), rules: vec![],
    };
    let pairs = |p: Vec<(&str, &str)>| p.into_iter().map(|(a, b)| (format!("polars_core::datatypes::{a}"), b.to_string())).collect::<Vec<_>>();
    let entry = |p: Vec<(String, String)>, cite: &str| ArraySnapshot { key: "k".into(), path: format!("{ca}::downcast_as_array"), pairs: p, cite: cite.into() };
    let good_c = mk(vec![], DOWNCAST_AS_ARRAY_RETURN);
    let good = entry(pairs(vec![("Int8Type", "i8"), ("UInt64Type", "u64"), ("StringType", "str")]), "t");
    assert!(good.check(&good_c).is_ok());
    let mut owned = good_c.clone(); owned.receiver = "self".into();
    let mut generic = good_c.clone(); generic.generics_canonical = vec![("F".into(), "".into())];
    let mut with_where = good_c.clone(); with_where.impl_where = vec!["T: core::fmt::Debug".into()];
    for (label, c, e, why) in [
        ("blank citation", good_c.clone(), entry(pairs(vec![("Int8Type", "i8")]), " "), "no citation"),
        ("an owned receiver", owned, good.clone(), "without parameters or method generics"),
        ("a method generic", generic, good.clone(), "without parameters or method generics"),
        ("a where-clause", with_where, good.clone(), "is not ChunkedArray<T: PolarsDataType>"),
        ("a parameter", mk(vec![Param { name: "idx".into(), ty: "usize".into(), ty_canonical: "usize".into() }], DOWNCAST_AS_ARRAY_RETURN), good.clone(), "without parameters or method generics"),
        ("an optional return", mk(vec![], "core::option::Option<&T::Array>"), good.clone(), "is not &T::Array"),
        ("a fallible return", mk(vec![], "polars_error::PolarsResult<&T::Array>"), good.clone(), "is not &T::Array"),
        ("a mispaired kind", good_c.clone(), entry(pairs(vec![("StringType", "binary")]), "t"), "nor a listed scalar owner and its kind"),
        ("Struct", good_c.clone(), entry(pairs(vec![("StructType", "struct")]), "t"), "nor a listed scalar owner and its kind"),
        ("a duplicate pair", good_c.clone(), entry(pairs(vec![("Int8Type", "i8"), ("Int8Type", "i8")]), "t"), "is listed twice"),
    ] {
        let r = e.check(&c);
        assert!(r.as_ref().is_err_and(|m| m.contains(why)), "{label}: {r:?}");
    }
    let mut release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), instantiation: InstantiationScope::default(), api_crates: vec!["polars_core".into()], unordered: vec![], excluded_oracle: vec![], refused: vec![], bitmap_returns: vec![], bitmap_inputs: vec![], iterator_returns: vec![], cow_returns: vec![], free_instantiations: vec![], method_scalar_generics: vec![], bounded_readbacks: vec![], hash_tokens: vec![], null_aware_returns: vec![], sized_self_methods: vec![], external_bounds: vec![], chunk_snapshots: vec![], indexed_chunk_snapshots: vec![], array_snapshots: vec![], iter_snapshots: vec![], callback_mutable: vec![], callback_invocation: vec![], callback_sink: vec![], callback_safe: vec![], callback_recipe: vec![] };
    assert!(array_snapshot_entry(&release, &good_c).is_none());
    release.array_snapshots = vec![good.clone(), good];
    assert!(matches!(array_snapshot_entry(&release, &good_c), Some(Err(ref m)) if m == "listed twice"));
    let owner = "polars_core::datatypes::Int8Chunked";
    let sup = |path: &str| Supporting {
        key: path.to_string(), kind: "struct".into(), canonical_path: path.to_string(),
        found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())], crate_paths: vec![path.to_string()],
        public_fields: 0, fields_canonical: vec![], variant_shapes: vec![], variant_payloads: vec![], generic: false, lifetime: false, hidden: false,
        derived: vec!["Clone".into(), "Debug".into()], alias_target: None, implementors: vec![], impls: vec![],
    };
    let conc = |key: &str, ret: &str| { let mut c = mk(vec![], ret); c.key = key.into(); c.owner = owner.into(); c.canonical_path = format!("{owner}::downcast_as_array"); c.impl_head = None; c.impl_bounds.clear(); c.owner_generic = false; c.bucket = "mechanical".into(); c };
    let prim = |n: &str| format!("&polars_arrow::array::primitive::PrimitiveArray<{n}>");
    let inv = Inventory { callables: vec![
        conc("i8", &prim("i8")), conc("u64", &prim("u64")), conc("bin", "&polars_arrow::array::binview::BinaryViewArrayGeneric<[u8]>"),
        conc("other", &prim("i16")), conc("optional", &format!("core::option::Option<{}>", prim("i8"))), conc("owned", "polars_arrow::array::primitive::PrimitiveArray<i8>"), conc("unscoped", &prim("i8")),
    ], supporting: vec![sup(owner)], provenance: None };
    let world = World::new(&inv, &release, &["mechanical"]);
    let empty = || Emitted { from_names: BTreeMap::new(), functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    let emit = |key: &str, kind: Option<&str>| {
        let mut e = empty();
        *world.array_snapshot.borrow_mut() = kind.map(|n| ("downcast_as_array".to_string(), n.to_string()));
        emit_method(&world, &mut e, inv.callables.iter().find(|c| c.key == key).unwrap(), owner, None, false);
        *world.array_snapshot.borrow_mut() = None;
        (e.entries[0].status.clone(), e.entries[0].reason.clone().unwrap_or_default(), e.functions)
    };
    for (key, kind, want) in [("i8", "i8", "support::array_snapshot::<i8, _>(__r, \"downcast_as_array\", |__r| Ok::<_, Error>((__r as i64)))?"), ("u64", "u64", "support::widen::<u64>(__r, \"downcast_as_array\")?"), ("bin", "binary", "support::array_snapshot_binview(__r, \"downcast_as_array\")?")] {
        let (status, reason, f) = emit(key, Some(kind));
        assert_eq!(status, "generated", "{key}: {reason}");
        assert!(f.contains(want) && !f.contains("-> Result<Option<"), "{key}: one vector, no second optional layer: {f}");
    }
    for (key, kind) in [("other", Some("i8")), ("optional", Some("i8")), ("owned", Some("i8")), ("unscoped", None), ("bin", Some("str"))] {
        let (status, reason, f) = emit(key, kind);
        assert_eq!(status, "unsupported", "{key} must be refused, got {reason}");
        assert!(f.is_empty(), "{key}: no binding text: {f}");
    }
    assert!(world.array_snapshot.borrow().is_none());
    println!("array-snapshot self-test: ok");
}

/// Record 0101 controls: an `[[indexed_chunk_snapshots]]` entry admits
/// only `&self, usize -> Option<&T::Array>` on `ChunkedArray<T:
/// PolarsDataType>` and fails closed, naming the fault, on a blank citation,
/// another impl, receiver, parameter or generic, another return, a bad pair
/// and a double listing. With the scope set, only `Option<&Array>` of the
/// pair's own array binds (numeric through the scalar rule, scalar kinds
/// through their copier); another array, a nested option, a non-optional
/// borrow and an unscoped return stay unsupported with no text.
fn indexed_chunk_self_test() {
    let ca = "polars_core::chunked_array::ChunkedArray";
    let idx = || vec![Param { name: "idx".into(), ty: "usize".into(), ty_canonical: "usize".into() }];
    let mk = |params: Vec<Param>, ret: &str| Callable {
        key: "k".into(), kind: "inherent".into(), krate: "polars_core".into(), owner: ca.into(), name: "downcast_get".into(), canonical_path: format!("{ca}::downcast_get"),
        found_paths: vec![], crate_paths: vec![], receiver: "&self".into(), params,
        ret: None, ret_canonical: Some(ret.into()), generics_canonical: vec![],
        impl_for: None, impl_bounds: vec![("T".into(), "polars_core::datatypes::PolarsDataType".into())], impl_head: Some(format!("{ca}<T>")), impl_where: vec![], impl_assoc: vec![], docs_first: None, owner_generic: true, is_unsafe: false, is_async: false,
        deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "generic".into(), rules: vec![],
    };
    let pairs = |p: Vec<(&str, &str)>| p.into_iter().map(|(a, b)| (format!("polars_core::datatypes::{a}"), b.to_string())).collect::<Vec<_>>();
    let entry = |p: Vec<(String, String)>, cite: &str| IndexedChunkSnapshot { key: "k".into(), path: format!("{ca}::downcast_get"), pairs: p, cite: cite.into() };
    let good_c = mk(idx(), DOWNCAST_GET_RETURN);
    let good = entry(pairs(vec![("Int8Type", "i8"), ("UInt64Type", "u64"), ("StringType", "str"), ("BinaryOffsetType", "binary_offset")]), "t");
    assert!(good.check(&good_c).is_ok());
    let mut owned = good_c.clone(); owned.receiver = "self".into();
    let mut generic = good_c.clone(); generic.generics_canonical = vec![("F".into(), "".into())];
    for (label, c, e, why) in [
        ("blank citation", good_c.clone(), entry(pairs(vec![("Int8Type", "i8")]), " "), "no citation"),
        ("an owned receiver", owned, good.clone(), "not a `&self` method taking one usize"),
        ("a method generic", generic, good.clone(), "not a `&self` method taking one usize"),
        ("no parameter", mk(vec![], DOWNCAST_GET_RETURN), good.clone(), "not a `&self` method taking one usize"),
        ("an i64 index", mk(vec![Param { name: "idx".into(), ty: "i64".into(), ty_canonical: "i64".into() }], DOWNCAST_GET_RETURN), good.clone(), "not a `&self` method taking one usize"),
        ("a non-optional return", mk(idx(), "&T::Array"), good.clone(), "is not core::option::Option<&T::Array>"),
        ("a fallible return", mk(idx(), "polars_error::PolarsResult<&T::Array>"), good.clone(), "is not core::option::Option<&T::Array>"),
        ("a mispaired kind", good_c.clone(), entry(pairs(vec![("StringType", "binary")]), "t"), "nor a listed scalar owner and its kind"),
        ("List", good_c.clone(), entry(pairs(vec![("ListType", "list")]), "t"), "nor a listed scalar owner and its kind"),
        ("a duplicate pair", good_c.clone(), entry(pairs(vec![("Int8Type", "i8"), ("Int8Type", "i8")]), "t"), "is listed twice"),
    ] {
        let r = e.check(&c);
        assert!(r.as_ref().is_err_and(|m| m.contains(why)), "{label}: {r:?}");
    }
    let mut release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), instantiation: InstantiationScope::default(), api_crates: vec!["polars_core".into()], unordered: vec![], excluded_oracle: vec![], refused: vec![], bitmap_returns: vec![], bitmap_inputs: vec![], iterator_returns: vec![], cow_returns: vec![], free_instantiations: vec![], method_scalar_generics: vec![], bounded_readbacks: vec![], hash_tokens: vec![], null_aware_returns: vec![], sized_self_methods: vec![], external_bounds: vec![], chunk_snapshots: vec![], indexed_chunk_snapshots: vec![], array_snapshots: vec![], iter_snapshots: vec![], callback_mutable: vec![], callback_invocation: vec![], callback_sink: vec![], callback_safe: vec![], callback_recipe: vec![] };
    assert!(indexed_chunk_entry(&release, &good_c).is_none());
    release.indexed_chunk_snapshots = vec![good.clone(), good];
    assert!(matches!(indexed_chunk_entry(&release, &good_c), Some(Err(ref m)) if m == "listed twice"));
    let owner = "polars_core::datatypes::Int8Chunked";
    let sup = |path: &str| Supporting {
        key: path.to_string(), kind: "struct".into(), canonical_path: path.to_string(),
        found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())], crate_paths: vec![path.to_string()],
        public_fields: 0, fields_canonical: vec![], variant_shapes: vec![], variant_payloads: vec![], generic: false, lifetime: false, hidden: false,
        derived: vec!["Clone".into(), "Debug".into()], alias_target: None, implementors: vec![], impls: vec![],
    };
    let conc = |key: &str, ret: &str| { let mut c = mk(idx(), ret); c.key = key.into(); c.owner = owner.into(); c.canonical_path = format!("{owner}::downcast_get"); c.impl_head = None; c.impl_bounds.clear(); c.owner_generic = false; c.bucket = "mechanical".into(); c };
    let prim = |n: &str| format!("core::option::Option<&polars_arrow::array::primitive::PrimitiveArray<{n}>>");
    let utf8 = "core::option::Option<&polars_arrow::array::binview::BinaryViewArrayGeneric<str>>";
    let inv = Inventory { callables: vec![
        conc("i8", &prim("i8")), conc("u64", &prim("u64")), conc("str", utf8),
        conc("other_array", &prim("i16")), conc("nested", &format!("core::option::Option<{}>", prim("i8"))), conc("borrow", "&polars_arrow::array::primitive::PrimitiveArray<i8>"), conc("unscoped", &prim("i8")),
    ], supporting: vec![sup(owner)], provenance: None };
    let world = World::new(&inv, &release, &["mechanical"]);
    let empty = || Emitted { from_names: BTreeMap::new(), functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    let emit = |key: &str, kind: Option<&str>| {
        let mut e = empty();
        *world.indexed_chunk.borrow_mut() = kind.map(|n| ("downcast_get".to_string(), n.to_string()));
        emit_method(&world, &mut e, inv.callables.iter().find(|c| c.key == key).unwrap(), owner, None, false);
        *world.indexed_chunk.borrow_mut() = None;
        (e.entries[0].status.clone(), e.entries[0].reason.clone().unwrap_or_default(), e.functions)
    };
    for (key, kind, want) in [("i8", "i8", "support::indexed_snapshot::<i8, _>(__r, \"downcast_get\", |__r| Ok::<_, Error>((__r as i64)))?"), ("u64", "u64", "support::indexed_snapshot::<u64, _>(__r, \"downcast_get\", |__r| Ok::<_, Error>(support::widen::<u64>(__r, \"downcast_get\")?))?"), ("str", "str", "support::indexed_snapshot_str(__r, \"downcast_get\")?")] {
        let (status, reason, f) = emit(key, Some(kind));
        assert_eq!(status, "generated", "{key}: {reason}");
        assert!(f.contains(want) && f.contains("support::narrow::<usize>(idx, \"idx\")?"), "{key}: {f}");
    }
    for (key, kind) in [("other_array", Some("i8")), ("nested", Some("i8")), ("borrow", Some("i8")), ("unscoped", None), ("str", Some("i8"))] {
        let (status, reason, f) = emit(key, kind);
        assert_eq!(status, "unsupported", "{key} must be refused, got {reason}");
        assert!(f.is_empty(), "{key}: no binding text: {f}");
    }
    assert!(world.indexed_chunk.borrow().is_none());
    println!("indexed-chunk self-test: ok");
}

/// Record 0099 controls: a `[[chunk_snapshots]]` entry admits only
/// `&self -> &Vec<ArrayRef>` on `ChunkedArray<T: PolarsDataType>` and fails
/// closed, naming the fault, on a blank citation, another impl, a receiver,
/// parameter or generic, another return, no pair, a nonnumeric, mispaired or
/// duplicated pair, and a double listing. With the scope set, only that
/// exact top-level return binds, through `support::chunk_snapshot` with the
/// scalar rule (`u64` checked); an owned or other Arrow return, a nested
/// chunk list and an unscoped chunk list stay unsupported with no text.
fn chunk_snapshot_self_test() {
    let ca = "polars_core::chunked_array::ChunkedArray";
    let mk = |ret: &str| Callable {
        key: "k".into(), kind: "inherent".into(), krate: "polars_core".into(), owner: ca.into(), name: "chunks".into(), canonical_path: format!("{ca}::chunks"),
        found_paths: vec![], crate_paths: vec![], receiver: "&self".into(), params: vec![],
        ret: None, ret_canonical: Some(ret.into()), generics_canonical: vec![],
        impl_for: None, impl_bounds: vec![("T".into(), "polars_core::datatypes::PolarsDataType".into())], impl_head: Some(format!("{ca}<T>")), impl_where: vec![], impl_assoc: vec![], docs_first: None, owner_generic: true, is_unsafe: false, is_async: false,
        deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "generic".into(), rules: vec![],
    };
    let pairs = |p: Vec<(&str, &str)>| p.into_iter().map(|(a, b)| (format!("polars_core::datatypes::{a}"), b.to_string())).collect::<Vec<_>>();
    let entry = |p: Vec<(String, String)>, cite: &str| ChunkSnapshot { key: "k".into(), path: format!("{ca}::chunks"), pairs: p, cite: cite.into() };
    let good_c = mk(CHUNKS_RETURN);
    let good = entry(pairs(vec![("Int8Type", "i8"), ("UInt64Type", "u64"), ("Float32Type", "f32")]), "t");
    assert!(good.check(&good_c).is_ok());
    assert_eq!(good.native_for(&format!("{ca}<polars_core::datatypes::UInt64Type>")), Some("u64"));
    assert_eq!(good.native_for(&format!("{ca}<polars_core::datatypes::StringType>")), None);
    let mut other_bound = good_c.clone(); other_bound.impl_bounds = vec![("T".into(), "polars_core::datatypes::PolarsNumericType".into())];
    let mut with_where = good_c.clone(); with_where.impl_where = vec!["T::Native: core::fmt::Debug".into()];
    let mut owned = good_c.clone(); owned.receiver = "self".into();
    let mut with_param = good_c.clone(); with_param.params = vec![Param { name: "x".into(), ty: "usize".into(), ty_canonical: "usize".into() }];
    for (label, c, e, why) in [
        ("blank citation", good_c.clone(), entry(pairs(vec![("Int8Type", "i8")]), " "), "no citation"),
        ("another owner bound", other_bound, good.clone(), "is not ChunkedArray<T: PolarsDataType>"),
        ("a where-clause", with_where, good.clone(), "is not ChunkedArray<T: PolarsDataType>"),
        ("an owned receiver", owned, good.clone(), "not a `&self` method"),
        ("a parameter", with_param, good.clone(), "not a `&self` method"),
        ("a mutable borrow", mk("&mut alloc::vec::Vec<polars_arrow::array::ArrayRef>"), good.clone(), "is not &alloc::vec::Vec"),
        ("an owned vector", mk("alloc::vec::Vec<polars_arrow::array::ArrayRef>"), good.clone(), "is not &alloc::vec::Vec"),
        ("another Arrow return", mk("&polars_arrow::array::ArrayRef"), good.clone(), "is not &alloc::vec::Vec"),
        ("no pair", good_c.clone(), entry(vec![], "t"), "no listed pair"),
        ("an unlisted nonnumeric owner", good_c.clone(), entry(pairs(vec![("StructType", "struct")]), "t"), "is not a numeric type and its native"),
        ("a mispaired native", good_c.clone(), entry(pairs(vec![("Int8Type", "i64")]), "t"), "is not a numeric type and its native"),
        ("a duplicate pair", good_c.clone(), entry(pairs(vec![("Int8Type", "i8"), ("Int8Type", "i8")]), "t"), "is listed twice"),
        // record 0100: a scalar owner must carry its own kind
        ("String as binary", good_c.clone(), entry(pairs(vec![("StringType", "binary")]), "t"), "nor a listed scalar owner and its kind"),
        ("Boolean as a number", good_c.clone(), entry(pairs(vec![("BooleanType", "u8")]), "t"), "nor a listed scalar owner and its kind"),
        ("List", good_c.clone(), entry(pairs(vec![("ListType", "list")]), "t"), "nor a listed scalar owner and its kind"),
    ] {
        let r = e.check(&c);
        assert!(r.as_ref().is_err_and(|m| m.contains(why)), "{label}: {r:?}");
    }
    let mut release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), instantiation: InstantiationScope::default(), api_crates: vec!["polars_core".into()], unordered: vec![], excluded_oracle: vec![], refused: vec![], bitmap_returns: vec![], bitmap_inputs: vec![], iterator_returns: vec![], cow_returns: vec![], free_instantiations: vec![], method_scalar_generics: vec![], bounded_readbacks: vec![], hash_tokens: vec![], null_aware_returns: vec![], sized_self_methods: vec![], external_bounds: vec![], chunk_snapshots: vec![], indexed_chunk_snapshots: vec![], array_snapshots: vec![], iter_snapshots: vec![], callback_mutable: vec![], callback_invocation: vec![], callback_sink: vec![], callback_safe: vec![], callback_recipe: vec![] };
    let scalars = entry(pairs(vec![("BooleanType", "bool"), ("StringType", "str"), ("BinaryType", "binary"), ("BinaryOffsetType", "binary_offset")]), "t");
    assert!(scalars.check(&good_c).is_ok(), "the four scalar owners with their kinds pass");
    assert!(chunk_snapshot_entry(&release, &good_c).is_none());
    release.chunk_snapshots = vec![good.clone(), good];
    assert!(matches!(chunk_snapshot_entry(&release, &good_c), Some(Err(ref m)) if m == "listed twice"));
    // the emitter, with the pair scope set
    let owner = "polars_core::datatypes::Int8Chunked";
    let sup = |path: &str| Supporting {
        key: path.to_string(), kind: "struct".into(), canonical_path: path.to_string(),
        found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())], crate_paths: vec![path.to_string()],
        public_fields: 0, fields_canonical: vec![], variant_shapes: vec![], variant_payloads: vec![], generic: false, lifetime: false, hidden: false,
        derived: vec!["Clone".into(), "Debug".into()], alias_target: None, implementors: vec![], impls: vec![],
    };
    let conc = |key: &str, ret: &str| { let mut c = mk(ret); c.key = key.into(); c.owner = owner.into(); c.canonical_path = format!("{owner}::chunks"); c.impl_head = None; c.impl_bounds.clear(); c.owner_generic = false; c.bucket = "mechanical".into(); c };
    let boxed = "&alloc::vec::Vec<alloc::boxed::Box<dyn polars_arrow::array::Array>>";
    let inv = Inventory { callables: vec![conc("i8", boxed), conc("u64", boxed), conc("owned", "alloc::vec::Vec<alloc::boxed::Box<dyn polars_arrow::array::Array>>"), conc("nested", &format!("core::option::Option<{boxed}>")), conc("one", "&alloc::boxed::Box<dyn polars_arrow::array::Array>"), conc("unscoped", boxed)], supporting: vec![sup(owner)], provenance: None };
    let world = World::new(&inv, &release, &["mechanical"]);
    let empty = || Emitted { from_names: BTreeMap::new(), functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    let emit = |key: &str, native: Option<&str>| {
        let mut e = empty();
        *world.chunk_snapshot.borrow_mut() = native.map(|n| ("chunks".to_string(), n.to_string()));
        emit_method(&world, &mut e, inv.callables.iter().find(|c| c.key == key).unwrap(), owner, None, false);
        *world.chunk_snapshot.borrow_mut() = None;
        (e.entries[0].status.clone(), e.entries[0].reason.clone().unwrap_or_default(), e.functions)
    };
    for (key, native, elem) in [("i8", "i8", "(__r as i64)"), ("u64", "u64", "support::widen::<u64>(__r, \"chunks\")?")] {
        let (status, reason, f) = emit(key, Some(native));
        assert_eq!(status, "generated", "{key}: {reason}");
        assert!(f.contains("-> Result<Vec<Vec<Option<i64>>>, Error>") && f.contains(&format!("support::chunk_snapshot::<{native}, _>(__r, \"chunks\", |__r| Ok::<_, Error>({elem}))?")), "{key}: {f}");
    }
    // record 0100: each scalar kind has its own copier and element type
    for (kind, copier, elem) in [("bool", "support::chunk_snapshot_bool", "bool"), ("str", "support::chunk_snapshot_str", "String"), ("binary", "support::chunk_snapshot_binview", "Vec<i64>"), ("binary_offset", "support::chunk_snapshot_binary_offset", "Vec<i64>")] {
        let (status, reason, f) = emit("i8", Some(kind));
        assert_eq!(status, "generated", "{kind}: {reason}");
        assert!(f.contains(&format!("-> Result<Vec<Vec<Option<{elem}>>>, Error>")) && f.contains(&format!("{copier}(__r, \"chunks\")?")), "{kind}: {f}");
    }
    for (key, native) in [("owned", Some("i8")), ("nested", Some("i8")), ("one", Some("i8")), ("unscoped", None)] {
        let (status, reason, f) = emit(key, native);
        assert_eq!(status, "unsupported", "{key} must be refused, got {reason}");
        assert!(f.is_empty(), "{key}: no binding text: {f}");
    }
    assert!(world.chunk_snapshot.borrow().is_none());
    println!("chunk-snapshot self-test: ok");
}

/// Record 0098 controls: an `[[external_bounds]]` entry admits only the
/// cited shape and fails closed, naming the fault, on a blank citation,
/// another head or owner bound, a receiver, parameter or method generic, an
/// altered return, where-clauses that differ (`Canonical` removed or
/// replaced, an extra external bound), a discharge that is not a listed
/// `T::Native` clause (or is the owner bound), a nonfloat, swapped or
/// duplicated pair, and a double listing.
fn external_bound_self_test() {
    let ca = "polars_core::chunked_array::ChunkedArray";
    let owner_bound = "T: polars_core::datatypes::PolarsFloatType";
    let float = "T::Native: num_traits::float::Float";
    let canon = "T::Native: num_traits::float::Float + polars_core::chunked_array::float::Canonical";
    let mk = |ret: &str, wh: Vec<&str>| Callable {
        key: "k".into(), kind: "inherent".into(), krate: "polars_core".into(), owner: ca.into(), name: "to_canonical".into(), canonical_path: format!("{ca}::to_canonical"),
        found_paths: vec![], crate_paths: vec![], receiver: "&self".into(), params: vec![],
        ret: None, ret_canonical: Some(ret.into()), generics_canonical: vec![],
        impl_for: None, impl_bounds: vec![("T".into(), "polars_core::datatypes::PolarsFloatType".into())], impl_head: Some(format!("{ca}<T>")), impl_where: wh.into_iter().map(String::from).collect(), impl_assoc: vec![], docs_first: None, owner_generic: true, is_unsafe: false, is_async: false,
        deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "generic".into(), rules: vec![],
    };
    let pairs = |p: Vec<(&str, &str)>| p.into_iter().map(|(a, b)| (format!("polars_core::datatypes::{a}"), b.to_string())).collect::<Vec<_>>();
    let entry = |wh: Vec<&str>, discharge: Vec<&str>, p: Vec<(String, String)>, cite: &str| ExternalBound { key: "k".into(), path: format!("{ca}::to_canonical"), ret: "Self".into(), where_: wh.into_iter().map(String::from).collect(), discharge: discharge.into_iter().map(String::from).collect(), pairs: p, cite: cite.into() };
    let floats = pairs(vec![("Float32Type", "f32"), ("Float64Type", "f64")]);
    let good_c = mk("Self", vec![owner_bound, canon]);
    let good = entry(vec![owner_bound, canon], vec![canon], floats.clone(), "t");
    assert!(good.check(&good_c).is_ok());
    let mut other_head = good_c.clone(); other_head.impl_head = Some(format!("{ca}<U>"));
    let mut other_bound = good_c.clone(); other_bound.impl_bounds = vec![("T".into(), "polars_core::datatypes::PolarsNumericType".into())];
    let mut owned = good_c.clone(); owned.receiver = "self".into();
    let mut with_param = good_c.clone(); with_param.params = vec![Param { name: "x".into(), ty: "usize".into(), ty_canonical: "usize".into() }];
    let mut with_generic = good_c.clone(); with_generic.generics_canonical = vec![("F".into(), "".into())];
    for (label, c, e, why) in [
        ("blank citation", good_c.clone(), entry(vec![owner_bound, canon], vec![canon], floats.clone(), " "), "no citation"),
        ("another head", other_head, good.clone(), "is not ChunkedArray<T>"),
        ("another owner bound", other_bound, good.clone(), "are not exactly T: PolarsFloatType"),
        ("an owned receiver", owned, good.clone(), "not a `&self` method"),
        ("a parameter", with_param, good.clone(), "not a `&self` method"),
        ("a method generic", with_generic, good.clone(), "not a `&self` method"),
        ("a fallible return", mk("polars_error::PolarsResult<Self>", vec![owner_bound, canon]), good.clone(), "is not the listed"),
        ("Canonical removed", mk("Self", vec![owner_bound, float]), good.clone(), "are not the listed"),
        ("Canonical replaced", mk("Self", vec![owner_bound, "T::Native: num_traits::float::Float + polars_core::other::Trait"]), good.clone(), "are not the listed"),
        ("an extra external bound", mk("Self", vec![owner_bound, canon, "T::Native: core::fmt::LowerExp"]), good.clone(), "are not the listed"),
        ("a discharge outside where", good_c.clone(), entry(vec![owner_bound, canon], vec![float], floats.clone(), "t"), "are not listed `T::Native` where-clauses"),
        ("discharging the owner bound", good_c.clone(), entry(vec![owner_bound, canon], vec![owner_bound], floats.clone(), "t"), "are not listed `T::Native` where-clauses"),
        ("no pair", good_c.clone(), entry(vec![owner_bound, canon], vec![canon], vec![], "t"), "no listed pair"),
        ("a nonfloat pair", good_c.clone(), entry(vec![owner_bound, canon], vec![canon], pairs(vec![("Int32Type", "i32")]), "t"), "is not a float type and its native"),
        ("swapped natives", good_c.clone(), entry(vec![owner_bound, canon], vec![canon], pairs(vec![("Float32Type", "f64"), ("Float64Type", "f32")]), "t"), "is not a float type and its native"),
        ("a duplicate pair", good_c.clone(), entry(vec![owner_bound, canon], vec![canon], pairs(vec![("Float32Type", "f32"), ("Float32Type", "f32")]), "t"), "is listed twice"),
    ] {
        let r = e.check(&c);
        assert!(r.as_ref().is_err_and(|m| m.contains(why)), "{label}: {r:?}");
    }
    let mut release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), instantiation: InstantiationScope::default(), api_crates: vec!["polars_core".into()], unordered: vec![], excluded_oracle: vec![], refused: vec![], bitmap_returns: vec![], bitmap_inputs: vec![], iterator_returns: vec![], cow_returns: vec![], free_instantiations: vec![], method_scalar_generics: vec![], bounded_readbacks: vec![], hash_tokens: vec![], null_aware_returns: vec![], sized_self_methods: vec![], external_bounds: vec![], chunk_snapshots: vec![], indexed_chunk_snapshots: vec![], array_snapshots: vec![], iter_snapshots: vec![], callback_mutable: vec![], callback_invocation: vec![], callback_sink: vec![], callback_safe: vec![], callback_recipe: vec![] };
    assert!(external_bound_entry(&release, &good_c).is_none(), "an unlisted method keeps the ordinary proof");
    release.external_bounds = vec![good.clone()];
    assert!(matches!(external_bound_entry(&release, &good_c), Some(Ok(_))));
    let mut other_key = good_c.clone(); other_key.key = "other".into();
    assert!(external_bound_entry(&release, &other_key).is_none(), "the same path under another key is not listed");
    release.external_bounds.push(good);
    assert!(matches!(external_bound_entry(&release, &good_c), Some(Err(ref m)) if m == "listed twice"));
    println!("external-bound self-test: ok");
}

/// Record 0097 controls: a `[[sized_self_methods]]` entry admits only the
/// cited shape (ChunkedArray owner, `&self`, exactly `Self: Sized`, the
/// listed parameter types, a `Self` return) and fails closed, naming the
/// fault, on a blank citation, another owner or receiver, extra or other
/// generics, a fallible or borrowed return, altered parameters, or a double
/// listing; an unlisted `Self: Sized` method has no entry. With the scope
/// set, the binding checks the receiver length before the call and refuses
/// a return that is not the receiver, with no binding text.
fn sized_self_self_test() {
    let ca = "polars_core::chunked_array::ChunkedArray";
    let mk = |key: &str, owner: &str, receiver: &str, params: Vec<&str>, generics: Vec<(&str, &str)>, ret: &str| Callable {
        key: key.into(), kind: "inherent".into(), krate: "polars_core".into(), owner: owner.into(), name: "head".into(), canonical_path: format!("{ca}::head"),
        found_paths: vec![], crate_paths: vec![], receiver: receiver.into(), params: params.iter().enumerate().map(|(i, t)| Param { name: format!("p{i}"), ty: t.to_string(), ty_canonical: t.to_string() }).collect(),
        ret: None, ret_canonical: Some(ret.into()), generics_canonical: generics.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
        impl_for: None, impl_bounds: vec![], impl_head: Some(format!("{ca}<T>")), impl_where: vec![], impl_assoc: vec![], docs_first: None, owner_generic: true, is_unsafe: false, is_async: false,
        deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "generic".into(), rules: vec![],
    };
    let sized = vec![("Self", "core::marker::Sized")];
    let opt = "core::option::Option<usize>";
    let entry = |params: Vec<&str>, cite: &str| SizedSelfMethod { key: "k".into(), path: format!("{ca}::head"), params: params.into_iter().map(String::from).collect(), cite: cite.into() };
    let good = mk("k", ca, "&self", vec![opt], sized.clone(), "Self");
    assert!(entry(vec![opt], "t").check(&good).is_ok());
    for (label, c, e, why) in [
        ("blank citation", good.clone(), entry(vec![opt], " "), "no citation"),
        ("another owner", mk("k", "polars_core::chunked_array::logical::Logical", "&self", vec![opt], sized.clone(), "Self"), entry(vec![opt], "t"), "is not ChunkedArray"),
        ("owned receiver", mk("k", ca, "self", vec![opt], sized.clone(), "Self"), entry(vec![opt], "t"), "is not &self"),
        ("an extra generic", mk("k", ca, "&self", vec![opt], vec![("Self", "core::marker::Sized"), ("F", "core::ops::function::Fn()")], "Self"), entry(vec![opt], "t"), "are not exactly Self: Sized"),
        ("another bound", mk("k", ca, "&self", vec![opt], vec![("Self", "core::clone::Clone")], "Self"), entry(vec![opt], "t"), "are not exactly Self: Sized"),
        ("no generic", mk("k", ca, "&self", vec![opt], vec![], "Self"), entry(vec![opt], "t"), "are not exactly Self: Sized"),
        ("a fallible return", mk("k", ca, "&self", vec![opt], sized.clone(), "polars_error::PolarsResult<Self>"), entry(vec![opt], "t"), "is not Self"),
        ("a borrowed return", mk("k", ca, "&self", vec![opt], sized.clone(), "&Self"), entry(vec![opt], "t"), "is not Self"),
        ("an altered parameter", mk("k", ca, "&self", vec!["usize"], sized.clone(), "Self"), entry(vec![opt], "t"), "are not the listed"),
        ("an extra parameter", mk("k", ca, "&self", vec![opt, "bool"], sized.clone(), "Self"), entry(vec![opt], "t"), "are not the listed"),
    ] {
        let r = e.check(&c);
        assert!(r.as_ref().is_err_and(|m| m.contains(why)), "{label}: {r:?}");
    }
    let mut release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), instantiation: InstantiationScope::default(), api_crates: vec!["polars_core".into()], unordered: vec![], excluded_oracle: vec![], refused: vec![], bitmap_returns: vec![], bitmap_inputs: vec![], iterator_returns: vec![], cow_returns: vec![], free_instantiations: vec![], method_scalar_generics: vec![], bounded_readbacks: vec![], hash_tokens: vec![], null_aware_returns: vec![], sized_self_methods: vec![], external_bounds: vec![], chunk_snapshots: vec![], indexed_chunk_snapshots: vec![], array_snapshots: vec![], iter_snapshots: vec![], callback_mutable: vec![], callback_invocation: vec![], callback_sink: vec![], callback_safe: vec![], callback_recipe: vec![] };
    assert!(sized_self_entry(&release, &good).is_none(), "an unlisted Self: Sized method has no entry and stays with the generic census");
    release.sized_self_methods = vec![entry(vec![opt], "t")];
    assert!(matches!(sized_self_entry(&release, &good), Some(Ok(_))));
    let mut other_key = good.clone();
    other_key.key = "other".into();
    assert!(sized_self_entry(&release, &other_key).is_none(), "the same path under another key is not listed");
    release.sized_self_methods.push(entry(vec![opt], "t"));
    assert!(matches!(sized_self_entry(&release, &good), Some(Err(ref m)) if m == "listed twice"));
    // the emitter: guard before the call, and only on a receiver-typed return
    let owner = "polars_core::datatypes::Int8Chunked";
    let sup = |path: &str| Supporting {
        key: path.to_string(), kind: "struct".into(), canonical_path: path.to_string(),
        found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())], crate_paths: vec![path.to_string()],
        public_fields: 0, fields_canonical: vec![], variant_shapes: vec![], variant_payloads: vec![], generic: false, lifetime: false, hidden: false,
        derived: vec!["Clone".into(), "Debug".into()], alias_target: None, implementors: vec![], impls: vec![],
    };
    let conc = |key: &str, ret: &str| { let mut c = mk(key, owner, "&self", vec![opt], vec![], ret); c.owner = owner.into(); c.canonical_path = format!("{owner}::head"); c.impl_head = None; c.owner_generic = false; c.bucket = "mechanical".into(); c };
    let inv = Inventory { callables: vec![conc("ok", owner), conc("other_ret", "polars_core::datatypes::Int16Chunked")], supporting: vec![sup(owner), sup("polars_core::datatypes::Int16Chunked")], provenance: None };
    let release = Release { sized_self_methods: vec![], ..release };
    let world = World::new(&inv, &release, &["mechanical"]);
    let empty = || Emitted { from_names: BTreeMap::new(), functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    let emit = |key: &str| {
        let mut e = empty();
        *world.sized_self.borrow_mut() = Some("head".into());
        emit_method(&world, &mut e, inv.callables.iter().find(|c| c.key == key).unwrap(), owner, None, false);
        *world.sized_self.borrow_mut() = None;
        (e.entries[0].status.clone(), e.entries[0].reason.clone().unwrap_or_default(), e.functions)
    };
    let (status, reason, f) = emit("ok");
    assert_eq!(status, "generated", "{reason}");
    let guard = f.find("support::signed_len(this.0.len(), \"head\")?;").unwrap_or_else(|| panic!("guarded: {f}"));
    assert!(guard < f.find(">::head(").unwrap(), "the guard runs before Polars: {f}");
    assert!(f.contains("-> Result<W_polars_core__datatypes__Int8Chunked, Error>"), "the guard makes the binding fallible: {f}");
    let (status, reason, f) = emit("other_ret");
    assert_eq!(status, "unsupported", "a return that is not the receiver is refused: {reason}");
    assert!(f.is_empty(), "no binding text: {f}");
    assert!(world.sized_self.borrow().is_none());
    println!("sized-self self-test: ok");
}

/// Record 0096 controls: a `[[null_aware_returns]]` entry fails closed
/// (citation, numeric types, no duplicates) and resolves only listed types;
/// with the scope set, only the exact `Either<Vec<N>, Vec<Option<N>>>` for
/// the pair's native binds, bounded before the call and converting both
/// branches through the scalar rule (`u64` checked); a mismatched branch or
/// native, a non-`&self` receiver, an `Either` nested in the return, a
/// fallible return without the `Either`, and any `Either` outside the scope
/// stay unsupported with no binding text; the pair-level gate rejects every
/// return other than the exact top-level shape; the scope is clear afterwards.
fn null_aware_self_test() {
    let int = |t: &str| format!("polars_core::datatypes::{t}");
    let entry = |types: Vec<String>, cite: &str| NullAwareReturn { key: "k".into(), path: "p".into(), types, cite: cite.into() };
    let good = entry(vec![int("Int8Type"), int("UInt64Type"), int("Float32Type")], "t");
    assert!(good.check().is_ok());
    let ca = |t: &str| format!("polars_core::chunked_array::ChunkedArray<{}>", int(t));
    assert_eq!(good.native_for(&ca("Int8Type")), Some("i8"));
    assert_eq!(good.native_for(&ca("UInt64Type")), Some("u64"));
    assert_eq!(good.native_for(&ca("Float32Type")), Some("f32"));
    assert_eq!(good.native_for(&ca("Int64Type")), None, "an unlisted type is refused by name");
    assert_eq!(good.native_for(&ca("BooleanType")), None);
    for (label, bad) in [
        ("no citation", entry(vec![int("Int8Type")], " ")),
        ("no type", entry(vec![], "t")),
        ("a nonnumeric type", entry(vec![int("BooleanType")], "t")),
        ("a duplicate type", entry(vec![int("Int8Type"), int("Int8Type")], "t")),
    ] {
        assert!(bad.check().is_err(), "{label} must fail closed");
    }
    let owner = "polars_core::datatypes::Int8Chunked";
    let sup = |path: &str| Supporting {
        key: path.to_string(), kind: "struct".into(), canonical_path: path.to_string(),
        found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())], crate_paths: vec![path.to_string()],
        public_fields: 0, fields_canonical: vec![], variant_shapes: vec![], variant_payloads: vec![], generic: false, lifetime: false, hidden: false,
        derived: vec!["Clone".into(), "Debug".into()], alias_target: None, implementors: vec![], impls: vec![],
    };
    let either = |l: &str, r: &str| format!("either::Either<alloc::vec::Vec<{l}>, alloc::vec::Vec<{r}>>");
    let mk = |key: &str, receiver: &str, ret: &str| Callable {
        key: key.into(), kind: "inherent".into(), krate: "polars_core".into(), owner: owner.into(), name: "to_vec_null_aware".into(), canonical_path: format!("{owner}::to_vec_null_aware"),
        found_paths: vec![], crate_paths: vec![], receiver: receiver.into(), params: vec![],
        ret: None, ret_canonical: Some(ret.into()), generics_canonical: vec![],
        impl_for: None, impl_bounds: vec![], impl_head: None, impl_where: vec![], impl_assoc: vec![], docs_first: None, owner_generic: false, is_unsafe: false, is_async: false,
        deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "mechanical".into(), rules: vec![],
    };
    let cases = [
        ("i8", "&self", either("i8", "core::option::Option<i8>")),
        ("u64", "&self", either("u64", "core::option::Option<u64>")),
        ("f32", "&self", either("f32", "core::option::Option<f32>")),
        ("wrong_native", "&self", either("i16", "core::option::Option<i16>")),
        ("wrong_right", "&self", either("i8", "i8")),
        ("mixed", "&self", either("i8", "core::option::Option<i16>")),
        ("owned_receiver", "self", either("i8", "core::option::Option<i8>")),
        ("nested", "&self", format!("core::option::Option<{}>", either("i8", "core::option::Option<i8>"))),
        ("fallible_plain", "&self", "polars_error::PolarsResult<i64>".to_string()),
        ("unscoped", "&self", either("i8", "core::option::Option<i8>")),
    ];
    let inv = Inventory { callables: cases.iter().map(|(k, r, t)| mk(k, r, t)).collect(), supporting: vec![sup(owner)], provenance: None };
    let release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), instantiation: InstantiationScope::default(), api_crates: vec!["polars_core".into()], unordered: vec![], excluded_oracle: vec![], refused: vec![], bitmap_returns: vec![], bitmap_inputs: vec![], iterator_returns: vec![], cow_returns: vec![], free_instantiations: vec![], method_scalar_generics: vec![], bounded_readbacks: vec![], hash_tokens: vec![], null_aware_returns: vec![], sized_self_methods: vec![], external_bounds: vec![], chunk_snapshots: vec![], indexed_chunk_snapshots: vec![], array_snapshots: vec![], iter_snapshots: vec![], callback_mutable: vec![], callback_invocation: vec![], callback_sink: vec![], callback_safe: vec![], callback_recipe: vec![] };
    let world = World::new(&inv, &release, &["mechanical"]);
    let empty = || Emitted { from_names: BTreeMap::new(), functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    let emit = |key: &str, native: Option<&str>| {
        let mut e = empty();
        *world.null_aware.borrow_mut() = native.map(|n| ("to_vec_null_aware".to_string(), n.to_string()));
        emit_method(&world, &mut e, inv.callables.iter().find(|c| c.key == key).unwrap(), owner, None, false);
        *world.null_aware.borrow_mut() = None;
        (e.entries[0].status.clone(), e.entries[0].reason.clone().unwrap_or_default(), e.functions)
    };
    for (key, native, elem) in [("i8", "i8", "(__r as i64)"), ("u64", "u64", "support::widen::<u64>(__r, \"to_vec_null_aware\")?"), ("f32", "f32", "(__r as f64)")] {
        let (status, reason, f) = emit(key, Some(native));
        assert_eq!(status, "generated", "{key}: {reason}");
        assert!(f.contains("-> Result<Vec<Option<"), "{key}: one fallible vector of options: {f}");
        let bound = f.find("support::null_aware_bound(this.0.len(), \"to_vec_null_aware\")?;").unwrap_or_else(|| panic!("{key}: bounded: {f}"));
        let call = f.find(">::to_vec_null_aware(").unwrap();
        assert!(bound < call, "{key}: the bound is checked before Polars allocates: {f}");
        assert!(f.contains("__r.either(") && f.contains(&format!("Some({elem})")) && f.contains("None => None"), "{key}: both branches, each element through the scalar rule: {f}");
    }
    for (key, native) in [("wrong_native", Some("i8")), ("wrong_right", Some("i8")), ("mixed", Some("i8")), ("owned_receiver", Some("i8")), ("nested", Some("i8")), ("fallible_plain", Some("i64")), ("unscoped", None)] {
        let (status, reason, f) = emit(key, native);
        assert_eq!(status, "unsupported", "{key} must be refused, got {reason}");
        assert!(f.is_empty(), "{key}: no binding text: {f}");
    }
    // the pair-level gate compares the whole substituted return
    assert!(null_aware_return_matches(Some(&either("i8", "core::option::Option<i8>")), "i8"));
    for (label, ret) in [
        ("nested in Option", Some(format!("core::option::Option<{}>", either("i8", "core::option::Option<i8>")))),
        ("nested in Result", Some(format!("polars_error::PolarsResult<{}>", either("i8", "core::option::Option<i8>")))),
        ("fallible without Either", Some("polars_error::PolarsResult<i64>".to_string())),
        ("another native", Some(either("i16", "core::option::Option<i16>"))),
        ("unit", None),
    ] {
        assert!(!null_aware_return_matches(ret.as_deref(), "i8"), "{label} must not match");
    }
    assert!(world.null_aware.borrow().is_none());
    println!("null-aware self-test: ok");
}

/// Record 0094 controls, from a synthetic inventory: the five exact
/// hash-token shapes (a `u64` return, an `Option<u64>` return, a `u64`
/// parameter parsed before the call) emit tokens; an unlisted same-named
/// method, a listed path under another key, and every malformed entry
/// (return or parameter type altered, missing or wrong parameter, missing
/// citation, unknown direction) either stay on the checked integer rule or
/// are refused with no binding text; the scope is clear afterwards.
fn hash_token_self_test() {
    let cats = "polars_dtype::categorical::Categories";
    let map = "polars_dtype::categorical::mapping::CategoricalMapping";
    let sup = |path: &str| Supporting {
        key: path.to_string(), kind: "struct".into(), canonical_path: path.to_string(),
        found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())], crate_paths: vec![path.to_string()],
        public_fields: 0, fields_canonical: vec![], variant_shapes: vec![], variant_payloads: vec![], generic: false, lifetime: false, hidden: false,
        derived: vec!["Debug".into()], alias_target: None, implementors: vec![], impls: vec![],
    };
    let param = |name: &str, ty: &str| Param { name: name.into(), ty: ty.into(), ty_canonical: ty.into() };
    let mk = |key: &str, owner: &str, name: &str, params: Vec<Param>, ret: &str| Callable {
        key: key.into(), kind: "inherent".into(), krate: "polars_dtype".into(), owner: owner.into(), name: name.into(), canonical_path: format!("{owner}::{name}"),
        found_paths: vec![], crate_paths: vec![], receiver: "&self".into(), params,
        ret: None, ret_canonical: Some(ret.into()), generics_canonical: vec![],
        impl_for: None, impl_bounds: vec![], impl_head: None, impl_where: vec![], impl_assoc: vec![], docs_first: None, owner_generic: false, is_unsafe: false, is_async: false,
        deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "mechanical".into(), rules: vec![],
    };
    let two = || vec![param("s", "&str"), param("hash", "u64")];
    let inv = Inventory {
        callables: vec![
            mk("ret", cats, "hash", vec![], "u64"),
            mk("opt", map, "cat_to_hash", vec![param("cat", "u32")], "core::option::Option<u64>"),
            mk("get", map, "get_cat_with_hash", two(), "core::option::Option<u32>"),
            mk("ins", map, "insert_cat_with_hash", two(), "core::option::Option<u32>"),
            mk("other_key", cats, "hash", vec![], "u64"),
            mk("unlisted", map, "hash", vec![], "u64"),
            mk("ret_altered", map, "stored_hash", vec![], "u32"),
            mk("opt_altered", map, "maybe_hash", vec![], "u64"),
            mk("param_altered", map, "get_with_small_hash", vec![param("s", "&str"), param("hash", "u32")], "core::option::Option<u32>"),
            mk("param_missing", map, "get_without_hash", vec![param("s", "&str")], "core::option::Option<u32>"),
            mk("uncited", cats, "uncited_hash", vec![], "u64"),
            mk("direction", cats, "sideways_hash", vec![], "u64"),
            mk("ret_names_param", cats, "named_hash", vec![], "u64"),
        ],
        supporting: vec![sup(cats), sup(map)],
        provenance: None,
    };
    let mut release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), instantiation: InstantiationScope::default(), api_crates: vec!["polars_dtype".into()], unordered: vec![], excluded_oracle: vec![], refused: vec![], bitmap_returns: vec![], bitmap_inputs: vec![], iterator_returns: vec![], cow_returns: vec![], free_instantiations: vec![], method_scalar_generics: vec![], bounded_readbacks: vec![], hash_tokens: vec![], null_aware_returns: vec![], sized_self_methods: vec![], external_bounds: vec![], chunk_snapshots: vec![], indexed_chunk_snapshots: vec![], array_snapshots: vec![], iter_snapshots: vec![], callback_mutable: vec![], callback_invocation: vec![], callback_sink: vec![], callback_safe: vec![], callback_recipe: vec![] };
    let entry = |key: &str, owner: &str, name: &str, direction: &str, param: &str, source: &str, cite: &str| HashToken { key: key.into(), path: format!("{owner}::{name}"), direction: direction.into(), param: param.into(), source: source.into(), cite: cite.into() };
    release.hash_tokens = vec![
        entry("ret", cats, "hash", "return", "", "u64", "t"),
        entry("opt", map, "cat_to_hash", "return", "", "Option<u64>", "t"),
        entry("get", map, "get_cat_with_hash", "parameter", "hash", "u64", "t"),
        entry("ins", map, "insert_cat_with_hash", "parameter", "hash", "u64", "t"),
        entry("ret_altered", map, "stored_hash", "return", "", "u64", "t"),
        entry("opt_altered", map, "maybe_hash", "return", "", "Option<u64>", "t"),
        entry("param_altered", map, "get_with_small_hash", "parameter", "hash", "u64", "t"),
        entry("param_missing", map, "get_without_hash", "parameter", "hash", "u64", "t"),
        entry("uncited", cats, "uncited_hash", "return", "", "u64", " "),
        entry("direction", cats, "sideways_hash", "sideways", "", "u64", "t"),
        entry("ret_names_param", cats, "named_hash", "return", "hash", "u64", "t"),
    ];
    let world = World::new(&inv, &release, &["mechanical"]);
    let empty = || Emitted { from_names: BTreeMap::new(), functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    let emit = |key: &str| { let mut e = empty(); emit_callable(&world, &mut e, inv.callables.iter().find(|c| c.key == key).unwrap(), &["mechanical"]); (e.entries[0].status.clone(), e.entries[0].reason.clone().unwrap_or_default(), e.functions) };
    let (status, reason, f) = emit("ret");
    assert_eq!(status, "generated", "ret: {reason}");
    assert!(f.contains("-> String") && f.contains("support::hash_token(__r)") && !f.contains("widen") && !f.contains("Result<"), "a u64 hash return is an infallible exact token: {f}");
    let (status, reason, f) = emit("opt");
    assert_eq!(status, "generated", "opt: {reason}");
    assert!(f.contains("Option<String>") && f.contains("Some(support::hash_token(__r))") && f.contains("support::narrow::<u32>(cat"), "an optional hash is an optional token; other fallibility stays: {f}");
    for (key, op) in [("get", "get_cat_with_hash"), ("ins", "insert_cat_with_hash")] {
        let (status, reason, f) = emit(key);
        assert_eq!(status, "generated", "{key}: {reason}");
        assert!(f.contains("hash: &str") && f.contains(&format!("let __hash_hash = support::hash_from_token(hash, \"{op}\")?;")), "{key}: the hash parameter is a token parsed with the operation's name: {f}");
        let parse = f.find("support::hash_from_token(").unwrap();
        let call = f.find(&format!(">::{op}(")).unwrap();
        assert!(parse < call, "{key}: the token is parsed before the Polars call: {f}");
        assert!(f.contains(", __hash_hash)") && !f.contains("narrow::<u64>"), "{key}: the parsed bits reach Polars unchanged: {f}");
    }
    for key in ["other_key", "unlisted"] {
        let (status, reason, f) = emit(key);
        assert_eq!(status, "generated", "{key}: {reason}");
        assert!(f.contains("support::widen::<u64>(") && !f.contains("hash_token"), "{key}: an unlisted or other-key hash stays on the checked integer rule: {f}");
    }
    for (key, why) in [("ret_altered", "does not match"), ("opt_altered", "does not match"), ("param_altered", "not u64"), ("param_missing", "is not a parameter"), ("uncited", "no citation"), ("direction", "is not return or parameter"), ("ret_names_param", "names a parameter")] {
        let (status, reason, f) = emit(key);
        assert_eq!(status, "unsupported", "{key} must be refused, got {status}: {reason}");
        assert!(reason.contains(why), "{key}: {reason}");
        assert!(f.is_empty(), "{key}: a refused entry emits no binding text: {f}");
    }
    assert!(world.hash_token.borrow().is_none(), "the scope never outlives its callable");
    println!("hash-token self-test: ok");
}

/// Record 0093 controls, from a synthetic inventory: a u64/usize read-back is
/// range-checked (`support::widen`, fallible, named by its operation) on every
/// return route; only a listed key+path pair keeps the plain bounded `usize`;
/// an unlisted same-named callable, the same path under another key and an
/// uncited `height` stay checked; narrow integers keep the lossless cast; a
/// callback's `u64` input fails the callback instead of wrapping.
fn checked_readback_self_test() {
    let frame = "polars_core::frame::dataframe::DataFrame";
    let sup = |path: &str| Supporting {
        key: path.to_string(), kind: "struct".into(), canonical_path: path.to_string(),
        found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())], crate_paths: vec![path.to_string()],
        public_fields: 0, fields_canonical: vec![], variant_shapes: vec![], variant_payloads: vec![], generic: false, lifetime: false, hidden: false,
        derived: vec!["Clone".into(), "Debug".into()], alias_target: None, implementors: vec![], impls: vec![],
    };
    let mk = |key: &str, owner: &str, name: &str, ret: &str| Callable {
        key: key.into(), kind: "inherent".into(), krate: "polars_core".into(), owner: owner.into(), name: name.into(), canonical_path: format!("{owner}::{name}"),
        found_paths: vec![], crate_paths: vec![], receiver: "&self".into(), params: vec![],
        ret: None, ret_canonical: Some(ret.into()), generics_canonical: vec![],
        impl_for: None, impl_bounds: vec![], impl_head: None, impl_where: vec![], impl_assoc: vec![], docs_first: None, owner_generic: false, is_unsafe: false, is_async: false,
        deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "mechanical".into(), rules: vec![],
    };
    let other = "polars_core::series::Series";
    let inv = Inventory {
        callables: vec![
            mk("width", frame, "width", "usize"),
            mk("width_other_key", frame, "width", "usize"),
            mk("width_other_owner", other, "width", "usize"),
            mk("len", frame, "height", "usize"),
            mk("hash", frame, "hash_rows", "u64"),
            mk("opt", frame, "maybe_count", "core::option::Option<u64>"),
            mk("tuple", frame, "shape", "(usize, usize)"),
            mk("signed", frame, "offset", "isize"),
            mk("narrow", frame, "small", "u32"),
        ],
        supporting: vec![sup(frame), sup(other)],
        provenance: None,
    };
    let mut release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), instantiation: InstantiationScope::default(), api_crates: vec!["polars_core".into()], unordered: vec![], excluded_oracle: vec![], refused: vec![], bitmap_returns: vec![], bitmap_inputs: vec![], iterator_returns: vec![], cow_returns: vec![], free_instantiations: vec![], method_scalar_generics: vec![], bounded_readbacks: vec![], hash_tokens: vec![], null_aware_returns: vec![], sized_self_methods: vec![], external_bounds: vec![], chunk_snapshots: vec![], indexed_chunk_snapshots: vec![], array_snapshots: vec![], iter_snapshots: vec![], callback_mutable: vec![], callback_invocation: vec![], callback_sink: vec![], callback_safe: vec![], callback_recipe: vec![] };
    release.bounded_readbacks.push(BoundedReadback { key: "width".into(), path: format!("{frame}::width"), cite: "t".into() });
    // an entry without a citation proves nothing: `height` stays checked below
    release.bounded_readbacks.push(BoundedReadback { key: "len".into(), path: format!("{frame}::height"), cite: " ".into() });
    let world = World::new(&inv, &release, &["mechanical"]);
    let empty = || Emitted { from_names: BTreeMap::new(), functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    let emit = |key: &str| { let mut e = empty(); emit_callable(&world, &mut e, inv.callables.iter().find(|c| c.key == key).unwrap(), &["mechanical"]); (e.entries[0].status.clone(), e.entries[0].reason.clone().unwrap_or_default(), e.functions) };
    let (status, reason, f) = emit("width");
    assert_eq!(status, "generated", "width: {reason}");
    assert!(f.contains("support::bounded_usize(") && !f.contains("Result<") && !f.contains("widen"), "a listed bounded read-back keeps the plain API: {f}");
    for (key, source, op) in [("width_other_key", "usize", "width"), ("width_other_owner", "usize", "width"), ("len", "usize", "height"), ("hash", "u64", "hash_rows"), ("opt", "u64", "maybe_count"), ("tuple", "usize", "shape"), ("signed", "isize", "offset")] {
        let (status, reason, f) = emit(key);
        assert_eq!(status, "generated", "{key}: {reason}");
        assert!(f.contains("-> Result<"), "{key}: a checked read-back is fallible: {f}");
        assert!(f.contains(&format!("support::widen::<{source}>(")), "{key}: widened with a range check: {f}");
        assert!(f.contains(&format!("\"{op}\"")), "{key}: the error names the method {op}, as every return conversion has since record 0087: {f}");
        assert!(!f.contains("bounded_usize") && !f.contains(" as i64"), "{key}: no residual unchecked cast: {f}");
        assert!(!f.contains("__OP__"), "{key}: operation placeholder substituted: {f}");
    }
    let (_, _, f) = emit("tuple");
    assert_eq!(f.matches("support::widen::<usize>(").count(), 2, "both tuple fields are checked: {f}");
    let (status, reason, f) = emit("narrow");
    assert_eq!(status, "generated", "narrow: {reason}");
    assert!(f.contains(" as i64") && !f.contains("widen") && !f.contains("Result<"), "a u32 read-back stays a lossless plain cast: {f}");
    for t in ["u64", "usize", "&[u64]"] {
        let conv = callback_input(&world, &ty::parse(t), "__x", None).unwrap_or_else(|e| panic!("{t}: {e:?}"));
        assert!(conv.contains(&format!("support::widen::<{}>(", t.trim_start_matches("&[").trim_end_matches(']'))) || conv.contains("support::copy_slice("), "{t}: a callback input is checked: {conv}");
        assert!(conv.contains("support::callback::unwind(") && conv.contains("CallbackFailure"), "{t}: a failed conversion fails the callback, typed: {conv}");
        assert!(!conv.contains(" as i64"), "{t}: no residual unchecked cast: {conv}");
    }
    assert!(!world.bounded_ok.get(), "the scope never outlives its callable");
    println!("checked-readback self-test: ok");
}

/// Record 0088 gate 2 controls, from a synthetic inventory: a listed
/// callable's `Cow<Self>` or `Cow<Wrapped>` return is made owned inside the
/// call (inside the engine closure when routed); an unlisted `Cow<Wrapped>`,
/// a listed path under another key, `Cow<Field>`-style unwrapped inners and
/// a `Cow` of an Arrow array stay refused.
fn cow_return_self_test() {
    let series = "polars_core::series::Series";
    let frame = "polars_core::frame::dataframe::DataFrame";
    let sup = |path: &str| Supporting {
        key: path.to_string(), kind: "struct".into(), canonical_path: path.to_string(),
        found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())], crate_paths: vec![path.to_string()],
        public_fields: 0, fields_canonical: vec![], variant_shapes: vec![], variant_payloads: vec![], generic: false, lifetime: false, hidden: false,
        derived: vec!["Clone".into(), "Debug".into()], alias_target: None, implementors: vec![], impls: vec![],
    };
    let mk = |key: &str, name: &str, ret: &str| Callable {
        key: key.into(), kind: "inherent".into(), krate: "polars_core".into(), owner: series.into(), name: name.into(), canonical_path: format!("{series}::{name}"),
        found_paths: vec![], crate_paths: vec![], receiver: "&self".into(), params: vec![],
        ret: None, ret_canonical: Some(ret.into()), generics_canonical: vec![],
        impl_for: None, impl_bounds: vec![], impl_head: None, impl_where: vec![], impl_assoc: vec![], docs_first: None, owner_generic: false, is_unsafe: false, is_async: false,
        deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "mechanical".into(), rules: vec![],
    };
    let inv = Inventory {
        callables: vec![
            mk("self", "rechunk_cow", "alloc::borrow::Cow<Self>"),
            mk("other", "to_frame_cow", &format!("alloc::borrow::Cow<{frame}>")),
            mk("same_path_other_key", "rechunk_cow", "alloc::borrow::Cow<Self>"),
            mk("unlisted", "maybe_owned", "alloc::borrow::Cow<Self>"),
            mk("unwrapped", "name_cow", "alloc::borrow::Cow<str>"),
            mk("arrow", "chunk_cow", "alloc::borrow::Cow<polars_arrow::array::PrimitiveArray<i64>>"),
        ],
        supporting: vec![sup(series), sup(frame)],
        provenance: None,
    };
    let mut release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), instantiation: InstantiationScope::default(), api_crates: vec!["polars_core".into()], unordered: vec![], excluded_oracle: vec![], refused: vec![], bitmap_returns: vec![], bitmap_inputs: vec![], iterator_returns: vec![], cow_returns: vec![], free_instantiations: vec![], method_scalar_generics: vec![], bounded_readbacks: vec![], hash_tokens: vec![], null_aware_returns: vec![], sized_self_methods: vec![], external_bounds: vec![], chunk_snapshots: vec![], indexed_chunk_snapshots: vec![], array_snapshots: vec![], iter_snapshots: vec![], callback_mutable: vec![], callback_invocation: vec![], callback_sink: vec![], callback_safe: vec![], callback_recipe: vec![] };
    for (k, n) in [("self", "rechunk_cow"), ("other", "to_frame_cow"), ("unwrapped", "name_cow"), ("arrow", "chunk_cow")] { release.cow_returns.push(CowReturn { key: k.into(), path: format!("{series}::{n}"), cite: "t".into() }); }
    let world = World::new(&inv, &release, &["mechanical"]);
    let empty = || Emitted { from_names: BTreeMap::new(), functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    let emit = |key: &str| { let mut e = empty(); emit_callable(&world, &mut e, inv.callables.iter().find(|c| c.key == key).unwrap(), &["mechanical"]); (e.entries[0].status.clone(), e.entries[0].reason.clone().unwrap_or_default(), e.functions) };
    for key in ["self", "other"] {
        let (status, reason, f) = emit(key);
        assert_eq!(status, "generated", "{key}: {reason}");
        assert!(f.contains(".into_owned()"), "{key}: the Cow is made owned inside the call: {f}");
        assert!(!f.contains("Cow<"), "{key}: no Cow in the binding's signature: {f}");
    }
    for key in ["same_path_other_key", "unlisted", "unwrapped", "arrow"] {
        let (status, reason, _) = emit(key);
        assert_eq!(status, "unsupported", "{key} must stay refused, got {reason}");
    }
    assert!(!world.cow_ok.get(), "the scope never outlives its callable");
    println!("cow-return self-test: ok");
}

/// Record 0087 gate 2 controls, from a synthetic inventory: a listed
/// callable's concrete `Map` iterator (through its alias) materializes as
/// exact-size, range-checked integers inside the call; an unlisted method
/// returning the same alias, a different `Map` and an Arrow array return
/// stay refused, and the scope is clear after emission.
fn iterator_return_self_test() {
    let series = "polars_core::series::Series";
    let alias = "polars_core::chunked_array::ChunkLenIter";
    let map = "core::iter::adapters::map::Map<core::slice::iter::Iter<'a, alloc::boxed::Box<dyn polars_arrow::array::Array>>, fn(&alloc::boxed::Box<dyn polars_arrow::array::Array>) -> usize>";
    let sup = |path: &str| Supporting {
        key: path.to_string(), kind: "struct".into(), canonical_path: path.to_string(),
        found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())], crate_paths: vec![path.to_string()],
        public_fields: 0, fields_canonical: vec![], variant_shapes: vec![], variant_payloads: vec![], generic: false, lifetime: false, hidden: false,
        derived: vec!["Clone".into(), "Debug".into()], alias_target: None, implementors: vec![], impls: vec![],
    };
    let mut alias_sup = sup(alias);
    alias_sup.kind = "type_alias".into();
    alias_sup.alias_target = Some(map.into());
    let mk = |key: &str, name: &str, ret: &str| Callable {
        key: key.into(), kind: "inherent".into(), krate: "polars_core".into(), owner: series.into(), name: name.into(), canonical_path: format!("{series}::{name}"),
        found_paths: vec![], crate_paths: vec![], receiver: "&self".into(), params: vec![],
        ret: None, ret_canonical: Some(ret.into()), generics_canonical: vec![],
        impl_for: None, impl_bounds: vec![], impl_head: None, impl_where: vec![], impl_assoc: vec![], docs_first: None, owner_generic: false, is_unsafe: false, is_async: false,
        deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "mechanical".into(), rules: vec![],
    };
    let inv = Inventory {
        callables: vec![
            mk("listed", "chunk_lengths", alias),
            mk("unlisted", "other_lengths", alias),
            mk("other_map", "names", "core::iter::adapters::map::Map<core::slice::iter::Iter<'a, alloc::string::String>, fn(&alloc::string::String) -> usize>"),
            mk("arrays", "chunks", "&alloc::vec::Vec<alloc::boxed::Box<dyn polars_arrow::array::Array>>"),
        ],
        supporting: vec![sup(series), alias_sup],
        provenance: None,
    };
    let mut release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), instantiation: InstantiationScope::default(), api_crates: vec!["polars_core".into()], unordered: vec![], excluded_oracle: vec![], refused: vec![], bitmap_returns: vec![], bitmap_inputs: vec![], iterator_returns: vec![], cow_returns: vec![], free_instantiations: vec![], method_scalar_generics: vec![], bounded_readbacks: vec![], hash_tokens: vec![], null_aware_returns: vec![], sized_self_methods: vec![], external_bounds: vec![], chunk_snapshots: vec![], indexed_chunk_snapshots: vec![], array_snapshots: vec![], iter_snapshots: vec![], callback_mutable: vec![], callback_invocation: vec![], callback_sink: vec![], callback_safe: vec![], callback_recipe: vec![] };
    release.iterator_returns.push(IteratorReturn { path: format!("{series}::chunk_lengths"), item: "usize".into(), cite: "t".into() });
    let world = World::new(&inv, &release, &["mechanical"]);
    let empty = || Emitted { from_names: BTreeMap::new(), functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    let emit = |key: &str| { let mut e = empty(); emit_callable(&world, &mut e, inv.callables.iter().find(|c| c.key == key).unwrap(), &["mechanical"]); (e.entries[0].status.clone(), e.entries[0].reason.clone().unwrap_or_default(), e.functions) };
    let (status, reason, f) = emit("listed");
    assert_eq!(status, "generated", "{reason}");
    assert!(f.contains("-> Result<Vec<i64>, Error>") && f.contains("support::materialize_exact(__it, \"chunk_lengths\"") && f.contains("support::widen::<usize>(__r, \"chunk_lengths\")?"), "exact-size, range-checked, materialized inside the call: {f}");
    assert!(f.find("chunk_lengths(__arg0)").unwrap() < f.find("materialize_exact").unwrap() && !f.contains("__it }"), "the iterator is consumed before the receiver borrow ends: {f}");
    for key in ["unlisted", "other_map", "arrays"] {
        let (status, reason, _) = emit(key);
        assert_eq!(status, "unsupported", "{key} must stay refused, got {reason}");
    }
    assert!(world.iter_return.borrow().is_none(), "the scope never outlives its callable");
    println!("iterator-return self-test: ok");
}

/// Record 0086 gate 2 controls, from a synthetic inventory: a listed
/// callable's `Bitmap` parameter is built from a script `Vec<bool>` with
/// its length rule (receiver, values, none), directly or optionally; an
/// unlisted bitmap input, an unlisted bitmap output and an Arrow array stay
/// refused, and the scope is clear after emission.
fn bitmap_input_self_test() {
    let series = "polars_core::series::Series";
    let bitmap = "polars_arrow::bitmap::immutable::Bitmap";
    let sup = |path: &str| Supporting {
        key: path.to_string(), kind: "struct".into(), canonical_path: path.to_string(),
        found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())], crate_paths: vec![path.to_string()],
        public_fields: 0, fields_canonical: vec![], variant_shapes: vec![], variant_payloads: vec![], generic: false, lifetime: false, hidden: false,
        derived: vec!["Clone".into(), "Debug".into()], alias_target: None, implementors: vec![], impls: vec![],
    };
    let mk = |key: &str, name: &str, receiver: &str, params: Vec<(&str, &str)>, ret: Option<&str>| Callable {
        key: key.into(), kind: "inherent".into(), krate: "polars_core".into(), owner: series.into(), name: name.into(), canonical_path: format!("{series}::{name}"),
        found_paths: vec![], crate_paths: vec![], receiver: receiver.into(), params: params.iter().map(|(n, t)| Param { name: n.to_string(), ty: t.to_string(), ty_canonical: t.to_string() }).collect(),
        ret: None, ret_canonical: ret.map(String::from), generics_canonical: vec![],
        impl_for: None, impl_bounds: vec![], impl_head: None, impl_where: vec![], impl_assoc: vec![], docs_first: None, owner_generic: false, is_unsafe: false, is_async: false,
        deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "mechanical".into(), rules: vec![],
    };
    let opt = format!("core::option::Option<{bitmap}>");
    let inv = Inventory {
        callables: vec![
            mk("set", "set_mask", "&mut self", vec![("validity", &opt)], None),
            mk("values", "from_values_mask", "none", vec![("name", "polars_utils::pl_str::PlSmallStr"), ("values", "alloc::vec::Vec<i64>"), ("buffer", &opt)], Some(series)),
            mk("bits", "from_bits", "none", vec![("name", "polars_utils::pl_str::PlSmallStr"), ("bitmap", bitmap)], Some(series)),
            mk("unlisted_in", "other_mask", "&mut self", vec![("validity", &opt)], None),
            mk("unlisted_out", "mask_out", "&self", vec![], Some(&opt)),
            mk("array", "with_chunk", "&self", vec![("arr", "polars_arrow::array::PrimitiveArray<i64>")], Some(series)),
        ],
        supporting: vec![sup(series)],
        provenance: None,
    };
    let mut release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), instantiation: InstantiationScope::default(), api_crates: vec!["polars_core".into()], unordered: vec![], excluded_oracle: vec![], refused: vec![], bitmap_returns: vec![], bitmap_inputs: vec![], iterator_returns: vec![], cow_returns: vec![], free_instantiations: vec![], method_scalar_generics: vec![], bounded_readbacks: vec![], hash_tokens: vec![], null_aware_returns: vec![], sized_self_methods: vec![], external_bounds: vec![], chunk_snapshots: vec![], indexed_chunk_snapshots: vec![], array_snapshots: vec![], iter_snapshots: vec![], callback_mutable: vec![], callback_invocation: vec![], callback_sink: vec![], callback_safe: vec![], callback_recipe: vec![] };
    for (n, l) in [("set_mask", "receiver"), ("from_values_mask", "values"), ("from_bits", "none")] { release.bitmap_inputs.push(BitmapInput { path: format!("{series}::{n}"), length: l.into(), cite: "t".into() }); }
    let world = World::new(&inv, &release, &["mechanical"]);
    let empty = || Emitted { from_names: BTreeMap::new(), functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    let emit = |key: &str| { let mut e = empty(); emit_callable(&world, &mut e, inv.callables.iter().find(|c| c.key == key).unwrap(), &["mechanical"]); (e.entries[0].status.clone(), e.entries[0].reason.clone().unwrap_or_default(), e.functions) };
    let (status, reason, f) = emit("set");
    assert_eq!(status, "generated", "{reason}");
    assert!(f.contains("let __mask_len = this.0.len();") && f.contains("None => None") && f.contains("support::bitmap_from_bools(&v, \"Series::set_mask\", Some(__mask_len))?"), "an optional receiver-length mask; None stays None: {f}");
    let (status, reason, f) = emit("values");
    assert_eq!(status, "generated", "{reason}");
    assert!(f.contains("let __mask_len = support::vec_len(&values, \"values\")?;"), "a values-length mask: {f}");
    let (status, reason, f) = emit("bits");
    assert_eq!(status, "generated", "{reason}");
    assert!(f.contains("support::bitmap_from_bools(&bitmap, \"Series::from_bits\", None)?"), "no length rule: {f}");
    for key in ["unlisted_in", "unlisted_out", "array"] {
        let (status, reason, _) = emit(key);
        assert_eq!(status, "unsupported", "{key} must stay refused, got {reason}");
    }
    assert!(world.bitmap_input.borrow().is_none() && !world.bitmap_ok.get(), "the bitmap scope never outlives its callable");
    println!("bitmap-input self-test: ok");
}

/// Record 0085 gate 2 controls, from a synthetic inventory: a validity
/// `Bitmap` return maps to an owned `Vec<bool>` only for the paths the
/// release lists, directly, optionally and as iterator items (one bound
/// for the whole call); an unlisted path, a bitmap input and an Arrow
/// array stay refused.
fn bitmap_self_test() {
    let series = "polars_core::series::Series";
    let bitmap = "polars_arrow::bitmap::immutable::Bitmap";
    let sup = |path: &str| Supporting {
        key: path.to_string(), kind: "struct".into(), canonical_path: path.to_string(),
        found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())], crate_paths: vec![path.to_string()],
        public_fields: 0, fields_canonical: vec![], variant_shapes: vec![], variant_payloads: vec![], generic: false, lifetime: false, hidden: false,
        derived: vec!["Clone".into(), "Debug".into()], alias_target: None, implementors: vec![], impls: vec![],
    };
    let mk = |key: &str, name: &str, params: Vec<(&str, &str)>, ret: Option<&str>| Callable {
        key: key.into(), kind: "inherent".into(), krate: "polars_core".into(), owner: series.into(), name: name.into(), canonical_path: format!("{series}::{name}"),
        found_paths: vec![], crate_paths: vec![], receiver: "&self".into(), params: params.iter().map(|(n, t)| Param { name: n.to_string(), ty: t.to_string(), ty_canonical: t.to_string() }).collect(),
        ret: None, ret_canonical: ret.map(String::from), generics_canonical: vec![],
        impl_for: None, impl_bounds: vec![], impl_head: None, impl_where: vec![], impl_assoc: vec![], docs_first: None, owner_generic: false, is_unsafe: false, is_async: false,
        deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "mechanical".into(), rules: vec![],
    };
    let opt = format!("core::option::Option<{bitmap}>");
    let items = format!("impl core::iter::traits::exact_size::ExactSizeIterator<Item = core::option::Option<&{bitmap}>>");
    let inv = Inventory {
        callables: vec![
            mk("direct", "bits", vec![], Some(bitmap)),
            mk("optional", "maybe_bits", vec![], Some(&opt)),
            mk("items", "bits_per_chunk", vec![], Some(&items)),
            mk("unlisted", "other_bits", vec![], Some(&opt)),
            mk("input", "with_bits", vec![("validity", bitmap)], Some(series)),
            mk("array", "chunk", vec![], Some("polars_arrow::array::PrimitiveArray<i64>")),
        ],
        supporting: vec![sup(series)],
        provenance: None,
    };
    let mut release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), instantiation: InstantiationScope::default(), api_crates: vec!["polars_core".into()], unordered: vec![], excluded_oracle: vec![], refused: vec![], bitmap_returns: vec![], bitmap_inputs: vec![], iterator_returns: vec![], cow_returns: vec![], free_instantiations: vec![], method_scalar_generics: vec![], bounded_readbacks: vec![], hash_tokens: vec![], null_aware_returns: vec![], sized_self_methods: vec![], external_bounds: vec![], chunk_snapshots: vec![], indexed_chunk_snapshots: vec![], array_snapshots: vec![], iter_snapshots: vec![], callback_mutable: vec![], callback_invocation: vec![], callback_sink: vec![], callback_safe: vec![], callback_recipe: vec![] };
    for n in ["bits", "maybe_bits", "bits_per_chunk", "with_bits"] { release.bitmap_returns.push(format!("{series}::{n}")); }
    let world = World::new(&inv, &release, &["mechanical"]);
    let empty = || Emitted { from_names: BTreeMap::new(), functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    let emit = |key: &str| { let mut e = empty(); emit_callable(&world, &mut e, inv.callables.iter().find(|c| c.key == key).unwrap(), &["mechanical"]); (e.entries[0].status.clone(), e.entries[0].reason.clone().unwrap_or_default(), e.functions) };
    let (status, reason, f) = emit("direct");
    assert_eq!(status, "generated", "{reason}");
    assert!(f.contains("-> Result<Vec<bool>, Error>") && f.contains("support::copy_bits(&__r, \"bits\")?"), "{f}");
    let (status, reason, f) = emit("optional");
    assert_eq!(status, "generated", "{reason}");
    assert!(f.contains("Result<Option<Vec<bool>>, Error>") && f.contains("None => None"), "None stays None, never an empty vector: {f}");
    let (status, reason, f) = emit("items");
    assert_eq!(status, "generated", "{reason}");
    assert!(f.contains("support::SliceBudget::enter()") && f.contains("support::materialize_exact") && f.contains("support::copy_bits("), "one cumulative bound over every chunk's bitmap: {f}");
    for (key, why) in [("unlisted", "unreachable polars type"), ("input", "unreachable polars type"), ("array", "")] {
        let (status, reason, _) = emit(key);
        assert_eq!(status, "unsupported", "{key} must stay refused, got {reason}");
        assert!(reason.contains(why), "{key}: `{why}` expected, got {reason}");
    }
    assert!(!world.bitmap_ok.get(), "the listed-path flag never outlives its callable");
    println!("bitmap self-test: ok");
}

/// Record 0084 gate 2 controls, from a synthetic inventory: chained generic
/// inference (`I: IntoIterator<Item = S>, S: AsRef<str>`; `E: AsRef<[IE]>,
/// IE: Into<Expr>`), iterator inputs lowered from a script vector (owned
/// items through `into_iter`, borrowed `&str`/`&[u8]`/`Option` items through
/// a held temporary), and the refusals that stay: a tuple-with-`Field`
/// item, a return-only generic, a closure bound, and a release-refused path.
fn generic_input_self_test() {
    fn sup(path: &str) -> Supporting {
        Supporting {
            key: path.to_string(), kind: "struct".into(), canonical_path: path.to_string(),
            found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())], crate_paths: vec![path.to_string()],
            public_fields: 0, fields_canonical: vec![], variant_shapes: vec![], variant_payloads: vec![], generic: false, lifetime: false, hidden: false,
            derived: vec!["Clone".into(), "Debug".into()], alias_target: None, implementors: vec![], impls: vec![],
        }
    }
    let frame = "polars_core::frame::dataframe::DataFrame";
    let expr = "polars_plan::dsl::expr::Expr";
    let field = "polars_core::datatypes::field::Field";
    let mk = |key: &str, name: &str, params: Vec<(&str, &str)>, generics: Vec<(&str, &str)>, ret: Option<&str>| Callable {
        key: key.into(), kind: "inherent".into(), krate: "polars_core".into(), owner: frame.into(), name: name.into(), canonical_path: format!("{frame}::{name}"),
        found_paths: vec![], crate_paths: vec![], receiver: "&self".into(), params: params.iter().map(|(n, t)| Param { name: n.to_string(), ty: t.to_string(), ty_canonical: t.to_string() }).collect(),
        ret: None, ret_canonical: ret.map(String::from), generics_canonical: generics.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
        impl_for: None, impl_bounds: vec![], impl_head: None, impl_where: vec![], impl_assoc: vec![], docs_first: None, owner_generic: false, is_unsafe: false, is_async: false,
        deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "generic".into(), rules: vec![],
    };
    let into_iter = |item: &str| format!("core::iter::traits::collect::IntoIterator<Item = {item}>");
    let iter = |item: &str| format!("core::iter::traits::iterator::Iterator<Item = {item}>");
    let inv = Inventory {
        callables: vec![
            mk("chain", "pick", vec![("names", "I")], vec![("I", &into_iter("S")), ("S", "core::convert::AsRef<str>")], Some(frame)),
            mk("chain_small", "drop_some", vec![("names", "I")], vec![("I", &into_iter("S")), ("S", "core::convert::Into<polars_utils::pl_str::PlSmallStr>")], Some(frame)),
            mk("two_chains", "rename_some", vec![("existing", "I"), ("new", "J")], vec![("I", &into_iter("T")), ("J", &into_iter("S")), ("T", "core::convert::AsRef<str>"), ("S", "core::convert::AsRef<str>")], Some(frame)),
            mk("exprs", "over_some", vec![("partition_by", "E")], vec![("E", "core::convert::AsRef<[IE]>"), ("IE", &format!("core::convert::Into<{expr}> + core::clone::Clone"))], Some(frame)),
            mk("opt_exprs", "over_opt", vec![("partition_by", "core::option::Option<E>")], vec![("E", "core::convert::AsRef<[IE]>"), ("IE", &format!("core::convert::Into<{expr}> + core::clone::Clone"))], Some(frame)),
            mk("owned_iter", "take_ids", vec![("ids", "I")], vec![("I", &iter("usize"))], Some(frame)),
            mk("trusted", "take_flags", vec![("flags", "I")], vec![("I", &format!("{} + polars_arrow::trusted_len::TrustedLen", iter("core::option::Option<bool>")))], Some(frame)),
            mk("strs", "take_strs", vec![("iter", "I")], vec![("I", &iter("&str"))], Some(frame)),
            mk("bytes", "take_bytes", vec![("iter", "I")], vec![("I", &iter("&[u8]"))], Some(frame)),
            mk("opt_strs", "take_opt_strs", vec![("iter", "I")], vec![("I", &format!("{} + polars_arrow::trusted_len::TrustedLen", iter("core::option::Option<&str>")))], Some(frame)),
            mk("tuple_field", "with_fields", vec![("iter", "I")], vec![("I", &into_iter("F")), ("F", &format!("core::convert::Into<(polars_utils::pl_str::PlSmallStr, {field})>"))], Some(frame)),
            mk("ret_only", "total", vec![], vec![("T", "num_traits::cast::NumCast")], Some("core::option::Option<T>")),
            mk("closure", "each", vec![("f", "F")], vec![("F", "core::ops::function::FnMut(i64) -> i64")], None),
            mk("policy", "refused_by_release", vec![("ids", "I")], vec![("I", &iter("usize"))], Some(frame)),
        ],
        supporting: vec![sup(frame), sup(expr), sup(field)],
        provenance: None,
    };
    let mut release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), instantiation: InstantiationScope::default(), api_crates: vec!["polars_core".into(), "polars_plan".into()], unordered: vec![], excluded_oracle: vec![], refused: vec![], bitmap_returns: vec![], bitmap_inputs: vec![], iterator_returns: vec![], cow_returns: vec![], free_instantiations: vec![], method_scalar_generics: vec![], bounded_readbacks: vec![], hash_tokens: vec![], null_aware_returns: vec![], sized_self_methods: vec![], external_bounds: vec![], chunk_snapshots: vec![], indexed_chunk_snapshots: vec![], array_snapshots: vec![], iter_snapshots: vec![], callback_mutable: vec![], callback_invocation: vec![], callback_sink: vec![], callback_safe: vec![], callback_recipe: vec![] };
    release.refused.push(RefusedOperation { path: format!("{frame}::refused_by_release"), reason: "validated first".into(), cite: "t".into() });
    let world = World::new(&inv, &release, &["mechanical", "generic_fn"]);
    let empty = || Emitted { from_names: BTreeMap::new(), functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    let emit = |key: &str| { let mut e = empty(); emit_callable(&world, &mut e, inv.callables.iter().find(|c| c.key == key).unwrap(), &["mechanical", "generic_fn"]); (e.entries[0].status.clone(), e.entries[0].reason.clone().unwrap_or_default(), e.functions) };
    for (key, must) in [
        ("chain", vec!["let v: String = support::borrow_element", "collect::<Result<Vec<_>, Error>>()?"]),
        ("chain_small", vec!["let v: String = support::borrow_element"]),
        ("two_chains", vec!["borrow_vec(&existing, \"existing\")", "borrow_vec(&new, \"new\")"]),
        ("exprs", vec!["support::take::<Expr>(&v, \"v\")?.0"]),
        ("opt_exprs", vec!["Some(v) => Some(", "support::take::<Expr>(&v, \"v\")?.0"]),
        ("owned_iter", vec![").into_iter()", "let v: i64 = support::borrow_element"]),
        ("trusted", vec![").into_iter()", "let v: Option<bool> = support::borrow_element"]),
        ("strs", vec!["let __hold_iter = ", "__hold_iter.iter().map(String::as_str)"]),
        ("bytes", vec!["let __hold_iter = ", "__hold_iter.iter().map(Vec::as_slice)", "support::narrow::<u8>"]),
        ("opt_strs", vec!["let __hold_iter = ", "__hold_iter.iter().map(Option::as_deref)"]),
    ] {
        let (status, reason, functions) = emit(key);
        assert_eq!(status, "generated", "{key}: {reason}");
        for m in must { assert!(functions.contains(m), "{key}: expected `{m}` in:\n{functions}"); }
        assert!(!functions.contains("as_str()?"), "{key}: no vector of borrowed strings");
    }
    let (_, _, functions) = emit("chain");
    assert!(!functions.contains("Vec<&str>") && !functions.contains("v.as_str()"), "the chained item is an owned String, never a borrowed vector element:\n{functions}");
    for (key, why) in [("tuple_field", "tuple conversion"), ("ret_only", "generic parameter not inferable"), ("closure", "callback"), ("policy", "release policy")] {
        let (status, reason, _) = emit(key);
        assert_eq!(status, "unsupported", "{key} must stay refused, got {reason}");
        assert!(reason.contains(why), "{key}: refusal must say `{why}`, got {reason}");
    }
    let plain = World::new(&inv, &release, &["mechanical"]);
    let mut e = empty();
    emit_callable(&plain, &mut e, inv.callables.iter().find(|c| c.key == "chain").unwrap(), &["mechanical"]);
    assert_eq!(e.entries[0].status, "unsupported", "without the generic_fn token the generic bucket stays closed");
    println!("generic-input self-test: ok");
}

/// Record 0082 gate 2 controls, from a synthetic inventory: immutable
/// borrowed slices are copied into bounded owned vectors as returns,
/// as iterator items and as callback inputs; a mutable slice, an Arrow
/// element and a slice of iterators stay refused.
fn slice_self_test() {
    fn sup(path: &str) -> Supporting {
        Supporting {
            key: path.to_string(), kind: "struct".into(), canonical_path: path.to_string(),
            found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())], crate_paths: vec![path.to_string()],
            public_fields: 0, fields_canonical: vec![], variant_shapes: vec![], variant_payloads: vec![], generic: false, lifetime: false, hidden: false,
            derived: vec!["Clone".into(), "Debug".into()], alias_target: None, implementors: vec![], impls: vec![],
        }
    }
    let series = "polars_core::series::Series";
    let field = "polars_core::datatypes::field::Field";
    let mk = |key: &str, name: &str, params: Vec<(&str, &str)>, generics: Vec<(&str, &str)>, ret: Option<&str>, bucket: &str| Callable {
        key: key.into(), kind: "inherent".into(), krate: "polars_core".into(), owner: series.into(), name: name.into(), canonical_path: format!("{series}::{name}"),
        found_paths: vec![], crate_paths: vec![], receiver: "&self".into(), params: params.iter().map(|(n, t)| Param { name: n.to_string(), ty: t.to_string(), ty_canonical: t.to_string() }).collect(),
        ret: None, ret_canonical: ret.map(String::from), generics_canonical: generics.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
        impl_for: None, impl_bounds: vec![], impl_head: None, impl_where: vec![], impl_assoc: vec![], docs_first: None, owner_generic: false, is_unsafe: false, is_async: false,
        deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: bucket.into(), rules: vec![],
    };
    let inv = Inventory {
        callables: vec![
            mk("ints", "ints", vec![], vec![], Some("&[i64]"), "mechanical"),
            mk("bytes", "bytes", vec![], vec![], Some("&[u8]"), "mechanical"),
            mk("opt", "opt", vec![], vec![], Some("core::option::Option<&[u8]>"), "mechanical"),
            mk("fields", "fields", vec![], vec![], Some(&format!("&[{field}]")), "mechanical"),
            mk("nested", "nested", vec![], vec![], Some("&[&[u8]]"), "mechanical"),
            mk("items", "items", vec![], vec![], Some("impl core::iter::traits::iterator::Iterator<Item = &[i64]>"), "mechanical"),
            mk("cb", "each_bytes", vec![("f", "F")], vec![("F", "core::ops::function::FnMut(core::option::Option<&[u8]>)")], None, "callback"),
            mk("mutable", "mutable", vec![], vec![], Some("&mut [u8]"), "mechanical"),
            mk("arrow", "arrow", vec![], vec![], Some("&[alloc::boxed::Box<dyn polars_arrow::array::Array>]"), "mechanical"),
            mk("iters", "iters", vec![], vec![], Some("&[impl core::iter::traits::iterator::Iterator<Item = i64>]"), "mechanical"),
        ],
        supporting: vec![sup(series), sup(field)],
        provenance: None,
    };
    let mut release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), instantiation: InstantiationScope::default(), api_crates: vec!["polars_core".into()], unordered: vec![], excluded_oracle: vec![], refused: vec![], bitmap_returns: vec![], bitmap_inputs: vec![], iterator_returns: vec![], cow_returns: vec![], free_instantiations: vec![], method_scalar_generics: vec![], bounded_readbacks: vec![], hash_tokens: vec![], null_aware_returns: vec![], sized_self_methods: vec![], external_bounds: vec![], chunk_snapshots: vec![], indexed_chunk_snapshots: vec![], array_snapshots: vec![], iter_snapshots: vec![], callback_mutable: vec![], callback_invocation: vec![], callback_sink: vec![], callback_safe: vec![], callback_recipe: vec![] };
    release.callback_invocation.push(CallbackInvocation { path: format!("{series}::each_bytes"), param: "f".into(), invocation: "immediate".into(), sinks: vec![], cite: "t".into() });
    let world = World::new(&inv, &release, &["mechanical", "callback"]);
    let empty = || Emitted { from_names: BTreeMap::new(), functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    let emit = |key: &str| { let mut e = empty(); emit_callable(&world, &mut e, inv.callables.iter().find(|c| c.key == key).unwrap(), &["mechanical", "callback"]); (e.entries[0].status.clone(), e.entries[0].reason.clone().unwrap_or_default(), e.functions) };
    for key in ["ints", "bytes", "opt", "fields", "nested", "items"] {
        let (status, reason, functions) = emit(key);
        assert_eq!(status, "generated", "{key}: {reason}");
        assert!(functions.contains("support::copy_slice(__r, \"") && functions.contains(&format!("\"{}\"", key.replace("ints", "ints"))), "{key}: the slice is copied under the bound, naming the operation:\n{functions}");
        assert!(functions.contains("-> Result<"), "{key}: a bounded copy is fallible");
    }
    let (_, _, functions) = emit("nested");
    assert_eq!(functions.matches("support::copy_slice(").count(), 2, "a slice of slices copies at both levels");
    let (_, _, functions) = emit("items");
    assert!(functions.contains("support::SliceBudget::enter()") && functions.contains("support::materialize_"), "iterator items share one cumulative bound over the materialized run");
    let (status, reason, functions) = emit("cb");
    assert_eq!(status, "generated", "cb: {reason}");
    assert!(functions.contains("support::copy_slice(") && functions.contains("CallbackFailure { op: \"Series::each_bytes\""), "a callback's slice is a bounded snapshot whose refusal is the typed failure:\n{functions}");
    // a mutable slice return falls under the pre-existing rule for `&mut`
    // returns (the receiver is mutated in place, the binding returns unit):
    // no slice is exposed and nothing is copied
    let (status, _, functions) = emit("mutable");
    assert_eq!(status, "generated");
    assert!(!functions.contains("support::copy_slice(") && functions.contains("mutated in place"), "a mutable slice is never copied or exposed:\n{functions}");
    for (key, why) in [("arrow", "foreign type"), ("iters", "iterator")] {
        let (status, reason, _) = emit(key);
        assert_eq!(status, "unsupported", "{key} must stay refused");
        assert!(reason.contains(why), "{key}: refusal must say `{why}`, got {reason}");
    }
    println!("slice self-test: ok");
}

/// Record 0079 gate 1 controls, from a synthetic inventory: the closure
/// classifier's dispositions and the sink rule.
fn callback_self_test() {
    fn sup(path: &str, derived: &[&str]) -> Supporting {
        Supporting {
            key: path.to_string(), kind: "struct".into(), canonical_path: path.to_string(),
            found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())], crate_paths: vec![path.to_string()],
            public_fields: 0, fields_canonical: vec![], variant_shapes: vec![], variant_payloads: vec![], generic: false, lifetime: false, hidden: false,
            derived: derived.iter().map(|d| d.to_string()).collect(), alias_target: None, implementors: vec![], impls: vec![],
        }
    }
    let series = "polars_core::series::Series";
    let column = "polars_core::frame::column::Column";
    let field = "polars_core::datatypes::field::Field";
    let expr = "polars_plan::dsl::expr::Expr";
    let mk = |key: &str, owner: &str, name: &str, receiver: &str, params: Vec<(&str, &str)>, generics: Vec<(&str, &str)>, ret: Option<&str>, owner_generic: bool| Callable {
        key: key.into(), kind: "inherent".into(), krate: "polars_core".into(), owner: owner.into(), name: name.into(), canonical_path: format!("{owner}::{name}"),
        found_paths: vec![], crate_paths: vec![], receiver: receiver.into(), params: params.iter().map(|(n, t)| Param { name: n.to_string(), ty: t.to_string(), ty_canonical: t.to_string() }).collect(),
        ret: None, ret_canonical: ret.map(String::from), generics_canonical: generics.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
        impl_for: None, impl_bounds: vec![], impl_head: None, impl_where: vec![], impl_assoc: vec![], docs_first: None, owner_generic, is_unsafe: false, is_async: false,
        deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "callback".into(), rules: vec![],
    };
    let ca = "polars_core::chunked_array::ChunkedArray";
    let inv = Inventory {
        callables: vec![
            mk("ok", column, "apply_unary_elementwise", "&self", vec![("f", "F")], vec![("F", "impl core::ops::function::Fn(&polars_core::series::Series) -> polars_core::series::Series")], Some(column), false),
            mk("stored", expr, "map", "self", vec![("function", "F"), ("output_type", "DT")], vec![("F", "core::ops::function::Fn(polars_core::frame::column::Column) -> polars_error::PolarsResult<polars_core::frame::column::Column> + 'static + core::marker::Send + core::marker::Sync"), ("DT", "core::ops::function::Fn(&polars_core::schema::Schema, &polars_core::datatypes::field::Field) -> polars_error::PolarsResult<polars_core::datatypes::field::Field> + 'static + core::marker::Send + core::marker::Sync")], Some("Self"), false),
            mk("free", column, "try_apply_with", "&self", vec![("f", "F")], vec![("F", "core::ops::function::FnMut(polars_core::series::Series) -> core::result::Result<K, E>"), ("K", ""), ("E", "")], Some("core::result::Result<K, E>"), false),
            mk("arrow", column, "apply_kernel", "&self", vec![("f", "F")], vec![("F", "core::ops::function::Fn(&polars_arrow::array::Array) -> polars_arrow::array::ArrayRef")], Some(column), false),
            mk("amort", column, "amortized", "&self", vec![("f", "F")], vec![("F", "core::ops::function::FnMut(core::option::Option<polars_core::series::amortized_iter::AmortSeries>) -> polars_core::series::Series")], Some(column), false),
            mk("udf", expr, "with_udf", "self", vec![("schema", "core::option::Option<alloc::sync::Arc<dyn polars_plan::dsl::UdfSchema>>")], vec![], Some("Self"), false),
            mk("borrowed", ca, "apply_mut", "&self", vec![("f", "F")], vec![("F", "core::ops::function::FnMut(&str) -> &str")], Some("Self"), true),
            mk("slice", expr, "map_many", "self", vec![("function", "F"), ("arguments", "&[polars_plan::dsl::expr::Expr]")], vec![("F", "core::ops::function::Fn(&mut [polars_core::frame::column::Column]) -> polars_error::PolarsResult<polars_core::frame::column::Column> + 'static + core::marker::Send + core::marker::Sync")], Some("Self"), false),
            mk("buffer", ca, "apply_into_string_amortized", "&self", vec![("f", "F")], vec![("F", "core::ops::function::FnMut(T::Physical, &mut alloc::string::String)")], Some("polars_core::datatypes::StringChunked"), true),
            mk("readback", "polars_core::schema::Schema", "retain_mut", "&mut self", vec![("f", "F")], vec![("F", "core::ops::function::FnMut(&mut polars_core::datatypes::field::Field) -> bool")], None, false),
            mk("native", ca, "apply_mut", "&mut self", vec![("f", "F")], vec![("F", "core::ops::function::Fn(T::Native) -> T::Native + core::marker::Copy")], None, true),
            mk("otherarg", column, "apply_with_state", "&self", vec![("f", "F"), ("state", "polars_arrow::bitmap::Bitmap")], vec![("F", "core::ops::function::Fn(&polars_core::series::Series) -> polars_core::series::Series")], Some(column), false),
            mk("sink", "polars_lazy::frame::LazyFrame", "collect", "self", vec![], vec![], Some("polars_error::PolarsResult<polars_core::frame::dataframe::DataFrame>"), false),
            mk("plan", "polars_lazy::frame::LazyFrame", "filter", "self", vec![("p", expr)], vec![], Some("Self"), false),
        ],
        supporting: vec![sup(series, &["Clone", "Debug"]), sup(column, &["Clone", "Debug"]), sup(field, &["Clone", "Debug"]), sup(expr, &["Clone", "Debug"]), sup("polars_core::schema::Schema", &["Clone", "Debug"]), sup("polars_core::datatypes::StringChunked", &["Clone"]), sup("polars_lazy::frame::LazyFrame", &["Clone"]), sup("polars_core::frame::dataframe::DataFrame", &["Clone", "Debug"])],
        provenance: None,
    };
    let mut release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), instantiation: InstantiationScope { families: vec!["numeric".into()], exclude: vec![] }, api_crates: vec!["polars_core".into(), "polars_plan".into(), "polars_lazy".into()], unordered: vec![], excluded_oracle: vec![], refused: vec![], bitmap_returns: vec![], bitmap_inputs: vec![], iterator_returns: vec![], cow_returns: vec![], free_instantiations: vec![], method_scalar_generics: vec![], bounded_readbacks: vec![], hash_tokens: vec![], null_aware_returns: vec![], sized_self_methods: vec![], external_bounds: vec![], chunk_snapshots: vec![], indexed_chunk_snapshots: vec![], array_snapshots: vec![], iter_snapshots: vec![], callback_mutable: vec![], callback_invocation: vec![], callback_sink: vec![], callback_safe: vec![], callback_recipe: vec![] };
    release.callback_mutable.push(CallbackMutable { path: format!("{expr}::map_many"), param: "function".into(), contract: "vector".into(), cite: "t".into() });
    release.callback_mutable.push(CallbackMutable { path: format!("{ca}::apply_into_string_amortized"), param: "f".into(), contract: "result buffer".into(), cite: "t".into() });
    // the source audit: every feasible closure gets its invocation; `stored`
    // names its sink groups; every non-plan-returning method of a plan holder is classified
    let audit = |path: String, param: &str, invocation: &str, sinks: &[&str]| CallbackInvocation { path, param: param.into(), invocation: invocation.into(), sinks: sinks.iter().map(|s| s.to_string()).collect(), cite: "t".into() };
    for (path, param, inv_, sinks) in [
        (format!("{column}::apply_unary_elementwise"), "f", "immediate", vec![]),
        (format!("{expr}::map"), "function", "stored", vec!["plan execution"]),
        (format!("{expr}::map"), "output_type", "stored", vec!["schema resolution"]),
        (format!("{expr}::map_many"), "function", "stored", vec!["plan execution"]),
        (format!("{ca}::apply_into_string_amortized"), "f", "immediate", vec![]),
        (format!("{ca}::apply_mut"), "f", "immediate", vec![]),
    ] { release.callback_invocation.push(audit(path, param, inv_, &sinks)); }
    release.callback_sink.push(CallbackSink { path: "polars_lazy::frame::LazyFrame::collect".into(), sink: "plan execution".into(), cite: "t".into() });
    release.callback_sink.push(CallbackSink { path: "polars_lazy::frame::LazyFrame::collect_schema".into(), sink: "schema resolution".into(), cite: "t".into() });
    let mut inv = inv;
    inv.callables.push(mk("schema", "polars_lazy::frame::LazyFrame", "collect_schema", "self", vec![], vec![], Some("polars_error::PolarsResult<polars_core::schema::SchemaRef>"), false));
    // a `'static` closure the audit does not cover: neither stored nor immediate, unresolved
    inv.callables.push(mk("unaudited", column, "apply_later", "&self", vec![("f", "F")], vec![("F", "core::ops::function::Fn(&polars_core::series::Series) -> polars_core::series::Series + 'static")], Some(column), false));
    let world = World::new(&inv, &release, &["mechanical", "conversion", "callback"]);
    let census = callback_census(&world, &inv, &[]);
    let row = |key: &str| census["rows"].as_array().unwrap().iter().find(|r| r["key"] == key).unwrap_or_else(|| panic!("no row {key}")).clone();
    let disp = |key: &str| row(key)["disposition"].as_str().unwrap().to_string();
    let refusals = |key: &str| row(key)["refusals"].as_array().map(|v| v.iter().map(|x| x.as_str().unwrap().to_string()).collect::<Vec<_>>()).unwrap_or_default();
    let unresolved = |key: &str| row(key)["unresolved"].as_array().map(|v| v.iter().map(|x| x.as_str().unwrap().to_string()).collect::<Vec<_>>()).unwrap_or_default();
    assert_eq!(disp("ok"), "feasible", "{:?} {:?}", refusals("ok"), unresolved("ok"));
    assert_eq!(row("ok")["invocation"][0]["invocation"], "immediate");
    assert_eq!(disp("stored"), "feasible", "{:?} {:?}", refusals("stored"), unresolved("stored"));
    assert_eq!(row("stored")["invocation"][0]["invocation"], "stored");
    assert_eq!(row("stored")["invocation"][1]["sinks"][0], "schema resolution", "each closure has its own audited sinks");
    assert_eq!(row("stored")["closures"].as_array().unwrap().len(), 2, "two closures, one row");
    assert_eq!(disp("unaudited"), "unresolved", "a 'static bound does not classify: without an audit entry the operation is unresolved");
    assert_eq!(row("unaudited")["invocation"][0]["static_bound"], true, "the bound stays a recorded fact");
    assert!(unresolved("unaudited")[0].contains("invocation not audited"));
    assert_eq!(disp("free"), "refused");
    assert!(refusals("free").iter().any(|r| r.contains("is the free generic `E`") || r.contains("is the free generic `K`")), "{:?}", refusals("free"));
    assert!(refusals("arrow").iter().any(|r| r.contains("argument `&polars_arrow::array::Array`")), "{:?}", refusals("arrow"));
    assert!(refusals("amort").iter().any(|r| r.contains("AmortSeries")), "{:?}", refusals("amort"));
    assert!(refusals("udf").iter().any(|r| r.contains("Udf trait object")), "{:?}", refusals("udf"));
    assert!(refusals("borrowed").iter().any(|r| r.contains("borrowed from the argument")), "{:?}", refusals("borrowed"));
    assert_eq!(disp("slice"), "feasible (vector argument)", "{:?}", refusals("slice"));
    assert_eq!(disp("buffer"), "feasible (per family, result buffer)", "{:?}", refusals("buffer"));
    assert!(refusals("readback").iter().any(|r| r.contains("cannot write back")), "{:?}", refusals("readback"));
    assert_eq!(disp("native"), "feasible (per family)", "{:?}", refusals("native"));
    assert!(refusals("otherarg").iter().any(|r| r.starts_with("parameter `state`")), "a closure that maps on a callable whose other argument does not is not feasible: {:?}", refusals("otherarg"));
    let sinks = census["sinks"]["bindings"].as_array().unwrap();
    assert!(sinks.iter().any(|s| s["key"] == "sink") && !sinks.iter().any(|s| s["key"] == "plan"), "collect is a classified sink, filter (returns the plan) is not a candidate");
    assert!(census["sinks"]["unclassified"].as_array().unwrap().is_empty());
    let empty = || Emitted { from_names: BTreeMap::new(), functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    let mut emitted = empty();
    emit_callable(&world, &mut emitted, inv.callables.iter().find(|c| c.key == "ok").unwrap(), &["callback"]);
    assert_eq!(emitted.entries[0].status, "generated");
    assert!(emitted.functions.contains("engine::run"), "an immediate callback is routed in emitted code");
    let mut no_invocation = release.clone();
    no_invocation.callback_invocation.retain(|a| a.path != format!("{column}::apply_unary_elementwise"));
    let world_no_invocation = World::new(&inv, &no_invocation, &["callback"]);
    let mut refused = empty();
    emit_callable(&world_no_invocation, &mut refused, inv.callables.iter().find(|c| c.key == "ok").unwrap(), &["callback"]);
    assert_eq!(refused.entries[0].status, "unsupported", "missing invocation audit must stop emission");
    // removing the classification of one execution path makes every stored operation unresolved, immediate ones stay feasible
    let mut fewer = release.clone();
    fewer.callback_sink.retain(|k| !k.path.ends_with("::collect_schema"));
    let world2 = World::new(&inv, &fewer, &["mechanical", "conversion", "callback"]);
    let mut guarded = empty();
    emit_callable(&world2, &mut guarded, inv.callables.iter().find(|c| c.key == "stored").unwrap(), &["callback"]);
    assert_eq!(guarded.entries[0].status, "unsupported", "missing sink classification must stop a stored binding");
    let mut immediate = empty();
    emit_callable(&world2, &mut immediate, inv.callables.iter().find(|c| c.key == "ok").unwrap(), &["callback"]);
    assert_eq!(immediate.entries[0].status, "generated", "independently audited immediate callback remains bound");
    let census2 = callback_census(&world2, &inv, &[]);
    let row2 = |key: &str| census2["rows"].as_array().unwrap().iter().find(|r| r["key"] == key).unwrap().clone();
    assert_eq!(census2["sinks"]["unclassified"][0], "polars_lazy::frame::LazyFrame::collect_schema");
    assert_eq!(row2("stored")["disposition"], "unresolved", "an unclassified execution path leaves stored callbacks unresolved");
    assert!(row2("stored")["unresolved"][0].as_str().unwrap().contains("unclassified execution path"));
    assert_eq!(row2("ok")["disposition"], "feasible", "an immediate callback does not depend on the sinks");
    // the routing rule: a closure-taking callable on a non-data owner is routed with the reason `callback`
    assert!(!routed("wrap_msg", Some("polars_error::PolarsError"), &[], Some("Self")), "today's rules leave wrap_msg unrouted");
    assert!(routed_for_callbacks(&mk("w", "polars_error::PolarsError", "wrap_msg", "&self", vec![("func", "F")], vec![("F", "core::ops::function::FnOnce(&str) -> alloc::string::String")], Some("Self"), false)), "the rule routes it");
    println!("callback self-test: ok");
}

/// Record 0079: the routing rule for callbacks. Every binding that accepts
/// a closure is routed through `engine::run`, whatever its owner, name or
/// types would decide, because the callback may be invoked immediately by
/// that call and its failures are translated only at that boundary.
fn routed_for_callbacks(c: &Callable) -> bool {
    c.params.iter().any(|p| closure_signature(c, p).is_some()) || routed(&c.name, Some(&c.owner), &c.params, c.ret_canonical.as_deref())
}

fn routed_binding(world: &World, c: &Callable, owner: Option<&str>) -> bool {
    if world.release.callback_safe.iter().any(|safe| safe.path == c.canonical_path) {
        return false;
    }
    if world.release.callback_sink.iter().any(|sink| sink.path == c.canonical_path && sink.sink != "none") {
        return true;
    }
    c.params.iter().any(|p| closure_signature(c, p).is_some()) || routed(&c.name, owner, &c.params, c.ret_canonical.as_deref())
}

fn binding_route_reason(world: &World, c: &Callable, routed: bool) -> Option<String> {
    if world.release.callback_safe.iter().any(|safe| safe.path == c.canonical_path) { return Some("callback-safe (audited)".into()); }
    if c.params.iter().any(|p| closure_signature(c, p).is_some()) { return Some("callback".into()); }
    if world.release.callback_sink.iter().any(|sink| sink.path == c.canonical_path && sink.sink != "none") { return Some("executes callbacks".into()); }
    if routed { Some("engine thread".into()) } else { None }
}

/// The source audit is an admission rule. A missing invocation or execution
/// path cannot acquire a binding merely because its Rust types map.
fn callback_gate(world: &World, c: &Callable) -> Result<(), String> {
    if c.params.iter().any(|p| closure_signature(c, p).is_some()) {
        if let Some(disposition) = world.callback_dispositions.get(&c.key) {
            if !disposition.starts_with("feasible") { return Err(disposition.clone()); }
        }
    }
    for p in &c.params {
        let Some(sig) = closure_signature(c, p) else { continue };
        let Some(audit) = world.release.callback_invocation.iter().find(|a| a.path == c.canonical_path && a.param == p.name) else {
            return Err(format!("{}: invocation not audited", p.name));
        };
        if audit.invocation == "stored" {
            if !world.callback_unclassified_sinks.is_empty() {
                return Err(format!("{}: unclassified execution path(s): {}", p.name, world.callback_unclassified_sinks.join(", ")));
            }
            for group in &audit.sinks {
                if !world.callback_sink_groups.contains(group) { return Err(format!("{}: sink group `{group}` has no classified member", p.name)); }
            }
        }
        for arg in &sig.args {
            if arg.trim().starts_with("&mut ") && !world.release.callback_mutable.iter().any(|m| m.path == c.canonical_path && m.param == p.name) {
                return Err(format!("{}: mutable argument `{arg}` has no audited contract", p.name));
            }
        }
    }
    Ok(())
}

fn conversion_census(entries: &[Entry], inv: &Inventory, from_names: &BTreeMap<String, String>) -> serde_json::Value {
    let by_key: BTreeMap<&str, &Entry> = entries.iter().map(|e| (e.key.as_str(), e)).collect();
    let mut rows = Vec::new();
    let mut disp: BTreeMap<String, usize> = BTreeMap::new();
    for c in &inv.callables {
        if c.kind != "foreign_trait_impl" { continue; }
        let short = c.name.split('<').next().unwrap_or("");
        let class = if short == "From" { "from" } else if ASSIGN_OPS.iter().any(|(n, _, _, _)| *n == short) { "assign" } else if short == "Not" { "not" } else { continue };
        let name = match class { "from" => from_names.get(&c.key).cloned().unwrap_or_default(), "assign" => ASSIGN_OPS.iter().find(|(n, _, _, _)| *n == short).map(|x| x.2.to_string()).unwrap_or_default(), _ => "not_".into() };
        let source = c.params.first().map(|p| p.ty_canonical.clone()).unwrap_or_default();
        let disposition = match by_key.get(c.key.as_str()) {
            Some(e) if e.status == "generated" => "generated".to_string(),
            Some(e) => format!("{}: {}", e.status, e.reason.as_deref().unwrap_or("")),
            None => "not eligible".to_string(),
        };
        *disp.entry(format!("{class}: {}", disposition.split(':').next().unwrap_or(""))).or_insert(0) += 1;
        rows.push(serde_json::json!({"key": c.key, "class": class, "owner": c.owner, "source": source, "name": name, "disposition": disposition}));
    }
    serde_json::json!({"count": rows.len(), "by_disposition": disp, "rows": rows})
}

fn census_summary(pairs: &[PairRecord]) -> serde_json::Value {
    let mut per: BTreeMap<&str, BTreeMap<&str, usize>> = BTreeMap::new();
    for p in pairs {
        let k = match &p.result { Applicability::Proven => "proven", Applicability::Rejected(_) => "rejected", Applicability::Unresolved(_) => "unresolved" };
        *per.entry(p.family).or_default().entry(k).or_insert(0) += 1;
        *per.entry("all").or_default().entry(k).or_insert(0) += 1;
    }
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    for p in pairs {
        if let Applicability::Rejected(r) | Applicability::Unresolved(r) = &p.result {
            let key = r.split('`').next().unwrap_or(r).trim().to_string();
            *reasons.entry(key).or_insert(0) += 1;
        }
    }
    let mut eligible: BTreeMap<&str, usize> = BTreeMap::new();
    for p in pairs.iter().filter(|p| p.eligible) {
        let k = match &p.result { Applicability::Proven => "proven", Applicability::Rejected(_) => "rejected", Applicability::Unresolved(_) => "unresolved" };
        *eligible.entry(k).or_insert(0) += 1;
    }
    let mut disp: BTreeMap<String, usize> = BTreeMap::new();
    for p in pairs.iter().filter(|p| matches!(p.result, Applicability::Proven)) {
        let d = p.disposition.as_deref().unwrap_or("none").split(':').next().unwrap_or("none").to_string();
        *disp.entry(d).or_insert(0) += 1;
    }
    serde_json::json!({"pairs": pairs.len(), "by_family": per, "eligible": eligible, "proven_dispositions": disp, "exception_reasons": reasons})
}

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
            impls: vec![],
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
    let release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), instantiation: InstantiationScope::default(), api_crates: vec!["polars_core".into(), "polars_plan".into()], unordered: vec![], excluded_oracle: vec![], refused: vec![], bitmap_returns: vec![], bitmap_inputs: vec![], iterator_returns: vec![], cow_returns: vec![], free_instantiations: vec![], method_scalar_generics: vec![], bounded_readbacks: vec![], hash_tokens: vec![], null_aware_returns: vec![], sized_self_methods: vec![], external_bounds: vec![], chunk_snapshots: vec![], indexed_chunk_snapshots: vec![], array_snapshots: vec![], iter_snapshots: vec![], callback_mutable: vec![], callback_invocation: vec![], callback_sink: vec![], callback_safe: vec![], callback_recipe: vec![] };
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
    // deref targets: only a `dyn Trait` target of a wrapped type names a route
    let mk = |owner: &str, name: &str, assoc: Vec<(String, String)>| Callable {
        key: format!("{owner}#{name}"), kind: "foreign_trait_impl".into(), krate: "polars_core".into(), owner: owner.into(), name: name.into(), canonical_path: format!("{owner} as {name}"),
        found_paths: vec![], crate_paths: vec![], receiver: "&self".into(), params: vec![], ret: None, ret_canonical: None, generics_canonical: vec![], impl_for: Some(owner.into()), impl_bounds: vec![],
        impl_head: None, impl_where: vec![], impl_assoc: assoc, docs_first: None, owner_generic: false, is_unsafe: false, is_async: false, deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "generic".into(), rules: vec![],
    };
    let mut tr = sup("polars_core::schema::SomeTrait", "trait", None);
    tr.kind = "trait".into();
    let inv = Inventory {
        callables: vec![
            mk("polars_core::schema::Field", "Deref", vec![("Target".into(), "dyn polars_core::schema::SomeTrait".into())]),
            mk("polars_plan::dsl::Field", "Deref", vec![("Target".into(), "alloc::vec::Vec<u8>".into())]),
            mk("polars_plan::dsl::Field", "DerefMut", vec![]),
        ],
        provenance: None,
        supporting: vec![sup("polars_core::schema::Field", "struct", None), sup("polars_plan::dsl::Field", "struct", None), tr],
    };
    let w = World::new(&inv, &release, &["mechanical"]);
    assert_eq!(w.deref_targets.get("polars_core::schema::SomeTrait").map(|v| v.len()), Some(1), "a dyn-trait target names one route");
    assert!(w.deref_targets.values().flatten().all(|o| o == "polars_core::schema::Field"), "a Deref to a non-trait target binds nothing");
    assert!(w.deref_mut.contains("polars_plan::dsl::Field") && !w.deref_mut.contains("polars_core::schema::Field"));
    println!("deref self-test: ok");
    applicability_self_test();
}

/// Record 0076 gate 3 controls, from a synthetic inventory: a specialized
/// head matches only its alias; repeated parameters reject; `Self: Trait`
/// proves only for recorded implementors; a bound on a parameter the head
/// leaves open is unresolved; an associated-type equality proves and
/// rejects by the recorded binding; an unresolved projection is an
/// exception, never a binding.
fn applicability_self_test() {
    fn sup(path: &str, kind: &str, target: Option<&str>) -> Supporting {
        Supporting {
            key: path.to_string(), kind: kind.to_string(), canonical_path: path.to_string(),
            found_paths: vec![format!("polars::{}", path.split("::").skip(1).collect::<Vec<_>>().join("::"))], crate_paths: vec![path.to_string()],
            public_fields: 0, fields_canonical: vec![], variant_shapes: vec![], variant_payloads: vec![], generic: false, lifetime: false, hidden: false,
            derived: vec![], alias_target: target.map(|t| t.to_string()), implementors: vec![], impls: vec![],
        }
    }
    fn method(name: &str, head: &str, bounds: &[(&str, &str)], wh: &[&str], params: &[&str], ret: &str) -> Callable {
        Callable {
            key: format!("k:{name}"), kind: "inherent".into(), krate: "polars_core".into(), owner: "polars_core::chunked_array::ChunkedArray".into(), name: name.into(),
            canonical_path: format!("polars_core::chunked_array::ChunkedArray::{name}"), found_paths: vec![], crate_paths: vec![], receiver: "&self".into(),
            params: params.iter().enumerate().map(|(i, t)| Param { name: format!("a{i}"), ty: t.to_string(), ty_canonical: t.to_string() }).collect(),
            ret: Some(ret.into()), ret_canonical: Some(ret.into()), generics_canonical: vec![], impl_for: None, impl_bounds: bounds.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
            impl_head: Some(head.into()), impl_where: wh.iter().map(|w| w.to_string()).collect(), impl_assoc: vec![], docs_first: None, owner_generic: true, is_unsafe: false, is_async: false,
            deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "generic".into(), rules: vec![],
        }
    }
    let ca = "polars_core::chunked_array::ChunkedArray";
    let mut base = sup(ca, "struct", None);
    base.generic = true;
    let mut numeric = sup("polars_core::datatypes::PolarsNumericType", "trait", None);
    numeric.implementors = vec!["polars_core::datatypes::Int64Type".into()];
    numeric.impls = vec![model::TraitImpl { for_type: "polars_core::datatypes::Int64Type".into(), blanket: false, bounds: vec![], where_predicates: vec![], assoc_types: vec![("Native".into(), "i64".into())] }];
    let mut data = sup("polars_core::datatypes::PolarsDataType", "trait", None);
    data.implementors = vec!["polars_core::datatypes::Int64Type".into(), "polars_core::datatypes::BooleanType".into()];
    data.impls = vec![
        model::TraitImpl { for_type: "polars_core::datatypes::Int64Type".into(), blanket: false, bounds: vec![], where_predicates: vec![], assoc_types: vec![("Physical".into(), "i64".into())] },
        model::TraitImpl { for_type: "polars_core::datatypes::BooleanType".into(), blanket: false, bounds: vec![], where_predicates: vec![], assoc_types: vec![("Physical".into(), "bool".into())] },
    ];
    let mut logical = sup("polars_core::chunked_array::logical::LogicalType", "trait", None);
    logical.impls = vec![model::TraitImpl { for_type: format!("{ca}<polars_core::datatypes::Int64Type>"), blanket: false, bounds: vec![], where_predicates: vec![], assoc_types: vec![] }];
    let inv = Inventory {
        callables: vec![],
        provenance: None,
        supporting: vec![
            base, numeric, data, logical,
            sup("polars_core::datatypes::Int64Type", "struct", None), sup("polars_core::datatypes::BooleanType", "struct", None), sup("polars_core::datatypes::StringType", "struct", None),
            sup("polars_core::datatypes::Int64Chunked", "type_alias", Some(&format!("{ca}<polars_core::datatypes::Int64Type>"))),
            sup("polars_core::datatypes::BooleanChunked", "type_alias", Some(&format!("{ca}<polars_core::datatypes::BooleanType>"))),
            sup("polars_core::datatypes::StringChunked", "type_alias", Some(&format!("{ca}<polars_core::datatypes::StringType>"))),
        ],
    };
    let release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), instantiation: InstantiationScope::default(), api_crates: vec!["polars_core".into()], unordered: vec![], excluded_oracle: vec![], refused: vec![], bitmap_returns: vec![], bitmap_inputs: vec![], iterator_returns: vec![], cow_returns: vec![], free_instantiations: vec![], method_scalar_generics: vec![], bounded_readbacks: vec![], hash_tokens: vec![], null_aware_returns: vec![], sized_self_methods: vec![], external_bounds: vec![], chunk_snapshots: vec![], indexed_chunk_snapshots: vec![], array_snapshots: vec![], iter_snapshots: vec![], callback_mutable: vec![], callback_invocation: vec![], callback_sink: vec![], callback_safe: vec![], callback_recipe: vec![] };
    let w = World::new(&inv, &release, &["mechanical"]);
    let i64c = format!("{ca}<polars_core::datatypes::Int64Type>");
    let boolc = format!("{ca}<polars_core::datatypes::BooleanType>");
    let strc = format!("{ca}<polars_core::datatypes::StringType>");
    let ok = |r: &Applicability| matches!(r, Applicability::Proven);
    // a specialized head matches only its alias
    let m = method("all", &boolc, &[], &[], &[], "bool");
    assert!(ok(&w.applicability(&m, &boolc).0));
    assert!(matches!(w.applicability(&m, &i64c).0, Applicability::Rejected(_)), "a specialized head must reject another alias");
    // a generic head with a local-trait bound proves by recorded implementors and rejects otherwise
    let m = method("sum", &format!("{ca}<T>"), &[("T", "polars_core::datatypes::PolarsNumericType")], &[], &[], "T::Native");
    assert!(ok(&w.applicability(&m, &i64c).0));
    assert!(matches!(w.applicability(&m, &boolc).0, Applicability::Rejected(_)), "a bound the alias does not satisfy must reject");
    // the substituted signature resolves the projection; an unrecorded one is an exception
    let (_, subst) = w.applicability(&m, &i64c);
    assert_eq!(w.substitute_signature(&m, &i64c, &subst).unwrap().1.as_deref(), Some("i64"));
    let m2 = method("weird", &format!("{ca}<T>"), &[("T", "polars_core::datatypes::PolarsNumericType")], &[], &[], "T::Unknown");
    let (_, subst) = w.applicability(&m2, &i64c);
    assert!(w.substitute_signature(&m2, &i64c, &subst).is_err(), "an unresolved projection is an exception");
    // repeated type parameters must bind consistently
    let m = method("pair", &format!("{ca}<T>"), &[("T", "")], &[], &[], "bool");
    let mut params = BTreeSet::new(); params.insert("T".to_string());
    assert!(unify("polars_core::chunked_array::logical::Logical<T, T>", "polars_core::chunked_array::logical::Logical<A, B>", &params).is_err(), "repeated parameters with different arguments must reject");
    assert!(unify("polars_core::chunked_array::logical::Logical<T, T>", "polars_core::chunked_array::logical::Logical<A, A>", &params).is_ok());
    let _ = m;
    // Self: LocalTrait proves only for recorded implementors
    let m = method("logical", &format!("{ca}<T>"), &[("T", "polars_core::datatypes::PolarsDataType")], &["Self: polars_core::chunked_array::logical::LogicalType"], &[], "bool");
    assert!(ok(&w.applicability(&m, &i64c).0), "Self: LogicalType holds for the recorded impl");
    assert!(matches!(w.applicability(&m, &boolc).0, Applicability::Rejected(_)), "Self: LogicalType rejects an alias without an impl");
    // a bound involving a parameter the head does not bind is unresolved
    let m = method("open", &format!("{ca}<T>"), &[("T", "polars_core::datatypes::PolarsDataType"), ("U", "polars_core::datatypes::PolarsDataType")], &[], &[], "bool");
    assert!(matches!(w.applicability(&m, &i64c).0, Applicability::Unresolved(_)), "a bound on an unbound parameter is unresolved");
    // an associated-type equality proves and rejects by the recorded binding
    let m = method("phys", &format!("{ca}<T>"), &[("T", "polars_core::datatypes::PolarsDataType<Physical = i64>")], &[], &[], "bool");
    assert!(ok(&w.applicability(&m, &i64c).0));
    assert!(matches!(w.applicability(&m, &boolc).0, Applicability::Rejected(_)), "a differing associated type must reject");
    // a trait with no recorded impl is unresolved, an alias outside every impl of a trait that has some is rejected
    let m = method("num", &format!("{ca}<T>"), &[("T", "polars_core::datatypes::PolarsNumericType")], &[], &[], "bool");
    assert!(matches!(w.applicability(&m, &strc).0, Applicability::Rejected(_)));
    let m = method("foreign", &format!("{ca}<T>"), &[("T", "num_traits::float::Float")], &[], &[], "bool");
    assert!(matches!(w.applicability(&m, &i64c).0, Applicability::Unresolved(_)), "a trait outside the inventory is unresolved");
    // core traits by type: floats have no Eq/Ord/Hash; a trait argument is never erased
    assert!(matches!(w.holds("f64", "core::cmp::Eq", 0), Applicability::Rejected(_)), "f64: Eq must be rejected");
    assert!(matches!(w.holds("f64", "core::cmp::Ord", 0), Applicability::Rejected(_)));
    assert!(matches!(w.holds("f64", "core::cmp::PartialEq", 0), Applicability::Proven));
    assert!(matches!(w.holds("i64", "core::cmp::Eq", 0), Applicability::Proven));
    assert!(matches!(w.holds("alloc::string::String", "core::marker::Copy", 0), Applicability::Rejected(_)));
    assert!(matches!(w.holds("polars_core::datatypes::Int64Type", "polars_core::datatypes::PolarsNumericType<unmodeled::Argument>", 0), Applicability::Unresolved(_)), "a generic trait argument must not be erased");
    assert!(matches!(w.holds("i64", "core::ops::function::Fn(i64) -> i64", 0), Applicability::Unresolved(_)));
    assert!(matches!(w.holds("polars_core::datatypes::Int64Type", "polars_core::datatypes::PolarsNumericType", 0), Applicability::Proven));
    // unsized str: neither Clone, Default nor Sized; sizedness by type
    assert!(matches!(w.holds("str", "core::clone::Clone", 0), Applicability::Rejected(_)), "str: Clone must be rejected");
    assert!(matches!(w.holds("str", "core::default::Default", 0), Applicability::Rejected(_)), "str: Default must be rejected");
    assert!(matches!(w.holds("str", "core::marker::Sized", 0), Applicability::Rejected(_)), "str: Sized must be rejected");
    assert!(matches!(w.holds("str", "core::fmt::Debug", 0), Applicability::Proven));
    assert!(matches!(w.holds("&str", "core::marker::Sized", 0), Applicability::Proven));
    assert!(matches!(w.holds("&str", "core::clone::Clone", 0), Applicability::Proven));
    assert!(matches!(w.holds("&mut str", "core::clone::Clone", 0), Applicability::Rejected(_)));
    assert!(matches!(w.holds("[u8]", "core::marker::Sized", 0), Applicability::Rejected(_)));
    assert!(matches!(w.holds("dyn polars_core::series::series_trait::SeriesTrait", "core::marker::Sized", 0), Applicability::Rejected(_)));
    assert!(matches!(w.holds("alloc::string::String", "core::marker::Sized", 0), Applicability::Proven));
    assert!(matches!(w.holds("polars_core::datatypes::Int64Type", "core::marker::Sized", 0), Applicability::Proven));
    assert!(matches!(w.holds(&i64c, "core::marker::Sized", 0), Applicability::Proven));
    assert!(matches!(w.holds("unknown::Type", "core::marker::Sized", 0), Applicability::Unresolved(_)), "an unknown type's sizedness is unresolved");
    println!("applicability self-test: ok");
    iterator_self_test();
}

/// Record 0077 gate 1 control: the recognizer accepts each bound spelling
/// the inventory shows, tells known from unknown length, sees the wrapper,
/// and rejects an `impl Trait` that is not an iterator.
fn iterator_self_test() {
    let known = |t: &str| iterator_return(&ty::parse(t)).map(|(item, known, wrap)| (item.render(), known, wrap));
    assert_eq!(known("impl '_ + core::marker::Send + core::marker::Sync + core::iter::traits::exact_size::ExactSizeIterator<Item = i64>"), Some(("i64".to_string(), IterLen::Exact, IterWrap::Plain)));
    assert_eq!(known("impl polars_core::chunked_array::iterator::PolarsIterator<Item = core::option::Option<&[u8]>>"), Some(("core::option::Option<&[u8]>".to_string(), IterLen::Exact, IterWrap::Plain)));
    assert_eq!(known("impl core::iter::traits::double_ended::DoubleEndedIterator<Item = &[f64]>"), Some(("&[f64]".to_string(), IterLen::Unknown, IterWrap::Plain)));
    assert_eq!(known("core::option::Option<impl core::iter::traits::double_ended::DoubleEndedIterator<Item = &[&[u8]]>>"), Some(("&[&[u8]]".to_string(), IterLen::Unknown, IterWrap::Option)));
    assert_eq!(known("polars_error::PolarsResult<impl core::iter::traits::iterator::Iterator<Item = polars_core::series::Series>>"), Some(("polars_core::series::Series".to_string(), IterLen::Unknown, IterWrap::Result)));
    assert_eq!(known("impl polars_arrow::trusted_len::TrustedLen<Item = usize>"), Some(("usize".to_string(), IterLen::Trusted, IterWrap::Plain)));
    assert_eq!(known("impl polars_arrow::trusted_len::TrustedLen<Item = usize> + core::iter::traits::exact_size::ExactSizeIterator"), Some(("usize".to_string(), IterLen::Exact, IterWrap::Plain)), "an exact bound beside TrustedLen wins");
    assert_eq!(known("impl core::fmt::Display"), None, "a non-iterator impl return is not an iterator");
    assert_eq!(known("impl core::iter::traits::iterator::Iterator"), None, "an iterator without an Item is not mapped");
    println!("iterator self-test: ok");
    from_naming_self_test();
}

/// Record 0078 gate 1 controls: a by-reference twin gets its own `_ref`
/// name; two distinct sources with one last segment get crate-qualified
/// names; a twin pair is never merged, whatever the impls return; the
/// segment rule spells `Vec` and `Option` sources.
fn from_naming_self_test() {
    fn from(key: &str, owner: &str, src: &str) -> Callable {
        Callable {
            key: key.into(), kind: "foreign_trait_impl".into(), krate: "polars_core".into(), owner: owner.into(), name: format!("From<{}>", src.rsplit("::").next().unwrap()), canonical_path: format!("{owner} as core::convert::From"),
            found_paths: vec![], crate_paths: vec![], receiver: "none".into(), params: vec![Param { name: "value".into(), ty: src.into(), ty_canonical: src.into() }], ret: None, ret_canonical: Some(owner.into()),
            generics_canonical: vec![], impl_for: Some(owner.into()), impl_bounds: vec![], impl_head: None, impl_where: vec![], impl_assoc: vec![], docs_first: None, owner_generic: false, is_unsafe: false, is_async: false,
            deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "conversion".into(), rules: vec![],
        }
    }
    let o = "polars_core::datatypes::field::Field";
    let inv = Inventory {
        callables: vec![
            from("a", o, "polars_core::datatypes::dtype::DataType"),
            from("b", o, "&polars_core::datatypes::dtype::DataType"),
            from("c", o, "polars_core::schema::Field"),
            from("d", o, "polars_arrow::datatypes::field::Field"),
            from("e", o, "alloc::vec::Vec<polars_core::series::Series>"),
            from("f", o, "core::option::Option<i64>"),
            from("g", o, "i64"),
        ],
        supporting: vec![],
        provenance: None,
    };
    let names = plan_from_names(&inv);
    assert_eq!(names["a"], "from_data_type");
    assert_eq!(names["b"], "from_data_type_ref", "a by-reference twin is its own binding");
    assert_ne!(names["a"], names["b"], "twins are never merged");
    assert_eq!(names["c"], "from_core_field", "distinct sources with one last segment are crate-qualified");
    assert_eq!(names["d"], "from_arrow_field");
    assert_eq!(names["e"], "from_vec_series");
    assert_eq!(names["f"], "from_option_i64");
    assert_eq!(names["g"], "from_i64");
    let distinct: BTreeSet<&String> = names.values().collect();
    assert_eq!(distinct.len(), names.len(), "every impl has its own name");
    println!("from-naming self-test: ok");
    from_emission_self_test();
    callback_self_test();
    slice_self_test();
    generic_input_self_test();
    bitmap_self_test();
    bitmap_input_self_test();
    iterator_return_self_test();
    cow_return_self_test();
    free_instantiation_self_test();
    native_substitution_self_test();
    method_scalar_generic_self_test();
    checked_readback_self_test();
    hash_token_self_test();
    null_aware_self_test();
    sized_self_self_test();
    external_bound_self_test();
    chunk_snapshot_self_test();
    indexed_chunk_self_test();
    array_snapshot_self_test();
    iter_snapshot_self_test();
}

/// Record 0078 gate 1 controls, from a synthetic inventory through the
/// production emission: an inherent `from_x` makes the impl unsupported
/// with the collision named; an integer source is fallible and an `f32`
/// source is not; an unmappable source is refused with the type named; a
/// by-value/by-reference twin pair yields two bindings and two oracle
/// cases, each calling its own impl.
fn from_emission_self_test() {
    fn sup(path: &str, derived: &[&str]) -> Supporting {
        Supporting {
            key: path.to_string(), kind: "struct".into(), canonical_path: path.to_string(),
            found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())], crate_paths: vec![path.to_string()],
            public_fields: 0, fields_canonical: vec![], variant_shapes: vec![], variant_payloads: vec![], generic: false, lifetime: false, hidden: false,
            derived: derived.iter().map(|d| d.to_string()).collect(), alias_target: None, implementors: vec![], impls: vec![],
        }
    }
    let owner = "polars_core::scalar::Scalar";
    let dtype = "polars_core::datatypes::dtype::DataType";
    let field = "polars_core::datatypes::field::Field";
    let from = |key: &str, src: &str| Callable {
        key: key.into(), kind: "foreign_trait_impl".into(), krate: "polars_core".into(), owner: owner.into(), name: format!("From<{}>", src.rsplit("::").next().unwrap()), canonical_path: format!("{owner} as core::convert::From"),
        found_paths: vec![], crate_paths: vec![], receiver: "none".into(), params: vec![Param { name: "value".into(), ty: src.into(), ty_canonical: src.into() }], ret: None, ret_canonical: Some(owner.into()),
        generics_canonical: vec![], impl_for: Some(owner.into()), impl_bounds: vec![], impl_head: None, impl_where: vec![], impl_assoc: vec![], docs_first: None, owner_generic: false, is_unsafe: false, is_async: false,
        deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "conversion".into(), rules: vec![],
    };
    let inherent = Callable {
        key: "inh".into(), kind: "inherent".into(), krate: "polars_core".into(), owner: owner.into(), name: "from_data_type".into(), canonical_path: format!("{owner}::from_data_type"),
        found_paths: vec![], crate_paths: vec![], receiver: "none".into(), params: vec![Param { name: "d".into(), ty: dtype.into(), ty_canonical: dtype.into() }], ret: None, ret_canonical: Some(owner.into()),
        generics_canonical: vec![], impl_for: None, impl_bounds: vec![], impl_head: None, impl_where: vec![], impl_assoc: vec![], docs_first: None, owner_generic: false, is_unsafe: false, is_async: false,
        deprecated: false, hidden: false, implementors: vec![], trait_reachable: false, derived: false, bucket: "mechanical".into(), rules: vec![],
    };
    let inv = Inventory {
        callables: vec![inherent, from("clash", dtype), from("int", "i8"), from("float", "f32"), from("arrow", "polars_arrow::datatypes::field::Field"), from("twin_v", field), from("twin_r", &format!("&{field}"))],
        supporting: vec![sup(owner, &["Clone", "Debug", "PartialEq"]), sup(dtype, &["Clone", "Debug", "PartialEq", "Default"]), sup(field, &["Clone", "Debug", "PartialEq", "Default"])],
        provenance: None,
    };
    let release = Release { name: "t".into(), source: "t".into(), provenance: ReleaseProvenance::default(), instantiation: InstantiationScope::default(), api_crates: vec!["polars_core".into()], unordered: vec![], excluded_oracle: vec![], refused: vec![], bitmap_returns: vec![], bitmap_inputs: vec![], iterator_returns: vec![], cow_returns: vec![], free_instantiations: vec![], method_scalar_generics: vec![], bounded_readbacks: vec![], hash_tokens: vec![], null_aware_returns: vec![], sized_self_methods: vec![], external_bounds: vec![], chunk_snapshots: vec![], indexed_chunk_snapshots: vec![], array_snapshots: vec![], iter_snapshots: vec![], callback_mutable: vec![], callback_invocation: vec![], callback_sink: vec![], callback_safe: vec![], callback_recipe: vec![] };
    let world = World::new(&inv, &release, &["mechanical", "conversion"]);
    let mut out = Emitted { from_names: plan_from_names(&inv), functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    let buckets = ["mechanical", "conversion"];
    for c in &inv.callables {
        emit_callable(&world, &mut out, c, &buckets);
    }
    fn find<'a>(entries: &'a [Entry], key: &str) -> &'a Entry { entries.iter().find(|e| e.key == key).unwrap_or_else(|| panic!("no entry for {key}")) }
    let entry = |key: &str| find(&out.entries, key);
    let clash = entry("clash");
    assert_eq!(clash.status, "unsupported", "an inherent from_x makes the impl unsupported");
    assert!(clash.reason.as_deref().unwrap_or("").contains("name taken by inherent from_data_type"), "the collision is named: {:?}", clash.reason);
    assert_eq!((entry("int").status, entry("int").fallible), ("generated", Some(true)), "an integer source narrows fallibly");
    assert_eq!((entry("float").status, entry("float").fallible), ("generated", Some(false)), "an f32 source is an infallible cast");
    assert!(out.functions.contains("(value as f32)"), "the f32 cast is emitted as written");
    let arrow = entry("arrow");
    assert_eq!(arrow.status, "unsupported");
    assert!(arrow.reason.as_deref().unwrap_or("").starts_with("conversion source"), "{:?}", arrow.reason);
    assert!(arrow.reason.as_deref().unwrap_or("").contains("polars_arrow::datatypes::field::Field"), "the unmappable source is named: {:?}", arrow.reason);
    let (tv, tr) = (entry("twin_v"), entry("twin_r"));
    assert_eq!((tv.status, tr.status), ("generated", "generated"));
    assert_eq!(tv.bindings.len() + tr.bindings.len(), 2, "a twin pair is two bindings");
    assert_ne!(tv.bindings[0].rune, tr.bindings[0].rune, "with distinct names");
    // the arrow Field source shares the last segment, so both twins are crate-qualified
    assert_eq!((tv.bindings[0].rune.as_str(), tr.bindings[0].rune.as_str()), ("polars::Scalar::from_core_field", "polars::Scalar::from_core_field_ref"));
    assert!(out.functions.contains("<polars::prelude::Scalar as From<polars::prelude::Field>>::from") || out.functions.contains(&format!("<{} as From<{}>>::from", world.wrappers[owner].spell, world.wrappers[field].spell)), "the by-value twin calls its own impl");
    assert!(out.functions.contains(&format!("<{} as From<&{}>>::from", world.wrappers[owner].spell, world.wrappers[field].spell)), "the by-reference twin calls its own impl");
    assert!(!out.entries.iter().any(|e| e.status == "adapted"), "no impl is adapted as provided by another");
    let (_, harness, _, _) = emit_oracle(&world, &mut out.entries, &inv);
    let entry = |key: &str| find(&out.entries, key);
    let ids: Vec<String> = ["twin_v", "twin_r"].iter().map(|k| entry(k).bindings[0].case_id.clone().unwrap_or_else(|| panic!("{k}: no case ({:?})", entry(k).bindings[0].disposition))).collect();
    assert_ne!(ids[0], ids[1], "a twin pair is two oracle cases");
    assert!(ids.iter().all(|i| harness.contains(&format!("Case {{ id: \"{i}\""))), "both cases are emitted: {ids:?}");
    // the by-value case hands the prepared source to the call and returns it for comparison; the by-reference case calls with the prepared value
    assert!(harness.contains("let s = __fx[0]; let r = polars::Scalar::from_core_field(s); ((r, s), ())"), "the by-value case preserves its source");
    assert!(harness.contains("polars::Scalar::from_core_field_ref(__fx[0])"), "the by-reference case calls its own binding");
    assert!(harness.contains("as From<polars::prelude::Field>>::from(__a0.clone())") || harness.contains(&format!("<{} as From<{}>>::from(__a0.clone())", world.wrappers[owner].spell, world.wrappers[field].spell)), "the by-value oracle clones the source for its own impl");
    // duplicate listings: a repeated impl (one impl id on two pages) is
    // adapted with its retained counterpart named; an outward impl on the
    // source page alone is unsupported; a repeated impl whose retained
    // listing is unsupported is unsupported too
    let listing = |key: &str, page: &str, for_ty: &str, src: &str| {
        let mut c = from(key, src);
        c.owner = page.into();
        c.canonical_path = format!("{page} as core::convert::From");
        c.impl_for = Some(for_ty.into());
        c
    };
    let arrow = "polars_arrow::datatypes::field::Field";
    let inv = Inventory {
        callables: vec![
            listing("polars_core:s:100", owner, owner, dtype), listing("polars_core:d:100", dtype, owner, dtype),
            listing("polars_core:s:200", owner, "&'static str", owner), listing("polars_core:s:201", owner, "(polars_utils::pl_str::PlSmallStr, polars_core::datatypes::dtype::DataType)", owner),
            listing("polars_core:s:300", owner, owner, arrow), listing("polars_core:f:300", arrow, owner, arrow),
        ],
        supporting: vec![sup(owner, &["Clone", "Debug", "PartialEq"]), sup(dtype, &["Clone", "Debug", "PartialEq", "Default"])],
        provenance: None,
    };
    let world = World::new(&inv, &release, &["mechanical", "conversion"]);
    let mut out = Emitted { from_names: plan_from_names(&inv), functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    for c in &inv.callables {
        emit_callable(&world, &mut out, c, &buckets);
    }
    resolve_duplicates(&mut out.entries);
    let entry = |key: &str| find(&out.entries, key);
    assert_eq!(entry("polars_core:s:100").status, "generated");
    let dup = entry("polars_core:d:100");
    assert_eq!((dup.status, dup.counterpart.as_deref()), ("adapted", Some("polars_core:s:100")), "a repeated impl names its retained listing");
    assert_eq!(dup.rune, entry("polars_core:s:100").rune, "and carries that listing's binding");
    for k in ["polars_core:s:200", "polars_core:s:201"] {
        let e = entry(k);
        assert_eq!((e.status, e.counterpart.as_deref()), ("unsupported", None), "{k}: an outward impl is unsupported");
        assert!(e.reason.as_deref().unwrap_or("").starts_with("outward conversion"), "{k}: {:?}", e.reason);
    }
    assert_eq!(entry("polars_core:s:300").status, "unsupported");
    let e = entry("polars_core:f:300");
    assert_eq!((e.status, e.rune.as_deref()), ("unsupported", None), "a repeated impl whose retained listing is unsupported is unsupported");
    assert!(e.reason.as_deref().unwrap_or("").contains("retained listing is unsupported"), "{:?}", e.reason);
    assert!(!out.entries.iter().any(|e| e.status == "adapted" && e.counterpart.is_none()), "no listing is adapted without an identified counterpart");
    println!("from-emission self-test: ok");
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
    let inv = |release: Option<&str>, rev: Option<&str>| Inventory { callables: vec![], supporting: vec![], provenance: Some(model::Provenance { release: release.map(String::from), rev: rev.map(String::from), cfg: None, features: None }) };
    assert!(r.check_provenance(&inv(Some("9.9.9"), None)).is_ok());
    assert!(r.check_provenance(&inv(Some("0.55.2"), None)).is_err(), "another release is refused");
    assert!(r.check_provenance(&inv(None, Some("abc"))).is_err(), "a Git inventory is refused by a release-pinned file");
    assert!(r.check_provenance(&Inventory { callables: vec![], supporting: vec![], provenance: None }).is_err(), "no provenance is refused");
    let mut g = r.clone();
    g.provenance = ReleaseProvenance { release: None, rev: Some("abc".into()), cfg: None, features: vec![] };
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
    callback_dispositions: BTreeMap<String, String>,
    callback_unclassified_sinks: Vec<String>,
    callback_sink_groups: BTreeSet<String>,
    /// Counter for per-binding temporaries.
    tmp: std::cell::Cell<usize>,
    /// Record 0085: set while emitting a callable the release file lists
    /// under `bitmap_returns`; only then may a validity `Bitmap` return map.
    bitmap_ok: std::cell::Cell<bool>,
    /// Record 0086: (operation, length rule) while emitting a listed
    /// bitmap-input callable.
    bitmap_input: std::cell::RefCell<Option<(String, String)>>,
    /// Record 0087: the listed iterator item while emitting its callable.
    iter_return: std::cell::RefCell<Option<String>>,
    /// Record 0088: set while emitting a listed `Cow`-return callable.
    cow_ok: std::cell::Cell<bool>,
    /// Record 0093: set while emitting a listed bounded read-back callable.
    bounded_ok: std::cell::Cell<bool>,
    /// Record 0094: set while emitting a listed hash-token callable:
    /// (method name, return is a token, token parameter).
    hash_token: std::cell::RefCell<Option<(String, bool, Option<String>)>>,
    /// Record 0096: set while emitting one listed null-aware pair:
    /// (method name, the pair's native).
    null_aware: std::cell::RefCell<Option<(String, String)>>,
    /// Record 0097: set (to the method name) while emitting one pair of a
    /// listed `Self: Sized` method.
    sized_self: std::cell::RefCell<Option<String>>,
    /// Record 0099: set while emitting one listed chunk-snapshot pair:
    /// (method name, the pair's native).
    chunk_snapshot: std::cell::RefCell<Option<(String, String)>>,
    /// Record 0101: set while emitting one listed `downcast_get` pair:
    /// (method name, the pair's native or scalar kind).
    indexed_chunk: std::cell::RefCell<Option<(String, String)>>,
    /// Record 0102: set while emitting one listed `downcast_as_array` pair:
    /// (method name, the pair's native or scalar kind).
    array_snapshot: std::cell::RefCell<Option<(String, String)>>,
    /// Record 0103: set while emitting one listed `downcast_iter` pair.
    iter_snapshot: std::cell::RefCell<Option<(String, String)>>,
    /// Record 0091: (operation, parameter, check) while emitting a guarded
    /// free instantiation.
    arg_guard: std::cell::RefCell<Option<(String, String, String)>>,
    /// Record 0087: rendered return type of each listed iterator-return
    /// callable, to its item, so the oracle can frame the Rust result.
    iter_return_items: BTreeMap<String, String>,
    types: BTreeMap<String, Supporting>,
    wrappers: BTreeMap<String, Wrapper>,
    ambiguous_prelude: BTreeSet<String>,
    /// Types with a `Clone` impl, derived or hand-written.
    clonable: BTreeSet<String>,
    /// Foreign trait impls by owner: (trait short name, `for` type, bounds).
    impls: BTreeMap<String, Vec<(String, String, Vec<(String, String)>)>>,
    /// Record 0076: trait canonical path -> wrapped types whose `Deref`
    /// target is `dyn` that trait (`Series` -> `dyn SeriesTrait`).
    deref_targets: BTreeMap<String, Vec<String>>,
    /// Wrapped types with a `DerefMut` impl.
    deref_mut: BTreeSet<String>,
    /// Record 0076: an instantiation rendered canonically (`ChunkedArray<
    /// Int64Type>`) -> the alias wrapper key that holds exactly that type.
    by_identity: BTreeMap<String, String>,
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
            if !bucket_admitted(buckets, c) || !release.is_api(&c.krate) {
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
        let callback_unclassified_sinks = inv.callables.iter().filter(|c| release.is_api(&c.krate) && plan_holder_non_plan_method(c) && !release.callback_sink.iter().any(|s| s.path == c.canonical_path)).map(|c| c.canonical_path.clone()).collect();
        let callback_sink_groups = release.callback_sink.iter().filter(|s| s.sink != "none").map(|s| s.sink.clone()).collect();
        let mut w = World { release: release.clone(), callback_dispositions: BTreeMap::new(), callback_unclassified_sinks, callback_sink_groups, tmp: std::cell::Cell::new(0), bitmap_ok: std::cell::Cell::new(false), bitmap_input: std::cell::RefCell::new(None), iter_return: std::cell::RefCell::new(None), cow_ok: std::cell::Cell::new(false), bounded_ok: std::cell::Cell::new(false), hash_token: std::cell::RefCell::new(None), null_aware: std::cell::RefCell::new(None), sized_self: std::cell::RefCell::new(None), chunk_snapshot: std::cell::RefCell::new(None), indexed_chunk: std::cell::RefCell::new(None), array_snapshot: std::cell::RefCell::new(None), iter_snapshot: std::cell::RefCell::new(None), arg_guard: std::cell::RefCell::new(None), iter_return_items: inv.callables.iter().filter_map(|c| release.iterator_returns.iter().find(|r| r.path == c.canonical_path).and_then(|r| c.ret_canonical.as_ref().map(|t| (ty::parse(t).render(), r.item.clone())))).collect(), types, wrappers: BTreeMap::new(), ambiguous_prelude, clonable, impls, deref_targets: BTreeMap::new(), deref_mut: BTreeSet::new(), by_identity: BTreeMap::new() };
        w.assign_wrappers(&mentioned);
        for (path, wr) in &w.wrappers {
            if wr.rule == "alias" && wr.identity.contains('<') && wr.aliases.first().is_some_and(|a| a == path) {
                w.by_identity.insert(wr.identity.clone(), path.clone());
            }
        }
        for c in &inv.callables {
            if c.kind != "foreign_trait_impl" || !w.wrappers.contains_key(&c.owner) {
                continue;
            }
            let short = c.name.split('<').next().unwrap_or("");
            if short == "DerefMut" {
                w.deref_mut.insert(c.owner.clone());
            }
            if short == "Deref" {
                if let Some((_, target)) = c.impl_assoc.iter().find(|(n, _)| n == "Target") {
                    // only a trait-object target names a trait whose methods the type exposes
                    if let Some(tr) = target.strip_prefix("dyn ") {
                        let tr = tr.split(" + ").next().unwrap_or(tr).trim();
                        if w.types.get(tr).is_some_and(|t| t.kind == "trait") {
                            w.deref_targets.entry(tr.to_string()).or_default().push(c.owner.clone());
                        }
                    }
                }
            }
        }
        for v in w.deref_targets.values_mut() { v.sort(); v.dedup(); }
        // an alias wrapper clones when an impl covers its instantiation
        let mut more = Vec::new();
        for (path, wr) in &w.wrappers {
            if wr.base.is_some() && w.trait_holds(&wr.identity, "Clone", 0) {
                more.push(path.clone());
            }
        }
        w.clonable.extend(more);
        let callback_rows = callback_census(&w, inv, &[]);
        for row in callback_rows["rows"].as_array().into_iter().flatten() {
            if let (Some(key), Some(disposition)) = (row["key"].as_str(), row["disposition"].as_str()) {
                w.callback_dispositions.insert(key.into(), disposition.into());
            }
        }
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
    /// Record 0077: the return is an iterator, materialized inside the
    /// call (inside the routed closure when routed) under the bound.
    materialize: Option<Materialize>,
}

/// How an iterator return is materialized: the element conversion (over
/// `__r`, the element by value), whether the length is known exactly
/// (`ExactSizeIterator`, `PolarsIterator`, `TrustedLen`), and the wrapper
/// around the iterator (`Option`, `PolarsResult`, or none).
#[derive(Clone)]
struct Materialize {
    elem_conv: String,
    known: IterLen,
    wrap: IterWrap,
}
/// How an iterator's length is known: exactly through `len()`
/// (`ExactSizeIterator`, `PolarsIterator`), through the `TrustedLen`
/// contract's `size_hint` upper bound, or not at all.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum IterLen {
    Exact,
    Trusted,
    Unknown,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum IterWrap {
    Plain,
    Option,
    Result,
}

/// The iterator traits a return may name, with how each fixes the
/// length. `TrustedLen` is not `ExactSizeIterator` (Polars 0.55.2 defines
/// `pub unsafe trait TrustedLen: Iterator {}`), so it gets its own route.
const ITERATOR_TRAITS: &[(&str, IterLen)] = &[
    ("core::iter::traits::iterator::Iterator", IterLen::Unknown),
    ("core::iter::traits::double_ended::DoubleEndedIterator", IterLen::Unknown),
    ("core::iter::traits::exact_size::ExactSizeIterator", IterLen::Exact),
    ("polars_core::chunked_array::iterator::PolarsIterator", IterLen::Exact),
    ("polars_arrow::trusted_len::TrustedLen", IterLen::Trusted),
];

/// Recognize an iterator return: `impl` bounds naming an iterator trait
/// with an `Item`, possibly inside `Option` or `PolarsResult`.
fn iterator_return(t: &Ty) -> Option<(Ty, IterLen, IterWrap)> {
    let (inner, wrap) = match t {
        Ty::Path { path, args } if path == "core::option::Option" && args.len() == 1 => (&args[0], IterWrap::Option),
        Ty::Path { path, args } if (path == "polars_error::PolarsResult" || path == "core::result::Result") && !args.is_empty() => (&args[0], IterWrap::Result),
        other => (other, IterWrap::Plain),
    };
    let Ty::Impl(bounds) = inner else { return None };
    let mut item: Option<Ty> = None;
    let mut is_iter = false;
    let mut known = IterLen::Unknown;
    for b in bounds {
        if let Some((_, len)) = ITERATOR_TRAITS.iter().find(|(p, _)| *p == b.path) {
            is_iter = true;
            // the strongest knowledge among the bounds: exact beats trusted beats unknown
            known = match (known, *len) { (IterLen::Exact, _) | (_, IterLen::Exact) => IterLen::Exact, (IterLen::Trusted, _) | (_, IterLen::Trusted) => IterLen::Trusted, _ => IterLen::Unknown };
            if let Some(i) = &b.item { item = Some((**i).clone()); }
        }
    }
    if !is_iter { return None; }
    Some((item?, known, wrap))
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
/// Record 0093: integer source types whose range can exceed `i64` (or, for
/// `isize`, would on another target); read back through `support::widen`.
/// The rest of `INT_NARROW`, and `IdxSize` (`u32` without `bigidx`, which
/// `support` asserts at compile time), fit and keep `as i64`.
const RISKY_INTS: &[&str] = &["u64", "usize", "isize", "i128", "u128"];

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
        // record 0086: a listed callable's validity bitmap from a script Vec<bool>
        if let Ty::Path { path, args } = t {
            if path == "polars_arrow::bitmap::immutable::Bitmap" && args.is_empty() {
                if let Some((op, length)) = self.bitmap_input.borrow().clone() {
                    let (pre, expect, shape) = match length.as_str() {
                        // the oracle's receiver fixtures have 3 rows, the List one 2
                        "receiver" => (Some("let __mask_len = this.0.len();".to_string()), "Some(__mask_len)", if owner.is_some_and(|o| o.ends_with("::ListChunked")) { "mask2" } else { "mask3" }),
                        "values" => (Some("let __mask_len = support::vec_len(&values, \"values\")?;".to_string()), "Some(__mask_len)", "mask1"),
                        _ => (None, "None", "mask3"),
                    };
                    let mut a = ok_arg("rune::Value", format!("support::bitmap_from_bools(&{name}, \"{op}\", {expect})?"), "vector of bool (a validity mask, copied)")?;
                    a.pre.extend(pre);
                    a.shape = shape.into();
                    return Ok(a);
                }
            }
        }
        match t {
            Ty::Path { path, args } => match path.as_str() {
                "bool" => ok_arg("bool", name.into(), "bool"),
                "i64" => ok_arg("i64", name.into(), "int"),
                "f64" => ok_arg("f64", name.into(), "float"),
                "f32" => ok_arg("f64", format!("({name} as f32)"), "float"),
                // record 0091: a guarded parameter is converted and checked in `pre`,
                // before the call and before any receiver borrow
                "usize" if self.arg_guard.borrow().as_ref().is_some_and(|(_, param, _)| param == name) => {
                    let (op, _, check) = self.arg_guard.borrow().clone().unwrap();
                    if check != "below_idx_max" { return Err(Unsupported("unknown argument guard", check)); }
                    let mut a = ok_arg("i64", format!("__guarded_{name}"), "int (checked below the index maximum)")?;
                    a.pre.push(format!("let __guarded_{name} = support::below_idx_max(support::narrow::<usize>({name}, \"{name}\")?, \"{op}\", \"{name}\")?;"));
                    Ok(a)
                }
                // record 0094: a listed hash parameter is a token, parsed before the call
                "u64" if self.hash_token.borrow().as_ref().is_some_and(|h| h.2.as_deref() == Some(name)) => {
                    let op = self.hash_token.borrow().as_ref().unwrap().0.clone();
                    let mut a = ok_arg("&str", format!("__hash_{name}"), "string (a hash token: 16 lowercase hex digits)")?;
                    a.pre.push(format!("let __hash_{name} = support::hash_from_token({name}, \"{op}\")?;"));
                    a.fallible = true;
                    a.shape = "hash".into();
                    Ok(a)
                }
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
                    // record 0084: an element that maps to `&str` (`S: AsRef<str>`,
                    // `Into<PlSmallStr>`) is carried as an owned `String`
                    let (ity, conv) = if inner.rust_ty == "&str" && inner.borrow == 1 {
                        ("String".to_string(), "v".to_string())
                    } else {
                        if inner.borrow != 0 {
                            return Err(Unsupported("vector of borrows", t.render()));
                        }
                        by_value(&inner, "v")
                    };
                    let typed = if ity == "rune::Value" { String::new() } else { format!("let v: {ity} = support::borrow_element(&v, \"{name}\")?; ") };
                    Ok(shaped(ok_arg("rune::Value", format!("support::borrow_vec(&{name}, \"{name}\")?.into_iter().map(|v| {{ {typed}Ok::<_, Error>({conv}) }}).collect::<Result<Vec<_>, Error>>()?"), &format!("vector of {}", inner.doc))?, format!("vec({})", inner.shape)))
                }
                "core::result::Result" => Err(Unsupported("result argument", t.render())),
                "polars_error::PolarsResult" => Err(Unsupported("result argument", t.render())),
                "Self" => match owner {
                    Some(o) => self.arg(&Ty::Path { path: o.to_string(), args: vec![] }, name, generics, owner, depth + 1),
                    None => Err(Unsupported("Self without owner", t.render())),
                },
                _ => {
                    // an instantiation an alias wrapper holds exactly (record 0076)
                    if !args.is_empty() {
                        if let Some(c) = self.by_identity.get(&t.render()) {
                            return self.arg(&Ty::Path { path: c.clone(), args: vec![] }, name, generics, owner, depth + 1);
                        }
                    }
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
                    let typed = if ity == "rune::Value" { String::new() } else { format!("let v: {ity} = support::borrow_element(&v, \"{name}\")?; ") };
                    let owned = format!("support::borrow_vec(&{name}, \"{name}\")?.into_iter().map(|v| {{ {typed}Ok::<_, Error>({conv}) }}).collect::<Result<Vec<_>, Error>>()?");
                    let tmp = self.tmp();
                    let mut a = ok_arg("rune::Value", format!("&{tmp}[..]"), &format!("vector of {}", inner.doc))?;
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
        const ITERATOR_BOUNDS: &[&str] = &["Iterator", "ExactSizeIterator", "DoubleEndedIterator", "TrustedLen", "PolarsIterator"];
        let mut target: Option<Ty> = None;
        // record 0084: an iterator bound is lowered from a script vector
        let mut iterator: Option<Ty> = None;
        for b in bounds {
            if neutral.contains(&b.path.as_str()) || b.path.starts_with('\'') {
                continue;
            }
            let l = last(&b.path);
            if l.starts_with("Fn") {
                return Err(Unsupported("callback", t.render()));
            }
            if ITERATOR_BOUNDS.contains(&l) {
                match (&b.item, &iterator) {
                    (Some(item), None) if target.is_none() => { iterator = Some((**item).clone()); continue; }
                    (None, Some(_)) => continue, // `+ TrustedLen`, `+ ExactSizeIterator` beside the item-bearing bound
                    (None, None) if target.is_none() && bounds.iter().any(|o| ITERATOR_BOUNDS.contains(&last(&o.path)) && o.item.is_some()) => continue,
                    (None, None) => return Err(Unsupported("iterator without item", t.render())),
                    _ => return Err(Unsupported("extra bound", t.render())),
                }
            }
            if target.is_some() || iterator.is_some() {
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
                        // record 0084: `Into<(PlSmallStr, Field)>` and kin stay refused until a
                        // tuple-and-wrapper input mapping is proven on a real binding
                        Ty::Tuple(_) => return Err(Unsupported("tuple conversion", t.render())),
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
        if let Some(item) = iterator {
            return self.iterator_input(&item, name, generics, owner, depth, t);
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

    /// Record 0084: a generic iterator input. The script passes a vector;
    /// owned items travel through `Vec<Item>::into_iter()` (which is also
    /// `ExactSizeIterator` and `TrustedLen`); `&str`, `&[u8]` and their
    /// `Option` forms are borrowed from an owned temporary that the
    /// binding holds for the whole Polars call, so no Rust borrow reaches
    /// a script value.
    fn iterator_input(&self, item: &Ty, name: &str, generics: &BTreeMap<String, String>, owner: Option<&str>, depth: u8, t: &Ty) -> Result<Arg, Unsupported> {
        let u8_ = Ty::Path { path: "u8".into(), args: vec![] };
        let string = Ty::Path { path: "alloc::string::String".into(), args: vec![] };
        let vec_u8 = Ty::Path { path: "alloc::vec::Vec".into(), args: vec![u8_.clone()] };
        let opt = |x: Ty| Ty::Path { path: "core::option::Option".into(), args: vec![x] };
        let is_str = |x: &Ty| matches!(x, Ty::Ref { mutable: false, inner } if matches!(&**inner, Ty::Path { path, .. } if path == "str"));
        let is_bytes = |x: &Ty| matches!(x, Ty::Ref { mutable: false, inner } if matches!(&**inner, Ty::Slice(e) if **e == u8_));
        let inner_opt = |x: &Ty| match x { Ty::Path { path, args } if path == "core::option::Option" && args.len() == 1 => Some(args[0].clone()), _ => None };
        // (owned element type, how the temporary is borrowed per item)
        let (owned, borrow_map): (Ty, Option<&str>) = if is_str(item) {
            (string.clone(), Some("String::as_str"))
        } else if is_bytes(item) {
            (vec_u8.clone(), Some("Vec::as_slice"))
        } else if let Some(i) = inner_opt(item) {
            if is_str(&i) { (opt(string.clone()), Some("Option::as_deref")) }
            else if is_bytes(&i) { (opt(vec_u8.clone()), Some("|o| o.as_deref()")) }
            else if matches!(i, Ty::Ref { .. }) { return Err(Unsupported("iterator of borrowed items", t.render())); }
            else { (item.clone(), None) }
        } else if matches!(item, Ty::Ref { .. }) {
            return Err(Unsupported("iterator of borrowed items", t.render()));
        } else {
            (item.clone(), None)
        };
        let vec = Ty::Path { path: "alloc::vec::Vec".into(), args: vec![owned] };
        let mut a = self.arg(&vec, name, generics, owner, depth + 1)?;
        let hold = format!("__hold_{}", sanitize(name));
        match borrow_map {
            Some(map) => {
                a.pre.push(format!("let {hold} = {};", a.conv));
                a.conv = format!("{hold}.iter().map({map})");
                a.doc = format!("{} (iterated, items borrowed from the script's values for the call)", a.doc);
            }
            None => {
                a.conv = format!("({}).into_iter()", a.conv);
                a.doc = format!("{} (iterated)", a.doc);
            }
        }
        a.shape = format!("iterator({})", a.shape);
        Ok(a)
    }

    /// An iterator return, bare or wrapped in `Option`/`PolarsResult`:
    /// a vector of the element's mapping, materialized under the bound.
    fn ret_iterator(&self, t: &Ty, owner: Option<&str>, depth: u8) -> Result<Ret, Unsupported> {
        let Some((item, known, wrap)) = iterator_return(t) else { return Err(Unsupported("impl return", t.render())) };
        let elem = self.ret(&item, owner, depth + 1).map_err(|Unsupported(why, what)| Unsupported("iterator item", format!("{why}: {what}")))?;
        if elem.materialize.is_some() { return Err(Unsupported("iterator item", "nested iterator".into())); }
        let vec_ty = format!("Vec<{}>", elem.rust_ty);
        let limit = 1usize << 20;
        let (rust_ty, doc) = match wrap {
            IterWrap::Plain => (vec_ty, format!("vector of {} (materialized, at most {limit} items)", elem.doc)),
            IterWrap::Option => (format!("Option<{vec_ty}>"), format!("option of vector of {} (materialized, at most {limit} items)", elem.doc)),
            IterWrap::Result => (vec_ty, format!("result of vector of {} (materialized, at most {limit} items)", elem.doc)),
        };
        Ok(Ret { materialize: Some(Materialize { elem_conv: elem.conv, known, wrap }), rust_ty, conv: "__r".into(), fallible: true, doc })
    }

    fn ret(&self, t: &Ty, owner: Option<&str>, depth: u8) -> Result<Ret, Unsupported> {
        // record 0103: exactly the pair's borrowed typed-array iterator, driven into owned chunks
        if let Some((_, kind)) = self.iter_snapshot.borrow().clone() {
            if depth == 0 {
                let want = indexed_array(&kind).ok_or_else(|| Unsupported("iterator snapshot", format!("no array for {kind}")))?;
                // the parsed structure, not `render` (which drops an impl's item):
                // exactly one DoubleEndedIterator bound, no arguments, and an
                // item that is a shared borrow of exactly the pair's array
                let exact = matches!(t, Ty::Impl(bs) if bs.len() == 1
                    && bs[0].path == "core::iter::traits::double_ended::DoubleEndedIterator"
                    && bs[0].args.is_empty()
                    && matches!(bs[0].item.as_deref(), Some(Ty::Ref { mutable: false, inner }) if inner.render() == ty::parse(&want).render()));
                if !exact {
                    return Err(Unsupported("iterator snapshot", format!("{t:?} is not impl DoubleEndedIterator<Item = &{want}>")));
                }
                if let Some((_, k, _, elem, _, _)) = SCALAR_CHUNKS.iter().find(|(_, k, ..)| *k == kind) {
                    let copier = match *k { "bool" => "support::iter_snapshot_bool", "str" => "support::iter_snapshot_str", "binary" => "support::iter_snapshot_binview", _ => "support::iter_snapshot_binary_offset" };
                    return Ok(Ret { materialize: None, rust_ty: format!("Vec<Vec<Option<{elem}>>>"), conv: format!("{copier}(this.0.chunks(), __r, \"__OP__\")?"), fallible: true, doc: format!("vector of chunks in iterator order, each a vector of option of {k} values (copied, bounded with payload bytes)") });
                }
                let e = self.ret(&Ty::Path { path: kind.clone(), args: vec![] }, owner, depth + 1)?;
                if e.materialize.is_some() || e.rust_ty.starts_with("Vec") {
                    return Err(Unsupported("iterator snapshot", format!("element {kind} is not a scalar")));
                }
                return Ok(Ret { materialize: None, rust_ty: format!("Vec<Vec<Option<{}>>>", e.rust_ty), conv: format!("support::iter_snapshot::<{kind}, _>(this.0.chunks(), __r, \"__OP__\", |__r| Ok::<_, Error>({}))?", e.conv), fallible: true, doc: format!("vector of chunks in iterator order, each a vector of option of {} (copied, bounded)", e.doc) });
            }
        }
        // record 0102: exactly `&T::Array` for the pair, the one array copied
        if let Some((_, kind)) = self.array_snapshot.borrow().clone() {
            if depth == 0 {
                let want = indexed_array(&kind).ok_or_else(|| Unsupported("array snapshot", format!("no array for {kind}")))?;
                let exact = matches!(t, Ty::Ref { mutable: false, inner } if inner.render() == ty::parse(&want).render());
                if !exact {
                    return Err(Unsupported("array snapshot", format!("{} is not &{want}", t.render())));
                }
                if let Some((_, k, _, elem, _, _)) = SCALAR_CHUNKS.iter().find(|(_, k, ..)| *k == kind) {
                    let copier = match *k { "bool" => "support::array_snapshot_bool", "str" => "support::array_snapshot_str", "binary" => "support::array_snapshot_binview", _ => "support::array_snapshot_binary_offset" };
                    return Ok(Ret { materialize: None, rust_ty: format!("Vec<Option<{elem}>>"), conv: format!("{copier}(__r, \"__OP__\")?"), fallible: true, doc: format!("the single array as a vector of option of {k} values (copied, bounded with payload bytes)") });
                }
                let e = self.ret(&Ty::Path { path: kind.clone(), args: vec![] }, owner, depth + 1)?;
                if e.materialize.is_some() || e.rust_ty.starts_with("Vec") {
                    return Err(Unsupported("array snapshot", format!("element {kind} is not a scalar")));
                }
                return Ok(Ret { materialize: None, rust_ty: format!("Vec<Option<{}>>", e.rust_ty), conv: format!("support::array_snapshot::<{kind}, _>(__r, \"__OP__\", |__r| Ok::<_, Error>({}))?", e.conv), fallible: true, doc: format!("the single array as a vector of option of {} (copied, bounded)", e.doc) });
            }
        }
        // record 0101: exactly `Option<&T::Array>` for the pair, the one selected chunk copied
        if let Some((_, kind)) = self.indexed_chunk.borrow().clone() {
            if depth == 0 {
                let want = indexed_array(&kind).ok_or_else(|| Unsupported("indexed chunk snapshot", format!("no array for {kind}")))?;
                let exact = matches!(t, Ty::Path { path, args } if path == "core::option::Option" && args.len() == 1 && matches!(&args[0], Ty::Ref { mutable: false, inner } if inner.render() == ty::parse(&want).render()));
                if !exact {
                    return Err(Unsupported("indexed chunk snapshot", format!("{} is not Option<&{want}>", t.render())));
                }
                if let Some((_, k, _, elem, _, _)) = SCALAR_CHUNKS.iter().find(|(_, k, ..)| *k == kind) {
                    let copier = match *k { "bool" => "support::indexed_snapshot_bool", "str" => "support::indexed_snapshot_str", "binary" => "support::indexed_snapshot_binview", _ => "support::indexed_snapshot_binary_offset" };
                    return Ok(Ret { materialize: None, rust_ty: format!("Option<Vec<Option<{elem}>>>"), conv: format!("{copier}(__r, \"__OP__\")?"), fallible: true, doc: format!("option of the selected chunk as a vector of option of {k} values (copied, bounded with payload bytes)") });
                }
                let e = self.ret(&Ty::Path { path: kind.clone(), args: vec![] }, owner, depth + 1)?;
                if e.materialize.is_some() || e.rust_ty.starts_with("Vec") {
                    return Err(Unsupported("indexed chunk snapshot", format!("element {kind} is not a scalar")));
                }
                return Ok(Ret { materialize: None, rust_ty: format!("Option<Vec<Option<{}>>>", e.rust_ty), conv: format!("support::indexed_snapshot::<{kind}, _>(__r, \"__OP__\", |__r| Ok::<_, Error>({}))?", e.conv), fallible: true, doc: format!("option of the selected chunk as a vector of option of {} (copied, bounded)", e.doc) });
            }
        }
        // record 0099: the exact chunk list, copied per chunk, for the listed pair's native
        if let Some((_, native)) = self.chunk_snapshot.borrow().clone() {
            if depth == 0 {
                let arrays = ["polars_arrow::array::ArrayRef", "alloc::boxed::Box<dyn polars_arrow::array::Array>"];
                let is_chunks = matches!(t, Ty::Ref { mutable: false, inner } if matches!(&**inner, Ty::Path { path, args } if path == "alloc::vec::Vec" && args.len() == 1 && arrays.contains(&args[0].render().as_str())));
                if !is_chunks {
                    return Err(Unsupported("chunk snapshot", format!("{} is not &Vec<ArrayRef>", t.render())));
                }
                // record 0100: a Boolean, string or binary owner has its own copier
                if let Some((_, kind, copier, elem, _, _)) = SCALAR_CHUNKS.iter().find(|(_, k, ..)| *k == native) {
                    return Ok(Ret { materialize: None, rust_ty: format!("Vec<Vec<Option<{elem}>>>"), conv: format!("{copier}(__r, \"__OP__\")?"), fallible: true, doc: format!("vector of chunks, each a vector of option of {kind} values (copied, chunk boundaries kept, bounded with payload bytes)") });
                }
                let e = self.ret(&Ty::Path { path: native.clone(), args: vec![] }, owner, depth + 1)?;
                if e.materialize.is_some() || e.rust_ty.starts_with("Vec") {
                    return Err(Unsupported("chunk snapshot", format!("element {native} is not a scalar")));
                }
                return Ok(Ret { materialize: None, rust_ty: format!("Vec<Vec<Option<{}>>>", e.rust_ty), conv: format!("support::chunk_snapshot::<{native}, _>(__r, \"__OP__\", |__r| Ok::<_, Error>({}))?", e.conv), fallible: true, doc: format!("vector of chunks, each a vector of option of {} (copied, chunk boundaries kept, bounded)", e.doc) });
            }
        }
        // record 0096: the exact null-aware shape, for the listed pair's native
        if let Ty::Path { path, args } = t {
            if path == "either::Either" {
                if let Some((_, native)) = self.null_aware.borrow().clone() {
                    if depth > 0 {
                        return Err(Unsupported("null-aware return", format!("{} is nested in the return", t.render())));
                    }
                    let elem = Ty::Path { path: native.clone(), args: vec![] };
                    let vec_of = |inner: Ty| Ty::Path { path: "alloc::vec::Vec".into(), args: vec![inner] };
                    let want = [vec_of(elem.clone()), vec_of(Ty::Path { path: "core::option::Option".into(), args: vec![elem.clone()] })];
                    if args.len() != 2 || args[0].render() != want[0].render() || args[1].render() != want[1].render() {
                        return Err(Unsupported("null-aware return", format!("{} is not Either<Vec<{native}>, Vec<Option<{native}>>>", t.render())));
                    }
                    let e = self.ret(&elem, owner, depth + 1)?;
                    if e.materialize.is_some() || e.rust_ty.starts_with("Vec") {
                        return Err(Unsupported("null-aware return", format!("element {native} is not a scalar")));
                    }
                    let conv = format!("__r.either(|__v| __v.into_iter().map(|__r| Ok::<_, Error>(Some({c}))).collect::<Result<Vec<_>, Error>>(), |__v| __v.into_iter().map(|__r| Ok::<_, Error>(match __r {{ Some(__r) => Some({c}), None => None }})).collect::<Result<Vec<_>, Error>>())?", c = e.conv);
                    return Ok(Ret { materialize: None, rust_ty: format!("Vec<Option<{}>>", e.rust_ty), conv, fallible: true, doc: format!("vector of option of {} (both Polars branches; bounded before the call)", e.doc) });
                }
            }
        }
        // record 0087: a listed callable's concrete `Map` iterator of integers
        // (reached through the `ChunkLenIter` alias, one level down)
        {
            if let (Some(item), Ty::Path { path, .. }) = (self.iter_return.borrow().clone(), t) {
                if path == "core::iter::adapters::map::Map" {
                    let limit = 1usize << 20;
                    return Ok(Ret { materialize: Some(Materialize { elem_conv: format!("support::widen::<{item}>(__r, \"__OP__\")?"), known: IterLen::Exact, wrap: IterWrap::Plain }), rust_ty: "Vec<i64>".into(), conv: "__r".into(), fallible: true, doc: format!("vector of int (materialized, at most {limit} items, each checked into range)") });
                }
            }
        }
        if depth > 6 {
            return Err(Unsupported("nesting", t.render()));
        }
        let r = |rust_ty: &str, conv: String, doc: &str| Ok(Ret { materialize: None, rust_ty: rust_ty.to_string(), fallible: false, conv, doc: doc.to_string() });
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
                Ok(Ret { materialize: None, rust_ty: format!("({})", tys.join(", ")), fallible, conv: format!("{{ let __t = __r; ({}) }}", convs.join(", ")), doc: format!("tuple of {}", docs.join(", ")) })
            }
            Ty::Ref { inner, mutable } => {
                // record 0082: a mutable slice needs an audited write-back contract; not admitted
                if *mutable && matches!(&**inner, Ty::Slice(_)) { return Err(Unsupported("mutable slice", t.render())); }
                if let Ty::Path { path, .. } = &**inner {
                    let p = if path == "Self" { owner.unwrap_or("") } else { path.as_str() };
                    if self.wrappers.contains_key(p) && !self.clonable.contains(p) {
                        return Err(Unsupported("borrowed return of a non-Clone type", t.render()));
                    }
                }
                let x = self.ret(inner, owner, depth + 1)?;
                // `&str` converts through `to_string`, and a slice is copied by
                // `copy_slice` (record 0082): cloning the reference is a no-op
                if matches!(&**inner, Ty::Path { path, .. } if path == "str") || matches!(&**inner, Ty::Slice(_)) {
                    return Ok(x);
                }
                Ok(Ret { materialize: None, rust_ty: x.rust_ty, fallible: x.fallible, conv: format!("{{ let __r = (__r).clone(); {} }}", x.conv), doc: x.doc })
            }
            // record 0088: a listed callable's `Cow<Wrapped>` becomes owned inside the call
            Ty::Path { path, args } if path == "alloc::borrow::Cow" && args.len() == 1 && self.cow_ok.get() => {
                let wrapped = match &args[0] {
                    Ty::Path { path: p, args: a } if a.is_empty() => { let p = if p == "Self" { owner.unwrap_or("") } else { p.as_str() }; self.wrapper_for(p).is_some() && self.clonable.contains(p) }
                    Ty::Generic(g) if g == "Self" => owner.is_some_and(|o| self.wrapper_for(o).is_some() && self.clonable.contains(o)),
                    _ => false,
                };
                if !wrapped { return Err(Unsupported("cow of an unwrapped type", t.render())); }
                let x = self.ret(&args[0], owner, depth + 1)?;
                Ok(Ret { materialize: None, rust_ty: x.rust_ty, fallible: x.fallible, conv: format!("{{ let __r = __r.into_owned(); {} }}", x.conv), doc: format!("{} (owned)", x.doc) })
            }
            Ty::Path { path, args } => match path.as_str() {
                "bool" => r("bool", "__r".into(), "bool"),
                "i64" => r("i64", "__r".into(), "int"),
                "f64" => r("f64", "__r".into(), "float"),
                "f32" => r("f64", "(__r as f64)".into(), "float"),
                // record 0093: a source scalar whose range can exceed a script
                // integer converts with a range check (ConversionError, naming
                // the operation) wherever it is read back, never with `as i64`
                // record 0094: a listed categorical hash is an exact hex token
                "u64" if self.hash_token.borrow().as_ref().is_some_and(|h| h.1) => r("String", "support::hash_token(__r)".into(), "string (a hash token: 16 lowercase hex digits)"),
                "usize" if self.bounded_ok.get() => r("i64", "support::bounded_usize(__r)".into(), "int (a proven length or index)"),
                p if RISKY_INTS.contains(&p) => Ok(Ret { materialize: None, rust_ty: "i64".into(), fallible: true, conv: format!("support::widen::<{p}>(__r, \"__OP__\")?"), doc: "int (checked into range)".into() }),
                p if INT_NARROW.contains(&p) => r("i64", "(__r as i64)".into(), "int"),
                "polars_utils::index::IdxSize" => r("i64", "(__r as i64)".into(), "int"),
                "char" => r("String", "__r.to_string()".into(), "string"),
                "str" | "alloc::string::String" | "polars_utils::pl_str::PlSmallStr" => r("String", "__r.to_string()".into(), "string"),
                "Self" => self.ret(&Ty::Path { path: owner.ok_or_else(|| Unsupported("Self without owner", t.render()))?.to_string(), args: vec![] }, owner, depth + 1),
                "core::option::Option" if args.len() == 1 && matches!(args[0], Ty::Impl(_)) => self.ret_iterator(t, owner, depth),
                "core::option::Option" if args.len() == 1 => {
                    let x = self.ret(&args[0], owner, depth + 1)?;
                    Ok(Ret { materialize: None, rust_ty: format!("Option<{}>", x.rust_ty), fallible: x.fallible, conv: format!("match __r {{ Some(__r) => Some({}), None => None }}", x.conv), doc: format!("option of {}", x.doc) })
                }
                "alloc::vec::Vec" if args.len() == 1 => {
                    let x = self.ret(&args[0], owner, depth + 1)?;
                    Ok(Ret { materialize: None, rust_ty: format!("Vec<{}>", x.rust_ty), fallible: x.fallible, conv: format!("{{ let mut __v = Vec::new(); for __r in __r {{ __v.push({}); }} __v }}", x.conv), doc: format!("vector of {}", x.doc) })
                }
                "polars_error::PolarsResult" if args.len() == 1 && matches!(args[0], Ty::Impl(_)) => self.ret_iterator(t, owner, depth),
                "polars_error::PolarsResult" if args.len() == 1 => {
                    let x = self.ret(&args[0], owner, depth + 1)?;
                    Ok(Ret { materialize: None, rust_ty: x.rust_ty, fallible: true, conv: format!("{{ let __r = __r.map_err(Error::from)?; {} }}", x.conv), doc: format!("result of {}", x.doc) })
                }
                "core::result::Result" if args.len() == 2 && args[1] == Ty::Path { path: "polars_error::PolarsError".into(), args: vec![] } => {
                    let x = self.ret(&args[0], owner, depth + 1)?;
                    Ok(Ret { materialize: None, rust_ty: x.rust_ty, fallible: true, conv: format!("{{ let __r = __r.map_err(Error::from)?; {} }}", x.conv), doc: format!("result of {}", x.doc) })
                }
                "core::result::Result" => Err(Unsupported("result with foreign error", t.render())),
                _ => {
                    if !args.is_empty() {
                        if let Some(c) = self.by_identity.get(&t.render()) {
                            return self.ret(&Ty::Path { path: c.clone(), args: vec![] }, owner, depth + 1);
                        }
                    }
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
                    // record 0085: a validity bitmap, only for the release's listed paths
                    if path == "polars_arrow::bitmap::immutable::Bitmap" && args.is_empty() && self.bitmap_ok.get() {
                        let limit = 1usize << 20;
                        return Ok(Ret { materialize: None, rust_ty: "Vec<bool>".into(), fallible: true, conv: "support::copy_bits(&__r, \"__OP__\")?".into(), doc: format!("vector of bool (validity bits copied from a bitmap, at most {limit} bits per call)") });
                    }
                    if path.starts_with("polars") {
                        return Err(Unsupported("unreachable polars type", t.render()));
                    }
                    Err(Unsupported("foreign type", t.render()))
                }
            },
            Ty::Generic(g) if g == "Self" => self.ret(&Ty::Path { path: "Self".into(), args: vec![] }, owner, depth + 1),
            Ty::Generic(_) => Err(Unsupported("generic return", t.render())),
            Ty::Impl(_) => self.ret_iterator(t, owner, depth),
            // Record 0082: an immutable borrowed slice is copied into an
            // owned vector under the materialize bound (cumulative over the
            // binding's slices); the script owns the result outright.
            Ty::Slice(elem) => {
                let x = self.ret(elem, owner, depth + 1).map_err(|Unsupported(why, what)| Unsupported("slice element", format!("{why}: {what}")))?;
                if x.materialize.is_some() { return Err(Unsupported("slice element", "iterator".into())); }
                let limit = 1usize << 20;
                Ok(Ret { materialize: None, rust_ty: format!("Vec<{}>", x.rust_ty), fallible: true, conv: format!("support::copy_slice(__r, \"__OP__\", |__r| Ok::<_, Error>({}))?", x.conv), doc: format!("vector of {} (copied from a borrowed slice, at most {limit} elements)", x.doc) })
            }
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

/// Map a Polars callback argument into an owned Rune value. Slices are
/// copied as vectors of wrapped values; mutable slices are deliberately
/// one-way, under the release file's audited vector contract.
/// Record 0093: a fallible element conversion is allowed into a callback only
/// when it is one of the checked read-backs the typed unwind can report.
fn checked_callback_conv(conv: &str) -> bool {
    conv.contains("support::widen::<") || conv.contains("support::copy_slice(")
}
fn callback_input(world: &World, t: &Ty, var: &str, owner: Option<&str>) -> Result<String, Unsupported> {
    // a fallible collection element fails the callback (typed unwind) before the script runs
    let collect = |mapped: &Ret, what: &'static str| -> Result<String, Unsupported> {
        if mapped.materialize.is_some() { return Err(Unsupported(what, t.render())); }
        if !mapped.fallible { return Ok(format!("{var}.iter().map(|__r| {{ let __r = __r.clone(); {} }}).collect::<Vec<_>>()", mapped.conv)); }
        if !checked_callback_conv(&mapped.conv) { return Err(Unsupported(what, t.render())); }
        Ok(format!("match {var}.iter().map(|__r| {{ let __r = __r.clone(); Ok::<_, Error>({}) }}).collect::<Result<Vec<_>, Error>>() {{ Ok(__v) => __v, Err(__e) => support::callback::unwind(crate::engine::CallbackFailure {{ op: \"__OP__\".into(), cause: __e.1 }}) }}", mapped.conv))
    };
    match t {
        Ty::Ref { inner, .. } if matches!(&**inner, Ty::Slice(_)) => {
            let Ty::Slice(elem) = &**inner else { unreachable!() };
            let mapped = world.ret(elem, owner, 0)?;
            collect(&mapped, "callback slice element")
        }
        Ty::Path { path, args } if path == "alloc::vec::Vec" && args.len() == 1 => {
            let mapped = world.ret(&args[0], owner, 0)?;
            collect(&mapped, "callback vector element")
        }
        _ => {
            let mapped = world.ret(t, owner, 0)?;
            if mapped.materialize.is_some() { return Err(Unsupported("callback argument", t.render())); }
            if mapped.fallible {
                // record 0082: only a bounded slice copy is fallible here; its
                // refusal is the callback's typed failure, naming the operation
                if !checked_callback_conv(&mapped.conv) { return Err(Unsupported("callback argument", t.render())); }
                return Ok(format!("{{ let __r = {var}; match (|| Ok::<_, Error>({}))() {{ Ok(__v) => __v, Err(__e) => support::callback::unwind(crate::engine::CallbackFailure {{ op: \"__OP__\".into(), cause: __e.1 }}) }} }}", mapped.conv));
            }
            Ok(format!("{{ let __r = {var}; {} }}", mapped.conv))
        }
    }
}

fn callback_rust_type(world: &World, raw: &str, owner: Option<&str>) -> String {
    let mut result = raw.replace("Self", owner.unwrap_or("Self")).replace("alloc::string::String", "String").replace("alloc::vec::Vec", "Vec");
    let mut paths: Vec<_> = world.wrappers.iter().collect();
    paths.sort_by_key(|(path, _)| std::cmp::Reverse(path.len()));
    for (path, wrapper) in paths {
        result = result.replace(path, &wrapper.spell);
    }
    result
}

fn callback_arg(world: &World, c: &Callable, sig: &ClosureSig, name: &str, owner: Option<&str>) -> Result<Arg, Unsupported> {
    if sig.udf { return Err(Unsupported("callback Udf", sig.param.clone())); }
    let operation = if let Some(o) = owner { format!("{}::{}", last(o), c.name) } else { c.name.clone() };
    let audit = world.release.callback_mutable.iter().find(|m| m.path == c.canonical_path && m.param == sig.param);
    let copy_bound = c.params.iter().find(|p| sanitize(&p.name) == name).is_some_and(|p| p.ty_canonical.contains("Copy")) || c.generics_canonical.iter().any(|(key, bound)| key == &c.params.iter().find(|p| sanitize(&p.name) == name).map(|p| p.ty_canonical.clone()).unwrap_or_default() && bound.contains("Copy"));
    let mut params = Vec::new();
    let mut values = Vec::new();
    let mut buffer: Option<String> = None;
    for (i, raw) in sig.args.iter().enumerate() {
        let t = ty::parse(raw);
        let var = format!("__cb_a{i}");
        let rust_type = callback_rust_type(world, raw, owner);
        params.push(format!("{var}: {rust_type}"));
        if let Ty::Ref { mutable: true, inner } = &t {
            if matches!(&**inner, Ty::Path { path, .. } if path == "alloc::string::String") && audit.is_some_and(|a| a.contract == "result buffer") {
                buffer = Some(var);
                continue;
            }
            if !matches!(&**inner, Ty::Slice(_)) || !audit.is_some_and(|a| a.contract == "vector") {
                return Err(Unsupported("callback mutable contract", raw.clone()));
            }
        }
        values.push(callback_input(world, &t, &var, owner)?.replace("__OP__", &operation));
    }
    let args = if values.is_empty() { "()".to_string() } else { format!("({},)", values.join(", ")) };
    let result = ty::parse(&sig.ret);
    let (inner, polars_result) = match &result {
        Ty::Path { path, args } if path == "polars_error::PolarsResult" && args.len() == 1 => (&args[0], true),
        _ => (&result, false),
    };
    let result_ty = if buffer.is_some() { "String".to_string() } else {
        let mapping = world.arg(inner, "__cb_result", &BTreeMap::new(), owner, 0)?;
        let rune_type = mapping.rust_ty.strip_prefix("&mut ").or_else(|| mapping.rust_ty.strip_prefix('&')).unwrap_or(&mapping.rust_ty).to_string();
        rune_type
    };
    let bridge_ref = if copy_bound { format!("__cb_{name}_ref") } else { format!("&__cb_{name}") };
    let bridge = format!("support::callback::bridge::<_, {result_ty}>(\"{operation}\", {bridge_ref}, {args})");
    let converted = if let Some(buffer) = buffer {
        format!("{bridge}.map(|text| {{ *{buffer} = text; }})")
    } else if matches!(inner, Ty::Tuple(ts) if ts.is_empty()) {
        bridge
    } else {
        let mapping = world.arg(inner, "__cb_result", &BTreeMap::new(), owner, 0)?;
        let conv = mapping.conv;
        format!("{bridge}.and_then(|__cb_result| support::callback::convert(\"{operation}\", || Ok::<_, Error>({conv})))")
    };
    let delivered = if polars_result { format!("{converted}.map_err(support::callback::compute_error)") } else { format!("{converted}.unwrap_or_else(support::callback::unwind)") };
    let closure = format!("move |{}| {{ {delivered} }}", params.join(", "));
    let borrow = c.params.iter().find(|p| sanitize(&p.name) == name).map(|p| p.ty_canonical.as_str()).unwrap_or("");
    let (conv, declaration) = if borrow.starts_with("&mut ") {
        (format!("&mut __cb_callable_{name}"), Some(format!("let mut __cb_callable_{name} = {closure};")))
    } else if borrow.starts_with('&') {
        (format!("&__cb_callable_{name}"), Some(format!("let __cb_callable_{name} = {closure};")))
    } else { (closure, None) };
    let mut arg = ok_arg("rune::runtime::Function", conv, "callback")?;
    arg.fallible = true;
    arg.pre.push(format!("let __cb_{name} = support::callback::install(\"{operation}\", {name})?;"));
    if copy_bound { arg.pre.push(format!("let __cb_{name}_ref = __cb_{name}.as_ref();")); }
    if let Some(declaration) = declaration { arg.pre.push(declaration); }
    arg.shape = format!("callback:{}({})->{}", sig.kind, sig.args.join(","), sig.ret);
    Ok(arg)
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
    /// Record 0076: the emitted bindings of this callable, one per
    /// receiver route. The entry's status is callable coverage; each
    /// binding carries its own disposition and, when it has one, its case.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    bindings: Vec<Binding>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    exceptions: Vec<RouteException>,
    /// Record 0078: for a duplicate rustdoc listing of a `From` impl, the
    /// key of the retained listing (on the impl's `for` type) that binds it.
    #[serde(skip_serializing_if = "Option::is_none")]
    counterpart: Option<String>,
}

/// One emitted binding of a callable: its identity is the callable plus
/// the canonical receiver and the route that produced it.
#[derive(Clone, serde::Serialize)]
struct Binding {
    /// Unique across the surface: the callable's sanitized path, with
    /// `__on__<receiver>` for every receiver after the first.
    id: String,
    rune: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    receiver: Option<String>,
    /// `inherent`, `free`, `protocol`, `implementor` (a trait method on a
    /// wrapped implementor).
    route: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    route_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reentry: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    disposition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    case_id: Option<String>,
    /// The Rust callee for the oracle when the route changes it (deref).
    #[serde(skip)]
    callee: Option<String>,
    /// The oracle information of this binding when it differs from the
    /// entry's (an instantiation has its own substituted signature).
    #[serde(skip)]
    info: Option<OracleInfo>,
}

/// A candidate receiver route that produced no binding, with its reason:
/// every candidate route of a callable has exactly one disposition, a
/// binding or one of these.
#[derive(Clone, serde::Serialize)]
struct RouteException {
    route: &'static str,
    receiver: String,
    reason: String,
}

fn binding_id(canonical_path: &str, receiver: Option<&str>, first: bool) -> String {
    let base = sanitize(canonical_path).to_lowercase();
    match receiver {
        Some(r) if !first => format!("{base}__on__{}", sanitize(last(r)).to_lowercase()),
        _ => base,
    }
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
    /// The binding reaches the trait through `Deref`: the oracle's receiver is `&*recv`.
    deref: bool,
}

struct Emitted {
    /// Record 0078: the planned `from_<source>` name per `From` impl key.
    from_names: BTreeMap<String, String>,
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
        self.entries.push(Entry { key: c.key.clone(), canonical_path: c.canonical_path.clone(), kind: c.kind.clone(), bucket: c.bucket.clone(), status: "unsupported", fallible: None, signature: signature_of(c), execution: None, oracle: None, reason: Some(format!("{reason}: {detail}")), rune: None, note: None, bindings: vec![], exceptions: vec![], counterpart: None });
    }
    fn adapted(&mut self, c: &Callable, reason: &str, rune: &str) {
        self.entries.push(Entry { key: c.key.clone(), canonical_path: c.canonical_path.clone(), kind: c.kind.clone(), bucket: c.bucket.clone(), status: "adapted", fallible: None, signature: signature_of(c), execution: None, oracle: None, reason: Some(reason.into()), rune: Some(rune.into()), note: None, bindings: vec![], exceptions: vec![], counterpart: None });
    }
    fn generated(&mut self, c: &Callable, rune: &str, note: Option<String>) {
        self.entries.push(Entry { key: c.key.clone(), canonical_path: c.canonical_path.clone(), kind: c.kind.clone(), bucket: c.bucket.clone(), status: "generated", fallible: None, signature: signature_of(c), execution: None, oracle: None, reason: None, rune: Some(rune.into()), note, bindings: vec![Binding { id: binding_id(&c.canonical_path, None, true), rune: rune.into(), receiver: None, route: "inherent", route_reason: None, reentry: None, disposition: None, case_id: None, callee: None, info: None }], exceptions: vec![], counterpart: None });
    }
    fn generated_with(&mut self, c: &Callable, rune: &str, note: Option<String>, info: OracleInfo) {
        let fallible = info.fallible;
        let route = match c.kind.as_str() { "inherent" => "inherent", "free_fn" => "free", "foreign_trait_impl" => "protocol", _ => "implementor" };
        let receiver = info.owner.as_ref().map(|(o, _)| o.clone());
        let bindings = vec![Binding { id: binding_id(&c.canonical_path, receiver.as_deref(), true), rune: rune.into(), receiver, route, route_reason: None, reentry: None, disposition: None, case_id: None, callee: None, info: None }];
        self.entries.push(Entry { key: c.key.clone(), canonical_path: c.canonical_path.clone(), kind: c.kind.clone(), bucket: c.bucket.clone(), status: "generated", fallible: Some(fallible), signature: signature_of(c), execution: None, oracle: Some(info), reason: None, rune: Some(rune.into()), note, bindings, exceptions: vec![], counterpart: None });
    }
    /// A trait method bound on several implementors: one binding per
    /// implementor, the entry's status counting the callable once.
    fn generated_on(&mut self, c: &Callable, per: &[(String, String, &'static str, Option<String>)], info: OracleInfo) {
        let fallible = info.fallible;
        let bindings: Vec<Binding> = per.iter().enumerate().map(|(i, (rune, owner, route, callee))| Binding { id: binding_id(&c.canonical_path, Some(owner), i == 0), rune: rune.clone(), receiver: Some(owner.clone()), route, route_reason: None, reentry: None, disposition: None, case_id: None, callee: callee.clone(), info: None }).collect();
        let rune = per.iter().map(|(r, _, _, _)| r.as_str()).collect::<Vec<_>>().join(" ");
        self.entries.push(Entry { key: c.key.clone(), canonical_path: c.canonical_path.clone(), kind: c.kind.clone(), bucket: c.bucket.clone(), status: "generated", fallible: Some(fallible), signature: signature_of(c), execution: None, oracle: Some(info), reason: None, rune: Some(rune), note: None, bindings, exceptions: vec![], counterpart: None });
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
        // record 0084: a generic that only appears in another generic's bound
        // (`I: IntoIterator<Item = S>`, `E: AsRef<[IE]>`) is inferred with it;
        // whether that chain has a script mapping is decided by `bounds`
        let used = c.params.iter().any(|p| mentions(&p.ty_canonical, g))
            || c.generics_canonical.iter().any(|(h, b)| h != g && mentions(b, g));
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

/// Record 0076 gate 4: one binding per proven (method, alias) pair of a
/// generic owner, with the owner's parameters substituted; each binding
/// has its own oracle information. Pairs outside the release's
/// instantiation scope, and proven pairs the mapping rules refuse, are
/// route exceptions with their reason.
fn emit_instantiations(world: &World, out: &mut Emitted, c: &Callable, pairs: &[&PairRecord]) {
    if let Err(reason) = callback_gate(world, c) {
        out.unsupported(c, "callback audit", &reason);
        return;
    }
    let scope = &world.release.instantiation.families;
    let mut done: Vec<(String, String, &'static str, Option<String>)> = Vec::new();
    let mut infos: Vec<OracleInfo> = Vec::new();
    let mut exceptions: Vec<RouteException> = Vec::new();
    let mut first_info: Option<OracleInfo> = None;
    for p in pairs {
        match &p.result {
            Applicability::Proven => {}
            Applicability::Rejected(r) => { exceptions.push(RouteException { route: "instantiation", receiver: p.alias.clone(), reason: format!("rejected: {r}") }); continue; }
            Applicability::Unresolved(r) => { exceptions.push(RouteException { route: "instantiation", receiver: p.alias.clone(), reason: format!("unresolved: {r}") }); continue; }
        }
        if !scope.is_empty() && !scope.iter().any(|f| f == p.family) {
            exceptions.push(RouteException { route: "instantiation", receiver: p.alias.clone(), reason: format!("not shipped: family `{}` is outside the release's instantiation scope", p.family) });
            continue;
        }
        if let Some(x) = world.release.instantiation.exclude.iter().find(|x| last(&p.alias) == x.alias && x.methods.iter().any(|m| *m == c.name)) {
            exceptions.push(RouteException { route: "instantiation", receiver: p.alias.clone(), reason: format!("excluded by the release file: {}", x.cite) });
            continue;
        }
        let (_, subst) = world.applicability(c, &p.identity);
        let Ok((params, ret)) = world.substitute_signature(c, &p.identity, &subst) else { continue };
        // an instantiation an alias wrapper holds exactly is spelled as that
        // alias everywhere downstream (mapping, oracle formatting)
        let norm = |t: String| -> String {
            let mut t = t;
            for (identity, alias) in &world.by_identity {
                if t.contains(identity.as_str()) { t = t.replace(identity.as_str(), alias); }
            }
            t
        };
        let params: Vec<String> = params.into_iter().map(norm).collect();
        let ret = ret.map(norm);
        let mut syn = c.clone();
        syn.owner = p.alias.clone();
        syn.bucket = "mechanical".into();
        for (q, t) in syn.params.iter_mut().zip(params) {
            q.ty_canonical = t;
        }
        // record 0092: a listed scalar function generic becomes this pair's native
        if let Some(m) = world.release.method_scalar_generics.iter().find(|m| m.key == c.key && m.path == c.canonical_path) {
            if let Err(why) = m.check(c) {
                exceptions.push(RouteException { route: "instantiation", receiver: p.alias.clone(), reason: format!("refused: method scalar generic: {why}") });
                continue;
            }
            let Some(native) = m.native_for(&p.identity) else {
                exceptions.push(RouteException { route: "instantiation", receiver: p.alias.clone(), reason: format!("refused: method scalar generic: `{}` is not a listed type", p.identity) });
                continue;
            };
            // the family substitution has already spelled `N` as its bound, so the
            // parameters that are exactly `N` in the original signature are
            // bound by position (`check` guarantees `N` appears nowhere else)
            for (q, orig) in syn.params.iter_mut().zip(&c.params) {
                if orig.ty_canonical.trim() == m.generic { q.ty_canonical = native.to_string(); }
            }
            if syn.params.iter().any(|q| mentions(&q.ty_canonical, &m.generic) || q.ty_canonical.contains("NumCast")) {
                exceptions.push(RouteException { route: "instantiation", receiver: p.alias.clone(), reason: format!("refused: method scalar generic: `{}` remains", m.generic) });
                continue;
            }
        }
        // record 0096: a listed null-aware return, for this pair's native only
        let null_aware = match world.release.null_aware_returns.iter().find(|n| n.key == c.key && n.path == c.canonical_path) {
            None => None,
            Some(n) => {
                if let Err(why) = n.check() {
                    exceptions.push(RouteException { route: "instantiation", receiver: p.alias.clone(), reason: format!("refused: null-aware return: {why}") });
                    continue;
                }
                match n.native_for(&p.identity) {
                    Some(native) if !null_aware_return_matches(ret.as_deref(), native) => {
                        exceptions.push(RouteException { route: "instantiation", receiver: p.alias.clone(), reason: format!("refused: null-aware return: `{}` is not Either<Vec<{native}>, Vec<Option<{native}>>>", ret.as_deref().unwrap_or("()")) });
                        continue;
                    }
                    Some(native) => Some((c.name.clone(), native.to_string())),
                    None => {
                        exceptions.push(RouteException { route: "instantiation", receiver: p.alias.clone(), reason: format!("refused: null-aware return: `{}` is not a listed type", p.identity) });
                        continue;
                    }
                }
            }
        };
        syn.ret_canonical = ret;
        syn.impl_head = None;
        syn.impl_bounds.clear();
        syn.impl_where.clear();
        syn.generics_canonical.clear(); // closure bounds are now in the substituted parameter types
        // record 0099: a listed chunk snapshot, for this pair's native only
        let chunk_snapshot = match chunk_snapshot_entry(&world.release, c) {
            None => None,
            Some(Err(why)) => {
                exceptions.push(RouteException { route: "instantiation", receiver: p.alias.clone(), reason: format!("refused: chunk snapshot: {why}") });
                continue;
            }
            Some(Ok(e)) => match e.native_for(&p.identity) {
                Some(n) => Some((c.name.clone(), n.to_string())),
                None => {
                    exceptions.push(RouteException { route: "instantiation", receiver: p.alias.clone(), reason: format!("refused: chunk snapshot: `{}` is not a listed pair", p.identity) });
                    continue;
                }
            },
        };
        // record 0101: a listed indexed chunk snapshot, for this pair's kind only
        let indexed_chunk = match indexed_chunk_entry(&world.release, c) {
            None => None,
            Some(Err(why)) => {
                exceptions.push(RouteException { route: "instantiation", receiver: p.alias.clone(), reason: format!("refused: indexed chunk snapshot: {why}") });
                continue;
            }
            Some(Ok(e)) => match e.native_for(&p.identity) {
                Some(n) => Some((c.name.clone(), n.to_string())),
                None => {
                    exceptions.push(RouteException { route: "instantiation", receiver: p.alias.clone(), reason: format!("refused: indexed chunk snapshot: `{}` is not a listed pair", p.identity) });
                    continue;
                }
            },
        };
        // record 0103: a listed iterator snapshot, for this pair's kind only
        let iter_snapshot = match iter_snapshot_entry(&world.release, c) {
            None => None,
            Some(Err(why)) => {
                exceptions.push(RouteException { route: "instantiation", receiver: p.alias.clone(), reason: format!("refused: iterator snapshot: {why}") });
                continue;
            }
            Some(Ok(e)) => match e.native_for(&p.identity) {
                Some(n) => Some((c.name.clone(), n.to_string())),
                None => {
                    exceptions.push(RouteException { route: "instantiation", receiver: p.alias.clone(), reason: format!("refused: iterator snapshot: `{}` is not a listed pair", p.identity) });
                    continue;
                }
            },
        };
        // record 0102: a listed array snapshot, for this pair's kind only
        let array_snapshot = match array_snapshot_entry(&world.release, c) {
            None => None,
            Some(Err(why)) => {
                exceptions.push(RouteException { route: "instantiation", receiver: p.alias.clone(), reason: format!("refused: array snapshot: {why}") });
                continue;
            }
            Some(Ok(e)) => match e.native_for(&p.identity) {
                Some(n) => Some((c.name.clone(), n.to_string())),
                None => {
                    exceptions.push(RouteException { route: "instantiation", receiver: p.alias.clone(), reason: format!("refused: array snapshot: `{}` is not a listed pair", p.identity) });
                    continue;
                }
            },
        };
        // record 0097: a listed `Self: Sized` method (re-checked on the original signature)
        let sized_self = match sized_self_entry(&world.release, c) {
            None => None,
            Some(Err(why)) => {
                exceptions.push(RouteException { route: "instantiation", receiver: p.alias.clone(), reason: format!("refused: sized-self method: {why}") });
                continue;
            }
            Some(Ok(_)) => {
                // `Self` is the method's generic (bound `Sized`), and the family
                // substitution spelled it as that bound: on a concrete pair it is
                // the receiver itself, and nothing else may mention it
                if syn.params.iter().any(|q| q.ty_canonical.contains("Sized")) {
                    exceptions.push(RouteException { route: "instantiation", receiver: p.alias.clone(), reason: "refused: sized-self method: `Sized` remains in a parameter".into() });
                    continue;
                }
                syn.ret_canonical = Some(p.alias.clone());
                Some(c.name.clone())
            }
        };
        let before = out.entries.len();
        *world.null_aware.borrow_mut() = null_aware;
        *world.sized_self.borrow_mut() = sized_self;
        *world.chunk_snapshot.borrow_mut() = chunk_snapshot;
        *world.indexed_chunk.borrow_mut() = indexed_chunk;
        *world.array_snapshot.borrow_mut() = array_snapshot;
        *world.iter_snapshot.borrow_mut() = iter_snapshot;
        emit_method(world, out, &syn, &p.alias, None, false);
        *world.iter_snapshot.borrow_mut() = None;
        *world.indexed_chunk.borrow_mut() = None;
        *world.array_snapshot.borrow_mut() = None;
        *world.null_aware.borrow_mut() = None;
        *world.sized_self.borrow_mut() = None;
        *world.chunk_snapshot.borrow_mut() = None;
        let e = out.entries.pop().unwrap();
        debug_assert_eq!(before, out.entries.len());
        if e.status == "generated" {
            done.push((e.rune.clone().unwrap(), p.alias.clone(), "instantiation", None));
            if let Some(i) = e.oracle.clone() {
                if first_info.is_none() { first_info = Some(i.clone()); }
                infos.push(i);
            }
        } else {
            exceptions.push(RouteException { route: "instantiation", receiver: p.alias.clone(), reason: format!("{}: {}", e.status, e.reason.unwrap_or_default()) });
        }
    }
    if done.is_empty() {
        let why = if pairs.iter().any(|p| matches!(p.result, Applicability::Proven)) { "generic (proven instantiations were not emitted; see exceptions)" } else { "generic (no proven instantiation)" };
        out.unsupported(c, "bucket", why);
        out.entries.last_mut().unwrap().exceptions = exceptions;
        return;
    }
    let info = first_info.unwrap();
    out.generated_on(c, &done, info);
    let e = out.entries.last_mut().unwrap();
    let reason = binding_route_reason(world, c, true);
    for binding in &mut e.bindings { binding.route_reason = reason.clone(); binding.reentry = Some(if e.fallible == Some(true) { "error" } else { "unwind" }.into()); }
    for (b, i) in e.bindings.iter_mut().zip(infos) {
        b.info = Some(i);
        // one canonical path can carry several callables (one per impl
        // head): every instantiation id names its receiver
        b.id = binding_id(&c.canonical_path, b.receiver.as_deref(), false);
    }
    e.note = Some(format!("instantiated on {} of {} alias identities", done.len(), pairs.len()));
    e.exceptions = exceptions;
}

/// The call expression of a binding whose return is materialized: the
/// iterator is created, driven under the bound and its elements
/// converted to owned values inside one block, which is the routed
/// closure when the binding is routed; only the owned vector leaves it.
fn materialized_call(m: &Materialize, callee_call: &str, name: &str, route: bool) -> String {
    let helper = match m.known { IterLen::Exact => "support::materialize_exact", IterLen::Trusted => "support::materialize_trusted", IterLen::Unknown => "support::materialize_unknown" };
    let conv = format!("|__r| Ok::<_, Error>({})", m.elem_conv.replace("__OP__", name));
    let inner = match m.wrap {
        IterWrap::Plain => format!("{{ let __it = {callee_call}; {helper}(__it, \"{name}\", {conv}) }}"),
        IterWrap::Result => format!("{{ let __it = {callee_call}.map_err(Error::from)?; {helper}(__it, \"{name}\", {conv}) }}"),
        IterWrap::Option => format!("(|| Ok::<_, Error>(match {callee_call} {{ Some(__it) => Some({helper}(__it, \"{name}\", {conv})?), None => None }}))()"),
    };
    // record 0082: slice items count against one cumulative bound for the whole iterator
    let inner = if m.elem_conv.contains("support::copy_slice(") || m.elem_conv.contains("support::copy_bits(") { format!("{{ let __slices = support::SliceBudget::enter(); {inner} }}") } else { inner };
    if route {
        format!("crate::engine::run(\"{name}\", move || {inner}).map_err(Error::engine)??")
    } else {
        format!("({inner})?")
    }
}

/// Record 0084: the `generic_fn` bucket token admits a `generic`-bucket
/// callable only when it carries function-level generics, so the chained
/// and iterator rules reach their targets without enabling the whole
/// bucket (whose other members are generic for unrelated reasons).
fn bucket_admitted(buckets: &[&str], c: &Callable) -> bool {
    buckets.contains(&c.bucket.as_str()) || (c.bucket == "generic" && !c.generics_canonical.is_empty() && buckets.contains(&"generic_fn"))
}

/// Generate one method/function binding. `owner` is the canonical owner
/// type (for methods) and `trait_spell` the trait for UFCS calls.
fn emit_callable(world: &World, out: &mut Emitted, c: &Callable, buckets: &[&str]) {
    #[allow(clippy::type_complexity)]
    struct BitmapScope<'a>(&'a std::cell::Cell<bool>, &'a std::cell::RefCell<Option<(String, String)>>, &'a std::cell::RefCell<Option<String>>, &'a std::cell::Cell<bool>, &'a std::cell::Cell<bool>, &'a std::cell::RefCell<Option<(String, bool, Option<String>)>>);
    impl Drop for BitmapScope<'_> { fn drop(&mut self) { self.0.set(false); *self.1.borrow_mut() = None; *self.2.borrow_mut() = None; self.3.set(false); self.4.set(false); *self.5.borrow_mut() = None; } }
    world.cow_ok.set(world.release.cow_returns.iter().any(|r| r.key == c.key && r.path == c.canonical_path));
    world.bounded_ok.set(world.release.bounded_readbacks.iter().any(|r| r.key == c.key && r.path == c.canonical_path && !r.cite.trim().is_empty()));
    world.bitmap_ok.set(world.release.bitmap_returns.iter().any(|p| *p == c.canonical_path));
    *world.bitmap_input.borrow_mut() = world.release.bitmap_inputs.iter().find(|b| b.path == c.canonical_path).map(|b| (format!("{}::{}", last(&c.owner), c.name), b.length.clone()));
    *world.iter_return.borrow_mut() = world.release.iterator_returns.iter().find(|r| r.path == c.canonical_path).map(|r| r.item.clone());
    let _bitmap_scope = BitmapScope(&world.bitmap_ok, &world.bitmap_input, &world.iter_return, &world.cow_ok, &world.bounded_ok, &world.hash_token);
    match hash_token_scope(&world.release, c) {
        Ok(scope) => *world.hash_token.borrow_mut() = scope.map(|(ret, param)| (c.name.clone(), ret, param)),
        Err(reason) => {
            out.unsupported(c, "release policy", &reason);
            return;
        }
    }
    if let Err(reason) = callback_gate(world, c) {
        out.unsupported(c, "callback audit", &reason);
        return;
    }
    if !bucket_admitted(buckets, c) {
        out.unsupported(c, "bucket", c.bucket.clone().as_str());
        return;
    }
    match c.kind.as_str() {
        "inherent" => emit_method(world, out, c, &c.owner, None, false),
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
            let mut done: Vec<(String, String, &'static str, Option<String>)> = Vec::new();
            let mut first_err: Option<(String, String)> = None;
            let mut first_info: Option<OracleInfo> = None;
            let mut infos: Vec<OracleInfo> = Vec::new();
            let mut exceptions: Vec<RouteException> = Vec::new();
            let derefs: Vec<&String> = world.deref_targets.get(trait_).map(|v| v.iter().collect()).unwrap_or_default();
            let candidates: Vec<(&String, bool)> = impls.iter().map(|o| (*o, false)).chain(derefs.iter().map(|o| (*o, true))).collect();
            for (owner, deref) in candidates {
                let route: &'static str = if deref { "deref" } else { "implementor" };
                let before = out.entries.len();
                emit_method(world, out, c, owner, Some(&tspell), deref);
                let e = out.entries.pop().unwrap();
                debug_assert_eq!(before, out.entries.len());
                if e.status == "generated" {
                    let callee = if deref { e.oracle.as_ref().map(|i| i.callee.clone()) } else { None };
                    done.push((e.rune.clone().unwrap(), owner.clone(), route, callee));
                    if let Some(i) = e.oracle.clone() {
                        infos.push(i);
                    }
                    if first_info.is_none() {
                        first_info = e.oracle.clone();
                    }
                } else {
                    let why = e.reason.clone().unwrap_or_default();
                    let why = if deref && why.starts_with("name taken on this type by") { format!("not separately exposed, inherent binding retained ({why})") } else { why };
                    exceptions.push(RouteException { route, receiver: owner.clone(), reason: why.clone() });
                    if first_err.is_none() {
                        first_err = Some((e.status.to_string(), why));
                    }
                }
            }
            if done.is_empty() {
                let (st, why) = first_err.unwrap();
                if st == "adapted" { out.adapted(c, &why, "") } else { out.unsupported(c, "on every implementor", &why) }
                out.entries.last_mut().unwrap().exceptions = exceptions;
            } else if let Some(mut info) = first_info {
                // every generated receiver is a binding with its own case
                info.implementors = infos.iter().filter_map(|i| i.owner.clone()).collect();
                out.generated_on(c, &done, info);
                let entry = out.entries.last_mut().unwrap();
                for binding in &mut entry.bindings {
                    let route = routed_binding(world, c, binding.receiver.as_deref());
                    binding.route_reason = binding_route_reason(world, c, route);
                    if route { binding.reentry = Some(if entry.fallible == Some(true) { "error" } else { "unwind" }.into()); }
                }
                entry.exceptions = exceptions;
            } else {
                out.generated(c, &done.iter().map(|(r, _, _, _)| r.as_str()).collect::<Vec<_>>().join(" "), None);
            }
        }
        "free_fn" if world.release.free_instantiations.iter().any(|f| f.key == c.key && f.path == c.canonical_path) => emit_free_instantiations(world, out, c),
        "free_fn" => emit_free(world, out, c),
        "foreign_trait_impl" => emit_foreign(world, out, c),
        _ => out.unsupported(c, "kind", c.kind.clone().as_str()),
    }
}

fn emit_method(world: &World, out: &mut Emitted, c: &Callable, owner: &str, trait_spell: Option<&str>, deref: bool) {
    emit_method_with(world, out, c, owner, trait_spell, deref, None)
}

/// `emit_method` with an explicit Rust callee (record 0078: a `From`
/// constructor calls `<T as From<X>>::from`, never a method named after
/// the binding).
fn emit_method_with(world: &World, out: &mut Emitted, c: &Callable, owner: &str, trait_spell: Option<&str>, deref: bool, callee_override: Option<&str>) {
    world.tmp.set(0);
    // record 0085: instantiated pairs reach here without `emit_callable`
    #[allow(clippy::type_complexity)]
    struct BitmapScope<'a>(&'a std::cell::Cell<bool>, &'a std::cell::RefCell<Option<(String, String)>>, &'a std::cell::RefCell<Option<String>>, &'a std::cell::Cell<bool>, &'a std::cell::Cell<bool>, &'a std::cell::RefCell<Option<(String, bool, Option<String>)>>);
    impl Drop for BitmapScope<'_> { fn drop(&mut self) { self.0.set(false); *self.1.borrow_mut() = None; *self.2.borrow_mut() = None; self.3.set(false); self.4.set(false); *self.5.borrow_mut() = None; } }
    world.cow_ok.set(world.release.cow_returns.iter().any(|r| r.key == c.key && r.path == c.canonical_path));
    world.bounded_ok.set(world.release.bounded_readbacks.iter().any(|r| r.key == c.key && r.path == c.canonical_path && !r.cite.trim().is_empty()));
    world.bitmap_ok.set(world.release.bitmap_returns.iter().any(|p| *p == c.canonical_path));
    *world.bitmap_input.borrow_mut() = world.release.bitmap_inputs.iter().find(|b| b.path == c.canonical_path).map(|b| (format!("{}::{}", last(&c.owner), c.name), b.length.clone()));
    *world.iter_return.borrow_mut() = world.release.iterator_returns.iter().find(|r| r.path == c.canonical_path).map(|r| r.item.clone());
    let _bitmap_scope = BitmapScope(&world.bitmap_ok, &world.bitmap_input, &world.iter_return, &world.cow_ok, &world.bounded_ok, &world.hash_token);
    match hash_token_scope(&world.release, c) {
        Ok(scope) => *world.hash_token.borrow_mut() = scope.map(|(ret, param)| (c.name.clone(), ret, param)),
        Err(reason) => {
            out.unsupported(c, "release policy", &reason);
            return;
        }
    }
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
    if let Some(r) = world.release.refused.iter().find(|r| r.path == c.canonical_path) {
        out.unsupported(c, "release policy", &format!("{} ({})", r.reason, r.cite));
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
        let mapped = match closure_signature(c, p) {
            Some(sig) => callback_arg(world, c, &sig, &sanitize(&p.name), Some(owner)),
            None => world.arg(&ty::parse(&p.ty_canonical), &sanitize(&p.name), &generics, Some(owner), 0),
        };
        match mapped {
            Ok(a) => params.push((sanitize(&p.name), a)),
            Err(Unsupported(why, what)) => {
                out.unsupported(c, why, &format!("{} ({what})", p.name));
                return;
            }
        }
    }
    let ret = match c.ret_canonical.as_deref() {
        None => Ret { materialize: None, rust_ty: "()".into(), fallible: false, conv: "__r".into(), doc: "unit".into() },
        Some(rc) => {
            let t = ty::parse(rc);
            // `&mut Self` chains return unit: the receiver was mutated in place
            if matches!(&t, Ty::Ref { mutable: true, .. }) {
                Ret { materialize: None, rust_ty: "()".into(), fallible: false, conv: "{ let _ = __r; }".into(), doc: "unit (receiver mutated in place)".into() }
            } else if let Ty::Path { path, args } = &t {
                if (path == "polars_error::PolarsResult" || path == "core::result::Result") && matches!(args.first(), Some(Ty::Ref { mutable: true, .. })) {
                    Ret { materialize: None, rust_ty: "()".into(), fallible: true, conv: "{ let _ = __r.map_err(Error::from)?; }".into(), doc: "result of unit (receiver mutated in place)".into() }
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
    if ret.materialize.is_some() && c.receiver == "&mut self" {
        out.unsupported(c, "mutable iterator receiver", "the receiver's state after a partial materialization cannot be expressed");
        return;
    }
    // the deref route: the receiver is `&*this.0`, a `&dyn Trait`; it needs
    // a reference receiver, `DerefMut` for `&mut self`, and cannot move out
    if deref {
        match c.receiver.as_str() {
            "&self" => {}
            "&mut self" if world.deref_mut.contains(owner) => {}
            "&mut self" => { out.unsupported(c, "deref route needs DerefMut", owner); return; }
            "self" => { out.unsupported(c, "deref route cannot move out of a dyn target", owner); return; }
            other => { out.unsupported(c, "deref route needs a receiver", other); return; }
        }
    }
    let (recv_sig, mut recv_expr, recv_note) = match c.receiver.as_str() {
        "none" => ("".to_string(), None, None),
        "self" if !world.clonable.contains(owner) => {
            out.unsupported(c, "receiver consumes a non-Clone type", owner);
            return;
        }
        "self" => (format!("this: &{}", w.rust), Some("this.0.clone()".to_string()), Some("consumes in Rust; the Rune value is cloned and stays usable")),
        "&self" if deref => (format!("this: &{}", w.rust), Some("&*this.0".to_string()), Some("through Deref, as Rust's autoderef would")),
        "&mut self" if deref => (format!("this: &mut {}", w.rust), Some("&mut *this.0".to_string()), Some("through DerefMut; mutates the Rune value in place")),
        "&self" => (format!("this: &{}", w.rust), Some("&this.0".to_string()), None),
        "&mut self" => (format!("this: &mut {}", w.rust), Some("&mut this.0".to_string()), Some("mutates the Rune value in place")),
        other => {
            out.unsupported(c, "receiver", other);
            return;
        }
    };
    let commit_receiver = c.receiver == "&mut self" && matches!(c.name.as_str(), "apply_mut" | "apply_in_place") && c.params.iter().any(|p| closure_signature(c, p).is_some());
    if commit_receiver { recv_expr = Some("&mut __work".into()); }
    let idx = out.fn_index;
    out.fn_index += 1;
    // trait methods are emitted once per implementor: the owner is part of the identity
    let ident = rust_ident("f", &format!("{}#{owner}{}", c.canonical_path, if deref { "#deref" } else { "" }), idx);
    let callee = match (callee_override, trait_spell, deref) {
        (Some(o), _, _) => o.to_string(),
        (None, Some(t), true) => format!("<dyn {t}>::{rust_name}"),
        (None, Some(t), false) => format!("<{} as {t}>::{rust_name}", w.spell),
        (None, None, _) => format!("<{}>::{rust_name}", w.spell),
    };
    let mut args: Vec<String> = Vec::new();
    if let Some(r) = &recv_expr {
        args.push(r.clone());
    }
    args.extend(params.iter().map(|(_, a)| a.conv.clone()));
    let sig: Vec<String> = std::iter::once(recv_sig).filter(|s| !s.is_empty()).chain(params.iter().map(|(n, a)| format!("{n}: {}", a.rust_ty))).collect();
    let route = routed_binding(world, c, Some(owner));
    let fallible = fallible || params.iter().any(|(_, a)| a.pre.iter().any(|p| p.contains('?')));
    let mut pre: String = params.iter().filter(|(_, a)| a.shape.starts_with("callback:")).chain(params.iter().filter(|(_, a)| !a.shape.starts_with("callback:"))).flat_map(|(_, a)| a.pre.iter()).map(|p| format!("{p} ")).collect();
    if commit_receiver { pre.push_str("let mut __work = this.0.clone(); "); }
    // record 0103: likewise for the borrowed typed-chunk iterator
    if world.iter_snapshot.borrow().is_some() && (routed_binding(world, c, Some(owner)) || !ret.conv.starts_with("support::iter_snapshot") || c.receiver != "&self") {
        out.unsupported(c, "iterator snapshot", "needs an unrouted `&self` call and the iterator-snapshot conversion");
        return;
    }
    // record 0102: likewise for the one borrowed array of `downcast_as_array`
    if world.array_snapshot.borrow().is_some() && (routed_binding(world, c, Some(owner)) || !ret.conv.starts_with("support::array_snapshot") || c.receiver != "&self") {
        out.unsupported(c, "array snapshot", "needs an unrouted `&self` call and the array-snapshot conversion");
        return;
    }
    // record 0101: likewise for the one borrowed chunk of `downcast_get`
    if world.indexed_chunk.borrow().is_some() && (routed_binding(world, c, Some(owner)) || !ret.conv.starts_with("support::indexed_snapshot") || c.receiver != "&self") {
        out.unsupported(c, "indexed chunk snapshot", "needs an unrouted `&self` call and the indexed-snapshot conversion");
        return;
    }
    // record 0099: the chunk borrow ends with the call, so the copy must happen in it
    if world.chunk_snapshot.borrow().is_some() && (routed_binding(world, c, Some(owner)) || !ret.conv.starts_with("support::chunk_snapshot") || c.receiver != "&self") {
        out.unsupported(c, "chunk snapshot", "needs an unrouted `&self` call and the chunk-snapshot conversion");
        return;
    }
    // record 0097: Polars's signed slice offsets need the receiver length within i64
    let sized_self_op = world.sized_self.borrow().clone();
    if let Some(op) = &sized_self_op {
        if c.receiver != "&self" || c.ret_canonical.as_deref().map(|r| ty::parse(r).render()) != Some(ty::parse(owner).render()) {
            out.unsupported(c, "sized-self method", &format!("needs a `&self` receiver returning the owner, got {}", c.ret_canonical.as_deref().unwrap_or("()")));
            return;
        }
        pre.insert_str(0, &format!("support::signed_len(this.0.len(), \"{op}\")?; "));
    }
    // record 0096: the whole null-aware result is bounded before Polars allocates it
    let null_aware_op = world.null_aware.borrow().as_ref().map(|(op, _)| op.clone());
    if let Some(op) = &null_aware_op {
        if c.receiver != "&self" || !ret.fallible || !ret.conv.starts_with("__r.either(") || !null_aware_return_matches(c.ret_canonical.as_deref(), &world.null_aware.borrow().as_ref().unwrap().1) {
            out.unsupported(c, "null-aware return", "needs a `&self` receiver and the null-aware conversion");
            return;
        }
        pre.insert_str(0, &format!("support::null_aware_bound(this.0.len(), \"{op}\")?; "));
    }
    let fallible = fallible || sized_self_op.is_some();
    let ret_ty = if fallible { format!("Result<{}, Error>", ret.rust_ty) } else { ret.rust_ty.clone() };
    // record 0088: a routed `Cow` result is made owned inside the engine closure,
    // so no borrow of the receiver leaves it
    const OWN: &str = "let __r = __r.into_owned(); ";
    let own_in_closure = route && ret.materialize.is_none() && ret.conv.starts_with("{ let __r = __r.into_owned(); ");
    // `{ let __r = __r.into_owned(); X }` becomes `X`: no leftover braces
    let ret_conv = if own_in_closure { ret.conv.replacen(OWN, "", 1).trim_start_matches("{ ").trim_end_matches(" }").to_string() } else { ret.conv.clone() };
    let body_conv = if fallible { format!("Ok({})", ret_conv) } else { ret_conv }.replace("__OP__", &name);
    let attr = if c.receiver == "none" { format!("#[rune::function(free, path = {}::{name})]", w.rust) } else { format!("#[rune::function(instance, path = {name})]") };
    let doc = doc_line(c);
    let rune = format!("{}::{name}", rune_path(w));
    let arg_docs: Vec<String> = params.iter().map(|(n, a)| format!("{n}: {}", a.doc)).collect();
    let summary = format!("{name}({}) -> {}{}", arg_docs.join(", "), ret.doc, if fallible { " (fallible)" } else { "" });
    let (pre, call) = if let Some(m) = &ret.materialize {
        // an iterator return: created, driven and converted inside the
        // (routed) block; the receiver of a `&self` iterator stays usable
        let mut pre = pre.clone();
        let mut hoisted = Vec::new();
        for (i, a) in args.iter().enumerate() {
            pre.push_str(&format!("let __arg{i} = {a}; "));
            hoisted.push(format!("__arg{i}"));
        }
        (pre, materialized_call(m, &format!("{callee}({})", hoisted.join(", ")), &name, route))
    } else if route {
        // conversions may fail with `?`, so they run before the closure
        let mut pre = pre.clone();
        let mut hoisted = Vec::new();
        for (i, a) in args.iter().enumerate() {
            pre.push_str(&format!("let __arg{i} = {a}; "));
            hoisted.push(format!("__arg{i}"));
        }
        // record 0082: a callback that copies slices does so on every
        // invocation; one guard around the whole engine-thread call makes
        // those copies share a single cumulative bound
        let copies = args.iter().chain(std::iter::once(&pre)).any(|a| a.contains("support::copy_slice("));
        let body = if copies { format!("{{ let __slices = support::SliceBudget::enter(); {callee}({}) }}", hoisted.join(", ")) } else { format!("{callee}({})", hoisted.join(", ")) };
        let body = if own_in_closure { format!("{body}.into_owned()") } else { body };
        let call = format!("crate::engine::run(\"{rune}\", move || {body})");
        (pre, if fallible { format!("{call}.map_err(Error::engine)?") } else { format!("crate::engine::infallible({call}, \"{rune}\")") })
    } else {
        (pre, format!("{callee}({})", args.join(", ")))
    };
    let commit = if commit_receiver { "this.0 = __work; " } else { "" };
    let docline = if doc.is_empty() { String::new() } else { format!("/// {doc}\n") };
    writeln!(out.functions, "{docline}/// Polars: `{}`. {}\n{attr}\nfn {ident}({}) -> {ret_ty} {{ {pre}let __r = {call}; {commit}{body_conv} }}", c.canonical_path, summary, sig.join(", ")).unwrap();
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
        deref: false,
    };
    out.generated_with(c, &rune, note, info);
    if let Some(binding) = out.entries.last_mut().unwrap().bindings.first_mut() {
        binding.route_reason = binding_route_reason(world, c, route);
        if route { binding.reentry = Some(if fallible { "error" } else { "unwind" }.into()); }
    }
}

/// Record 0091: the shape a `[[free_instantiations]]` guard must have: both
/// fields or neither, a known check, and a parameter of the callable whose
/// type is `usize` (the only type the check is defined for).
fn guard_shape(f: &FreeInstantiation, c: &Callable) -> Result<(), String> {
    if !f.natives.is_empty() { check_natives(&f.path, &f.types, &f.natives)?; }
    match (&f.guard_param, &f.guard) {
        (None, None) => Ok(()),
        (Some(_), None) | (None, Some(_)) => Err(format!("`{}`: guard_param and guard must be given together", f.path)),
        (Some(param), Some(check)) => {
            if check != "below_idx_max" { return Err(format!("`{}`: unknown guard `{check}`", f.path)); }
            match c.params.iter().find(|p| sanitize(&p.name) == *param) {
                None => Err(format!("`{}`: guard_param `{param}` is not a parameter", f.path)),
                Some(p) if p.ty_canonical.trim() != "usize" => Err(format!("`{}`: guard_param `{param}` is `{}`, not `usize`", f.path, p.ty_canonical)),
                Some(_) => Ok(()),
            }
        }
    }
}

/// Record 0090: replace `from` in `ty` only where it stands as a whole token:
/// not preceded by an identifier character or `:`, not followed by one.
fn replace_token(ty: &str, from: &str, to: &str) -> String {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = String::new();
    let mut i = 0;
    while let Some(k) = ty[i..].find(from) {
        let at = i + k;
        let end = at + from.len();
        let before_ok = ty[..at].chars().next_back().is_none_or(|c| !ident(c) && c != ':');
        let after_ok = ty[end..].chars().next().is_none_or(|c| !ident(c));
        out.push_str(&ty[i..at]);
        out.push_str(if before_ok && after_ok { to } else { from });
        i = end;
    }
    out.push_str(&ty[i..]);
    out
}

/// Record 0089: a generic free function instantiated once per concrete type
/// the release file lists. `ChunkedArray<T>` in its parameters is spelled as
/// that type's wrapper, the binding is a static function on the wrapper
/// (`polars::Int64Chunked::arg_min_numeric(ca)`), and it calls the Polars
/// function by its public path, Rust inferring `T` from the argument. A
/// `usize` result converts with a range check.
fn emit_free_instantiations(world: &World, out: &mut Emitted, c: &Callable) {
    let f = world.release.free_instantiations.iter().find(|f| f.key == c.key && f.path == c.canonical_path).unwrap().clone();
    // record 0091 review: a guard must be complete, known, and name one of the
    // callable's `usize` parameters, or nothing is emitted (fail closed)
    if let Err(why) = guard_shape(&f, c) {
        out.unsupported(c, "free instantiation guard", &why);
        return;
    }
    struct Guard<'a>(&'a std::cell::RefCell<Option<(String, String, String)>>);
    impl Drop for Guard<'_> { fn drop(&mut self) { *self.0.borrow_mut() = None; } }
    let mut done: Vec<(String, String, &'static str, Option<String>)> = Vec::new();
    let mut infos: Vec<OracleInfo> = Vec::new();
    let mut exceptions: Vec<RouteException> = Vec::new();
    let generic_ca = format!("polars_core::chunked_array::ChunkedArray<{}>", f.generic);
    let native_of = format!("{}::Native", f.generic);
    for (n, t) in f.types.iter().enumerate() {
        let identity = format!("polars_core::chunked_array::ChunkedArray<{t}>");
        let Some(alias) = world.by_identity.get(&identity).cloned() else {
            exceptions.push(RouteException { route: "free instantiation", receiver: t.clone(), reason: format!("no wrapper holds `{identity}`") });
            continue;
        };
        let mut syn = c.clone();
        syn.owner = alias.clone();
        syn.bucket = "mechanical".into();
        syn.kind = "inherent".into();
        syn.receiver = "none".into();
        // record 0090: whole-token substitution only; a path or bound that merely
        // contains the spelling is left alone and then refused as residual
        for q in syn.params.iter_mut() {
            q.ty_canonical = replace_token(&q.ty_canonical, &generic_ca, &alias);
            if let Some(native) = f.natives.get(n) { q.ty_canonical = replace_token(&q.ty_canonical, &native_of, native); }
        }
        syn.generics_canonical.clear();
        if syn.params.iter().any(|q| mentions(&q.ty_canonical, &f.generic) || q.ty_canonical.contains("::Native")) {
            exceptions.push(RouteException { route: "free instantiation", receiver: alias.clone(), reason: format!("`{}` remains in a parameter after substitution", f.generic) });
            continue;
        }
        if let (Some(param), Some(check)) = (&f.guard_param, &f.guard) {
            *world.arg_guard.borrow_mut() = Some((c.name.clone(), param.clone(), check.clone()));
        }
        let _guard = Guard(&world.arg_guard);
        let before = out.entries.len();
        emit_method_with(world, out, &syn, &alias, None, false, Some(&f.callee));
        let e = out.entries.pop().unwrap();
        debug_assert_eq!(before, out.entries.len());
        if e.status == "generated" {
            done.push((e.rune.clone().unwrap(), alias.clone(), "free instantiation", None));
            if let Some(i) = e.oracle.clone() { infos.push(i); }
        } else {
            exceptions.push(RouteException { route: "free instantiation", receiver: alias.clone(), reason: format!("{}: {}", e.status, e.reason.unwrap_or_default()) });
        }
    }
    if done.is_empty() || infos.is_empty() {
        out.unsupported(c, "free instantiation", "no listed type emitted; see exceptions");
        out.entries.last_mut().unwrap().exceptions = exceptions;
        return;
    }
    out.generated_on(c, &done, infos[0].clone());
    let e = out.entries.last_mut().unwrap();
    for (b, i) in e.bindings.iter_mut().zip(infos) {
        b.info = Some(i);
        b.id = binding_id(&c.canonical_path, b.receiver.as_deref(), false);
    }
    e.note = Some(format!("instantiated on {} of {} listed types ({})", done.len(), f.types.len(), f.cite));
    e.exceptions = exceptions;
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
    if let Some(r) = world.release.refused.iter().find(|r| r.path == c.canonical_path) {
        out.unsupported(c, "release policy", &format!("{} ({})", r.reason, r.cite));
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
        let mapped = match closure_signature(c, p) {
            Some(sig) => callback_arg(world, c, &sig, &sanitize(&p.name), None),
            None => world.arg(&ty::parse(&p.ty_canonical), &sanitize(&p.name), &generics, None, 0),
        };
        match mapped {
            Ok(a) => params.push((sanitize(&p.name), a)),
            Err(Unsupported(why, what)) => {
                out.unsupported(c, why, &format!("{} ({what})", p.name));
                return;
            }
        }
    }
    let ret = match c.ret_canonical.as_deref() {
        None => Ret { materialize: None, rust_ty: "()".into(), fallible: false, conv: "__r".into(), doc: "unit".into() },
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
    let route = routed_binding(world, c, None);
    let fallible = ret.fallible || params.iter().any(|(_, a)| a.fallible || a.pre.iter().any(|p| p.contains('?')));
    let pre: String = params.iter().filter(|(_, a)| a.shape.starts_with("callback:")).chain(params.iter().filter(|(_, a)| !a.shape.starts_with("callback:"))).flat_map(|(_, a)| a.pre.iter()).map(|p| format!("{p} ")).collect();
    let idx = out.fn_index;
    out.fn_index += 1;
    let ident = rust_ident("g", &c.canonical_path, idx);
    let sig: Vec<String> = params.iter().map(|(n, a)| format!("{n}: {}", a.rust_ty)).collect();
    let args: Vec<String> = params.iter().map(|(_, a)| a.conv.clone()).collect();
    let ret_ty = if fallible { format!("Result<{}, Error>", ret.rust_ty) } else { ret.rust_ty.clone() };
    let body_conv = if fallible { format!("Ok({})", ret.conv) } else { ret.conv.clone() }.replace("__OP__", &name);
    let doc = doc_line(c);
    let arg_docs: Vec<String> = params.iter().map(|(n, a)| format!("{n}: {}", a.doc)).collect();
    let summary = format!("{name}({}) -> {}{}", arg_docs.join(", "), ret.doc, if fallible { " (fallible)" } else { "" });
    let (pre, call) = if let Some(m) = &ret.materialize {
        let mut pre = pre.clone();
        let mut hoisted = Vec::new();
        for (i, a) in args.iter().enumerate() {
            pre.push_str(&format!("let __arg{i} = {a}; "));
            hoisted.push(format!("__arg{i}"));
        }
        (pre, materialized_call(m, &format!("{spelled}({})", hoisted.join(", ")), &name, route))
    } else if route {
        let mut pre = pre.clone();
        let mut hoisted = Vec::new();
        for (i, a) in args.iter().enumerate() {
            pre.push_str(&format!("let __arg{i} = {a}; "));
            hoisted.push(format!("__arg{i}"));
        }
        let call = format!("crate::engine::run(\"polars::{name}\", move || {spelled}({}))", hoisted.join(", "));
        (pre, if fallible { format!("{call}.map_err(Error::engine)?") } else { format!("crate::engine::infallible({call}, \"polars::{name}\")") })
    } else {
        (pre, format!("{spelled}({})", args.join(", ")))
    };
    let docline = if doc.is_empty() { String::new() } else { format!("/// {doc}\n") };
    writeln!(out.functions, "{docline}/// Polars: `{}`. {}\n#[rune::function(path = {name})]\nfn {ident}({}) -> {ret_ty} {{ {pre}let __r = {call}; {body_conv} }}", c.canonical_path, summary, sig.join(", ")).unwrap();
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
        deref: false,
    };
    out.generated_with(c, &rune, if notes.is_empty() { None } else { Some(notes.join("; ")) }, info);
    if let Some(binding) = out.entries.last_mut().unwrap().bindings.first_mut() {
        binding.route_reason = binding_route_reason(world, c, route);
        if route { binding.reentry = Some(if fallible { "error" } else { "unwind" }.into()); }
    }
}

/// Record 0078: assignment operators with their Rune protocols.
const ASSIGN_OPS: &[(&str, &str, &str, &str)] = &[("SubAssign", "SUB_ASSIGN", "-=", "sub_assign"), ("BitAndAssign", "BIT_AND_ASSIGN", "&=", "bitand_assign"), ("BitOrAssign", "BIT_OR_ASSIGN", "|=", "bitor_assign"), ("BitXorAssign", "BIT_XOR_ASSIGN", "^=", "bitxor_assign")];

/// The snake-case name of a conversion source's last segment, with `Vec`
/// and `Option` spelled as prefixes.
fn source_segment(src: &str) -> String {
    let t = src.trim().trim_start_matches('&').trim();
    if t.starts_with('(') { return "tuple".into(); }
    if let Some(inner) = t.strip_prefix("alloc::vec::Vec<").and_then(|r| r.strip_suffix('>')) { return format!("vec_{}", source_segment(inner)); }
    if let Some(inner) = t.strip_prefix("core::option::Option<").and_then(|r| r.strip_suffix('>')) { return format!("option_{}", source_segment(inner)); }
    let seg = t.split('<').next().unwrap_or(t).rsplit("::").next().unwrap_or(t);
    let mut out = String::new();
    for (i, ch) in seg.chars().enumerate() {
        if ch.is_ascii_uppercase() && i > 0 { out.push('_'); }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

/// The crate short name of a source type (`polars_core::…` gives `core`),
/// or the scalar's own name.
fn source_crate(src: &str) -> String {
    let t = src.trim().trim_start_matches('&').trim();
    let first = t.split("::").next().unwrap_or(t);
    if first.contains('<') || !t.contains("::") { return "scalar".into(); }
    first.trim_start_matches("polars_").to_string()
}

/// The `from_<source>` names of every `From` impl, per callable key
/// (record 0078). A by-reference impl beside its by-value twin gets
/// `_ref`; two distinct sources whose last segment coincides on one owner
/// get names qualified by the source's crate; nothing is merged.
fn plan_from_names(inv: &Inventory) -> BTreeMap<String, String> {
    let mut by_owner: BTreeMap<&str, Vec<(&Callable, String, bool)>> = BTreeMap::new();
    for c in &inv.callables {
        if c.kind != "foreign_trait_impl" || !c.name.starts_with("From<") { continue; }
        // rustdoc lists `impl From<&X> for Y` on X's page as well; the impl
        // belongs to its `for` type, and the listing on X is a duplicate
        if !from_impl_is_on_its_owner(c) { continue; }
        let Some(src) = c.params.first().map(|p| p.ty_canonical.as_str()) else { continue };
        let by_ref = src.trim().starts_with('&');
        by_owner.entry(c.owner.as_str()).or_default().push((c, src.to_string(), by_ref));
    }
    let mut names = BTreeMap::new();
    for (_, impls) in by_owner {
        // distinct (by-value) sources per last segment
        let mut per_seg: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (_, src, _) in &impls {
            per_seg.entry(source_segment(src)).or_default().insert(src.trim().trim_start_matches('&').trim().to_string());
        }
        for (c, src, by_ref) in &impls {
            let seg = source_segment(src);
            let base = if per_seg[&seg].len() > 1 { format!("from_{}_{seg}", source_crate(src)) } else { format!("from_{seg}") };
            names.insert(c.key.clone(), if *by_ref { format!("{base}_ref") } else { base });
        }
    }
    names
}

/// Whether a `From` impl's recorded `for` type is the type it is listed
/// under (rustdoc lists an impl on the pages of both types it mentions).
fn from_impl_is_on_its_owner(c: &Callable) -> bool {
    match &c.impl_for {
        Some(f) => f.split('<').next().unwrap_or(f) == c.owner,
        None => true,
    }
}

/// Record 0078: a duplicate listing stays adapted only when its
/// counterpart is generated; a counterpart refused for any reason makes
/// the listing unsupported with that reason, so nothing is called bound
/// that is not.
fn resolve_duplicates(entries: &mut [Entry]) {
    let by_key: BTreeMap<String, (&'static str, Option<String>, Option<String>)> = entries.iter().map(|e| (e.key.clone(), (e.status, e.rune.clone(), e.reason.clone()))).collect();
    for e in entries.iter_mut() {
        let Some(k) = e.counterpart.clone() else { continue };
        match by_key.get(&k) {
            Some(("generated", rune, _)) => {
                e.rune = rune.clone();
                e.reason = Some(format!("{}; bound there as `{}`", e.reason.take().unwrap_or_default(), rune.as_deref().unwrap_or("").rsplit("::").next().unwrap_or("")));
            }
            Some((status, _, reason)) => {
                e.status = "unsupported";
                e.rune = None;
                e.reason = Some(format!("duplicate listing whose retained listing is {status}: {}", reason.as_deref().unwrap_or("(no reason)")));
            }
            None => {
                e.status = "unsupported";
                e.rune = None;
                e.reason = Some("duplicate listing whose retained listing has no entry".into());
            }
        }
    }
}

/// The impl identity a rustdoc impl listing carries: crate and impl id
/// (`krate:owner:impl` keys share the impl id across the pages it is
/// listed on).
fn impl_identity(key: &str) -> String {
    let mut it = key.split(':');
    let krate = it.next().unwrap_or("");
    let last = key.rsplit(':').next().unwrap_or("");
    format!("{krate}:{last}")
}

/// A `From<X>` impl as a constructor binding `T::from_<source>(x)` calling
/// `<T as From<X>>::from` by UFCS; an unmappable source is refused with
/// the reason `conversion source`.
fn emit_from(world: &World, out: &mut Emitted, c: &Callable, name: &str) {
    let owner = &c.owner;
    let Some(w) = world.wrapper_for(owner) else { out.unsupported(c, "owner not wrapped", owner); return };
    let Some(src) = c.params.first() else { out.unsupported(c, "conversion source", "none"); return };
    if out.taken.contains_key(&(w.rust.clone(), name.to_string())) {
        out.unsupported(c, &format!("name taken by inherent {name}"), &out.taken[&(w.rust.clone(), name.to_string())].clone());
        return;
    }
    let mut syn = c.clone();
    syn.kind = "inherent".into();
    syn.receiver = "none".into();
    syn.name = name.to_string();
    syn.params = vec![Param { name: "value".into(), ty: src.ty.clone(), ty_canonical: src.ty_canonical.clone() }];
    syn.ret = Some(w.spell.clone());
    syn.ret_canonical = Some(owner.clone());
    syn.generics_canonical.clear();
    // the Rust identifier is hashed from the callable's path: make it name the source too
    syn.canonical_path = format!("{} as core::convert::From<{}>", owner, src.ty_canonical);
    let src_spell = &c.name[5..c.name.len() - 1]; // the `X` of `From<X>` as rustdoc rendered it
    let callee = format!("<{} as From<{}>>::from", w.spell, spell_source(world, src_spell, &src.ty_canonical));
    let before = out.entries.len();
    emit_method_with(world, out, &syn, owner, None, false, Some(&callee));
    debug_assert_eq!(before + 1, out.entries.len());
    let e = out.entries.last_mut().unwrap();
    e.key = c.key.clone();
    e.canonical_path = c.canonical_path.clone();
    e.kind = c.kind.clone();
    e.signature = signature_of(c);
    if e.status == "unsupported" {
        if let Some(r) = &e.reason {
            if !r.starts_with("name taken") { e.reason = Some(format!("conversion source: {r}")); }
        }
    } else if e.status == "generated" {
        e.note = Some(format!("From<{}> as `{name}`{}", src_spell, if src.ty_canonical.trim().starts_with('&') { " (by reference; its by-value twin, if any, is a separate binding)" } else { "" }));
    }
}

/// The Rust spelling of a conversion source for the UFCS path: a wrapped
/// type by its wrapper's spelling, a scalar as is, a reference kept.
fn spell_source(world: &World, rendered: &str, canonical: &str) -> String {
    let by_ref = canonical.trim().starts_with('&');
    let bare = canonical.trim().trim_start_matches('&').trim();
    let inner = match world.wrappers.get(bare) {
        Some(w) => w.spell.clone(),
        None => match bare.split('<').next().unwrap_or(bare) {
            "alloc::string::String" => "String".into(),
            "polars_utils::pl_str::PlSmallStr" => "polars::prelude::PlSmallStr".into(),
            "alloc::vec::Vec" | "core::option::Option" => rendered.trim_start_matches('&').to_string(),
            _ => bare.to_string(),
        },
    };
    if by_ref { format!("&{inner}") } else { inner }
}

const OPS: &[(&str, &str, &str)] = &[("Add", "ADD", "+"), ("Sub", "SUB", "-"), ("Mul", "MUL", "*"), ("Div", "DIV", "/"), ("Rem", "REM", "%"), ("BitAnd", "BIT_AND", "&"), ("BitOr", "BIT_OR", "|"), ("BitXor", "BIT_XOR", "^")];

fn emit_foreign(world: &World, out: &mut Emitted, c: &Callable) {
    let owner = &c.owner;
    let tname = c.name.split('<').next().unwrap_or("").to_string();
    if tname == "From" && !from_impl_is_on_its_owner(c) {
        // listed on its source type's page (wrapped or not); a duplicate only
        // when the retained listing on the impl's `for` type is identified
        // (same crate and impl id); its status is settled in
        // `resolve_duplicates` once every entry exists. Otherwise it is an
        // outward conversion `From<Owner> for Target` with no wrapped
        // constructor to bind.
        let target = c.impl_for.clone().unwrap_or_default();
        let counterpart = out.from_names.keys().find(|k| impl_identity(k) == impl_identity(&c.key) && k.as_str() != c.key.as_str()).cloned();
        match counterpart {
            Some(k) => {
                out.adapted(c, &format!("duplicate listing of impl {}: rustdoc lists it on this source type's page as well; its retained listing is on {}", impl_identity(&c.key), target), "");
                out.entries.last_mut().unwrap().counterpart = Some(k);
            }
            None => out.unsupported(c, "outward conversion, no retained listing on its target", &format!("From<{}> for {target}", owner.rsplit("::").next().unwrap_or(""))),
        }
        return;
    }
    let Some(w) = world.wrapper_for(owner) else {
        out.unsupported(c, "owner not wrapped", owner.clone().as_str());
        return;
    };
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
    if tname == "From" {
        let name = out.from_names.get(&c.key).cloned().unwrap_or_else(|| "from".into());
        emit_from(world, out, c, &name);
        return;
    }
    let (code, note) = match tname.as_str() {
        t if ASSIGN_OPS.iter().any(|(n, _, _, _)| *n == t) => {
            let (_, proto, op, method) = ASSIGN_OPS.iter().find(|(n, _, _, _)| *n == t).unwrap();
            let rhs_ok = c.params.first().is_some_and(|p| p.ty_canonical == *owner || p.ty_canonical == "Self");
            if !rhs_ok {
                out.unsupported(c, "assignment operand", &c.params.first().map(|p| p.ty_canonical.clone()).unwrap_or_default());
                return;
            }
            if !world.clonable.contains(owner) {
                out.unsupported(c, "operator on a non-Clone type", tname.as_str());
                return;
            }
            // `x op= y` mutates the Rune value `x` in place; `y` is cloned out and stays usable
            (format!("#[rune::function(instance, protocol = {proto})]\nfn {ident}(this: &mut {0}, rhs: &{0}) {{ <{1} as core::ops::{t}>::{method}(&mut this.0, rhs.0.clone()) }}", w.rust, w.spell), *op)
        }
        "Not" => {
            if !world.clonable.contains(owner) {
                out.unsupported(c, "operator on a non-Clone type", tname.as_str());
                return;
            }
            // Rune 0.14.2 has no unary NOT protocol: bound as a method, the receiver unchanged
            (format!("#[rune::function(instance, path = not_)]\nfn {ident}(this: &{0}) -> {0} {{ {0}(<{1} as core::ops::Not>::not(this.0.clone())) }}", w.rust, w.spell), "not_() (Rune has no unary NOT protocol; bound as a method, `not` is a keyword)")
        }
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
            let body = if fallible { format!("Ok({})", ret.conv) } else { ret.conv.clone() }.replace("__OP__", &c.name);
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
    let assign = ASSIGN_OPS.iter().find(|(n, _, _, _)| *n == tname);
    let info = OracleInfo {
        rune_owner: Some(rune_path(w)),
        rune_name: match assign { Some((_, _, op, _)) => op.to_string(), None if tname == "Not" => "not_".to_string(), None => format!("<{tname}>") },
        receiver: match assign { Some(_) => "&mut self".into(), None if tname == "Not" => "self".into(), None => "protocol".into() },
        owner: Some((owner.to_string(), w.rust.clone())),
        callee: match assign { Some((_, _, _, method)) => format!("<{} as core::ops::{tname}>::{method}", w.spell), None if tname == "Not" => format!("<{} as core::ops::Not>::not", w.spell), None => tname.clone() },
        params: match assign { Some(_) => vec![(format!("W:{owner}"), owner.to_string())], None => c.params.iter().map(|p| (String::new(), p.ty_canonical.clone())).collect() },
        param_names: c.params.iter().map(|p| sanitize(&p.name)).collect(),
        ret_canonical: if assign.is_some() { None } else { c.ret_canonical.clone() },
        ret_rust: if assign.is_some() { "()".into() } else { String::new() },
        fallible: false,
        generics: BTreeMap::new(),
        implementors: vec![],
        deref: false,
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
                    let body = if r.fallible { format!("Ok({})", r.conv) } else { r.conv.clone() }.replace("__OP__", fname);
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
    let buckets: Vec<String> = args.iter().position(|a| a == "--buckets").map(|i| args[i + 1].split(',').map(|s| s.to_string()).collect()).unwrap_or_else(|| vec!["mechanical".into(), "conversion".into(), "option_struct".into(), "callback".into()]);
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
    let mut out = Emitted { from_names: plan_from_names(&inv), functions: String::new(), registrations: vec![], catalogue: vec![], entries: vec![], taken: BTreeMap::new(), fn_index: 0 };
    let mut callables: Vec<&Callable> = inv.callables.iter().collect();
    callables.sort_by(|a, b| a.canonical_path.cmp(&b.canonical_path).then(a.key.cmp(&b.key)));
    // inherent methods take names before trait methods do
    callables.sort_by_key(|c| match c.kind.as_str() { "inherent" => 0, "free_fn" => 1, "foreign_trait_impl" => 2, _ => 3 });
    // record 0076 gate 3: the instantiation census, before any binding is emitted
    let census = instantiation_census(&world, &inv);
    {
        let summary = census_summary(&census);
        println!("instantiation census: {}", serde_json::to_string(&summary["by_family"]).unwrap());
    }
    let mut census_by_method: BTreeMap<String, Vec<&PairRecord>> = BTreeMap::new();
    for p in &census {
        census_by_method.entry(p.key.clone()).or_default().push(p);
    }
    let census_keys: BTreeSet<String> = census.iter().map(|p| p.key.clone()).collect();
    for c in callables {
        let api = release.is_api(&c.krate);
        if c.bucket == "unsupported" || c.bucket == "unknown" {
            continue; // not eligible in 0072's terms
        }
        if api && c.kind == "inherent" && c.bucket == "generic" {
            if let Some(pairs) = census_by_method.get(&c.key) {
                emit_instantiations(&world, &mut out, c, pairs);
                continue;
            }
        }
        // record 0077: a callable in the generic bucket only because its
        // return is an iterator (rule T3) is handled by the mapping rules,
        // which materialize it or refuse it with the item named
        if api && c.bucket == "generic" && !c.owner_generic && c.generics_canonical.is_empty() && c.ret_canonical.as_deref().is_some_and(|r| iterator_return(&ty::parse(r)).is_some()) {
            let mut with_generic: Vec<&str> = buckets.clone();
            with_generic.push("generic");
            emit_callable(&world, &mut out, c, &with_generic);
            continue;
        }
        if !api {
            out.entries.push(Entry { key: c.key.clone(), canonical_path: c.canonical_path.clone(), kind: c.kind.clone(), bucket: c.bucket.clone(), status: "out_of_scope", fallible: None, signature: signature_of(c), execution: None, oracle: None, reason: Some("internal crate reachable through the prelude".into()), rune: None, note: None, bindings: vec![], exceptions: vec![], counterpart: None });
            continue;
        }
        emit_callable(&world, &mut out, c, &buckets);
    }
    emit_struct_extras(&world, &mut out, &buckets);
    resolve_duplicates(&mut out.entries);
    // every pair of the census gets exactly one disposition, emitted or not
    let mut census = census;
    {
        let mut per_key: BTreeMap<&str, (&str, Option<&str>, Vec<(&str, &str)>, Vec<(&str, &str)>)> = BTreeMap::new();
        for e in &out.entries {
            if !census_keys.contains(&e.key) { continue; }
            let bs: Vec<(&str, &str)> = e.bindings.iter().filter(|b| b.route == "instantiation").filter_map(|b| b.receiver.as_deref().map(|r| (r, b.id.as_str()))).collect();
            let xs: Vec<(&str, &str)> = e.exceptions.iter().filter(|x| x.route == "instantiation").map(|x| (x.receiver.as_str(), x.reason.as_str())).collect();
            per_key.insert(e.key.as_str(), (e.status, e.reason.as_deref(), bs, xs));
        }
        let by_key: BTreeMap<&str, &Callable> = inv.callables.iter().map(|c| (c.key.as_str(), c)).collect();
        for p in census.iter_mut() {
            p.disposition = Some(match &p.result {
                Applicability::Rejected(r) => format!("rejected: {r}"),
                Applicability::Unresolved(r) => format!("unresolved: {r}"),
                Applicability::Proven => {
                    if !p.eligible {
                        let c = by_key[p.key.as_str()];
                        format!("not eligible: the callable is in 0072's `{}` bucket ({})", c.bucket, c.rules.join(" "))
                    } else if let Some((status, reason, bs, xs)) = per_key.get(p.key.as_str()) {
                        if bs.iter().any(|(r, _)| *r == p.alias) { "emitted".to_string() }
                        else if let Some((_, why)) = xs.iter().find(|(r, _)| *r == p.alias) {
                            if why.starts_with("excluded by the release file") { format!("excluded: {why}") } else if why.starts_with("not shipped") { why.to_string() } else { format!("refused: {why}") }
                        } else if *status == "unsupported" { format!("refused: {}", reason.unwrap_or("entry unsupported")) }
                        else { format!("no disposition: entry {status} ({})", reason.unwrap_or("")) }
                    } else {
                        "no disposition: no entry for the callable".to_string()
                    }
                }
            });
        }
    }
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
        "instantiation": { "summary": census_summary(&census), "pairs": census },
        "iterators": iterator_census(&out.entries, &inv, &census),
        "conversions": conversion_census(&out.entries, &inv, &out.from_names),
        "callbacks": callback_census(&world, &inv, &out.entries),
        "materialize_limit": 1usize << 20,
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
/// Record 0076: typed source fixtures. Each is a small deterministic value
/// of a wrapped type, named for its family, that feeds the producer
/// bindings it lists (`series_bool` feeds `Series::bool`); none replaces
/// the type's default fixture.
const TYPED_FIXTURES: &[(&str, &str, &str, &str, &[&str])] = &[
    ("polars_core::series::Series", "series_bool", "p::Series::new(\"x\".into(), [true, false, true])", "crate_oracle::series_repr(v)", &["bool"]),
    ("polars_core::series::Series", "series_str", "p::Series::new(\"x\".into(), [\"a\", \"bb\", \"ccc\"])", "crate_oracle::series_repr(v)", &["str"]),
    ("polars_core::series::Series", "series_binary", "p::Series::new(\"x\".into(), [&b\"ab\"[..], b\"\\x00\\xff\", b\"\"])", "crate_oracle::series_repr(v)", &["binary"]),
    ("polars_core::series::Series", "series_binary_offset", "p::Series::from_any_values_and_dtype(\"x\".into(), &[p::AnyValue::Binary(b\"ab\"), p::AnyValue::Binary(b\"\\x00\\xff\"), p::AnyValue::Binary(b\"\")], &p::DataType::BinaryOffset, true).unwrap()", "crate_oracle::series_repr(v)", &["binary_offset"]),
    ("polars_core::series::Series", "series_i8", "p::Series::new(\"x\".into(), [1i64, 2, 3]).cast(&p::DataType::Int8).unwrap()", "crate_oracle::series_repr(v)", &["i8"]),
    ("polars_core::series::Series", "series_i16", "p::Series::new(\"x\".into(), [1i64, 2, 3]).cast(&p::DataType::Int16).unwrap()", "crate_oracle::series_repr(v)", &["i16"]),
    ("polars_core::series::Series", "series_i32", "p::Series::new(\"x\".into(), [1i32, 2, 3])", "crate_oracle::series_repr(v)", &["i32"]),
    ("polars_core::series::Series", "series_i64", "p::Series::new(\"x\".into(), [1i64, 2, 3])", "crate_oracle::series_repr(v)", &["i64"]),
    ("polars_core::series::Series", "series_u8", "p::Series::new(\"x\".into(), [1i64, 2, 3]).cast(&p::DataType::UInt8).unwrap()", "crate_oracle::series_repr(v)", &["u8"]),
    ("polars_core::series::Series", "series_u16", "p::Series::new(\"x\".into(), [1i64, 2, 3]).cast(&p::DataType::UInt16).unwrap()", "crate_oracle::series_repr(v)", &["u16"]),
    ("polars_core::series::Series", "series_u32", "p::Series::new(\"x\".into(), [1u32, 2, 3])", "crate_oracle::series_repr(v)", &["u32", "idx"]),
    ("polars_core::series::Series", "series_u64", "p::Series::new(\"x\".into(), [1u64, 2, 3])", "crate_oracle::series_repr(v)", &["u64"]),
    // record 0091: values beyond u32 and at u64::MAX, which a script integer cannot spell; feeds no producer
    ("polars_core::series::Series", "series_u64_extremes", "p::Series::new(\"x\".into(), [u64::MAX, 4_294_967_297u64, 1])", "crate_oracle::series_repr(v)", &[]),
    // record 0100: two chunks each, feeding no producer: multibyte, empty and null strings;
    // zero, non-UTF-8, empty and null bytes (view and offset binary)
    ("polars_core::series::Series", "series_str_mixed", "{ let mut s = p::Series::new(\"x\".into(), [Some(\"é日本\"), Some(\"\")]); s.append(&p::Series::new(\"x\".into(), [None, Some(\"z\")])).unwrap(); s }", "crate_oracle::series_repr(v)", &[]),
    ("polars_core::series::Series", "series_binary_mixed", "{ let mut s = p::Series::new(\"x\".into(), [Some(&b\"\\xc3\\x28\"[..]), Some(&b\"\"[..])]); s.append(&p::Series::new(\"x\".into(), [None, Some(&b\"\\x00\\x00\\xff\"[..])])).unwrap(); s }", "crate_oracle::series_repr(v)", &[]),
    ("polars_core::series::Series", "series_binary_offset_mixed", "{ let mut s = p::Series::from_any_values_and_dtype(\"x\".into(), &[p::AnyValue::Binary(b\"\\xc3\\x28\"), p::AnyValue::Binary(b\"\")], &p::DataType::BinaryOffset, true).unwrap(); s.append(&p::Series::from_any_values_and_dtype(\"x\".into(), &[p::AnyValue::Null, p::AnyValue::Binary(b\"\\x00\\x00\\xff\")], &p::DataType::BinaryOffset, true).unwrap()).unwrap(); s }", "crate_oracle::series_repr(v)", &[]),
    // record 0098: two chunks of float specials (±0, distinct NaN payloads of both signs, ±inf, nulls); feed no producer
    ("polars_core::series::Series", "series_f64_specials", "{ let mut s = p::Series::new(\"x\".into(), [Some(1.5f64), Some(-0.0), Some(0.0), Some(f64::from_bits(0x7ff8_0000_0000_0001)), None]); s.append(&p::Series::new(\"x\".into(), [Some(f64::NEG_INFINITY), Some(f64::INFINITY), Some(f64::from_bits(0xfff0_0000_0000_0123)), None, Some(-2.5)])).unwrap(); s }", "crate_oracle::series_repr(v)", &[]),
    ("polars_core::series::Series", "series_f32_specials", "{ let mut s = p::Series::new(\"x\".into(), [Some(1.5f32), Some(-0.0), Some(0.0), Some(f32::from_bits(0x7fc0_0001)), None]); s.append(&p::Series::new(\"x\".into(), [Some(f32::NEG_INFINITY), Some(f32::INFINITY), Some(f32::from_bits(0xff80_0123)), None, Some(-2.5)])).unwrap(); s }", "crate_oracle::series_repr(v)", &[]),
    // record 0093: the read-back boundary i64::MAX, i64::MAX + 1, u64::MAX and a null; feeds no producer
    ("polars_core::series::Series", "series_u64_boundary", "p::Series::new(\"x\".into(), [Some(i64::MAX as u64), Some(i64::MAX as u64 + 1), Some(u64::MAX), None])", "crate_oracle::series_repr(v)", &[]),
    ("polars_core::series::Series", "series_f32", "p::Series::new(\"x\".into(), [1.5f32, 2.5, 3.5])", "crate_oracle::series_repr(v)", &["f32"]),
    ("polars_core::series::Series", "series_f64", "p::Series::new(\"x\".into(), [1.5f64, 2.5, 3.5])", "crate_oracle::series_repr(v)", &["f64"]),
    ("polars_core::series::Series", "series_struct", "p::IntoSeries::into_series(df().into_struct(\"x\".into()))", "crate_oracle::series_repr(v)", &["struct_"]),
    ("polars_core::series::Series", "series_list", "p::Series::new(\"x\".into(), [p::Series::new(\"a\".into(), [1i64, 2]), p::Series::new(\"b\".into(), [3i64])])", "crate_oracle::series_repr(v)", &["list"]),
    ("polars_core::series::Series", "series_date", "p::Series::new(\"x\".into(), [1i32, 2, 3]).cast(&p::DataType::Date).unwrap()", "crate_oracle::series_repr(v)", &["date"]),
    ("polars_core::series::Series", "series_datetime", "p::Series::new(\"x\".into(), [1i64, 2, 3]).cast(&p::DataType::Datetime(p::TimeUnit::Milliseconds, None)).unwrap()", "crate_oracle::series_repr(v)", &["datetime"]),
    ("polars_core::series::Series", "series_duration", "p::Series::new(\"x\".into(), [1i64, 2, 3]).cast(&p::DataType::Duration(p::TimeUnit::Milliseconds)).unwrap()", "crate_oracle::series_repr(v)", &["duration"]),
    ("polars_core::series::Series", "series_time", "p::Series::new(\"x\".into(), [1i64, 2, 3]).cast(&p::DataType::Time).unwrap()", "crate_oracle::series_repr(v)", &["time"]),
    // for the comparator controls: a long array, an array with a null, floats that display alike
    ("polars_core::series::Series", "series_long", "p::Series::new(\"x\".into(), (0..40i64).collect::<Vec<_>>())", "crate_oracle::series_repr(v)", &[]),
    ("polars_core::series::Series", "series_nulls", "p::Series::new(\"x\".into(), [Some(1i64), None, Some(3)])", "crate_oracle::series_repr(v)", &[]),
    ("polars_core::series::Series", "series_struct_null_field", "p::IntoSeries::into_series(p::df!(\"x\" => [None::<i64>], \"y\" => [\"a\"]).unwrap().into_struct(\"s\".into()))", "crate_oracle::series_repr(v)", &[]),
    ("polars_core::series::Series", "series_list_of_struct", "p::Series::new(\"l\".into(), [p::IntoSeries::into_series(p::df!(\"x\" => [None::<i64>], \"y\" => [\"a\"]).unwrap().into_struct(\"s\".into()))])", "crate_oracle::series_repr(v)", &[]),
    ("polars_core::series::Series", "series_list_long", "p::Series::new(\"x\".into(), [p::Series::new(\"i\".into(), (0..40i64).collect::<Vec<_>>())])", "crate_oracle::series_repr(v)", &[]),
    ("polars_core::series::Series", "series_list_nulls", "p::Series::new(\"x\".into(), [p::Series::new(\"i\".into(), [Some(1i64), None, Some(3)])])", "crate_oracle::series_repr(v)", &[]),
    ("polars_core::series::Series", "series_float_sum", "p::Series::new(\"x\".into(), [0.1f64 + 0.2])", "crate_oracle::series_repr(v)", &[]),
];

const FIXTURES: &[(&str, &str, &str, &str)] = &[
    ("polars_core::frame::dataframe::DataFrame", "df", "polars::df!(\"x\" => [1i64, 2, 3], \"y\" => [\"a\", \"b\", \"c\"], \"z\" => [1.5f64, 2.5, 3.5]).unwrap()", "crate_oracle::frame_repr(v)"),
    ("polars_lazy::frame::LazyFrame", "lf", "df().lazy()", "match v.clone().collect() { Ok(d) => crate_oracle::frame_repr(&d).prefixed(\"collected \"), Err(e) => crate_oracle::Repr::Text(format!(\"collect error: {}\", crate_oracle::error_kind(&e))) }"),
    ("polars_plan::dsl::expr::Expr", "expr", "p::col(\"x\")", "match df().lazy().select([v.clone()]).collect() { Ok(d) => crate_oracle::frame_repr(&d).prefixed(\"selected \"), Err(e) => crate_oracle::Repr::Text(format!(\"select error: {}\", crate_oracle::error_kind(&e))) }"),
    ("polars_core::series::Series", "series", "p::Series::new(\"x\".into(), [1i64, 2, 3])", "crate_oracle::series_repr(v)"),
    ("polars_core::frame::column::Column", "column", "series().into_column()", "crate_oracle::column_repr(v)"),
    ("polars_core::datatypes::dtype::DataType", "dtype", "p::DataType::Int64", "crate_oracle::Repr::Text(format!(\"{:?}\", v))"),
    ("polars_core::datatypes::field::Field", "field", "p::Field::new(\"x\".into(), p::DataType::Int64)", "crate_oracle::Repr::Text(format!(\"{:?}\", v))"),
    ("polars_core::series::implementations::null::NullChunked", "null_chunked", "p::Series::new_null(\"x\".into(), 2).null().unwrap().clone()", "crate_oracle::Repr::Text(format!(\"{}:{:?}:len={}\", p::SeriesTrait::name(v), p::SeriesTrait::dtype(v), v.len()))"),
    // record 0094: categorical receivers, built and unwrapped under one lock (support::categorical_fixtures)
    ("polars_dtype::categorical::Categories", "categories", "crate::generated::support::categorical_fixtures::categories()", "crate_oracle::Repr::Text(format!(\"{:?}\", v))"),
    ("polars_dtype::categorical::FrozenCategories", "frozen_categories", "crate::generated::support::categorical_fixtures::frozen_categories()", "crate_oracle::Repr::Text(format!(\"{:?}\", v))"),
    ("polars_dtype::categorical::mapping::CategoricalMapping", "categorical_mapping", "crate::generated::support::categorical_fixtures::mapping()", "crate_oracle::Repr::Text(format!(\"{:?}\", v))"),
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
    /// Record 0086: the length of the Rust mask fixture for the argument
    /// being paired (3 for a receiver-length mask, 1 for a values-length one).
    mask_len: std::cell::Cell<usize>,
    /// Record 0094: the case's return is a listed hash token, so the Rust
    /// side formats its `u64` as exact hex.
    hash_ret: std::cell::Cell<bool>,
    /// Record 0099: the case's pair is a listed chunk snapshot with this
    /// native, so the Rust side frames the chunk list as nested options.
    chunk_native: std::cell::RefCell<Option<String>>,
    /// Record 0101: the case's pair is a listed indexed chunk snapshot of this kind.
    indexed_native: std::cell::RefCell<Option<String>>,
    /// Record 0102: the case's pair is a listed array snapshot of this kind.
    array_native: std::cell::RefCell<Option<String>>,
    /// Record 0103: the case's pair is a listed iterator snapshot of this kind.
    iter_native: std::cell::RefCell<Option<String>>,
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
            // record 0086: masks sized to the receiver fixtures (3 rows) or the one-element values fixture
            "mask3" => Some("[true, false, true]".into()),
            "mask1" => Some("[false]".into()),
            "mask2" => Some("[true, false]".into()),
            // record 0094: the token of the Rust side's `2u64`
            "hash" => Some("\"0000000000000002\"".into()),
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
        // the same condition as the Rune side: a `Default` fixture exists
        // only where the `default_` binding does, so both sides build the
        // same value (record 0076: an impl on the generic base gave Rust an
        // empty array while Rune took the producer)
        if self.defaultable.contains(canonical) && self.default_bound.contains(canonical) {
            return Some(format!("<{}>::default()", w.spell));
        }
        None
    }

    /// Record 0075 gate 2: derive recipes for wrapped types the base rules
    /// cannot build, from constructor bindings generated in this run and
    /// from data-carrying variants, as a fixpoint. Deterministic: names in
    /// the fixed order `new`, inherent `from_*`, then alphabetical, `From`
    /// conversions last (record 0078); the first variant in declaration
    /// order. A recipe is only adopted when every
    /// argument has a fixture already.
    fn derive_recipes(&mut self, entries: &[Entry]) {
        // constructor candidates per owner: receiver none, returns Self or PolarsResult<Self>
        let mut ctors: BTreeMap<String, Vec<(String, OracleInfo, bool)>> = BTreeMap::new();
        // record 0078: a `From` conversion ranks after every inherent
        // constructor; its argument is a placeholder fixture (`Scalar::default()`
        // is a null scalar), where an inherent constructor's literal arguments
        // give the value the receiver's methods can act on
        let mut conversions: BTreeSet<(String, String)> = BTreeSet::new();
        for e in entries {
            if e.kind == "foreign_trait_impl" { if let Some(info) = &e.oracle { if let Some((owner, _)) = &info.owner { conversions.insert((owner.clone(), info.rune_name.clone())); } } }
        }
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
        for (owner, v) in ctors.iter_mut() {
            v.sort_by_key(|(n, _, _)| (if conversions.contains(&(owner.clone(), n.clone())) { 3 } else if n == "new" { 0 } else if n.starts_with("from_") { 1 } else { 2 }, n.clone()));
        }
        loop {
            let mut progress = false;
            let mut wanted: Vec<String> = self.world.wrappers.keys().filter(|c| self.rune_wrapped(c).is_none()).cloned().collect();
            wanted.sort();
            for canonical in wanted {
                let Some(w) = self.world.wrappers.get(&canonical) else { continue };
                let Some(s) = self.world.types.get(&canonical) else { continue };
                // rule 5a (record 0076): a producer on a typed source fixture
                // that names it comes first: it is a deliberately supplied
                // value, where a constructor's placeholder arguments may not be
                // valid for the type (`rand_bernoulli("x", 2, 1.5)`)
                let mut found: Option<Recipe> = self.producer_recipe(&canonical, entries, true);
                // rule 3: a constructor binding whose arguments all have fixtures
                for (name, info, fallible) in ctors.get(&canonical).cloned().unwrap_or_default() {
                    if found.is_some() { break; }
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
                // rule 5 (record 0076): a producer, a bound binding elsewhere
                // that returns this type, on a receiver whose fixture is typed
                // for it when a typed fixture lists the producer
                if found.is_none() {
                    found = self.producer_recipe(&canonical, entries, false);
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

    /// The producer recipe for a wrapped type: among generated `&self`
    /// bindings without parameters whose return is the type (directly, by
    /// reference, or through `PolarsResult`), on a receiver type that has
    /// a fixture, the first by shortest canonical path then alphabetical.
    /// The receiver fixture is the typed fixture that lists the producer's
    /// name, else the receiver type's default fixture.
    fn producer_recipe(&self, canonical: &str, entries: &[Entry], typed_only: bool) -> Option<Recipe> {
        let mut cands: Vec<(&Entry, &OracleInfo, bool, bool)> = Vec::new();
        for e in entries {
            if e.status != "generated" { continue }
            let Some(info) = &e.oracle else { continue };
            if info.receiver != "&self" || !info.params.is_empty() { continue }
            let Some((owner, _)) = &info.owner else { continue };
            if owner == canonical { continue }
            let ret = info.ret_canonical.as_deref().unwrap_or("");
            let (target, fallible) = match ty::parse(ret) {
                Ty::Path { path, args } if (path == "polars_error::PolarsResult" || path == "core::result::Result") && !args.is_empty() => (args[0].clone(), true),
                other => (other, false),
            };
            let (target, by_ref) = match target { Ty::Ref { inner, mutable: false } => (*inner, true), other => (other, false) };
            let hits = matches!(&target, Ty::Path { path, args } if args.is_empty() && path == canonical);
            if !hits { continue }
            cands.push((e, info, fallible, by_ref));
        }
        cands.sort_by_key(|(e, _, _, _)| (e.canonical_path.len(), e.canonical_path.clone()));
        for (e, info, fallible, by_ref) in cands {
            let (owner, _) = info.owner.as_ref().unwrap();
            let typed = TYPED_FIXTURES.iter().find(|f| f.0 == owner && f.4.iter().any(|n| *n == info.rune_name));
            if typed_only && typed.is_none() { continue }
            let (recv_rune, recv_rust) = match typed {
                Some(f) => (format!("fx::{}()", f.1), format!("{}()", f.1)),
                None => match (self.rune_wrapped(owner), self.rust_wrapped(owner)) { (Some(a), Some(b)) => (a, b), _ => continue },
            };
            let rune_call = format!("{recv_rune}.{}()", info.rune_name);
            let rune = if info.fallible { format!("match {rune_call} {{ Ok(v) => v, Err(e) => panic(`fixture: {} failed: ${{e}}`) }}", info.rune_name) } else { rune_call };
            let rust_call = format!("{}(&{recv_rust})", info.callee);
            let rust_call = if fallible { format!("match {rust_call} {{ Ok(v) => v, Err(e) => panic!(\"fixture: {} failed: {{e}}\") }}", info.rune_name) } else { rust_call };
            let rust = if by_ref { format!("{rust_call}.clone()") } else { rust_call };
            let via = match typed { Some(f) => format!(" on {}", f.1), None => String::new() };
            return Some(Recipe { rune, rust, kind: format!("producer {}{via}", e.canonical_path.rsplit("::").take(2).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("::")) });
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
                // record 0094: the categorical id alias (`u32` in the pinned source)
                "polars_dtype::categorical::catsize::CatSize" => Some("2u32".into()),
                "char" => Some("'x'".into()),
                "alloc::string::String" => Some("\"x\".to_string()".into()),
                "polars_utils::pl_str::PlSmallStr" => Some("p::PlSmallStr::from(\"x\")".into()),
                "polars_arrow::bitmap::immutable::Bitmap" => Some(match self.mask_len.get() { 1 => "polars_arrow::bitmap::Bitmap::from([false])".into(), 2 => "polars_arrow::bitmap::Bitmap::from([true, false])".into(), _ => "polars_arrow::bitmap::Bitmap::from([true, false, true])".into() }),
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
        // record 0076: an array alias is compared structurally, as the series
        // it converts to (name, dtype, every element, nulls), never by Debug,
        // whose formatting truncates and rounds
        if let Some(w) = self.world.wrappers.get(canonical) {
            if w.rule == "alias" && w.base.as_deref().is_some_and(|b| b == "polars_core::chunked_array::ChunkedArray" || b == "polars_core::chunked_array::logical::Logical") {
                return Some("crate_oracle::series_repr(&polars::prelude::IntoSeries::into_series(v.clone()))".into());
            }
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
        // record 0103: a listed `downcast_iter` result frames as the owned nested vectors the script receives
        if depth == 0 {
            if let Some(kind) = self.iter_native.borrow().clone() {
                let elem = SCALAR_CHUNKS.iter().find(|(_, k, ..)| *k == kind).map(|(.., oe)| oe.to_string()).unwrap_or_else(|| kind.clone());
                let own = match kind.as_str() { "bool" => "x", "str" => "x.map(|v| v.to_string())", "binary" | "binary_offset" => "x.map(|v| v.to_vec())", _ => "x.copied()" };
                let nested = ty::parse(&format!("alloc::vec::Vec<alloc::vec::Vec<core::option::Option<{elem}>>>"));
                return self.oracle_fmt(&nested, owner, depth + 1).map(|f| format!("{{ let __r: Vec<Vec<_>> = __r.map(|a| a.iter().map(|x| {own}).collect()).collect(); {f} }}"));
            }
        }
        // record 0102: a listed `downcast_as_array` result frames as the owned vector the script receives
        if depth == 0 {
            if let Some(kind) = self.array_native.borrow().clone() {
                let elem = SCALAR_CHUNKS.iter().find(|(_, k, ..)| *k == kind).map(|(.., oe)| oe.to_string()).unwrap_or_else(|| kind.clone());
                let own = match kind.as_str() { "bool" => "x", "str" => "x.map(|v| v.to_string())", "binary" | "binary_offset" => "x.map(|v| v.to_vec())", _ => "x.copied()" };
                let nested = ty::parse(&format!("alloc::vec::Vec<core::option::Option<{elem}>>"));
                return self.oracle_fmt(&nested, owner, depth + 1).map(|f| format!("{{ let __r: Vec<_> = __r.iter().map(|x| {own}).collect(); {f} }}"));
            }
        }
        // record 0101: a listed `downcast_get` result frames as the owned optional chunk the script receives
        if depth == 0 {
            if let Some(kind) = self.indexed_native.borrow().clone() {
                let elem = SCALAR_CHUNKS.iter().find(|(_, k, ..)| *k == kind).map(|(.., oe)| oe.to_string()).unwrap_or_else(|| kind.clone());
                let own = match kind.as_str() { "bool" => "x", "str" => "x.map(|v| v.to_string())", "binary" | "binary_offset" => "x.map(|v| v.to_vec())", _ => "x.copied()" };
                let nested = ty::parse(&format!("core::option::Option<alloc::vec::Vec<core::option::Option<{elem}>>>"));
                return self.oracle_fmt(&nested, owner, depth + 1).map(|f| format!("{{ let __r = __r.map(|a| a.iter().map(|x| {own}).collect::<Vec<_>>()); {f} }}"));
            }
        }
        match t {
            Ty::Tuple(ts) if ts.is_empty() => Some("\"()\".to_string()".into()),
            Ty::Tuple(ts) => {
                let parts: Option<Vec<String>> = ts.iter().enumerate().map(|(i, e)| self.oracle_fmt(e, owner, depth + 1).map(|f| format!("{{ let __r = __t.{i}; {f} }}"))).collect();
                parts.map(|p| format!("{{ let __t = __r; format!(\"({{}})\", [{}].iter().map(|e: &String| format!(\"{{}}:{{e}}\", e.len())).collect::<Vec<_>>().join(\", \")) }}", p.join(", ")))
            }
            Ty::Ref { inner, .. } if matches!(&**inner, Ty::Path { path, .. } if path == "str") => self.oracle_fmt(inner, owner, depth + 1),
            // record 0099: a listed chunk list frames as the nested options the script receives
            Ty::Ref { inner, .. } if depth == 0 && self.chunk_native.borrow().is_some() && matches!(&**inner, Ty::Path { path, args } if path == "alloc::vec::Vec" && args.len() == 1 && ["polars_arrow::array::ArrayRef", "alloc::boxed::Box<dyn polars_arrow::array::Array>"].contains(&args[0].render().as_str())) => {
                let n = self.chunk_native.borrow().clone().unwrap();
                // record 0100: a scalar owner's chunks, as the owned nested options the script receives
                if let Some((_, _, _, _, array, oracle_elem)) = SCALAR_CHUNKS.iter().find(|(_, k, ..)| *k == n) {
                    let nested = ty::parse(&format!("alloc::vec::Vec<alloc::vec::Vec<core::option::Option<{oracle_elem}>>>"));
                    let own = match n.as_str() { "bool" => "x", "str" => "x.map(|v| v.to_string())", _ => "x.map(|v| v.to_vec())" };
                    return self.oracle_fmt(&nested, owner, depth + 1).map(|f| format!("{{ let __r: Vec<Vec<Option<_>>> = __r.iter().map(|a| a.as_any().downcast_ref::<{array}>().expect(\"oracle: a {n} chunk\").iter().map(|x| {own}).collect()).collect(); {f} }}"));
                }
                let nested = ty::parse(&format!("alloc::vec::Vec<alloc::vec::Vec<core::option::Option<{n}>>>"));
                self.oracle_fmt(&nested, owner, depth + 1).map(|f| format!("{{ let __r: Vec<Vec<Option<{n}>>> = __r.iter().map(|a| a.as_any().downcast_ref::<polars_arrow::array::PrimitiveArray<{n}>>().expect(\"oracle: a numeric chunk\").iter().map(|x| x.copied()).collect()).collect(); {f} }}"))
            }
            Ty::Ref { inner, .. } => self.oracle_fmt(inner, owner, depth + 1).map(|f| format!("{{ let __r = (__r).clone(); {f} }}")),
            // record 0082: a borrowed slice frames its elements like a vector
            Ty::Slice(elem) => self.oracle_fmt(elem, owner, depth + 1).map(|f| format!("format!(\"[{{}}]\", __r.iter().map(|__r| {{ let __r = __r.clone(); let e: String = {f}; format!(\"{{}}:{{e}}\", e.len()) }}).collect::<Vec<_>>().join(\", \"))")),
            // record 0088: a `Cow` result frames as its owned value
            Ty::Path { path, args } if path == "alloc::borrow::Cow" && args.len() == 1 => self.oracle_fmt(&args[0], owner, depth + 1).map(|f| format!("{{ let __r = __r.into_owned(); {f} }}")),
            Ty::Path { .. } if self.world.iter_return_items.contains_key(&t.render()) => {
                // record 0087: a listed concrete iterator frames its items like a vector
                Some("format!(\"[{}]\", __r.map(|__r| { let e = format!(\"{}\", __r); format!(\"{}:{e}\", e.len()) }).collect::<Vec<_>>().join(\", \"))".into())
            }
            Ty::Path { path, args } => match path.as_str() {
                // record 0085: a validity bitmap frames its bits like a vector of bool
                "polars_arrow::bitmap::immutable::Bitmap" => Some("format!(\"[{}]\", __r.iter().map(|b| { let e = format!(\"{}\", b); format!(\"{}:{e}\", e.len()) }).collect::<Vec<_>>().join(\", \"))".into()),
                "bool" | "i64" | "f64" => Some("format!(\"{}\", __r)".into()),
                "f32" => Some("format!(\"{}\", __r as f64)".into()),
                // record 0093: a risky integer that does not fit a script integer is
                // the adapter's ConversionError, so a wrapping binding mismatches
                "u64" if self.hash_ret.get() => Some("format!(\"{:016x}\", __r)".into()),
                p if RISKY_INTS.contains(&p) => Some("match i64::try_from(__r) { Ok(__v) => format!(\"{}\", __v), Err(_) => \"<<ERR:ConversionError>>\".to_string() }".into()),
                p if INT_NARROW.contains(&p) => Some("format!(\"{}\", __r as i64)".into()),
                "polars_utils::index::IdxSize" => Some("format!(\"{}\", __r as i64)".into()),
                "polars_dtype::categorical::catsize::CatSize" => Some("format!(\"{}\", __r as i64)".into()),
                "char" | "str" | "alloc::string::String" | "polars_utils::pl_str::PlSmallStr" => Some("format!(\"{}\", __r.to_string())".into()),
                "Self" => self.oracle_fmt(&Ty::Path { path: owner?.to_string(), args: vec![] }, owner, depth + 1),
                "core::option::Option" if args.len() == 1 => self.oracle_fmt(&args[0], owner, depth + 1).map(|f| format!("match __r {{ Some(__r) => {{ let s: String = {f}; format!(\"Some({{}}:{{s}})\", s.len()) }}, None => \"None\".to_string() }}")),
                // record 0096: a null-aware result frames as the merged vector of options the script receives
                "either::Either" if args.len() == 2 && args[1].render() == format!("alloc::vec::Vec<core::option::Option<{}>>", match &args[0] { Ty::Path { path, args: a } if path == "alloc::vec::Vec" && a.len() == 1 => a[0].render(), _ => String::new() }) => self.oracle_fmt(&args[1], owner, depth + 1).map(|f| format!("{{ let __r: Vec<Option<_>> = __r.either(|__v| __v.into_iter().map(Some).collect(), |__v| __v); {f} }}")),
                "alloc::vec::Vec" if args.len() == 1 => self.oracle_fmt(&args[0], owner, depth + 1).map(|f| format!("format!(\"[{{}}]\", __r.into_iter().map(|__r| {{ let e: String = {f}; format!(\"{{}}:{{e}}\", e.len()) }}).collect::<Vec<_>>().join(\", \"))")),
                "polars_error::PolarsResult" | "core::result::Result" if !args.is_empty() => self.oracle_fmt(&args[0], owner, depth + 1).map(|f| format!("match __r {{ Ok(__r) => {f}, Err(e) => format!(\"<<ERR:{{}}>>\", crate_oracle::error_kind(&e)) }}")),
                _ => {
                    let show = self.show(path)?;
                    Some(format!("{{ let v = &__r; ({show}).to_text() }}"))
                }
            },
            Ty::Generic(g) if g == "Self" => self.oracle_fmt(&Ty::Path { path: "Self".into(), args: vec![] }, owner, depth + 1),
            // an iterator return (record 0077): the oracle drives it to a
            // vector and frames the elements like any vector
            Ty::Impl(_) => {
                let (item, _, _) = iterator_return(t)?;
                let f = self.oracle_fmt(&item, owner, depth + 1)?;
                Some(format!("format!(\"[{{}}]\", __r.into_iter().map(|__r| {{ let e: String = {f}; format!(\"{{}}:{{e}}\", e.len()) }}).collect::<Vec<_>>().join(\", \"))"))
            }
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
                    return Some(format!("match rune::from_value::<Option<rune::Value>>(v) {{ Ok(Some(v)) => ({f}).map(|s| format!(\"Some({{}}:{{s}})\", s.len())), Ok(None) => Ok(\"None\".to_string()), Err(e) => Err(e.to_string()) }}"));
                }
                if let Some(inner) = r.strip_prefix("Vec<").and_then(|s| s.strip_suffix('>')) {
                    let item_of = ret_canonical.and_then(|t| iterator_return(t)).map(|(i, _, _)| i);
                    let ic = match ret_canonical { Some(Ty::Path { path, args }) if path == "alloc::vec::Vec" && args.len() == 1 => Some(&args[0]), Some(Ty::Ref { inner, .. }) if matches!(&**inner, Ty::Slice(_)) => match &**inner { Ty::Slice(e) => Some(&**e), _ => None }, _ => item_of.as_ref() };
                    let f = self.script_fmt(inner, ic, owner, depth + 1)?;
                    // length-framed elements: equal-length vectors whose element texts would join alike stay apart
                    return Some(format!("match rune::from_value::<Vec<rune::Value>>(v) {{ Ok(items) => items.into_iter().map(|v| {f}).collect::<Result<Vec<_>, _>>().map(|s| format!(\"[{{}}]\", s.iter().map(|e| format!(\"{{}}:{{e}}\", e.len())).collect::<Vec<_>>().join(\", \"))), Err(e) => Err(e.to_string()) }}"));
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
    for recipe in &world.release.callback_recipe {
        for used in &recipe.uses {
            let safe = entries.iter().any(|entry| entry.status == "generated" && entry.bindings.iter().any(|binding| &binding.rune == used && binding.route_reason.as_deref() != Some("engine thread") && binding.route_reason.as_deref() != Some("executes callbacks") && binding.route_reason.as_deref() != Some("callback")));
            assert!(safe, "callback recipe {} calls routed or missing binding {used}", recipe.signature);
        }
    }
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
    let mut o = Oracle { world, debuggable, defaultable, default_bound, shown: std::cell::RefCell::new(BTreeSet::new()), recipes: BTreeMap::new(), no_recipe: BTreeMap::new(), mask_len: std::cell::Cell::new(3), hash_ret: std::cell::Cell::new(false), chunk_native: std::cell::RefCell::new(None), indexed_native: std::cell::RefCell::new(None), array_native: std::cell::RefCell::new(None), iter_native: std::cell::RefCell::new(None) };
    o.derive_recipes(entries);
    let mut cases = Vec::new();
    let mut skipped: Vec<(String, String)> = Vec::new();
    // Every binding of every generated entry gets exactly one disposition:
    // a case with its own id, or a reason. A trait method bound on several
    // implementors is visited once per binding.
    let plan: Vec<(usize, usize)> = entries.iter().enumerate().filter(|(_, e)| e.status == "generated").flat_map(|(ei, e)| (0..e.bindings.len().max(1)).map(move |bi| (ei, bi))).collect();
    for (ei, bi) in plan {
        let e = &mut entries[ei];
        let single = e.bindings.len() <= 1;
        let mut skip = |e: &mut Entry, why: String| {
            if let Some(b) = e.bindings.get_mut(bi) { b.disposition = Some(why.clone()); }
            if single { e.execution = Some(why.clone()); }
            skipped.push((e.canonical_path.clone(), why));
        };
        let Some(mut info) = e.oracle.clone() else {
            skip(e, "no call information".into());
            continue;
        };
        let id = e.bindings.get(bi).map(|b| b.id.clone()).unwrap_or_else(|| sanitize(&e.canonical_path).to_lowercase());
        // the route decides the callee and the receiver spelling, for one
        // binding or many; an instantiation carries its own information
        let mut own_info = false;
        if let Some(b) = e.bindings.get(bi) {
            if let Some(i) = &b.info { info = i.clone(); own_info = true; }
            if let Some(cal) = b.callee.clone() { info.callee = cal; }
            info.deref = b.route == "deref";
        }
        // a binding on a specific implementor: that receiver, not the first one with a fixture
        if !single && !own_info {
            let Some(r) = e.bindings[bi].receiver.clone() else { skip(e, "binding without a receiver".into()); continue };
            if o.rune_wrapped(&r).is_none() || o.rust_wrapped(&r).is_none() {
                skip(e, format!("no fixture for the receiver type ({})", o.no_recipe.get(&r).cloned().unwrap_or_else(|| "no fixture".into())));
                continue;
            }
            if e.bindings[bi].callee.is_none() {
                if let Some((c0, _)) = info.owner.clone() {
                    info.callee = info.callee.replacen(&format!("<{}", world.wrappers[&c0].spell), &format!("<{}", world.wrappers[&r].spell), 1);
                    // a `Self` in the return names this receiver's wrapper, not the first implementor's
                    let (w0, wr) = (world.wrappers[&c0].rust.clone(), world.wrappers[&r].rust.clone());
                    if w0 != wr { info.ret_rust = info.ret_rust.replace(&w0, &wr); }
                }
            }
            info.rune_owner = Some(rune_path(&world.wrappers[&r]));
            info.owner = Some((r.clone(), world.wrappers[&r].rust.clone()));
            info.implementors.clear();
        }
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
            let d = format!("case{}", o.recipe_note(&e.canonical_path, &[script.as_str()]));
            if let Some(b) = e.bindings.get_mut(bi) { b.disposition = Some(d.clone()); b.case_id = Some(id.clone()); }
            if single { e.execution = Some(d); }
            cases.push(OracleCase { id, path: e.canonical_path.clone(), script, has_receiver: false, fmt, oracle, unordered: false, policy: "ordered (protocol)".into() });
            continue;
        }
        // receiver: a single-binding trait method takes its one implementor
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
            if let Some(signature) = shape.strip_prefix("callback:") {
                match world.release.callback_recipe.iter().find(|r| r.signature == signature) {
                    Some(recipe) => { rune_args.push(recipe.rune.clone()); rust_args.push(recipe.rust.clone()); }
                    None => { missing = Some(format!("no callback-safe recipe for the closure signature ({signature})")); break; }
                }
                continue;
            }
            o.mask_len.set(if shape.contains("mask1") { 1 } else if shape.contains("mask2") { 2 } else { 3 });
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
            o.hash_ret.set(world.release.hash_tokens.iter().any(|h| h.key == e.key && h.path == e.canonical_path && h.direction == "return"));
            *o.iter_native.borrow_mut() = world.release.iter_snapshots.iter().find(|m| m.key == e.key && m.path == e.canonical_path).and_then(|m| owner.and_then(|a| world.wrappers.get(a)).and_then(|w| m.native_for(&w.identity)).map(String::from));
            *o.array_native.borrow_mut() = world.release.array_snapshots.iter().find(|m| m.key == e.key && m.path == e.canonical_path).and_then(|m| owner.and_then(|a| world.wrappers.get(a)).and_then(|w| m.native_for(&w.identity)).map(String::from));
            *o.indexed_native.borrow_mut() = world.release.indexed_chunk_snapshots.iter().find(|m| m.key == e.key && m.path == e.canonical_path).and_then(|m| owner.and_then(|a| world.wrappers.get(a)).and_then(|w| m.native_for(&w.identity)).map(String::from));
            *o.chunk_native.borrow_mut() = world.release.chunk_snapshots.iter().find(|m| m.key == e.key && m.path == e.canonical_path).and_then(|m| owner.and_then(|a| world.wrappers.get(a)).and_then(|w| m.native_for(&w.identity)).map(String::from));
            let of = match &ret_ty { None => Some("\"()\".to_string()".to_string()), Some(t) => o.oracle_fmt(t, owner, 0) };
            o.hash_ret.set(false);
            *o.chunk_native.borrow_mut() = None;
            *o.indexed_native.borrow_mut() = None;
            *o.array_native.borrow_mut() = None;
            *o.iter_native.borrow_mut() = None;
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
            let callback = info.params[i].0.starts_with("callback:");
            let raw = info.params[i].1.as_str();
            let mut_borrow = callback && raw.starts_with("&mut ");
            fixtures.push((format!("{}__a{i}", if mut_borrow { "mut " } else { "" }), a.clone()));
            passed.push(format!("{}__a{i}", if mut_borrow { "&mut " } else if callback && raw.starts_with('&') { "&" } else { "" }));
        }
        let args_s = passed.join(", ");
        let (script, fmt, oracle) = if mutating {
            let c = owner.unwrap();
            let Some(show) = o.show(c) else { skip(e, "receiver type has no comparison".into()); continue };
            let w = &world.wrappers[c];
            o.shown.borrow_mut().insert(c.to_string());
            // script: (receiver after the call, return); both compared
            let script = if ASSIGN_OPS.iter().any(|(_, _, op, _)| *op == info.rune_name) {
                // an assignment operator: the statement form, no return value
                format!("{} pub fn main(__fx) {{ let a = __fx[0]; a {} {args_r}; let r = (); ((a, r), ()) }}", setup_fn(&setup_rune), info.rune_name)
            } else {
                format!("{} pub fn main(__fx) {{ let a = __fx[0]; let r = a.{}({args_r}); ((a, r), ()) }}", setup_fn(&setup_rune), info.rune_name)
            };
            // receiver state after the call and the return are compared together;
            // an error return keeps the receiver in the comparison as `ret=ERR:kind`
            let fmt = format!("{{ let (a, r) = match rune::from_value::<(rune::Value, rune::Value)>(v) {{ Ok(x) => x, Err(e) => return crate_oracle::Side::Broken(e.to_string()) }}; let recv = match rnx_polars::generated::fixtures::show_{}(&a) {{ Ok(s) => s.to_text(), Err(e) => return crate_oracle::Side::Broken(e) }}; let ret = {{ let v = r; {} }}; match ret {{ crate_oracle::Side::Value(s) => crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"recv={{recv}};ret={{}}\", s.to_text()))), crate_oracle::Side::Error(k) => crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"recv={{recv}};ret=ERR:{{k}}\"))), other => other }} }}", w.rust.to_lowercase(), if info.fallible { value_side(&ret_fmt_script) } else { plain_side(&ret_fmt_script) });
            let mut fx = vec![("__o".to_string(), recv_rust.clone().unwrap())];
            fx.extend(fixtures.iter().cloned());
            let oracle = staged(&fx, &format!("{{ let mut __o = __o; let __r = {}({}__o, {args_s}); let ret = {}; let recv = ({{ let v = &__o; {show} }}).to_text(); let ret = ret.replace(\"<<ERR:\", \"ERR:\").replace(\">>\", \"\"); crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"recv={{recv}};ret={{ret}}\"))) }}", info.callee, if info.deref { "&mut *" } else { "&mut " }, ret_fmt_rust).replace(", )", ")"));
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
                "&self" if info.deref => format!("let __r = {}(&*__recv, {args_s});", info.callee),
                "&self" => format!("let __r = {}(&__recv, {args_s});", info.callee),
                _ => { skip(e, "receiver form".into()); continue }
            }.replace(", )", ")");
            let oracle_body = match &top {
                Some((_, true)) => format!("match __r {{ Ok(__r) => crate_oracle::Side::Value({{ let v = &__r; {ret_fmt_rust} }}), Err(e) => crate_oracle::Side::Error(crate_oracle::error_kind(&e)) }}"),
                Some((_, false)) => format!("crate_oracle::Side::Value({{ let v = &__r; {ret_fmt_rust} }})"),
                None => format!("crate_oracle::collapse({ret_fmt_rust})"),
            };
            // record 0078: a `From` constructor with a wrapped, clonable source
            // also compares the source after the call (the Rune value is
            // cloned out, so it must be unchanged); a scalar source is copied
            let from_source = if info.callee.contains(" as From<") && info.receiver == "none" && info.params.len() == 1 && !info.params[0].1.trim().starts_with('&') {
                let src = info.params[0].1.trim().to_string();
                match (world.wrappers.get(&src), o.show(&src), world.clonable.contains(&src)) {
                    (Some(sw), Some(show), true) => Some((sw.rust.to_lowercase(), show, src)),
                    _ => None,
                }
            } else { None };
            match from_source {
                Some((src_fn, src_show, src)) => {
                    o.shown.borrow_mut().insert(src);
                    let script = format!("{} pub fn main(__fx) {{ let s = __fx[0]; let r = {}::{}(s); ((r, s), ()) }}", setup_fn(&setup_rune), info.rune_owner.as_ref().unwrap(), info.rune_name);
                    let fmt = format!("{{ let (v, s) = match rune::from_value::<(rune::Value, rune::Value)>(v) {{ Ok(x) => x, Err(e) => return crate_oracle::Side::Broken(e.to_string()) }}; let src = match rnx_polars::generated::fixtures::show_{src_fn}(&s) {{ Ok(x) => x.to_text(), Err(e) => return crate_oracle::Side::Broken(e) }}; let ret = {{ {fmt} }}; match ret {{ crate_oracle::Side::Value(r) => crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"ret={{}};src={{src}}\", r.to_text()))), crate_oracle::Side::Error(k) => crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"ret=ERR:{{k}};src={{src}}\"))), other => other }} }}");
                    let call = call.replace("(__a0)", "(__a0.clone())");
                    let body = format!("{{ let __src = ({{ let v = &__a0; {src_show} }}).to_text(); {call} let ret = {oracle_body}; match ret {{ crate_oracle::Side::Value(r) => crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"ret={{}};src={{__src}}\", r.to_text()))), crate_oracle::Side::Error(k) => crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"ret=ERR:{{k}};src={{__src}}\"))), other => other }} }}");
                    (script, fmt, staged(&fx, &body))
                }
                None => (script, fmt, staged(&fx, &format!("{{ {call} {oracle_body} }}"))),
            }
        };
        let d = format!("case{}", o.recipe_note(&e.canonical_path, &[script.as_str()]));
        if let Some(b) = e.bindings.get_mut(bi) { b.disposition = Some(d.clone()); b.case_id = Some(id.clone()); }
        if single { e.execution = Some(d); }
        cases.push(OracleCase { id, path: e.canonical_path.clone(), script, has_receiver: !mutating && (info.receiver == "self" || info.receiver == "&self"), fmt, oracle, unordered, policy });
    }
    // callable-level disposition of a multi-binding entry: a case if any
    // binding has one, else the first binding's reason
    for e in entries.iter_mut() {
        if e.status == "generated" && e.bindings.len() > 1 {
            let n = e.bindings.iter().filter(|b| b.case_id.is_some()).count();
            e.execution = Some(if n > 0 { format!("case (on {n} of {} receivers)", e.bindings.len()) } else { e.bindings[0].disposition.clone().unwrap_or_else(|| "no disposition".into()) });
        }
    }
    // fixtures module inside the adapter
    let mut fixtures = String::from("//! GENERATED by tools/polars-gen: fixtures for the generated oracle tests.\n//! Only built with the `test-support` feature.\n#![allow(dead_code, non_snake_case, unused_imports, clippy::all)]\nuse super::types::*;\nuse crate::oracle as crate_oracle;\nuse crate::{DataFrame, Expr, LazyFrame, LazyGroupBy};\nuse polars::prelude as p;\nuse polars::prelude::{IntoColumn, IntoLazy};\nuse rnx::rune;\nuse values::*;\n\n/// The Polars values the oracle tests use on both sides.\npub mod values {\n    use polars::prelude as p;\n    use polars::prelude::*;\n");
    for (_, name, expr, _, _) in TYPED_FIXTURES {
        writeln!(fixtures, "    pub fn {name}() -> p::Series {{ {expr} }}").unwrap();
    }
    for (_, name, expr, _) in FIXTURES {
        let ret = match *name { "df" => "p::DataFrame", "lf" => "p::LazyFrame", "expr" => "p::Expr", "series" => "p::Series", "column" => "p::Column", "dtype" => "p::DataType", "field" => "p::Field", "group_by" => "p::LazyGroupBy", "null_chunked" => "p::NullChunked", "categories" => "polars_dtype::categorical::Categories", "frozen_categories" => "polars_dtype::categorical::FrozenCategories", "categorical_mapping" => "polars_dtype::categorical::CategoricalMapping", _ => unreachable!("core fixture {name} has no return type") };
        writeln!(fixtures, "    pub fn {name}() -> {ret} {{ {expr} }}").unwrap();
    }
    fixtures.push_str("}\n\n");
    // a synthetic inventory (self-test) may lack the fixture types; the
    // drift test and the oracle build catch a real inventory missing one
    for (canonical, name, _, _) in FIXTURES {
        let Some(w) = world.wrappers.get(*canonical) else { continue };
        writeln!(fixtures, "#[rune::function(path = {name})]\nfn fx_{name}() -> {} {{ {}(values::{name}()) }}", w.rust, w.rust).unwrap();
    }
    for (canonical, name, _, _, _) in TYPED_FIXTURES {
        let Some(w) = world.wrappers.get(*canonical) else { continue };
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
    for (_, name, _, _, _) in TYPED_FIXTURES {
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

/// The engine's counters are process-global, so the two tests of this
/// binary never interleave: whichever starts second waits.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn generated_bindings_match_polars() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
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
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
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
    // an explicit order the release file does not record: deterministic on
    // every release, so the control never depends on an unspecified order
    fn leftright_order_join_reversed() -> Side {
        let mut args = p::JoinArgs::new(p::JoinType::Inner); args.maintain_order = p::MaintainOrderJoin::LeftRight;
        let d = lf().join(lf(), [expr()], [expr()], args).into_lazy().collect().unwrap().reverse();
        Side::Value(crate_oracle::frame_repr(&d).prefixed("collected "))
    }
    let join_controls: &[(&str, Case, &[Outcome], bool)] = &[
        ("permuted rows pass for a justified unordered join iff the policy says unordered", Case { id: "j1", path: "control", script: "pub fn setup() { [fx::lf(), fx::lf(), fx::expr(), fx::expr()] } pub fn main(__fx) { let a = __fx[0]; (a.inner_join(__fx[1], __fx[2], __fx[3]), ()) }", has_receiver: false, unordered: J1_UNORDERED, policy: "@J1_POLICY@", fmt: lf_side, oracle: || ran(joined_reversed) }, j1_expected, j1_pass),
        ("permuted rows fail for the same join with an explicit order", Case { id: "j2", path: "control", script: "pub fn setup() { [fx::lf(), fx::lf(), polars::JoinArgs::default_().with_maintain_order(polars::MaintainOrderJoin::Left())] } pub fn main(__fx) { let a = __fx[0]; let args = __fx[2]; let j = match a.join(__fx[1], [fx::expr()], [fx::expr()], args) { Ok(v) => v, Err(e) => panic(`join: ${e}`) }; (j, ()) }", has_receiver: false, unordered: @J2@, policy: "@J2_POLICY@", fmt: lf_side, oracle: || ran(ordered_join_reversed) }, &[Outcome::Mismatch], false),
        ("permuted rows fail for an unrelated ordered operation", Case { id: "j3", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { let a = fx::df(); (a, ()) }", has_receiver: false, unordered: false, policy: "ordered", fmt: df_side, oracle: || ran(reversed) }, &[Outcome::Mismatch], false),
        ("permuted rows fail for the same join with an unrecognized configuration", Case { id: "j4", path: "control", script: "pub fn setup() { [fx::lf(), fx::lf(), polars::JoinArgs::default_().with_maintain_order(polars::MaintainOrderJoin::LeftRight())] } pub fn main(__fx) { let a = __fx[0]; let args = __fx[2]; let j = match a.join(__fx[1], [fx::expr()], [fx::expr()], args) { Ok(v) => v, Err(e) => panic(`join: ${e}`) }; (j, ()) }", has_receiver: false, unordered: @J4@, policy: "@J4_POLICY@", fmt: lf_side, oracle: || ran(leftright_order_join_reversed) }, &[Outcome::Mismatch], false),
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
    // the null-series comparator: name, dtype and length are the whole
    // logical content of an all-null array, and each must be distinguished
    fn null_side(v: Value) -> Side { rnx_polars::generated::fixtures::show_w_polars_core__series__implementations__null__nullchunked(&v).map(Side::Value).unwrap_or_else(Side::Broken) }
    let null_controls: &[(&str, Case)] = &[
        ("a null series of another length fails", Case { id: "n1", path: "control", script: "pub fn setup() { [fx::null_chunked()] } pub fn main(__fx) { (__fx[0], ()) }", has_receiver: false, unordered: false, policy: "control", fmt: null_side, oracle: || Staged::Ran(Side::Value(crate_oracle::Repr::Text("x:Null:len=3".into()))) }),
        ("a null series of another name fails", Case { id: "n2", path: "control", script: "pub fn setup() { [fx::null_chunked()] } pub fn main(__fx) { (__fx[0], ()) }", has_receiver: false, unordered: false, policy: "control", fmt: null_side, oracle: || Staged::Ran(Side::Value(crate_oracle::Repr::Text("y:Null:len=2".into()))) }),
        ("a null series of another dtype fails", Case { id: "n3", path: "control", script: "pub fn setup() { [fx::null_chunked()] } pub fn main(__fx) { (__fx[0], ()) }", has_receiver: false, unordered: false, policy: "control", fmt: null_side, oracle: || Staged::Ran(Side::Value(crate_oracle::Repr::Text("x:Int64:len=2".into()))) }),
    ];
    for (label, case) in null_controls {
        let (outcome, detail) = h.run(case);
        if outcome != Outcome::Mismatch { failures.push(format!("{label}: got {} ({detail})", outcome.name())); }
    }
    // the array comparator (record 0076): an alias array is compared as the
    // series it converts to, element by element; each control changes what
    // Debug's formatting would hide
    fn i64_side(v: Value) -> Side { rnx_polars::generated::fixtures::show_w_polars_core__datatypes__int64chunked(&v).map(Side::Value).unwrap_or_else(Side::Broken) }
    fn f64_side(v: Value) -> Side { rnx_polars::generated::fixtures::show_w_polars_core__datatypes__float64chunked(&v).map(Side::Value).unwrap_or_else(Side::Broken) }
    fn list_side(v: Value) -> Side { rnx_polars::generated::fixtures::show_w_polars_core__datatypes__listchunked(&v).map(Side::Value).unwrap_or_else(Side::Broken) }
    fn struct_side(v: Value) -> Side { rnx_polars::generated::fixtures::show_w_polars_core__chunked_array__struct___structchunked(&v).map(Side::Value).unwrap_or_else(Side::Broken) }
    fn long_changed() -> Staged {
        let mut v: Vec<i64> = (0..40).collect();
        v[20] = 21;
        Staged::Ran(Side::Value(crate_oracle::series_repr(&p::Series::new("x".into(), v))))
    }
    fn null_moved() -> Staged { Staged::Ran(Side::Value(crate_oracle::series_repr(&p::Series::new("x".into(), [None, Some(1i64), Some(3)])))) }
    fn float_alike() -> Staged { Staged::Ran(Side::Value(crate_oracle::series_repr(&p::Series::new("x".into(), [0.3f64])))) }
    fn long_same() -> Staged { Staged::Ran(Side::Value(crate_oracle::series_repr(&series_long()))) }
    let array_controls: &[(&str, Case, Outcome)] = &[
        ("a changed element outside Debug's window fails", Case { id: "a1", path: "control", script: "pub fn setup() { [fx::series_long()] } pub fn main(__fx) { (match __fx[0].i64() { Ok(v) => v, Err(e) => panic(`i64: ${e}`) }, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: i64_side, oracle: long_changed }, Outcome::Mismatch),
        ("a moved null fails", Case { id: "a2", path: "control", script: "pub fn setup() { [fx::series_nulls()] } pub fn main(__fx) { (match __fx[0].i64() { Ok(v) => v, Err(e) => panic(`i64: ${e}`) }, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: i64_side, oracle: null_moved }, Outcome::Mismatch),
        ("floats that display alike fail", Case { id: "a3", path: "control", script: "pub fn setup() { [fx::series_float_sum()] } pub fn main(__fx) { (match __fx[0].f64() { Ok(v) => v, Err(e) => panic(`f64: ${e}`) }, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: f64_side, oracle: float_alike }, Outcome::Mismatch),
        ("a changed inner element of a list cell, beyond Debug's ellipsis, fails", Case { id: "a5", path: "control", script: "pub fn setup() { [fx::series_list_long()] } pub fn main(__fx) { (match __fx[0].list() { Ok(v) => v, Err(e) => panic(`list: ${e}`) }, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: list_side, oracle: || { let mut inner: Vec<i64> = (0..40).collect(); inner[20] = 999; Staged::Ran(Side::Value(crate_oracle::series_repr(&p::Series::new("x".into(), [p::Series::new("i".into(), inner)])))) } }, Outcome::Mismatch),
        ("a null moved inside a list cell fails", Case { id: "a6", path: "control", script: "pub fn setup() { [fx::series_list_nulls()] } pub fn main(__fx) { (match __fx[0].list() { Ok(v) => v, Err(e) => panic(`list: ${e}`) }, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: list_side, oracle: || Staged::Ran(Side::Value(crate_oracle::series_repr(&p::Series::new("x".into(), [p::Series::new("i".into(), [None, Some(1i64), Some(3)])])))) }, Outcome::Mismatch),
        ("a changed struct field value fails", Case { id: "a7", path: "control", script: "pub fn setup() { [fx::series_struct()] } pub fn main(__fx) { (match __fx[0].struct_() { Ok(v) => v, Err(e) => panic(`struct: ${e}`) }, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: struct_side, oracle: || Staged::Ran(Side::Value(crate_oracle::series_repr(&p::IntoSeries::into_series(p::df!("x" => [1i64, 2, 4], "y" => ["a", "b", "c"], "z" => [1.5f64, 2.5, 3.5]).unwrap().into_struct("x".into()))))) }, Outcome::Mismatch),
        ("a null struct against a valid struct with null fields fails", Case { id: "a9", path: "control", script: "pub fn setup() { [fx::series_struct_null_field()] } pub fn main(__fx) { (match __fx[0].struct_() { Ok(v) => v, Err(e) => panic(`struct: ${e}`) }, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: struct_side, oracle: || Staged::Ran(Side::Value(crate_oracle::series_repr(&p::Series::full_null("s".into(), 1, series_struct_null_field().dtype())))) }, Outcome::Mismatch),
        ("a null struct inside a list cell fails", Case { id: "a10", path: "control", script: "pub fn setup() { [fx::series_list_of_struct()] } pub fn main(__fx) { (match __fx[0].list() { Ok(v) => v, Err(e) => panic(`list: ${e}`) }, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: list_side, oracle: || Staged::Ran(Side::Value(crate_oracle::series_repr(&p::Series::new("l".into(), [p::Series::full_null("s".into(), 1, series_struct_null_field().dtype())])))) }, Outcome::Mismatch),
        ("the same struct with a null field matches", Case { id: "a11", path: "control", script: "pub fn setup() { [fx::series_struct_null_field()] } pub fn main(__fx) { (match __fx[0].struct_() { Ok(v) => v, Err(e) => panic(`struct: ${e}`) }, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: struct_side, oracle: || Staged::Ran(Side::Value(crate_oracle::series_repr(&series_struct_null_field()))) }, Outcome::Match),
        ("the same struct matches", Case { id: "a8", path: "control", script: "pub fn setup() { [fx::series_struct()] } pub fn main(__fx) { (match __fx[0].struct_() { Ok(v) => v, Err(e) => panic(`struct: ${e}`) }, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: struct_side, oracle: || Staged::Ran(Side::Value(crate_oracle::series_repr(&series_struct()))) }, Outcome::Match),
        ("the same long array matches", Case { id: "a4", path: "control", script: "pub fn setup() { [fx::series_long()] } pub fn main(__fx) { (match __fx[0].i64() { Ok(v) => v, Err(e) => panic(`i64: ${e}`) }, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: i64_side, oracle: long_same }, Outcome::Match),
    ];
    for (label, case, expected) in array_controls {
        let (outcome, detail) = h.run(case);
        if outcome != *expected { failures.push(format!("{label}: got {} ({detail})", outcome.name())); }
    }
    // record 0077: the materialization bound through a real binding
    // (`DataFrame::materialized_column_iter`, three columns) with the
    // test-support limit; the receiver stays usable after a refusal
    fn cols_side(v: Value) -> Side { match rune::from_value::<Result<Vec<rune::Value>, rune::Value>>(v) { Ok(Ok(items)) => { let mut out = Vec::new(); for i in items { match rnx_polars::generated::fixtures::show_w_polars_core__series__series(&i) { Ok(r) => out.push(r.to_text()), Err(e) => return Side::Broken(e) } } Side::Value(crate_oracle::Repr::Text(format!("[{}]", out.iter().map(|e| format!("{}:{e}", e.len())).collect::<Vec<_>>().join(", ")))) } Ok(Err(e)) => crate_oracle::rune_error_kind(&e).map(Side::Error).unwrap_or_else(Side::Broken), Err(e) => Side::Broken(e.to_string()) } }
    fn cols_text() -> String { format!("[{}]", df().materialized_column_iter().map(|c| { let t = crate_oracle::series_repr(c).to_text(); format!("{}:{t}", t.len()) }).collect::<Vec<_>>().join(", ")) }
    let bound_controls: &[(&str, Case, Outcome)] = &[
        ("a bound below the length refuses on both sides with MaterializeLimit", Case { id: "b1", path: "control", script: "pub fn setup() { [fx::df()] } pub fn main(__fx) { polars::set_materialize_limit(2); let r = __fx[0].materialized_column_iter(); polars::set_materialize_limit(0); (r, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: cols_side, oracle: || Staged::Ran(Side::Error("MaterializeLimit".into())) }, Outcome::BothError),
        ("a bound equal to the length succeeds", Case { id: "b2", path: "control", script: "pub fn setup() { [fx::df()] } pub fn main(__fx) { polars::set_materialize_limit(3); let r = __fx[0].materialized_column_iter(); polars::set_materialize_limit(0); (r, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: cols_side, oracle: || Staged::Ran(Side::Value(crate_oracle::Repr::Text(cols_text()))) }, Outcome::Match),
        ("the receiver is usable after a refusal", Case { id: "b3", path: "control", script: "pub fn setup() { [fx::df()] } pub fn main(__fx) { let a = __fx[0]; polars::set_materialize_limit(2); let _ = a.materialized_column_iter(); polars::set_materialize_limit(0); (a.materialized_column_iter(), ()) }", has_receiver: false, unordered: false, policy: "control", fmt: cols_side, oracle: || Staged::Ran(Side::Value(crate_oracle::Repr::Text(cols_text()))) }, Outcome::Match),
        ("a wrong bound-refusal claim fails: value against MaterializeLimit", Case { id: "b4", path: "control", script: "pub fn setup() { [fx::df()] } pub fn main(__fx) { (__fx[0].materialized_column_iter(), ()) }", has_receiver: false, unordered: false, policy: "control", fmt: cols_side, oracle: || Staged::Ran(Side::Error("MaterializeLimit".into())) }, Outcome::Mismatch),
    ];
    for (label, case, expected) in bound_controls {
        let (outcome, detail) = h.run(case);
        if outcome != *expected { failures.push(format!("{label}: got {} ({detail})", outcome.name())); }
    }
    // length-framed vectors: equal-length string vectors whose element
    // texts would join alike stay apart; a changed and a reordered element fail
    fn strs_side(v: Value) -> Side { match rune::from_value::<Vec<String>>(v) { Ok(items) => Side::Value(crate_oracle::Repr::Text(format!("[{}]", items.iter().map(|e| format!("{}:{e}", e.len())).collect::<Vec<_>>().join(", ")))), Err(e) => Side::Broken(e.to_string()) } }
    let framed = |v: &[&str]| format!("[{}]", v.iter().map(|e| format!("{}:{e}", e.len())).collect::<Vec<_>>().join(", "));
    let seq_controls: Vec<(&str, Case, Outcome)> = vec![
        ("the string collision is a mismatch", Case { id: "q1", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { ([\"a, b\", \"c\"], ()) }", has_receiver: false, unordered: false, policy: "control", fmt: strs_side, oracle: || Staged::Ran(Side::Value(crate_oracle::Repr::Text(format!("[{}]", ["a", "b, c"].iter().map(|e| format!("{}:{e}", e.len())).collect::<Vec<_>>().join(", "))))) }, Outcome::Mismatch),
        ("a changed element fails", Case { id: "q2", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { ([\"a\", \"b\"], ()) }", has_receiver: false, unordered: false, policy: "control", fmt: strs_side, oracle: || Staged::Ran(Side::Value(crate_oracle::Repr::Text(format!("[{}]", ["a", "c"].iter().map(|e| format!("{}:{e}", e.len())).collect::<Vec<_>>().join(", "))))) }, Outcome::Mismatch),
        ("reordered elements fail", Case { id: "q3", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { ([\"a\", \"b\"], ()) }", has_receiver: false, unordered: true, policy: "control", fmt: strs_side, oracle: || Staged::Ran(Side::Value(crate_oracle::Repr::Text(format!("[{}]", ["b", "a"].iter().map(|e| format!("{}:{e}", e.len())).collect::<Vec<_>>().join(", "))))) }, Outcome::Mismatch),
        ("the same vector matches", Case { id: "q4", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { ([\"a, b\", \"c\"], ()) }", has_receiver: false, unordered: false, policy: "control", fmt: strs_side, oracle: || Staged::Ran(Side::Value(crate_oracle::Repr::Text(format!("[{}]", ["a, b", "c"].iter().map(|e| format!("{}:{e}", e.len())).collect::<Vec<_>>().join(", "))))) }, Outcome::Match),
    ];
    let _ = framed;
    for (label, case, expected) in &seq_controls {
        let (outcome, detail) = h.run(case);
        if outcome != *expected { failures.push(format!("{label}: got {} ({detail})", outcome.name())); }
    }
    // record 0093: integer read-back. `first()` on [u64::MAX, 2^32 + 1, 1]
    // must be a ConversionError on the script side and, through the checked
    // Rust formatter, on the Rust side (both_error); a wrapping Rust
    // formatter (the pre-0093 `as i64`) must disagree (mismatch), so the
    // oracle can no longer report two wrapped integers as a match; an
    // in-range element still matches as a value.
    fn readback_side(v: Value) -> Side { match rune::from_value::<Result<rune::Value, rune::Value>>(v) { Ok(Ok(v)) => match rune::from_value::<Option<i64>>(v) { Ok(Some(x)) => Side::Value(crate_oracle::Repr::Text(format!("Some({x})"))), Ok(None) => Side::Value(crate_oracle::Repr::Text("None".into())), Err(e) => Side::Broken(e.to_string()) }, Ok(Err(e)) => crate_oracle::rune_error_kind(&e).map(Side::Error).unwrap_or_else(Side::Broken), Err(e) => Side::Broken(e.to_string()) } }
    fn checked(v: Option<u64>) -> Side { match v { Some(v) => match i64::try_from(v) { Ok(x) => Side::Value(crate_oracle::Repr::Text(format!("Some({x})"))), Err(_) => Side::Error("ConversionError".into()) }, None => Side::Value(crate_oracle::Repr::Text("None".into())) } }
    fn extremes() -> p::UInt64Chunked { series_u64_extremes().u64().unwrap().clone() }
    let readback_controls: Vec<(&str, Case, Outcome)> = vec![
        ("u64::MAX read back is a ConversionError on both sides", Case { id: "r1", path: "control", script: "pub fn setup() { [fx::series_u64_extremes()] } pub fn main(__fx) { (__fx[0].u64().unwrap().first(), ()) }", has_receiver: false, unordered: false, policy: "control", fmt: readback_side, oracle: || Staged::Ran(checked(extremes().first())) }, Outcome::BothError),
        ("a wrapping Rust formatter disagrees with the checked binding", Case { id: "r2", path: "control", script: "pub fn setup() { [fx::series_u64_extremes()] } pub fn main(__fx) { (__fx[0].u64().unwrap().first(), ()) }", has_receiver: false, unordered: false, policy: "control", fmt: readback_side, oracle: || Staged::Ran(Side::Value(crate_oracle::Repr::Text(format!("Some({})", extremes().first().unwrap() as i64)))) }, Outcome::Mismatch),
        ("an in-range u64 element matches", Case { id: "r3", path: "control", script: "pub fn setup() { [fx::series_u64_extremes()] } pub fn main(__fx) { (__fx[0].u64().unwrap().last(), ()) }", has_receiver: false, unordered: false, policy: "control", fmt: readback_side, oracle: || Staged::Ran(checked(extremes().last())) }, Outcome::Match),
    ];
    for (label, case, expected) in &readback_controls {
        let (outcome, detail) = h.run(case);
        if outcome != *expected { failures.push(format!("{label}: got {} ({detail})", outcome.name())); }
    }
    // record 0094: a categorical hash is its exact hex token. The fixture's
    // hash has its high bit set, so a Rust formatter that casts through
    // `i64`, or one that drops the high bit (a truncating binding), must
    // disagree with the script's token.
    fn token_side(v: Value) -> Side { match rune::from_value::<String>(v) { Ok(s) => Side::Value(crate_oracle::Repr::Text(s)), Err(e) => Side::Broken(e.to_string()) } }
    fn cats_hash() -> u64 { categories().hash() }
    const TOKEN_SCRIPT: &str = "pub fn setup() { [fx::categories()] } pub fn main(__fx) { (__fx[0].hash(), ()) }";
    let token_controls: Vec<(&str, Case, Outcome)> = vec![
        ("the hash token matches the exact Rust hex", Case { id: "h1", path: "control", script: TOKEN_SCRIPT, has_receiver: false, unordered: false, policy: "control", fmt: token_side, oracle: || Staged::Ran(Side::Value(crate_oracle::Repr::Text(format!("{:016x}", cats_hash())))) }, Outcome::Match),
        ("a Rust formatter casting through i64 disagrees", Case { id: "h2", path: "control", script: TOKEN_SCRIPT, has_receiver: false, unordered: false, policy: "control", fmt: token_side, oracle: || Staged::Ran(Side::Value(crate_oracle::Repr::Text(format!("{}", cats_hash() as i64)))) }, Outcome::Mismatch),
        ("a token without the high bit disagrees", Case { id: "h3", path: "control", script: TOKEN_SCRIPT, has_receiver: false, unordered: false, policy: "control", fmt: token_side, oracle: || Staged::Ran(Side::Value(crate_oracle::Repr::Text(format!("{:016x}", cats_hash() & i64::MAX as u64)))) }, Outcome::Mismatch),
    ];
    assert!(cats_hash() > i64::MAX as u64, "the token controls need a hash with its high bit set");
    for (label, case, expected) in &token_controls {
        let (outcome, detail) = h.run(case);
        if outcome != *expected { failures.push(format!("{label}: got {} ({detail})", outcome.name())); }
    }
    // record 0078: assignment operators mutate the Rune value the way the
    // Rust impl does (the whole left value compared, the right operand
    // preserved), `not_` leaves its receiver, and the From constructors
    // narrow or cast as documented
    type Flags = polars::chunked_array::flags::StatisticsFlags;
    fn flags_pair_side(v: Value) -> Side { match rune::from_value::<(rune::Value, rune::Value)>(v) { Ok((a, b)) => { let sa = match rnx_polars::generated::fixtures::show_w_polars_core__chunked_array__flags__statisticsflags(&a) { Ok(s) => s.to_text(), Err(e) => return Side::Broken(e) }; let sb = match rnx_polars::generated::fixtures::show_w_polars_core__chunked_array__flags__statisticsflags(&b) { Ok(s) => s.to_text(), Err(e) => return Side::Broken(e) }; Side::Value(crate_oracle::Repr::Text(format!("a={sa};b={sb}"))) } Err(e) => Side::Broken(e.to_string()) } }
    fn sel_pair_side(v: Value) -> Side { match rune::from_value::<(rune::Value, rune::Value)>(v) { Ok((a, b)) => { let sa = match rnx_polars::generated::fixtures::show_w_polars_plan__dsl__selector__datatypeselector(&a) { Ok(s) => s.to_text(), Err(e) => return Side::Broken(e) }; let sb = match rnx_polars::generated::fixtures::show_w_polars_plan__dsl__selector__datatypeselector(&b) { Ok(s) => s.to_text(), Err(e) => return Side::Broken(e) }; Side::Value(crate_oracle::Repr::Text(format!("a={sa};b={sb}"))) } Err(e) => Side::Broken(e.to_string()) } }
    fn scalar_fallible_side(v: Value) -> Side { match rune::from_value::<Result<rune::Value, rune::Value>>(v) { Ok(Ok(v)) => rnx_polars::generated::fixtures::show_w_polars_core__scalar__scalar(&v).map(Side::Value).unwrap_or_else(Side::Broken), Ok(Err(e)) => crate_oracle::rune_error_kind(&e).map(Side::Error).unwrap_or_else(Side::Broken), Err(e) => Side::Broken(e.to_string()) } }
    fn scalar_side(v: Value) -> Side { rnx_polars::generated::fixtures::show_w_polars_core__scalar__scalar(&v).map(Side::Value).unwrap_or_else(Side::Broken) }
    fn pair_text<A: std::fmt::Debug, B: std::fmt::Debug>(a: &A, b: &B) -> Staged { Staged::Ran(Side::Value(crate_oracle::Repr::Text(format!("a={a:?};b={b:?}")))) }
    fn scalar_text(s: polars::prelude::Scalar) -> Staged { Staged::Ran(Side::Value(crate_oracle::Repr::Text(format!("{s:?}")))) }
    const FB3: &str = "match polars::StatisticsFlags::from_bits_retain(3) { Ok(v) => v, Err(e) => panic(`${e}`) }";
    const FB6: &str = "match polars::StatisticsFlags::from_bits_retain(6) { Ok(v) => v, Err(e) => panic(`${e}`) }";
    let flags_script = |op: &str| format!("pub fn setup() {{ [{FB3}, {FB6}] }} pub fn main(__fx) {{ let a = __fx[0]; let b = __fx[1]; a {op} b; ((a, b), ()) }}");
    let op_scripts: Vec<&'static str> = ["-=", "&=", "|=", "^="].iter().map(|op| &*Box::leak(flags_script(op).into_boxed_str())).collect();
    let op_controls: Vec<(&str, Case, Outcome)> = vec![
        ("`-=` on flags with several bits set", Case { id: "o1", path: "control", script: op_scripts[0], has_receiver: false, unordered: false, policy: "control", fmt: flags_pair_side, oracle: || { let mut a = Flags::from_bits_retain(3); let b = Flags::from_bits_retain(6); a -= b; pair_text(&a, &b) } }, Outcome::Match),
        ("`&=` on flags with several bits set", Case { id: "o2", path: "control", script: op_scripts[1], has_receiver: false, unordered: false, policy: "control", fmt: flags_pair_side, oracle: || { let mut a = Flags::from_bits_retain(3); let b = Flags::from_bits_retain(6); a &= b; pair_text(&a, &b) } }, Outcome::Match),
        ("`|=` on flags with several bits set", Case { id: "o3", path: "control", script: op_scripts[2], has_receiver: false, unordered: false, policy: "control", fmt: flags_pair_side, oracle: || { let mut a = Flags::from_bits_retain(3); let b = Flags::from_bits_retain(6); a |= b; pair_text(&a, &b) } }, Outcome::Match),
        ("`^=` on flags with several bits set", Case { id: "o4", path: "control", script: op_scripts[3], has_receiver: false, unordered: false, policy: "control", fmt: flags_pair_side, oracle: || { let mut a = Flags::from_bits_retain(3); let b = Flags::from_bits_retain(6); a ^= b; pair_text(&a, &b) } }, Outcome::Match),
        ("`^=` against the `|=` result fails: the whole left value is compared", Case { id: "o5", path: "control", script: op_scripts[3], has_receiver: false, unordered: false, policy: "control", fmt: flags_pair_side, oracle: || { let mut a = Flags::from_bits_retain(3); let b = Flags::from_bits_retain(6); a |= b; pair_text(&a, &b) } }, Outcome::Mismatch),
        ("a changed right operand fails: the right operand is compared too", Case { id: "o6", path: "control", script: op_scripts[2], has_receiver: false, unordered: false, policy: "control", fmt: flags_pair_side, oracle: || { let mut a = Flags::from_bits_retain(3); a |= Flags::from_bits_retain(6); pair_text(&a, &a) } }, Outcome::Mismatch),
        ("`not_()` returns the complement and leaves the receiver", Case { id: "o7", path: "control", script: "pub fn setup() { [match polars::StatisticsFlags::from_bits_retain(3) { Ok(v) => v, Err(e) => panic(`${e}`) }] } pub fn main(__fx) { let a = __fx[0]; let r = a.not_(); ((a, r), ()) }", has_receiver: false, unordered: false, policy: "control", fmt: flags_pair_side, oracle: || { let a = Flags::from_bits_retain(3); let r = !a; pair_text(&a, &r) } }, Outcome::Match),
        ("a receiver claimed changed by `not_()` fails", Case { id: "o8", path: "control", script: "pub fn setup() { [match polars::StatisticsFlags::from_bits_retain(3) { Ok(v) => v, Err(e) => panic(`${e}`) }] } pub fn main(__fx) { let a = __fx[0]; let r = a.not_(); ((a, r), ()) }", has_receiver: false, unordered: false, policy: "control", fmt: flags_pair_side, oracle: || { let a = !Flags::from_bits_retain(3); pair_text(&a, &a) } }, Outcome::Mismatch),
        ("`-=` on selectors that differ", Case { id: "o9", path: "control", script: "pub fn setup() { [polars::DataTypeSelector::Wildcard(), polars::DataTypeSelector::Float()] } pub fn main(__fx) { let a = __fx[0]; let b = __fx[1]; a -= b; ((a, b), ()) }", has_receiver: false, unordered: false, policy: "control", fmt: sel_pair_side, oracle: || { let mut a = polars_plan::dsl::DataTypeSelector::Wildcard; let b = polars_plan::dsl::DataTypeSelector::Float; a -= b.clone(); pair_text(&a, &b) } }, Outcome::Match),
        ("`from_i8` at -128", Case { id: "o10", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { (polars::Scalar::from_i8(-128), ()) }", has_receiver: false, unordered: false, policy: "control", fmt: scalar_fallible_side, oracle: || scalar_text(polars::prelude::Scalar::from(-128i8)) }, Outcome::Match),
        ("`from_i8` at 127", Case { id: "o11", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { (polars::Scalar::from_i8(127), ()) }", has_receiver: false, unordered: false, policy: "control", fmt: scalar_fallible_side, oracle: || scalar_text(polars::prelude::Scalar::from(127i8)) }, Outcome::Match),
        ("`from_i8` at 128 refuses with the conversion kind on both sides", Case { id: "o12", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { (polars::Scalar::from_i8(128), ()) }", has_receiver: false, unordered: false, policy: "control", fmt: scalar_fallible_side, oracle: || Staged::Ran(match i8::try_from(128i64) { Ok(v) => Side::Value(crate_oracle::Repr::Text(format!("{:?}", polars::prelude::Scalar::from(v)))), Err(_) => Side::Error("ConversionError".into()) }) }, Outcome::BothError),
        ("`from_f32` with 0.1 gives the f32-rounded value", Case { id: "o13", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { (polars::Scalar::from_f32(0.1), ()) }", has_receiver: false, unordered: false, policy: "control", fmt: scalar_side, oracle: || scalar_text(polars::prelude::Scalar::from(0.1f32)) }, Outcome::Match),
        ("`from_f32` with 1e40 gives infinity", Case { id: "o14", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { (polars::Scalar::from_f32(1e40), ()) }", has_receiver: false, unordered: false, policy: "control", fmt: scalar_side, oracle: || scalar_text(polars::prelude::Scalar::from(f32::INFINITY)) }, Outcome::Match),
        ("`from_f32` against the f64 scalar fails", Case { id: "o15", path: "control", script: "pub fn setup() { [] } pub fn main(__fx) { (polars::Scalar::from_f32(0.1), ()) }", has_receiver: false, unordered: false, policy: "control", fmt: scalar_side, oracle: || scalar_text(polars::prelude::Scalar::from(0.1f64)) }, Outcome::Mismatch),
    ];
    for (label, case, expected) in &op_controls {
        let (outcome, detail) = h.run(case);
        if outcome != *expected { failures.push(format!("{label}: got {} ({detail})", outcome.name())); }
    }
    // an aliased operand (`a |= a`, and a second binding to the same value)
    // is refused by Rune's dynamic borrow check before the Rust impl runs;
    // the value is unchanged afterwards
    {
        let script = "pub fn setup() { [match polars::StatisticsFlags::from_bits_retain(3) { Ok(v) => v, Err(e) => panic(`${e}`) }] } pub fn aliased(__fx) { let a = __fx[0]; a |= a; a } pub fn rebound(__fx) { let a = __fx[0]; let b = a; a |= b; a } pub fn after(__fx) { __fx[0] }";
        let mut sources = Sources::new();
        sources.insert(Source::memory(script).unwrap()).unwrap();
        let unit = Arc::new(rune::prepare(&mut sources).with_context(&h.context).build().unwrap());
        let mut vm = Vm::new(h.runtime.clone(), unit);
        let fx = vm.call(["setup"], ()).unwrap();
        for name in ["aliased", "rebound"] {
            match vm.call([name], (fx.clone(),)) {
                Ok(_) => failures.push(format!("aliased operand `{name}` was not refused")),
                // Rune 0.14.2 words the shared read of an exclusively held value "Cannot read, value is -X000000"
                Err(e) => { let t = e.to_string(); if !(t.contains("borrow") || t.contains("Cannot read")) { failures.push(format!("aliased operand `{name}`: not a borrow refusal: {t}")); } }
            }
        }
        let after = vm.call(["after"], (fx,)).unwrap();
        let shown = rnx_polars::generated::fixtures::show_w_polars_core__chunked_array__flags__statisticsflags(&after).map(|s| s.to_text());
        let expected = format!("{:?}", Flags::from_bits_retain(3));
        if shown.as_deref() != Ok(expected.as_str()) { failures.push(format!("aliased operand: value after the refusal {shown:?}, expected {expected}")); }
    }
    // two cases of one path (two receivers) survive result collection independently
    {
        let twin = |id: &'static str| Case { id, path: "control::twin", script: "pub fn setup() { [] } pub fn main(__fx) { let a = fx::df(); (a, ()) }", has_receiver: false, unordered: false, policy: "control", fmt: df_side, oracle: || ran(same) };
        let results: Vec<(&str, Outcome)> = [twin("t1"), twin("t1__on__other")].iter().map(|c| (c.id, h.run(c).0)).collect();
        if results.len() != 2 || results.iter().any(|(_, o)| *o != Outcome::Match) || results[0].0 == results[1].0 {
            failures.push(format!("twin receivers: {results:?}"));
        }
    }
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
    let mut with_none = base.to_vec(); with_none.push(("args", "polars::JoinArgs::default_().with_maintain_order(polars::MaintainOrderJoin::LeftRight())"));
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
