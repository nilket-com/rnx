use crate::census::PairRecord;
use crate::emit::Emitted;
use crate::emit::callable::emit_method;
use crate::families::bounds::NUMERIC_NATIVES;
use crate::families::callbacks::routed_binding;
use crate::families::{Check, Family, Listed, OracleSite, RetSite, State};
use crate::model::{Callable, Inventory, Param, Supporting};
use crate::oracle::Oracle;
use crate::release::{FamilyTables, InstantiationScope, Release, ReleaseProvenance};
use crate::ty;
use crate::ty::Ty;
use crate::world::World;
use crate::world::mapping::{Ret, Unsupported};
use std::collections::BTreeMap;

#[derive(serde::Deserialize, Clone)]
pub(crate) struct ChunkSnapshot {
	pub(crate) key: String,
	pub(crate) path: String,
	/// `[owner type, native]` numeric pairs
	pub(crate) pairs: Vec<(String, String)>,
	pub(crate) cite: String,
}
/// Record 0099: the exact canonical return a chunk snapshot maps.
pub(crate) const CHUNKS_RETURN: &str = "&alloc::vec::Vec<polars_arrow::array::ArrayRef>";
/// Record 0100: the non-numeric owners a chunk snapshot admits, with their
/// kind (polars-core datatypes/mod.rs:229-232: Utf8ViewArray,
/// BinaryViewArray, BinaryArray<i64>, BooleanArray), the support copier,
/// the script element type, the Arrow array and the oracle's element type.
pub(crate) const SCALAR_CHUNKS: &[(&str, &str, &str, &str, &str, &str)] = &[
	(
		"polars_core::datatypes::BooleanType",
		"bool",
		"support::chunk_snapshot_bool",
		"bool",
		"polars_arrow::array::BooleanArray",
		"bool",
	),
	(
		"polars_core::datatypes::StringType",
		"str",
		"support::chunk_snapshot_str",
		"String",
		"polars_arrow::array::Utf8ViewArray",
		"alloc::string::String",
	),
	(
		"polars_core::datatypes::BinaryType",
		"binary",
		"support::chunk_snapshot_binview",
		"Vec<i64>",
		"polars_arrow::array::BinaryViewArray",
		"alloc::vec::Vec<u8>",
	),
	(
		"polars_core::datatypes::BinaryOffsetType",
		"binary_offset",
		"support::chunk_snapshot_binary_offset",
		"Vec<i64>",
		"polars_arrow::array::BinaryArray<i64>",
		"alloc::vec::Vec<u8>",
	),
];
impl ChunkSnapshot {
	/// Fail closed on anything but the cited shape: `&self` with no
	/// parameters or method generics on `ChunkedArray<T>` with exactly
	/// `T: PolarsDataType` and no where-clauses, returning exactly
	/// `&Vec<ArrayRef>`, with numeric pairs from the fixed table.
	pub(crate) fn check(&self, c: &Callable) -> Result<(), String> {
		if self.cite.trim().is_empty() {
			return Err("no citation".into());
		}
		if c.impl_head.as_deref() != Some("polars_core::chunked_array::ChunkedArray<T>")
			|| c.impl_bounds
				!= [(
					"T".to_string(),
					"polars_core::datatypes::PolarsDataType".to_string(),
				)] || !c.impl_where.is_empty()
		{
			return Err(format!(
				"impl {} with {:?} / {:?} is not ChunkedArray<T: PolarsDataType>",
				c.impl_head.as_deref().unwrap_or("none"),
				c.impl_bounds,
				c.impl_where
			));
		}
		if c.receiver != "&self" || !c.params.is_empty() || !c.generics_canonical.is_empty() {
			return Err("not a `&self` method without parameters or method generics".into());
		}
		if c.ret_canonical.as_deref() != Some(CHUNKS_RETURN) {
			return Err(format!(
				"return {} is not {CHUNKS_RETURN}",
				c.ret_canonical.as_deref().unwrap_or("()")
			));
		}
		if self.pairs.is_empty() {
			return Err("no listed pair".into());
		}
		for (i, (t, n)) in self.pairs.iter().enumerate() {
			if !NUMERIC_NATIVES.iter().any(|(nt, nn)| nt == t && nn == n)
				&& !SCALAR_CHUNKS.iter().any(|(st, sk, ..)| st == t && sk == n)
			{
				return Err(format!(
					"`{t}` with `{n}` is not a numeric type and its native, nor a listed scalar owner and its kind"
				));
			}
			if self.pairs[..i].iter().any(|(u, _)| u == t) {
				return Err(format!("`{t}` is listed twice"));
			}
		}
		Ok(())
	}
	pub(crate) fn native_for(&self, identity: &str) -> Option<&str> {
		let t = identity
			.strip_prefix("polars_core::chunked_array::ChunkedArray<")?
			.strip_suffix('>')?;
		self.pairs
			.iter()
			.find(|(x, _)| x == t)
			.map(|(_, n)| n.as_str())
	}
}
#[derive(serde::Deserialize, Clone)]
pub(crate) struct IndexedChunkSnapshot {
	pub(crate) key: String,
	pub(crate) path: String,
	/// `[owner type, native or scalar kind]`, from NUMERIC_NATIVES or SCALAR_CHUNKS
	pub(crate) pairs: Vec<(String, String)>,
	pub(crate) cite: String,
}
/// Record 0101: the exact canonical return an indexed chunk snapshot maps.
pub(crate) const DOWNCAST_GET_RETURN: &str = "core::option::Option<&T::Array>";
/// Record 0101: the concrete Arrow array `T::Array` resolves to, per native
/// or scalar kind, which the substituted return must name exactly.
pub(crate) fn indexed_array(kind: &str) -> Option<String> {
	Some(match kind {
		"bool" => "polars_arrow::array::boolean::BooleanArray".into(),
		"str" => "polars_arrow::array::binview::BinaryViewArrayGeneric<str>".into(),
		"binary" => "polars_arrow::array::binview::BinaryViewArrayGeneric<[u8]>".into(),
		"binary_offset" => "polars_arrow::array::binary::BinaryArray<i64>".into(),
		n if NUMERIC_NATIVES.iter().any(|(_, x)| *x == n) => {
			format!("polars_arrow::array::primitive::PrimitiveArray<{n}>")
		}
		_ => return None,
	})
}
impl IndexedChunkSnapshot {
	/// Fail closed on anything but the cited shape: `&self` with exactly one
	/// `usize` parameter and no method generics, on `ChunkedArray<T>` with
	/// exactly `T: PolarsDataType` and no where-clauses, returning exactly
	/// `Option<&T::Array>`, with pairs from the fixed tables.
	pub(crate) fn check(&self, c: &Callable) -> Result<(), String> {
		if self.cite.trim().is_empty() {
			return Err("no citation".into());
		}
		if c.impl_head.as_deref() != Some("polars_core::chunked_array::ChunkedArray<T>")
			|| c.impl_bounds
				!= [(
					"T".to_string(),
					"polars_core::datatypes::PolarsDataType".to_string(),
				)] || !c.impl_where.is_empty()
		{
			return Err(format!(
				"impl {} with {:?} / {:?} is not ChunkedArray<T: PolarsDataType>",
				c.impl_head.as_deref().unwrap_or("none"),
				c.impl_bounds,
				c.impl_where
			));
		}
		if c.receiver != "&self"
			|| !c.generics_canonical.is_empty()
			|| c.params.len() != 1
			|| c.params[0].ty_canonical != "usize"
		{
			return Err("not a `&self` method taking one usize, without method generics".into());
		}
		if c.ret_canonical.as_deref() != Some(DOWNCAST_GET_RETURN) {
			return Err(format!(
				"return {} is not {DOWNCAST_GET_RETURN}",
				c.ret_canonical.as_deref().unwrap_or("()")
			));
		}
		if self.pairs.is_empty() {
			return Err("no listed pair".into());
		}
		for (i, (t, n)) in self.pairs.iter().enumerate() {
			if !NUMERIC_NATIVES.iter().any(|(nt, nn)| nt == t && nn == n)
				&& !SCALAR_CHUNKS.iter().any(|(st, sk, ..)| st == t && sk == n)
			{
				return Err(format!(
					"`{t}` with `{n}` is not a numeric type and its native, nor a listed scalar owner and its kind"
				));
			}
			if self.pairs[..i].iter().any(|(u, _)| u == t) {
				return Err(format!("`{t}` is listed twice"));
			}
		}
		Ok(())
	}
	pub(crate) fn native_for(&self, identity: &str) -> Option<&str> {
		let t = identity
			.strip_prefix("polars_core::chunked_array::ChunkedArray<")?
			.strip_suffix('>')?;
		self.pairs
			.iter()
			.find(|(x, _)| x == t)
			.map(|(_, n)| n.as_str())
	}
}
#[derive(serde::Deserialize, Clone)]
pub(crate) struct ArraySnapshot {
	pub(crate) key: String,
	pub(crate) path: String,
	/// `[owner type, native or scalar kind]`, from NUMERIC_NATIVES or SCALAR_CHUNKS
	pub(crate) pairs: Vec<(String, String)>,
	pub(crate) cite: String,
}
/// Record 0102: the exact canonical return an array snapshot maps.
pub(crate) const DOWNCAST_AS_ARRAY_RETURN: &str = "&T::Array";
impl ArraySnapshot {
	/// Fail closed on anything but the cited shape: `&self` with no
	/// parameters or method generics, on `ChunkedArray<T>` with exactly
	/// `T: PolarsDataType` and no where-clauses, returning exactly
	/// `&T::Array`, with pairs from the fixed tables.
	pub(crate) fn check(&self, c: &Callable) -> Result<(), String> {
		let as_indexed = IndexedChunkSnapshot {
			key: self.key.clone(),
			path: self.path.clone(),
			pairs: self.pairs.clone(),
			cite: self.cite.clone(),
		};
		let mut shaped = c.clone();
		// reuse 0101's checks of citation, impl, receiver, generics and pairs,
		// with this method's own parameters and return checked here
		if !c.params.is_empty() {
			return Err("not a `&self` method without parameters or method generics".into());
		}
		if c.ret_canonical.as_deref() != Some(DOWNCAST_AS_ARRAY_RETURN) {
			return Err(format!(
				"return {} is not {DOWNCAST_AS_ARRAY_RETURN}",
				c.ret_canonical.as_deref().unwrap_or("()")
			));
		}
		shaped.params = vec![Param {
			name: "idx".into(),
			ty: "usize".into(),
			ty_canonical: "usize".into(),
		}];
		shaped.ret_canonical = Some(DOWNCAST_GET_RETURN.into());
		as_indexed.check(&shaped).map_err(|e| {
			e.replace(
				"taking one usize, without method generics",
				"without parameters or method generics",
			)
		})
	}
	pub(crate) fn native_for(&self, identity: &str) -> Option<&str> {
		let t = identity
			.strip_prefix("polars_core::chunked_array::ChunkedArray<")?
			.strip_suffix('>')?;
		self.pairs
			.iter()
			.find(|(x, _)| x == t)
			.map(|(_, n)| n.as_str())
	}
}
#[derive(serde::Deserialize, Clone)]
pub(crate) struct IterSnapshot {
	pub(crate) key: String,
	pub(crate) path: String,
	/// `[owner type, native or scalar kind]`, from NUMERIC_NATIVES or SCALAR_CHUNKS
	pub(crate) pairs: Vec<(String, String)>,
	pub(crate) cite: String,
}
/// Record 0103: the exact canonical return an iterator snapshot maps.
pub(crate) const DOWNCAST_ITER_RETURN: &str =
	"impl core::iter::traits::double_ended::DoubleEndedIterator<Item = &T::Array>";
impl IterSnapshot {
	/// Fail closed on anything but the cited shape: no parameters and
	/// exactly the borrowed typed-array iterator return, then 0101's checks
	/// of citation, impl, receiver, generics and pairs.
	pub(crate) fn check(&self, c: &Callable) -> Result<(), String> {
		if !c.params.is_empty() {
			return Err("not a `&self` method without parameters or method generics".into());
		}
		// exact text: `Ty::render` prints only an `impl` type's trait paths,
		// so it cannot tell `Item = &T::Array` from `Item = T::Array`
		let squash = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
		if c.ret_canonical.as_deref().map(squash) != Some(squash(DOWNCAST_ITER_RETURN)) {
			return Err(format!(
				"return {} is not {DOWNCAST_ITER_RETURN}",
				c.ret_canonical.as_deref().unwrap_or("()")
			));
		}
		let mut shaped = c.clone();
		shaped.params = vec![Param {
			name: "idx".into(),
			ty: "usize".into(),
			ty_canonical: "usize".into(),
		}];
		shaped.ret_canonical = Some(DOWNCAST_GET_RETURN.into());
		IndexedChunkSnapshot {
			key: self.key.clone(),
			path: self.path.clone(),
			pairs: self.pairs.clone(),
			cite: self.cite.clone(),
		}
		.check(&shaped)
		.map_err(|e| {
			e.replace(
				"taking one usize, without method generics",
				"without parameters or method generics",
			)
		})
	}
	pub(crate) fn native_for(&self, identity: &str) -> Option<&str> {
		let t = identity
			.strip_prefix("polars_core::chunked_array::ChunkedArray<")?
			.strip_suffix('>')?;
		self.pairs
			.iter()
			.find(|(x, _)| x == t)
			.map(|(_, n)| n.as_str())
	}
}
#[derive(serde::Deserialize, Clone)]
pub(crate) struct ViewSnapshot {
	pub(crate) key: String,
	pub(crate) path: String,
	/// `[owner type, native or scalar kind]`, from NUMERIC_NATIVES or SCALAR_CHUNKS
	pub(crate) pairs: Vec<(String, String)>,
	pub(crate) cite: String,
}
/// Record 0104: the exact canonical return a view snapshot maps.
pub(crate) const DOWNCAST_CHUNKS_RETURN: &str =
	"polars_core::chunked_array::ops::downcast::Chunks<T::Array>";
pub(crate) const CHUNKS_VIEW: &str = "polars_core::chunked_array::ops::downcast::Chunks";
impl ViewSnapshot {
	/// Fail closed on anything but the cited shape: no parameters and
	/// exactly the `Chunks<T::Array>` return (compared as text), then 0101's
	/// checks of citation, impl, receiver, generics and pairs.
	pub(crate) fn check(&self, c: &Callable) -> Result<(), String> {
		if !c.params.is_empty() {
			return Err("not a `&self` method without parameters or method generics".into());
		}
		let squash = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
		if c.ret_canonical.as_deref().map(squash) != Some(squash(DOWNCAST_CHUNKS_RETURN)) {
			return Err(format!(
				"return {} is not {DOWNCAST_CHUNKS_RETURN}",
				c.ret_canonical.as_deref().unwrap_or("()")
			));
		}
		let mut shaped = c.clone();
		shaped.params = vec![Param {
			name: "idx".into(),
			ty: "usize".into(),
			ty_canonical: "usize".into(),
		}];
		shaped.ret_canonical = Some(DOWNCAST_GET_RETURN.into());
		IndexedChunkSnapshot {
			key: self.key.clone(),
			path: self.path.clone(),
			pairs: self.pairs.clone(),
			cite: self.cite.clone(),
		}
		.check(&shaped)
		.map_err(|e| {
			e.replace(
				"taking one usize, without method generics",
				"without parameters or method generics",
			)
		})
	}
	pub(crate) fn native_for(&self, identity: &str) -> Option<&str> {
		let t = identity
			.strip_prefix("polars_core::chunked_array::ChunkedArray<")?
			.strip_suffix('>')?;
		self.pairs
			.iter()
			.find(|(x, _)| x == t)
			.map(|(_, n)| n.as_str())
	}
}
#[derive(serde::Deserialize, Clone)]
pub(crate) struct OwnedIterSnapshot {
	pub(crate) key: String,
	pub(crate) path: String,
	/// `[owner type, native or scalar kind]`, from NUMERIC_NATIVES or SCALAR_CHUNKS
	pub(crate) pairs: Vec<(String, String)>,
	pub(crate) cite: String,
}
/// Record 0105: the exact canonical return an owned iterator snapshot maps.
pub(crate) const DOWNCAST_INTO_ITER_RETURN: &str =
	"impl core::iter::traits::double_ended::DoubleEndedIterator<Item = T::Array>";
impl OwnedIterSnapshot {
	/// Fail closed on anything but the cited shape: a consuming `self`
	/// receiver, no parameters, and exactly the owned-item iterator return
	/// (compared as text: `Ty::render` drops an impl's item), then 0101's
	/// checks of citation, impl, generics and pairs.
	pub(crate) fn check(&self, c: &Callable) -> Result<(), String> {
		if c.receiver != "self" {
			return Err(format!("receiver {} is not a consuming self", c.receiver));
		}
		if !c.params.is_empty() {
			return Err("not a method without parameters or method generics".into());
		}
		let squash = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
		if c.ret_canonical.as_deref().map(squash) != Some(squash(DOWNCAST_INTO_ITER_RETURN)) {
			return Err(format!(
				"return {} is not {DOWNCAST_INTO_ITER_RETURN}",
				c.ret_canonical.as_deref().unwrap_or("()")
			));
		}
		let mut shaped = c.clone();
		shaped.receiver = "&self".into();
		shaped.params = vec![Param {
			name: "idx".into(),
			ty: "usize".into(),
			ty_canonical: "usize".into(),
		}];
		shaped.ret_canonical = Some(DOWNCAST_GET_RETURN.into());
		IndexedChunkSnapshot {
			key: self.key.clone(),
			path: self.path.clone(),
			pairs: self.pairs.clone(),
			cite: self.cite.clone(),
		}
		.check(&shaped)
		.map_err(|e| {
			e.replace(
				"not a `&self` method taking one usize, without method generics",
				"not a method without parameters or method generics",
			)
		})
	}
	pub(crate) fn native_for(&self, identity: &str) -> Option<&str> {
		let t = identity
			.strip_prefix("polars_core::chunked_array::ChunkedArray<")?
			.strip_suffix('>')?;
		self.pairs
			.iter()
			.find(|(x, _)| x == t)
			.map(|(_, n)| n.as_str())
	}
}
#[derive(serde::Deserialize, Clone)]
pub(crate) struct LayoutSnapshot {
	pub(crate) key: String,
	pub(crate) path: String,
	/// `[owner type, native or scalar kind]`, from NUMERIC_NATIVES or SCALAR_CHUNKS
	pub(crate) pairs: Vec<(String, String)>,
	pub(crate) cite: String,
}
/// Record 0106: the exact canonical return a layout snapshot maps.
pub(crate) const LAYOUT_RETURN: &str = "polars_core::chunked_array::ChunkedArrayLayout<T>";
pub(crate) const LAYOUT_ENUM: &str = "polars_core::chunked_array::ChunkedArrayLayout";
impl LayoutSnapshot {
	/// Fail closed on anything but the recorded shape: `&self`, no
	/// parameters or method generics, the `ChunkedArray<T>` head with exactly
	/// `T: PolarsDataType` as both bound and where-clause, exactly the
	/// `ChunkedArrayLayout<T>` return, pairs from the fixed tables.
	pub(crate) fn check(&self, c: &Callable) -> Result<(), String> {
		if self.cite.trim().is_empty() {
			return Err("no citation".into());
		}
		let bound = "polars_core::datatypes::PolarsDataType";
		if c.impl_head.as_deref() != Some("polars_core::chunked_array::ChunkedArray<T>")
			|| c.impl_bounds != [("T".to_string(), bound.to_string())]
			|| c.impl_where != [format!("T: {bound}")]
		{
			return Err(format!(
				"impl {} with {:?} / where {:?} is not ChunkedArray<T: PolarsDataType> where T: PolarsDataType",
				c.impl_head.as_deref().unwrap_or("none"),
				c.impl_bounds,
				c.impl_where
			));
		}
		if c.receiver != "&self" || !c.params.is_empty() || !c.generics_canonical.is_empty() {
			return Err("not a `&self` method without parameters or method generics".into());
		}
		let squash = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
		if c.ret_canonical.as_deref().map(squash) != Some(squash(LAYOUT_RETURN)) {
			return Err(format!(
				"return {} is not {LAYOUT_RETURN}",
				c.ret_canonical.as_deref().unwrap_or("()")
			));
		}
		if self.pairs.is_empty() {
			return Err("no listed pair".into());
		}
		for (i, (t, n)) in self.pairs.iter().enumerate() {
			if !NUMERIC_NATIVES.iter().any(|(nt, nn)| nt == t && nn == n)
				&& !SCALAR_CHUNKS.iter().any(|(st, sk, ..)| st == t && sk == n)
			{
				return Err(format!(
					"`{t}` with `{n}` is not a numeric type and its native, nor a listed scalar owner and its kind"
				));
			}
			if self.pairs[..i].iter().any(|(u, _)| u == t) {
				return Err(format!("`{t}` is listed twice"));
			}
		}
		Ok(())
	}
	/// The pair's (owner type, kind), if listed.
	pub(crate) fn pair_for(&self, identity: &str) -> Option<(String, String)> {
		let t = identity
			.strip_prefix("polars_core::chunked_array::ChunkedArray<")?
			.strip_suffix('>')?;
		self.pairs
			.iter()
			.find(|(x, _)| x == t)
			.map(|(x, n)| (x.clone(), n.clone()))
	}
}
pub(crate) fn layout_entry<'a>(
	release: &'a Release,
	c: &Callable,
) -> Option<Result<&'a LayoutSnapshot, String>> {
	let listed: Vec<&LayoutSnapshot> = release
		.families
		.layout_snapshots
		.iter()
		.filter(|m| m.key == c.key && m.path == c.canonical_path)
		.collect();
	match listed.as_slice() {
		[] => None,
		[m] => Some(m.check(c).map(|_| *m)),
		_ => Some(Err("listed twice".into())),
	}
}
pub(crate) fn owned_iter_entry<'a>(
	release: &'a Release,
	c: &Callable,
) -> Option<Result<&'a OwnedIterSnapshot, String>> {
	let listed: Vec<&OwnedIterSnapshot> = release
		.families
		.owned_iter_snapshots
		.iter()
		.filter(|m| m.key == c.key && m.path == c.canonical_path)
		.collect();
	match listed.as_slice() {
		[] => None,
		[m] => Some(m.check(c).map(|_| *m)),
		_ => Some(Err("listed twice".into())),
	}
}
pub(crate) fn view_snapshot_entry<'a>(
	release: &'a Release,
	c: &Callable,
) -> Option<Result<&'a ViewSnapshot, String>> {
	let listed: Vec<&ViewSnapshot> = release
		.families
		.view_snapshots
		.iter()
		.filter(|m| m.key == c.key && m.path == c.canonical_path)
		.collect();
	match listed.as_slice() {
		[] => None,
		[m] => Some(m.check(c).map(|_| *m)),
		_ => Some(Err("listed twice".into())),
	}
}
pub(crate) fn iter_snapshot_entry<'a>(
	release: &'a Release,
	c: &Callable,
) -> Option<Result<&'a IterSnapshot, String>> {
	let listed: Vec<&IterSnapshot> = release
		.families
		.iter_snapshots
		.iter()
		.filter(|m| m.key == c.key && m.path == c.canonical_path)
		.collect();
	match listed.as_slice() {
		[] => None,
		[m] => Some(m.check(c).map(|_| *m)),
		_ => Some(Err("listed twice".into())),
	}
}
pub(crate) fn array_snapshot_entry<'a>(
	release: &'a Release,
	c: &Callable,
) -> Option<Result<&'a ArraySnapshot, String>> {
	let listed: Vec<&ArraySnapshot> = release
		.families
		.array_snapshots
		.iter()
		.filter(|m| m.key == c.key && m.path == c.canonical_path)
		.collect();
	match listed.as_slice() {
		[] => None,
		[m] => Some(m.check(c).map(|_| *m)),
		_ => Some(Err("listed twice".into())),
	}
}
pub(crate) fn indexed_chunk_entry<'a>(
	release: &'a Release,
	c: &Callable,
) -> Option<Result<&'a IndexedChunkSnapshot, String>> {
	let listed: Vec<&IndexedChunkSnapshot> = release
		.families
		.indexed_chunk_snapshots
		.iter()
		.filter(|m| m.key == c.key && m.path == c.canonical_path)
		.collect();
	match listed.as_slice() {
		[] => None,
		[m] => Some(m.check(c).map(|_| *m)),
		_ => Some(Err("listed twice".into())),
	}
}
pub(crate) fn chunk_snapshot_entry<'a>(
	release: &'a Release,
	c: &Callable,
) -> Option<Result<&'a ChunkSnapshot, String>> {
	let listed: Vec<&ChunkSnapshot> = release
		.families
		.chunk_snapshots
		.iter()
		.filter(|m| m.key == c.key && m.path == c.canonical_path)
		.collect();
	match listed.as_slice() {
		[] => None,
		[m] => Some(m.check(c).map(|_| *m)),
		_ => Some(Err("listed twice".into())),
	}
}

// ---------------------------------------------------------------- record 0115: this module's families

/// Records 0099/0100: a listed `chunks` snapshot.
pub(crate) struct ChunkFamily;
pub(crate) static CHUNK: ChunkFamily = ChunkFamily;

impl Family for ChunkFamily {
	fn name(&self) -> &'static str {
		"chunk_snapshot"
	}
	fn listed_pair_post(
		&self,
		world: &World,
		c: &Callable,
		p: &PairRecord,
		_syn: &mut Callable,
	) -> Listed {
		match chunk_snapshot_entry(&world.release, c) {
			None => Listed::Unlisted,
			Some(Err(why)) => Listed::Refused("chunk snapshot", why),
			Some(Ok(e)) => match e.native_for(&p.identity) {
				Some(n) => Listed::Active(State::Two(c.name.clone(), n.to_string())),
				None => Listed::Refused(
					"chunk snapshot",
					format!("`{}` is not a listed pair", p.identity),
				),
			},
		}
	}
	fn ret(
		&self,
		world: &World,
		state: &State,
		site: RetSite,
		t: &Ty,
		owner: Option<&str>,
		depth: u8,
	) -> Result<Option<Ret>, Unsupported> {
		if site != RetSite::Top {
			return Ok(None);
		}
		// record 0099: the exact chunk list, copied per chunk, for the listed pair's native
		if let Some((_, native)) = state.two() {
			if depth == 0 {
				let arrays = [
					"polars_arrow::array::ArrayRef",
					"alloc::boxed::Box<dyn polars_arrow::array::Array>",
				];
				let is_chunks = matches!(t, Ty::Ref { mutable: false, inner } if matches!(&**inner, Ty::Path { path, args } if path == "alloc::vec::Vec" && args.len() == 1 && arrays.contains(&args[0].render().as_str())));
				if !is_chunks {
					return Err(Unsupported(
						"chunk snapshot",
						format!("{} is not &Vec<ArrayRef>", t.render()),
					));
				}
				// record 0100: a Boolean, string or binary owner has its own copier
				if let Some((_, kind, copier, elem, _, _)) =
					SCALAR_CHUNKS.iter().find(|(_, k, ..)| *k == native)
				{
					return Ok(Some(Ret {
						materialize: None,
						rust_ty: format!("Vec<Vec<Option<{elem}>>>"),
						conv: format!("{copier}(__r, \"__OP__\")?"),
						fallible: true,
						doc: format!(
							"vector of chunks, each a vector of option of {kind} values (copied, chunk boundaries kept, bounded with payload bytes)"
						),
					}));
				}
				let e = world.ret(
					&Ty::Path {
						path: native.clone(),
						args: vec![],
					},
					owner,
					depth + 1,
				)?;
				if e.materialize.is_some() || e.rust_ty.starts_with("Vec") {
					return Err(Unsupported(
						"chunk snapshot",
						format!("element {native} is not a scalar"),
					));
				}
				return Ok(Some(Ret {
					materialize: None,
					rust_ty: format!("Vec<Vec<Option<{}>>>", e.rust_ty),
					conv: format!(
						"support::chunk_snapshot::<{native}, _>(__r, \"__OP__\", |__r| Ok::<_, Error>({}))?",
						e.conv
					),
					fallible: true,
					doc: format!(
						"vector of chunks, each a vector of option of {} (copied, chunk boundaries kept, bounded)",
						e.doc
					),
				}));
			}
		}
		Ok(None)
	}
	fn check(&self, world: &World, _state: &State, c: &Callable, owner: &str, ret: &Ret) -> Check {
		// record 0099: the chunk borrow ends with the call, so the copy must happen in it
		if routed_binding(world, c, Some(owner))
			|| !ret.conv.starts_with("support::chunk_snapshot")
			|| c.receiver != "&self"
		{
			return Check::Refuse(
				"chunk snapshot",
				"needs an unrouted `&self` call and the chunk-snapshot conversion".into(),
			);
		}
		Check::Pass
	}
	fn oracle_state(
		&self,
		world: &World,
		key: &str,
		path: &str,
		owner: Option<&str>,
	) -> Option<State> {
		world
			.release
			.families
			.chunk_snapshots
			.iter()
			.find(|m| m.key == key && m.path == path)
			.and_then(|m| {
				owner
					.and_then(|a| world.wrappers.get(a))
					.and_then(|w| m.native_for(&w.identity))
					.map(String::from)
			})
			.map(State::One)
	}
	fn oracle_fmt(
		&self,
		o: &Oracle,
		state: &State,
		site: OracleSite,
		t: &Ty,
		owner: Option<&str>,
		depth: u8,
	) -> Option<Option<String>> {
		if site != OracleSite::Ref {
			return None;
		}
		// record 0099: a listed chunk list frames as the nested options the script receives
		let Ty::Ref { inner, .. } = t else {
			return None;
		};
		if !(depth == 0
			&& matches!(&**inner, Ty::Path { path, args } if path == "alloc::vec::Vec" && args.len() == 1 && ["polars_arrow::array::ArrayRef", "alloc::boxed::Box<dyn polars_arrow::array::Array>"].contains(&args[0].render().as_str())))
		{
			return None;
		}
		Some((|| -> Option<String> {
			let n = state.one().unwrap();
			// record 0100: a scalar owner's chunks, as the owned nested options the script receives
			if let Some((_, _, _, _, array, oracle_elem)) =
				SCALAR_CHUNKS.iter().find(|(_, k, ..)| *k == n)
			{
				let nested = ty::parse(&format!(
					"alloc::vec::Vec<alloc::vec::Vec<core::option::Option<{oracle_elem}>>>"
				));
				let own = match n.as_str() {
					"bool" => "x",
					"str" => "x.map(|v| v.to_string())",
					_ => "x.map(|v| v.to_vec())",
				};
				return o.oracle_fmt(&nested, owner, depth + 1).map(|f| format!("{{ let __r: Vec<Vec<Option<_>>> = __r.iter().map(|a| a.as_any().downcast_ref::<{array}>().expect(\"oracle: a {n} chunk\").iter().map(|x| {own}).collect()).collect(); {f} }}"));
			}
			let nested = ty::parse(&format!(
				"alloc::vec::Vec<alloc::vec::Vec<core::option::Option<{n}>>>"
			));
			o.oracle_fmt(&nested, owner, depth + 1).map(|f| format!("{{ let __r: Vec<Vec<Option<{n}>>> = __r.iter().map(|a| a.as_any().downcast_ref::<polars_arrow::array::PrimitiveArray<{n}>>().expect(\"oracle: a numeric chunk\").iter().map(|x| x.copied()).collect()).collect(); {f} }}"))
		})())
	}
}

/// Record 0101: a listed `downcast_get` snapshot.
pub(crate) struct IndexedChunkFamily;
pub(crate) static INDEXED: IndexedChunkFamily = IndexedChunkFamily;

impl Family for IndexedChunkFamily {
	fn name(&self) -> &'static str {
		"indexed_chunk"
	}
	fn listed_pair_post(
		&self,
		world: &World,
		c: &Callable,
		p: &PairRecord,
		_syn: &mut Callable,
	) -> Listed {
		match indexed_chunk_entry(&world.release, c) {
			None => Listed::Unlisted,
			Some(Err(why)) => Listed::Refused("indexed chunk snapshot", why),
			Some(Ok(e)) => match e.native_for(&p.identity) {
				Some(n) => Listed::Active(State::Two(c.name.clone(), n.to_string())),
				None => Listed::Refused(
					"indexed chunk snapshot",
					format!("`{}` is not a listed pair", p.identity),
				),
			},
		}
	}
	fn ret(
		&self,
		world: &World,
		state: &State,
		site: RetSite,
		t: &Ty,
		owner: Option<&str>,
		depth: u8,
	) -> Result<Option<Ret>, Unsupported> {
		if site != RetSite::Top {
			return Ok(None);
		}
		// record 0101: exactly `Option<&T::Array>` for the pair, the one selected chunk copied
		if let Some((_, kind)) = state.two() {
			if depth == 0 {
				let want = indexed_array(&kind).ok_or_else(|| {
					Unsupported("indexed chunk snapshot", format!("no array for {kind}"))
				})?;
				let exact = matches!(t, Ty::Path { path, args } if path == "core::option::Option" && args.len() == 1 && matches!(&args[0], Ty::Ref { mutable: false, inner } if inner.render() == ty::parse(&want).render()));
				if !exact {
					return Err(Unsupported(
						"indexed chunk snapshot",
						format!("{} is not Option<&{want}>", t.render()),
					));
				}
				if let Some((_, k, _, elem, _, _)) =
					SCALAR_CHUNKS.iter().find(|(_, k, ..)| *k == kind)
				{
					let copier = match *k {
						"bool" => "support::indexed_snapshot_bool",
						"str" => "support::indexed_snapshot_str",
						"binary" => "support::indexed_snapshot_binview",
						_ => "support::indexed_snapshot_binary_offset",
					};
					return Ok(Some(Ret {
						materialize: None,
						rust_ty: format!("Option<Vec<Option<{elem}>>>"),
						conv: format!("{copier}(__r, \"__OP__\")?"),
						fallible: true,
						doc: format!(
							"option of the selected chunk as a vector of option of {k} values (copied, bounded with payload bytes)"
						),
					}));
				}
				let e = world.ret(
					&Ty::Path {
						path: kind.clone(),
						args: vec![],
					},
					owner,
					depth + 1,
				)?;
				if e.materialize.is_some() || e.rust_ty.starts_with("Vec") {
					return Err(Unsupported(
						"indexed chunk snapshot",
						format!("element {kind} is not a scalar"),
					));
				}
				return Ok(Some(Ret {
					materialize: None,
					rust_ty: format!("Option<Vec<Option<{}>>>", e.rust_ty),
					conv: format!(
						"support::indexed_snapshot::<{kind}, _>(__r, \"__OP__\", |__r| Ok::<_, Error>({}))?",
						e.conv
					),
					fallible: true,
					doc: format!(
						"option of the selected chunk as a vector of option of {} (copied, bounded)",
						e.doc
					),
				}));
			}
		}
		Ok(None)
	}
	fn check(&self, world: &World, _state: &State, c: &Callable, owner: &str, ret: &Ret) -> Check {
		// record 0101: likewise for the one borrowed chunk of `downcast_get`
		if routed_binding(world, c, Some(owner))
			|| !ret.conv.starts_with("support::indexed_snapshot")
			|| c.receiver != "&self"
		{
			return Check::Refuse(
				"indexed chunk snapshot",
				"needs an unrouted `&self` call and the indexed-snapshot conversion".into(),
			);
		}
		Check::Pass
	}
	fn oracle_state(
		&self,
		world: &World,
		key: &str,
		path: &str,
		owner: Option<&str>,
	) -> Option<State> {
		world
			.release
			.families
			.indexed_chunk_snapshots
			.iter()
			.find(|m| m.key == key && m.path == path)
			.and_then(|m| {
				owner
					.and_then(|a| world.wrappers.get(a))
					.and_then(|w| m.native_for(&w.identity))
					.map(String::from)
			})
			.map(State::One)
	}
	fn oracle_fmt(
		&self,
		o: &Oracle,
		state: &State,
		site: OracleSite,
		_t: &Ty,
		owner: Option<&str>,
		depth: u8,
	) -> Option<Option<String>> {
		if site != OracleSite::Top {
			return None;
		}
		// record 0101: a listed `downcast_get` result frames as the owned optional chunk the script receives
		if depth == 0 {
			if let Some(kind) = state.one() {
				let elem = SCALAR_CHUNKS
					.iter()
					.find(|(_, k, ..)| *k == kind)
					.map(|(.., oe)| oe.to_string())
					.unwrap_or_else(|| kind.clone());
				let own = match kind.as_str() {
					"bool" => "x",
					"str" => "x.map(|v| v.to_string())",
					"binary" | "binary_offset" => "x.map(|v| v.to_vec())",
					_ => "x.copied()",
				};
				let nested = ty::parse(&format!(
					"core::option::Option<alloc::vec::Vec<core::option::Option<{elem}>>>"
				));
				return Some(o.oracle_fmt(&nested, owner, depth + 1).map(|f| {
					format!(
						"{{ let __r = __r.map(|a| a.iter().map(|x| {own}).collect::<Vec<_>>()); {f} }}"
					)
				}));
			}
		}
		None
	}
}

/// Record 0102: a listed `downcast_as_array` snapshot.
pub(crate) struct ArraySnapshotFamily;
pub(crate) static ARRAY: ArraySnapshotFamily = ArraySnapshotFamily;

impl Family for ArraySnapshotFamily {
	fn name(&self) -> &'static str {
		"array_snapshot"
	}
	fn listed_pair_post(
		&self,
		world: &World,
		c: &Callable,
		p: &PairRecord,
		_syn: &mut Callable,
	) -> Listed {
		match array_snapshot_entry(&world.release, c) {
			None => Listed::Unlisted,
			Some(Err(why)) => Listed::Refused("array snapshot", why),
			Some(Ok(e)) => match e.native_for(&p.identity) {
				Some(n) => Listed::Active(State::Two(c.name.clone(), n.to_string())),
				None => Listed::Refused(
					"array snapshot",
					format!("`{}` is not a listed pair", p.identity),
				),
			},
		}
	}
	fn ret(
		&self,
		world: &World,
		state: &State,
		site: RetSite,
		t: &Ty,
		owner: Option<&str>,
		depth: u8,
	) -> Result<Option<Ret>, Unsupported> {
		if site != RetSite::Top {
			return Ok(None);
		}
		// record 0102: exactly `&T::Array` for the pair, the one array copied
		if let Some((_, kind)) = state.two() {
			if depth == 0 {
				let want = indexed_array(&kind)
					.ok_or_else(|| Unsupported("array snapshot", format!("no array for {kind}")))?;
				let exact = matches!(t, Ty::Ref { mutable: false, inner } if inner.render() == ty::parse(&want).render());
				if !exact {
					return Err(Unsupported(
						"array snapshot",
						format!("{} is not &{want}", t.render()),
					));
				}
				if let Some((_, k, _, elem, _, _)) =
					SCALAR_CHUNKS.iter().find(|(_, k, ..)| *k == kind)
				{
					let copier = match *k {
						"bool" => "support::array_snapshot_bool",
						"str" => "support::array_snapshot_str",
						"binary" => "support::array_snapshot_binview",
						_ => "support::array_snapshot_binary_offset",
					};
					return Ok(Some(Ret {
						materialize: None,
						rust_ty: format!("Vec<Option<{elem}>>"),
						conv: format!("{copier}(__r, \"__OP__\")?"),
						fallible: true,
						doc: format!(
							"the single array as a vector of option of {k} values (copied, bounded with payload bytes)"
						),
					}));
				}
				let e = world.ret(
					&Ty::Path {
						path: kind.clone(),
						args: vec![],
					},
					owner,
					depth + 1,
				)?;
				if e.materialize.is_some() || e.rust_ty.starts_with("Vec") {
					return Err(Unsupported(
						"array snapshot",
						format!("element {kind} is not a scalar"),
					));
				}
				return Ok(Some(Ret {
					materialize: None,
					rust_ty: format!("Vec<Option<{}>>", e.rust_ty),
					conv: format!(
						"support::array_snapshot::<{kind}, _>(__r, \"__OP__\", |__r| Ok::<_, Error>({}))?",
						e.conv
					),
					fallible: true,
					doc: format!(
						"the single array as a vector of option of {} (copied, bounded)",
						e.doc
					),
				}));
			}
		}
		Ok(None)
	}
	fn check(&self, world: &World, _state: &State, c: &Callable, owner: &str, ret: &Ret) -> Check {
		// record 0102: likewise for the one borrowed array of `downcast_as_array`
		if routed_binding(world, c, Some(owner))
			|| !ret.conv.starts_with("support::array_snapshot")
			|| c.receiver != "&self"
		{
			return Check::Refuse(
				"array snapshot",
				"needs an unrouted `&self` call and the array-snapshot conversion".into(),
			);
		}
		Check::Pass
	}
	fn oracle_state(
		&self,
		world: &World,
		key: &str,
		path: &str,
		owner: Option<&str>,
	) -> Option<State> {
		world
			.release
			.families
			.array_snapshots
			.iter()
			.find(|m| m.key == key && m.path == path)
			.and_then(|m| {
				owner
					.and_then(|a| world.wrappers.get(a))
					.and_then(|w| m.native_for(&w.identity))
					.map(String::from)
			})
			.map(State::One)
	}
	fn oracle_fmt(
		&self,
		o: &Oracle,
		state: &State,
		site: OracleSite,
		_t: &Ty,
		owner: Option<&str>,
		depth: u8,
	) -> Option<Option<String>> {
		if site != OracleSite::Top {
			return None;
		}
		// record 0102: a listed `downcast_as_array` result frames as the owned vector the script receives
		if depth == 0 {
			if let Some(kind) = state.one() {
				let elem = SCALAR_CHUNKS
					.iter()
					.find(|(_, k, ..)| *k == kind)
					.map(|(.., oe)| oe.to_string())
					.unwrap_or_else(|| kind.clone());
				let own = match kind.as_str() {
					"bool" => "x",
					"str" => "x.map(|v| v.to_string())",
					"binary" | "binary_offset" => "x.map(|v| v.to_vec())",
					_ => "x.copied()",
				};
				let nested = ty::parse(&format!("alloc::vec::Vec<core::option::Option<{elem}>>"));
				return Some(o.oracle_fmt(&nested, owner, depth + 1).map(|f| {
					format!("{{ let __r: Vec<_> = __r.iter().map(|x| {own}).collect(); {f} }}")
				}));
			}
		}
		None
	}
}

/// Record 0103: a listed `downcast_iter` snapshot.
pub(crate) struct IterSnapshotFamily;
pub(crate) static ITER: IterSnapshotFamily = IterSnapshotFamily;

impl Family for IterSnapshotFamily {
	fn name(&self) -> &'static str {
		"iter_snapshot"
	}
	fn listed_pair_post(
		&self,
		world: &World,
		c: &Callable,
		p: &PairRecord,
		_syn: &mut Callable,
	) -> Listed {
		match iter_snapshot_entry(&world.release, c) {
			None => Listed::Unlisted,
			Some(Err(why)) => Listed::Refused("iterator snapshot", why),
			Some(Ok(e)) => match e.native_for(&p.identity) {
				Some(n) => Listed::Active(State::Two(c.name.clone(), n.to_string())),
				None => Listed::Refused(
					"iterator snapshot",
					format!("`{}` is not a listed pair", p.identity),
				),
			},
		}
	}
	fn ret(
		&self,
		world: &World,
		state: &State,
		site: RetSite,
		t: &Ty,
		owner: Option<&str>,
		depth: u8,
	) -> Result<Option<Ret>, Unsupported> {
		if site != RetSite::Top {
			return Ok(None);
		}
		// record 0103: exactly the pair's borrowed typed-array iterator, driven into owned chunks
		if let Some((_, kind)) = state.two() {
			if depth == 0 {
				let want = indexed_array(&kind).ok_or_else(|| {
					Unsupported("iterator snapshot", format!("no array for {kind}"))
				})?;
				// the parsed structure, not `render` (which drops an impl's item):
				// exactly one DoubleEndedIterator bound, no arguments, and an
				// item that is a shared borrow of exactly the pair's array
				let exact = matches!(t, Ty::Impl(bs) if bs.len() == 1
                    && bs[0].path == "core::iter::traits::double_ended::DoubleEndedIterator"
                    && bs[0].args.is_empty()
                    && matches!(bs[0].item.as_deref(), Some(Ty::Ref { mutable: false, inner }) if inner.render() == ty::parse(&want).render()));
				if !exact {
					return Err(Unsupported(
						"iterator snapshot",
						format!("{t:?} is not impl DoubleEndedIterator<Item = &{want}>"),
					));
				}
				if let Some((_, k, _, elem, _, _)) =
					SCALAR_CHUNKS.iter().find(|(_, k, ..)| *k == kind)
				{
					let copier = match *k {
						"bool" => "support::iter_snapshot_bool",
						"str" => "support::iter_snapshot_str",
						"binary" => "support::iter_snapshot_binview",
						_ => "support::iter_snapshot_binary_offset",
					};
					return Ok(Some(Ret {
						materialize: None,
						rust_ty: format!("Vec<Vec<Option<{elem}>>>"),
						conv: format!("{copier}(this.0.chunks(), __r, \"__OP__\")?"),
						fallible: true,
						doc: format!(
							"vector of chunks in iterator order, each a vector of option of {k} values (copied, bounded with payload bytes)"
						),
					}));
				}
				let e = world.ret(
					&Ty::Path {
						path: kind.clone(),
						args: vec![],
					},
					owner,
					depth + 1,
				)?;
				if e.materialize.is_some() || e.rust_ty.starts_with("Vec") {
					return Err(Unsupported(
						"iterator snapshot",
						format!("element {kind} is not a scalar"),
					));
				}
				return Ok(Some(Ret {
					materialize: None,
					rust_ty: format!("Vec<Vec<Option<{}>>>", e.rust_ty),
					conv: format!(
						"support::iter_snapshot::<{kind}, _>(this.0.chunks(), __r, \"__OP__\", |__r| Ok::<_, Error>({}))?",
						e.conv
					),
					fallible: true,
					doc: format!(
						"vector of chunks in iterator order, each a vector of option of {} (copied, bounded)",
						e.doc
					),
				}));
			}
		}
		Ok(None)
	}
	fn check(&self, world: &World, _state: &State, c: &Callable, owner: &str, ret: &Ret) -> Check {
		// record 0103: likewise for the borrowed typed-chunk iterator
		if routed_binding(world, c, Some(owner))
			|| !ret.conv.starts_with("support::iter_snapshot")
			|| c.receiver != "&self"
		{
			return Check::Refuse(
				"iterator snapshot",
				"needs an unrouted `&self` call and the iterator-snapshot conversion".into(),
			);
		}
		Check::Pass
	}
	fn oracle_state(
		&self,
		world: &World,
		key: &str,
		path: &str,
		owner: Option<&str>,
	) -> Option<State> {
		world
			.release
			.families
			.iter_snapshots
			.iter()
			.find(|m| m.key == key && m.path == path)
			.and_then(|m| {
				owner
					.and_then(|a| world.wrappers.get(a))
					.and_then(|w| m.native_for(&w.identity))
					.map(String::from)
			})
			.map(State::One)
	}
	fn oracle_fmt(
		&self,
		o: &Oracle,
		state: &State,
		site: OracleSite,
		_t: &Ty,
		owner: Option<&str>,
		depth: u8,
	) -> Option<Option<String>> {
		if site != OracleSite::Top {
			return None;
		}
		// record 0103: a listed `downcast_iter` result frames as the owned nested vectors the script receives
		if depth == 0 {
			if let Some(kind) = state.one() {
				let elem = SCALAR_CHUNKS
					.iter()
					.find(|(_, k, ..)| *k == kind)
					.map(|(.., oe)| oe.to_string())
					.unwrap_or_else(|| kind.clone());
				let own = match kind.as_str() {
					"bool" => "x",
					"str" => "x.map(|v| v.to_string())",
					"binary" | "binary_offset" => "x.map(|v| v.to_vec())",
					_ => "x.copied()",
				};
				let nested = ty::parse(&format!(
					"alloc::vec::Vec<alloc::vec::Vec<core::option::Option<{elem}>>>"
				));
				return Some(o.oracle_fmt(&nested, owner, depth + 1).map(|f| {
					format!(
						"{{ let __r: Vec<Vec<_>> = __r.map(|a| a.iter().map(|x| {own}).collect()).collect(); {f} }}"
					)
				}));
			}
		}
		None
	}
}

/// Record 0104: a listed `downcast_chunks` snapshot.
pub(crate) struct ViewSnapshotFamily;
pub(crate) static VIEW: ViewSnapshotFamily = ViewSnapshotFamily;

impl Family for ViewSnapshotFamily {
	fn name(&self) -> &'static str {
		"view_snapshot"
	}
	fn listed_pair_post(
		&self,
		world: &World,
		c: &Callable,
		p: &PairRecord,
		_syn: &mut Callable,
	) -> Listed {
		match view_snapshot_entry(&world.release, c) {
			None => Listed::Unlisted,
			Some(Err(why)) => Listed::Refused("view snapshot", why),
			Some(Ok(e)) => match e.native_for(&p.identity) {
				Some(n) => Listed::Active(State::Two(c.name.clone(), n.to_string())),
				None => Listed::Refused(
					"view snapshot",
					format!("`{}` is not a listed pair", p.identity),
				),
			},
		}
	}
	fn ret(
		&self,
		world: &World,
		state: &State,
		site: RetSite,
		t: &Ty,
		owner: Option<&str>,
		depth: u8,
	) -> Result<Option<Ret>, Unsupported> {
		if site != RetSite::Top {
			return Ok(None);
		}
		// record 0104: exactly `Chunks<the pair's array>`, read by index into owned chunks
		if let Some((_, kind)) = state.two() {
			if depth == 0 {
				let want = indexed_array(&kind)
					.ok_or_else(|| Unsupported("view snapshot", format!("no array for {kind}")))?;
				// the parsed type, compared structurally
				let exact = matches!(t, Ty::Path { path, args } if path == CHUNKS_VIEW && args.len() == 1 && args[0] == ty::parse(&want));
				if !exact {
					return Err(Unsupported(
						"view snapshot",
						format!("{t:?} is not {CHUNKS_VIEW}<{want}>"),
					));
				}
				if let Some((_, k, _, elem, _, _)) =
					SCALAR_CHUNKS.iter().find(|(_, k, ..)| *k == kind)
				{
					let copier = match *k {
						"bool" => "support::view_snapshot_bool",
						"str" => "support::view_snapshot_str",
						"binary" => "support::view_snapshot_binview",
						_ => "support::view_snapshot_binary_offset",
					};
					return Ok(Some(Ret {
						materialize: None,
						rust_ty: format!("Vec<Vec<Option<{elem}>>>"),
						conv: format!(
							"{copier}(this.0.chunks(), __r.len(), |__i| __r.get(__i), \"__OP__\")?"
						),
						fallible: true,
						doc: format!(
							"vector of chunks in index order, each a vector of option of {k} values (copied, bounded with payload bytes)"
						),
					}));
				}
				let e = world.ret(
					&Ty::Path {
						path: kind.clone(),
						args: vec![],
					},
					owner,
					depth + 1,
				)?;
				if e.materialize.is_some() || e.rust_ty.starts_with("Vec") {
					return Err(Unsupported(
						"view snapshot",
						format!("element {kind} is not a scalar"),
					));
				}
				return Ok(Some(Ret {
					materialize: None,
					rust_ty: format!("Vec<Vec<Option<{}>>>", e.rust_ty),
					conv: format!(
						"support::view_snapshot::<{kind}, _>(this.0.chunks(), __r.len(), |__i| __r.get(__i), \"__OP__\", |__r| Ok::<_, Error>({}))?",
						e.conv
					),
					fallible: true,
					doc: format!(
						"vector of chunks in index order, each a vector of option of {} (copied, bounded)",
						e.doc
					),
				}));
			}
		}
		Ok(None)
	}
	fn check(&self, world: &World, _state: &State, c: &Callable, owner: &str, ret: &Ret) -> Check {
		// record 0104: likewise for the borrowed indexed chunk view
		if routed_binding(world, c, Some(owner))
			|| !ret.conv.starts_with("support::view_snapshot")
			|| c.receiver != "&self"
		{
			return Check::Refuse(
				"view snapshot",
				"needs an unrouted `&self` call and the view-snapshot conversion".into(),
			);
		}
		Check::Pass
	}
	fn oracle_state(
		&self,
		world: &World,
		key: &str,
		path: &str,
		owner: Option<&str>,
	) -> Option<State> {
		world
			.release
			.families
			.view_snapshots
			.iter()
			.find(|m| m.key == key && m.path == path)
			.and_then(|m| {
				owner
					.and_then(|a| world.wrappers.get(a))
					.and_then(|w| m.native_for(&w.identity))
					.map(String::from)
			})
			.map(State::One)
	}
	fn oracle_fmt(
		&self,
		o: &Oracle,
		state: &State,
		site: OracleSite,
		_t: &Ty,
		owner: Option<&str>,
		depth: u8,
	) -> Option<Option<String>> {
		if site != OracleSite::Top {
			return None;
		}
		// record 0104: a listed `downcast_chunks` view frames as the owned nested vectors the script receives
		if depth == 0 {
			if let Some(kind) = state.one() {
				let elem = SCALAR_CHUNKS
					.iter()
					.find(|(_, k, ..)| *k == kind)
					.map(|(.., oe)| oe.to_string())
					.unwrap_or_else(|| kind.clone());
				let own = match kind.as_str() {
					"bool" => "x",
					"str" => "x.map(|v| v.to_string())",
					"binary" | "binary_offset" => "x.map(|v| v.to_vec())",
					_ => "x.copied()",
				};
				let nested = ty::parse(&format!(
					"alloc::vec::Vec<alloc::vec::Vec<core::option::Option<{elem}>>>"
				));
				return Some(o.oracle_fmt(&nested, owner, depth + 1).map(|f| format!("{{ let __r: Vec<Vec<_>> = (0..__r.len()).map(|i| __r.get(i).expect(\"oracle: an in-range chunk\").iter().map(|x| {own}).collect()).collect(); {f} }}")));
			}
		}
		None
	}
}

/// Record 0105: a listed `downcast_into_iter` snapshot.
pub(crate) struct OwnedIterFamily;
pub(crate) static OWNED_ITER: OwnedIterFamily = OwnedIterFamily;

impl Family for OwnedIterFamily {
	fn name(&self) -> &'static str {
		"owned_iter"
	}
	fn listed_pair_post(
		&self,
		world: &World,
		c: &Callable,
		p: &PairRecord,
		_syn: &mut Callable,
	) -> Listed {
		match owned_iter_entry(&world.release, c) {
			None => Listed::Unlisted,
			Some(Err(why)) => Listed::Refused("owned iterator snapshot", why),
			Some(Ok(e)) => match e.native_for(&p.identity) {
				Some(n) => Listed::Active(State::Two(c.name.clone(), n.to_string())),
				None => Listed::Refused(
					"owned iterator snapshot",
					format!("`{}` is not a listed pair", p.identity),
				),
			},
		}
	}
	fn ret(
		&self,
		world: &World,
		state: &State,
		site: RetSite,
		t: &Ty,
		owner: Option<&str>,
		depth: u8,
	) -> Result<Option<Ret>, Unsupported> {
		if site != RetSite::Top {
			return Ok(None);
		}
		// record 0105: exactly the owned-item iterator of the pair's array, streamed after the preflight
		if let Some((_, kind)) = state.two() {
			if depth == 0 {
				let want = indexed_array(&kind).ok_or_else(|| {
					Unsupported("owned iterator snapshot", format!("no array for {kind}"))
				})?;
				let exact = matches!(t, Ty::Impl(bs) if bs.len() == 1
                    && bs[0].path == "core::iter::traits::double_ended::DoubleEndedIterator"
                    && bs[0].args.is_empty()
                    && bs[0].item.as_deref() == Some(&ty::parse(&want)));
				if !exact {
					return Err(Unsupported(
						"owned iterator snapshot",
						format!("{t:?} is not impl DoubleEndedIterator<Item = {want}>"),
					));
				}
				if let Some((_, k, _, elem, _, _)) =
					SCALAR_CHUNKS.iter().find(|(_, k, ..)| *k == kind)
				{
					let copier = match *k {
						"bool" => "support::owned_snapshot_bool",
						"str" => "support::owned_snapshot_str",
						"binary" => "support::owned_snapshot_binview",
						_ => "support::owned_snapshot_binary_offset",
					};
					return Ok(Some(Ret {
						materialize: None,
						rust_ty: format!("Vec<Vec<Option<{elem}>>>"),
						conv: format!("{copier}(this.0.chunks(), __total, __r, \"__OP__\")?"),
						fallible: true,
						doc: format!(
							"vector of chunks in order, each a vector of option of {k} values (bounded before the receiver is cloned)"
						),
					}));
				}
				let e = world.ret(
					&Ty::Path {
						path: kind.clone(),
						args: vec![],
					},
					owner,
					depth + 1,
				)?;
				if e.materialize.is_some() || e.rust_ty.starts_with("Vec") {
					return Err(Unsupported(
						"owned iterator snapshot",
						format!("element {kind} is not a scalar"),
					));
				}
				return Ok(Some(Ret {
					materialize: None,
					rust_ty: format!("Vec<Vec<Option<{}>>>", e.rust_ty),
					conv: format!(
						"support::owned_snapshot::<{kind}, _>(this.0.chunks(), __total, __r, \"__OP__\", |__r| Ok::<_, Error>({}))?",
						e.conv
					),
					fallible: true,
					doc: format!(
						"vector of chunks in order, each a vector of option of {} (bounded before the receiver is cloned)",
						e.doc
					),
				}));
			}
		}
		Ok(None)
	}
	fn check(&self, world: &World, state: &State, c: &Callable, owner: &str, ret: &Ret) -> Check {
		// record 0105: the consuming iterator runs on a clone; its preflight goes
		// first, before the clone in the call and before Polars
		let Some((op, kind)) = state.two() else {
			return Check::Pass;
		};
		if routed_binding(world, c, Some(owner))
			|| !ret.conv.starts_with("support::owned_snapshot")
			|| c.receiver != "self"
		{
			return Check::Refuse(
				"owned iterator snapshot",
				"needs an unrouted consuming call and the owned-snapshot conversion".into(),
			);
		}
		let preflight = match kind.as_str() {
			"bool" => "support::preflight_bool".to_string(),
			"str" => "support::preflight_str".into(),
			"binary" => "support::preflight_binview".into(),
			"binary_offset" => "support::preflight_binary_offset".into(),
			n => format!("support::preflight_numeric::<{n}>"),
		};
		Check::Pre(
			format!("let __total = {preflight}(this.0.chunks(), \"{op}\")?; "),
			false,
		)
	}
	fn oracle_state(
		&self,
		world: &World,
		key: &str,
		path: &str,
		owner: Option<&str>,
	) -> Option<State> {
		world
			.release
			.families
			.owned_iter_snapshots
			.iter()
			.find(|m| m.key == key && m.path == path)
			.and_then(|m| {
				owner
					.and_then(|a| world.wrappers.get(a))
					.and_then(|w| m.native_for(&w.identity))
					.map(String::from)
			})
			.map(State::One)
	}
	fn oracle_fmt(
		&self,
		o: &Oracle,
		state: &State,
		site: OracleSite,
		_t: &Ty,
		owner: Option<&str>,
		depth: u8,
	) -> Option<Option<String>> {
		if site != OracleSite::Top {
			return None;
		}
		// record 0105: a listed `downcast_into_iter` drives the real owned iterator into the same nested options
		if depth == 0 {
			if let Some(kind) = state.one() {
				let elem = SCALAR_CHUNKS
					.iter()
					.find(|(_, k, ..)| *k == kind)
					.map(|(.., oe)| oe.to_string())
					.unwrap_or_else(|| kind.clone());
				let own = match kind.as_str() {
					"bool" => "x",
					"str" => "x.map(|v| v.to_string())",
					"binary" | "binary_offset" => "x.map(|v| v.to_vec())",
					_ => "x.copied()",
				};
				let nested = ty::parse(&format!(
					"alloc::vec::Vec<alloc::vec::Vec<core::option::Option<{elem}>>>"
				));
				return Some(o.oracle_fmt(&nested, owner, depth + 1).map(|f| {
					format!(
						"{{ let __r: Vec<Vec<_>> = __r.map(|a| a.iter().map(|x| {own}).collect()).collect(); {f} }}"
					)
				}));
			}
		}
		None
	}
}

/// Record 0106: a listed `layout` snapshot.
pub(crate) struct LayoutFamily;
pub(crate) static LAYOUT: LayoutFamily = LayoutFamily;

impl Family for LayoutFamily {
	fn name(&self) -> &'static str {
		"layout"
	}
	fn listed_pair_post(
		&self,
		world: &World,
		c: &Callable,
		p: &PairRecord,
		_syn: &mut Callable,
	) -> Listed {
		match layout_entry(&world.release, c) {
			None => Listed::Unlisted,
			Some(Err(why)) => Listed::Refused("layout snapshot", why),
			Some(Ok(e)) => match e.pair_for(&p.identity) {
				Some((t, n)) => Listed::Active(State::Three(c.name.clone(), n, t)),
				None => Listed::Refused(
					"layout snapshot",
					format!("`{}` is not a listed pair", p.identity),
				),
			},
		}
	}
	fn ret(
		&self,
		world: &World,
		state: &State,
		site: RetSite,
		t: &Ty,
		owner: Option<&str>,
		depth: u8,
	) -> Result<Option<Ret>, Unsupported> {
		if site != RetSite::Top {
			return Ok(None);
		}
		// record 0106: exactly `ChunkedArrayLayout<the pair's owner type>`, tagged and copied after the preflight
		if let Some((_, kind, owner_ty)) = state.three() {
			if depth == 0 {
				let exact = matches!(t, Ty::Path { path, args } if path == LAYOUT_ENUM && args.len() == 1 && args[0] == ty::parse(&owner_ty));
				if !exact {
					return Err(Unsupported(
						"layout snapshot",
						format!("{t:?} is not {LAYOUT_ENUM}<{owner_ty}>"),
					));
				}
				if let Some((_, k, _, elem, _, _)) =
					SCALAR_CHUNKS.iter().find(|(_, k, ..)| *k == kind)
				{
					let copier = match *k {
						"bool" => "support::layout_snapshot_bool",
						"str" => "support::layout_snapshot_str",
						"binary" => "support::layout_snapshot_binview",
						_ => "support::layout_snapshot_binary_offset",
					};
					return Ok(Some(Ret {
						materialize: None,
						rust_ty: format!("(String, Vec<Vec<Option<{elem}>>>)"),
						conv: format!("{copier}(this.0.chunks(), __total, __r, \"__OP__\")?"),
						fallible: true,
						doc: format!(
							"tuple of the layout variant's name and its chunks, each a vector of option of {k} values (bounded before the call)"
						),
					}));
				}
				let e = world.ret(
					&Ty::Path {
						path: kind.clone(),
						args: vec![],
					},
					owner,
					depth + 1,
				)?;
				if e.materialize.is_some() || e.rust_ty.starts_with("Vec") {
					return Err(Unsupported(
						"layout snapshot",
						format!("element {kind} is not a scalar"),
					));
				}
				return Ok(Some(Ret {
					materialize: None,
					rust_ty: format!("(String, Vec<Vec<Option<{}>>>)", e.rust_ty),
					conv: format!(
						"support::layout_snapshot::<{owner_ty}, {kind}, _>(this.0.chunks(), __total, __r, \"__OP__\", |__r| Ok::<_, Error>({}))?",
						e.conv
					),
					fallible: true,
					doc: format!(
						"tuple of the layout variant's name and its chunks, each a vector of option of {} (bounded before the call)",
						e.doc
					),
				}));
			}
		}
		Ok(None)
	}
	fn check(&self, world: &World, state: &State, c: &Callable, owner: &str, ret: &Ret) -> Check {
		// record 0106: the layout's preflight goes before the Polars call too
		let Some((op, kind, _)) = state.three() else {
			return Check::Pass;
		};
		if routed_binding(world, c, Some(owner))
			|| !ret.conv.starts_with("support::layout_snapshot")
			|| c.receiver != "&self"
		{
			return Check::Refuse(
				"layout snapshot",
				"needs an unrouted `&self` call and the layout-snapshot conversion".into(),
			);
		}
		let preflight = match kind.as_str() {
			"bool" => "support::preflight_bool".to_string(),
			"str" => "support::preflight_str".into(),
			"binary" => "support::preflight_binview".into(),
			"binary_offset" => "support::preflight_binary_offset".into(),
			n => format!("support::preflight_numeric::<{n}>"),
		};
		Check::Pre(
			format!("let __total = {preflight}(this.0.chunks(), \"{op}\")?; "),
			false,
		)
	}
	fn oracle_state(
		&self,
		world: &World,
		key: &str,
		path: &str,
		owner: Option<&str>,
	) -> Option<State> {
		world
			.release
			.families
			.layout_snapshots
			.iter()
			.find(|m| m.key == key && m.path == path)
			.and_then(|m| {
				owner
					.and_then(|a| world.wrappers.get(a))
					.and_then(|w| m.pair_for(&w.identity))
					.map(|(_, k)| k)
			})
			.map(State::One)
	}
	fn oracle_fmt(
		&self,
		o: &Oracle,
		state: &State,
		site: OracleSite,
		_t: &Ty,
		owner: Option<&str>,
		depth: u8,
	) -> Option<Option<String>> {
		if site != OracleSite::Top {
			return None;
		}
		// record 0106: a listed `layout` result frames as the variant's name and its owned chunks
		if depth == 0 {
			if let Some(kind) = state.one() {
				let elem = SCALAR_CHUNKS
					.iter()
					.find(|(_, k, ..)| *k == kind)
					.map(|(.., oe)| oe.to_string())
					.unwrap_or_else(|| kind.clone());
				let own = match kind.as_str() {
					"bool" => "x",
					"str" => "x.map(|v| v.to_string())",
					"binary" | "binary_offset" => "x.map(|v| v.to_vec())",
					_ => "x.copied()",
				};
				let tuple = ty::parse(&format!(
					"(alloc::string::String, alloc::vec::Vec<alloc::vec::Vec<core::option::Option<{elem}>>>)"
				));
				let one = format!("vec![a.iter().map(|x| {own}).collect()]");
				let many = format!(
					"ca.downcast_iter().map(|a| a.iter().map(|x| {own}).collect()).collect()"
				);
				return Some(o.oracle_fmt(&tuple, owner, depth + 1).map(|f| format!("{{ use polars_core::chunked_array::ChunkedArrayLayout as __L; let __r: (String, Vec<Vec<Option<_>>>) = match __r {{ __L::SingleNoNull(a) => (\"SingleNoNull\".to_string(), {one}), __L::Single(a) => (\"Single\".to_string(), {one}), __L::MultiNoNull(ca) => (\"MultiNoNull\".to_string(), {many}), __L::Multi(ca) => (\"Multi\".to_string(), {many}) }}; {f} }}")));
			}
		}
		None
	}
	fn script_fmt(
		&self,
		o: &Oracle,
		_state: &State,
		r: &str,
		owner: Option<&str>,
		depth: u8,
	) -> Option<Option<String>> {
		// record 0106: a listed layout's (tag, chunks) pair, framed as the
		// Rust side's tuple formatter frames it; other tuples stay uncompared
		if !(r.starts_with('(') && depth == 0) {
			return None;
		}
		Some((|| -> Option<String> {
			let parts = ty::split_top(r.trim_start_matches('(').trim_end_matches(')'));
			if parts.len() != 2 {
				return None;
			}
			let f0 = o.script_fmt(&parts[0], None, owner, depth + 1)?;
			let f1 = o.script_fmt(&parts[1], None, owner, depth + 1)?;
			return Some(format!(
				"match rune::from_value::<(rune::Value, rune::Value)>(v) {{ Ok((a, b)) => {{ let fa: Result<String, String> = {{ let v = a; {f0} }}; let fb: Result<String, String> = {{ let v = b; {f1} }}; match (fa, fb) {{ (Ok(x), Ok(y)) => Ok(format!(\"({{}}:{{x}}, {{}}:{{y}})\", x.len(), y.len())), (Err(e), _) | (_, Err(e)) => Err(e) }} }}, Err(e) => Err(e.to_string()) }}"
			));
		})())
	}
}

/// Record 0106 controls: a `[[layout_snapshots]]` entry admits only the
/// recorded shape, including the `T: PolarsDataType` where-clause, and fails
/// closed naming the fault; with the scope set only
/// `ChunkedArrayLayout<the pair's owner type>` binds, its preflight emitted
/// before the Polars call; another owner type, a wrapped or unscoped layout
/// stays unsupported with no text.
pub(crate) fn layout_self_test() {
	let ca = "polars_core::chunked_array::ChunkedArray";
	let bound = "polars_core::datatypes::PolarsDataType";
	let mk = |receiver: &str, ret: &str, wh: Vec<String>| Callable {
		key: "k".into(),
		kind: "inherent".into(),
		krate: "polars_core".into(),
		owner: ca.into(),
		name: "layout".into(),
		canonical_path: format!("{ca}::layout"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: receiver.into(),
		params: vec![],
		ret: None,
		ret_canonical: Some(ret.into()),
		generics_canonical: vec![],
		impl_for: None,
		impl_bounds: vec![("T".into(), bound.into())],
		impl_head: Some(format!("{ca}<T>")),
		impl_where: wh,
		impl_assoc: vec![],
		docs_first: None,
		owner_generic: true,
		is_unsafe: false,
		is_async: false,
		deprecated: false,
		hidden: false,
		implementors: vec![],
		trait_reachable: false,
		derived: false,
		bucket: "generic".into(),
		rules: vec![],
		trait_lifetimes: vec![],
	};
	let wh = || vec![format!("T: {bound}")];
	let pairs = |p: Vec<(&str, &str)>| {
		p.into_iter()
			.map(|(a, b)| (format!("polars_core::datatypes::{a}"), b.to_string()))
			.collect::<Vec<_>>()
	};
	let entry = |p: Vec<(String, String)>, cite: &str| LayoutSnapshot {
		key: "k".into(),
		path: format!("{ca}::layout"),
		pairs: p,
		cite: cite.into(),
	};
	let good_c = mk("&self", LAYOUT_RETURN, wh());
	let good = entry(pairs(vec![("Int8Type", "i8"), ("StringType", "str")]), "t");
	assert!(good.check(&good_c).is_ok());
	assert_eq!(
		good.pair_for(&format!("{ca}<polars_core::datatypes::Int8Type>")),
		Some(("polars_core::datatypes::Int8Type".into(), "i8".into()))
	);
	let mut with_param = good_c.clone();
	with_param.params = vec![Param {
		name: "i".into(),
		ty: "usize".into(),
		ty_canonical: "usize".into(),
	}];
	for (label, c, e, why) in [
		(
			"blank citation",
			good_c.clone(),
			entry(pairs(vec![("Int8Type", "i8")]), " "),
			"no citation",
		),
		(
			"no where-clause",
			mk("&self", LAYOUT_RETURN, vec![]),
			good.clone(),
			"is not ChunkedArray<T: PolarsDataType> where T: PolarsDataType",
		),
		(
			"an extra where-clause",
			mk(
				"&self",
				LAYOUT_RETURN,
				vec![format!("T: {bound}"), "T::Native: core::fmt::Debug".into()],
			),
			good.clone(),
			"is not ChunkedArray<T: PolarsDataType> where",
		),
		(
			"an owned receiver",
			mk("self", LAYOUT_RETURN, wh()),
			good.clone(),
			"not a `&self` method",
		),
		(
			"a parameter",
			with_param,
			good.clone(),
			"not a `&self` method",
		),
		(
			"an optional return",
			mk(
				"&self",
				&format!("core::option::Option<{LAYOUT_RETURN}>"),
				wh(),
			),
			good.clone(),
			"is not polars_core::chunked_array::ChunkedArrayLayout<T>",
		),
		(
			"a layout of the array",
			mk(
				"&self",
				"polars_core::chunked_array::ChunkedArrayLayout<T::Array>",
				wh(),
			),
			good.clone(),
			"is not polars_core::chunked_array::ChunkedArrayLayout<T>",
		),
		(
			"a mispaired kind",
			good_c.clone(),
			entry(pairs(vec![("Int8Type", "i16")]), "t"),
			"nor a listed scalar owner and its kind",
		),
		(
			"Struct",
			good_c.clone(),
			entry(pairs(vec![("StructType", "struct")]), "t"),
			"nor a listed scalar owner and its kind",
		),
		(
			"a duplicate pair",
			good_c.clone(),
			entry(pairs(vec![("Int8Type", "i8"), ("Int8Type", "i8")]), "t"),
			"is listed twice",
		),
	] {
		let r = e.check(&c);
		assert!(r.as_ref().is_err_and(|m| m.contains(why)), "{label}: {r:?}");
	}
	let mut release = Release {
		name: "t".into(),
		source: "t".into(),
		provenance: ReleaseProvenance::default(),
		instantiation: InstantiationScope::default(),
		api_crates: vec!["polars_core".into()],
		unordered: vec![],
		excluded_oracle: vec![],
		refused: vec![],
		families: FamilyTables::default(),
	};
	assert!(layout_entry(&release, &good_c).is_none());
	release.families.layout_snapshots = vec![good.clone(), good];
	assert!(matches!(layout_entry(&release, &good_c), Some(Err(ref m)) if m == "listed twice"));
	let owner = "polars_core::datatypes::Int8Chunked";
	let sup = |path: &str| Supporting {
		trait_params: vec![],
		key: path.to_string(),
		kind: "struct".into(),
		canonical_path: path.to_string(),
		found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())],
		crate_paths: vec![path.to_string()],
		public_fields: 0,
		fields_canonical: vec![],
		variant_shapes: vec![],
		variant_payloads: vec![],
		generic: false,
		lifetime: false,
		hidden: false,
		derived: vec!["Clone".into(), "Debug".into()],
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	let conc = |key: &str, ret: &str| {
		let mut c = mk("&self", ret, vec![]);
		c.key = key.into();
		c.owner = owner.into();
		c.canonical_path = format!("{owner}::layout");
		c.impl_head = None;
		c.impl_bounds.clear();
		c.owner_generic = false;
		c.bucket = "mechanical".into();
		c
	};
	let lay = |t: &str| format!("{LAYOUT_ENUM}<polars_core::datatypes::{t}>");
	let inv = Inventory {
		callables: vec![
			conc("i8", &lay("Int8Type")),
			conc("str", &lay("StringType")),
			conc("other", &lay("Int16Type")),
			conc(
				"wrapped",
				&format!("core::option::Option<{}>", lay("Int8Type")),
			),
			conc(
				"chunked",
				&format!(
					"{LAYOUT_ENUM}<polars_core::chunked_array::ChunkedArray<polars_core::datatypes::Int8Type>>"
				),
			),
			conc("unscoped", &lay("Int8Type")),
		],
		supporting: vec![sup(owner)],
		provenance: None,
	};
	let world = World::new(&inv, &release, &["mechanical"]);
	let empty = || Emitted {
		from_names: BTreeMap::new(),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
		frozen_ids: Default::default(),
	};
	let emit = |key: &str, scope: Option<(&str, &str)>| {
		let mut e = empty();
		world.active.set(
			"layout",
			(scope.map(|(k, t)| {
				(
					"layout".to_string(),
					k.to_string(),
					format!("polars_core::datatypes::{t}"),
				)
			}))
			.map(|(a, b, c)| State::Three(a, b, c)),
		);
		emit_method(
			&world,
			&mut e,
			inv.callables.iter().find(|c| c.key == key).unwrap(),
			owner,
			None,
			false,
		);
		world.active.set("layout", None);
		(
			e.entries[0].status.clone(),
			e.entries[0].reason.clone().unwrap_or_default(),
			e.functions,
		)
	};
	for (key, scope, preflight, copier) in [
		(
			"i8",
			("i8", "Int8Type"),
			"support::preflight_numeric::<i8>(this.0.chunks(), \"layout\")?",
			"support::layout_snapshot::<polars_core::datatypes::Int8Type, i8, _>(this.0.chunks(), __total, __r,",
		),
		(
			"str",
			("str", "StringType"),
			"support::preflight_str(this.0.chunks(), \"layout\")?",
			"support::layout_snapshot_str(this.0.chunks(), __total, __r,",
		),
	] {
		let (status, reason, f) = emit(key, Some(scope));
		assert_eq!(status, "generated", "{key}: {reason}");
		let (pf, call, cp) = (
			f.find(preflight),
			f.find(">::layout(&this.0)"),
			f.find(copier),
		);
		assert!(
			pf.is_some() && call.is_some() && cp.is_some() && pf < call && call < cp,
			"{key}: preflight, one Polars call, then the copy: {f}"
		);
		assert!(
			f.contains("-> Result<(String, Vec<Vec<Option<"),
			"{key}: a (tag, chunks) pair: {f}"
		);
	}
	for (key, scope) in [
		("other", Some(("i8", "Int8Type"))),
		("wrapped", Some(("i8", "Int8Type"))),
		("chunked", Some(("i8", "Int8Type"))),
		("unscoped", None),
		("str", Some(("i8", "Int8Type"))),
	] {
		let (status, reason, f) = emit(key, scope);
		assert_eq!(status, "unsupported", "{key} must be refused, got {reason}");
		assert!(f.is_empty(), "{key}: no binding text: {f}");
	}
	assert!(!world.active.is_set("layout"));
	println!("layout self-test: ok");
}

/// Record 0105 controls: an `[[owned_iter_snapshots]]` entry admits only
/// a consuming `self` returning `impl DoubleEndedIterator<Item = T::Array>`
/// and fails closed, naming the fault; with the scope set only an owned
/// item of the pair's array binds, with its preflight emitted before the
/// receiver clone and the Polars call; a borrowed, other-array or unscoped
/// item and a borrowing receiver stay unsupported with no text.
pub(crate) fn owned_iter_self_test() {
	let ca = "polars_core::chunked_array::ChunkedArray";
	let mk = |receiver: &str, params: Vec<Param>, ret: &str| Callable {
		key: "k".into(),
		kind: "inherent".into(),
		krate: "polars_core".into(),
		owner: ca.into(),
		name: "downcast_into_iter".into(),
		canonical_path: format!("{ca}::downcast_into_iter"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: receiver.into(),
		params,
		ret: None,
		ret_canonical: Some(ret.into()),
		generics_canonical: vec![],
		impl_for: None,
		impl_bounds: vec![("T".into(), "polars_core::datatypes::PolarsDataType".into())],
		impl_head: Some(format!("{ca}<T>")),
		impl_where: vec![],
		impl_assoc: vec![],
		docs_first: None,
		owner_generic: true,
		is_unsafe: false,
		is_async: false,
		deprecated: false,
		hidden: false,
		implementors: vec![],
		trait_reachable: false,
		derived: false,
		bucket: "generic".into(),
		rules: vec![],
		trait_lifetimes: vec![],
	};
	let pairs = |p: Vec<(&str, &str)>| {
		p.into_iter()
			.map(|(a, b)| (format!("polars_core::datatypes::{a}"), b.to_string()))
			.collect::<Vec<_>>()
	};
	let entry = |p: Vec<(String, String)>, cite: &str| OwnedIterSnapshot {
		key: "k".into(),
		path: format!("{ca}::downcast_into_iter"),
		pairs: p,
		cite: cite.into(),
	};
	let good_c = mk("self", vec![], DOWNCAST_INTO_ITER_RETURN);
	let good = entry(pairs(vec![("Int8Type", "i8"), ("StringType", "str")]), "t");
	assert!(good.check(&good_c).is_ok());
	for (label, c, e, why) in [
		(
			"blank citation",
			good_c.clone(),
			entry(pairs(vec![("Int8Type", "i8")]), " "),
			"no citation",
		),
		(
			"a borrowing receiver",
			mk("&self", vec![], DOWNCAST_INTO_ITER_RETURN),
			good.clone(),
			"is not a consuming self",
		),
		(
			"a parameter",
			mk(
				"self",
				vec![Param {
					name: "i".into(),
					ty: "usize".into(),
					ty_canonical: "usize".into(),
				}],
				DOWNCAST_INTO_ITER_RETURN,
			),
			good.clone(),
			"without parameters or method generics",
		),
		(
			"a borrowed item",
			mk(
				"self",
				vec![],
				"impl core::iter::traits::double_ended::DoubleEndedIterator<Item = &T::Array>",
			),
			good.clone(),
			"is not impl",
		),
		(
			"a plain Iterator",
			mk(
				"self",
				vec![],
				"impl core::iter::traits::iterator::Iterator<Item = T::Array>",
			),
			good.clone(),
			"is not impl",
		),
		(
			"a mispaired kind",
			good_c.clone(),
			entry(pairs(vec![("StringType", "binary")]), "t"),
			"nor a listed scalar owner and its kind",
		),
		(
			"List",
			good_c.clone(),
			entry(pairs(vec![("ListType", "list")]), "t"),
			"nor a listed scalar owner and its kind",
		),
		(
			"a duplicate pair",
			good_c.clone(),
			entry(pairs(vec![("Int8Type", "i8"), ("Int8Type", "i8")]), "t"),
			"is listed twice",
		),
	] {
		let r = e.check(&c);
		assert!(r.as_ref().is_err_and(|m| m.contains(why)), "{label}: {r:?}");
	}
	let mut release = Release {
		name: "t".into(),
		source: "t".into(),
		provenance: ReleaseProvenance::default(),
		instantiation: InstantiationScope::default(),
		api_crates: vec!["polars_core".into()],
		unordered: vec![],
		excluded_oracle: vec![],
		refused: vec![],
		families: FamilyTables::default(),
	};
	assert!(owned_iter_entry(&release, &good_c).is_none());
	release.families.owned_iter_snapshots = vec![good.clone(), good];
	assert!(matches!(owned_iter_entry(&release, &good_c), Some(Err(ref m)) if m == "listed twice"));
	let owner = "polars_core::datatypes::Int8Chunked";
	let sup = |path: &str| Supporting {
		trait_params: vec![],
		key: path.to_string(),
		kind: "struct".into(),
		canonical_path: path.to_string(),
		found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())],
		crate_paths: vec![path.to_string()],
		public_fields: 0,
		fields_canonical: vec![],
		variant_shapes: vec![],
		variant_payloads: vec![],
		generic: false,
		lifetime: false,
		hidden: false,
		derived: vec!["Clone".into(), "Debug".into()],
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	let conc = |key: &str, receiver: &str, ret: &str| {
		let mut c = mk(receiver, vec![], ret);
		c.key = key.into();
		c.owner = owner.into();
		c.canonical_path = format!("{owner}::downcast_into_iter");
		c.impl_head = None;
		c.impl_bounds.clear();
		c.owner_generic = false;
		c.bucket = "mechanical".into();
		c
	};
	let it = |item: &str| {
		format!("impl core::iter::traits::double_ended::DoubleEndedIterator<Item = {item}>")
	};
	let inv = Inventory {
		callables: vec![
			conc(
				"i8",
				"self",
				&it("polars_arrow::array::primitive::PrimitiveArray<i8>"),
			),
			conc(
				"str",
				"self",
				&it("polars_arrow::array::binview::BinaryViewArrayGeneric<str>"),
			),
			conc(
				"borrowed",
				"self",
				&it("&polars_arrow::array::primitive::PrimitiveArray<i8>"),
			),
			conc(
				"other",
				"self",
				&it("polars_arrow::array::primitive::PrimitiveArray<i16>"),
			),
			conc(
				"by_ref",
				"&self",
				&it("polars_arrow::array::primitive::PrimitiveArray<i8>"),
			),
			conc(
				"unscoped",
				"self",
				&it("polars_arrow::array::primitive::PrimitiveArray<i8>"),
			),
		],
		supporting: vec![sup(owner)],
		provenance: None,
	};
	let world = World::new(&inv, &release, &["mechanical"]);
	let empty = || Emitted {
		from_names: BTreeMap::new(),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
		frozen_ids: Default::default(),
	};
	let emit = |key: &str, kind: Option<&str>| {
		let mut e = empty();
		world.active.set(
			"owned_iter",
			(kind.map(|n| ("downcast_into_iter".to_string(), n.to_string())))
				.map(|(a, b)| State::Two(a, b)),
		);
		emit_method(
			&world,
			&mut e,
			inv.callables.iter().find(|c| c.key == key).unwrap(),
			owner,
			None,
			false,
		);
		world.active.set("owned_iter", None);
		(
			e.entries[0].status,
			e.entries[0].reason.clone().unwrap_or_default(),
			e.functions,
		)
	};
	for (key, kind, preflight, copier) in [
		(
			"i8",
			"i8",
			"support::preflight_numeric::<i8>(this.0.chunks(), \"downcast_into_iter\")?",
			"support::owned_snapshot::<i8, _>(this.0.chunks(), __total, __r,",
		),
		(
			"str",
			"str",
			"support::preflight_str(this.0.chunks(), \"downcast_into_iter\")?",
			"support::owned_snapshot_str(this.0.chunks(), __total, __r,",
		),
	] {
		let (status, reason, f) = emit(key, Some(kind));
		assert_eq!(status, "generated", "{key}: {reason}");
		let (pf, clone, call, cp) = (
			f.find(preflight),
			f.find("this.0.clone()"),
			f.find(">::downcast_into_iter("),
			f.find(copier),
		);
		assert!(
			pf.is_some() && clone.is_some() && call.is_some() && cp.is_some(),
			"{key}: {f}"
		);
		assert!(
			pf < clone && pf < call && call < cp,
			"{key}: the preflight runs before the clone and the Polars call: {f}"
		);
		assert_eq!(
			f.matches("this.0.clone()").count(),
			1,
			"{key}: one clone: {f}"
		);
	}
	for (key, kind) in [
		("borrowed", Some("i8")),
		("other", Some("i8")),
		("by_ref", Some("i8")),
		("unscoped", None),
		("str", Some("i8")),
	] {
		let (status, reason, f) = emit(key, kind);
		assert_eq!(status, "unsupported", "{key} must be refused, got {reason}");
		assert!(f.is_empty(), "{key}: no binding text: {f}");
	}
	assert!(!world.active.is_set("owned_iter"));
	println!("owned-iter self-test: ok");
}

/// Record 0104 controls: a `[[view_snapshots]]` entry admits only
/// `&self -> Chunks<T::Array>` and fails closed, naming the fault; with the
/// scope set only `Chunks<the pair's array>` binds, handing the copier the
/// receiver's chunks, the view's length and its `get`, while another array,
/// an optional view, a bare array and an unscoped view stay unsupported.
pub(crate) fn view_snapshot_self_test() {
	let ca = "polars_core::chunked_array::ChunkedArray";
	let mk = |params: Vec<Param>, ret: &str| Callable {
		key: "k".into(),
		kind: "inherent".into(),
		krate: "polars_core".into(),
		owner: ca.into(),
		name: "downcast_chunks".into(),
		canonical_path: format!("{ca}::downcast_chunks"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: "&self".into(),
		params,
		ret: None,
		ret_canonical: Some(ret.into()),
		generics_canonical: vec![],
		impl_for: None,
		impl_bounds: vec![("T".into(), "polars_core::datatypes::PolarsDataType".into())],
		impl_head: Some(format!("{ca}<T>")),
		impl_where: vec![],
		impl_assoc: vec![],
		docs_first: None,
		owner_generic: true,
		is_unsafe: false,
		is_async: false,
		deprecated: false,
		hidden: false,
		implementors: vec![],
		trait_reachable: false,
		derived: false,
		bucket: "generic".into(),
		rules: vec![],
		trait_lifetimes: vec![],
	};
	let pairs = |p: Vec<(&str, &str)>| {
		p.into_iter()
			.map(|(a, b)| (format!("polars_core::datatypes::{a}"), b.to_string()))
			.collect::<Vec<_>>()
	};
	let entry = |p: Vec<(String, String)>, cite: &str| ViewSnapshot {
		key: "k".into(),
		path: format!("{ca}::downcast_chunks"),
		pairs: p,
		cite: cite.into(),
	};
	let good_c = mk(vec![], DOWNCAST_CHUNKS_RETURN);
	let good = entry(pairs(vec![("Int8Type", "i8"), ("StringType", "str")]), "t");
	assert!(good.check(&good_c).is_ok());
	let mut owned = good_c.clone();
	owned.receiver = "self".into();
	for (label, c, e, why) in [
		(
			"blank citation",
			good_c.clone(),
			entry(pairs(vec![("Int8Type", "i8")]), " "),
			"no citation",
		),
		(
			"an owned receiver",
			owned,
			good.clone(),
			"without parameters or method generics",
		),
		(
			"a parameter",
			mk(
				vec![Param {
					name: "i".into(),
					ty: "usize".into(),
					ty_canonical: "usize".into(),
				}],
				DOWNCAST_CHUNKS_RETURN,
			),
			good.clone(),
			"without parameters or method generics",
		),
		(
			"another element",
			mk(
				vec![],
				"polars_core::chunked_array::ops::downcast::Chunks<T>",
			),
			good.clone(),
			"is not polars_core",
		),
		(
			"an optional view",
			mk(
				vec![],
				&format!("core::option::Option<{DOWNCAST_CHUNKS_RETURN}>"),
			),
			good.clone(),
			"is not polars_core",
		),
		(
			"a mispaired kind",
			good_c.clone(),
			entry(pairs(vec![("StringType", "binary")]), "t"),
			"nor a listed scalar owner and its kind",
		),
		(
			"Struct",
			good_c.clone(),
			entry(pairs(vec![("StructType", "struct")]), "t"),
			"nor a listed scalar owner and its kind",
		),
		(
			"a duplicate pair",
			good_c.clone(),
			entry(pairs(vec![("Int8Type", "i8"), ("Int8Type", "i8")]), "t"),
			"is listed twice",
		),
	] {
		let r = e.check(&c);
		assert!(r.as_ref().is_err_and(|m| m.contains(why)), "{label}: {r:?}");
	}
	let mut release = Release {
		name: "t".into(),
		source: "t".into(),
		provenance: ReleaseProvenance::default(),
		instantiation: InstantiationScope::default(),
		api_crates: vec!["polars_core".into()],
		unordered: vec![],
		excluded_oracle: vec![],
		refused: vec![],
		families: FamilyTables::default(),
	};
	assert!(view_snapshot_entry(&release, &good_c).is_none());
	release.families.view_snapshots = vec![good.clone(), good];
	assert!(
		matches!(view_snapshot_entry(&release, &good_c), Some(Err(ref m)) if m == "listed twice")
	);
	let owner = "polars_core::datatypes::Int8Chunked";
	let sup = |path: &str| Supporting {
		trait_params: vec![],
		key: path.to_string(),
		kind: "struct".into(),
		canonical_path: path.to_string(),
		found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())],
		crate_paths: vec![path.to_string()],
		public_fields: 0,
		fields_canonical: vec![],
		variant_shapes: vec![],
		variant_payloads: vec![],
		generic: false,
		lifetime: false,
		hidden: false,
		derived: vec!["Clone".into(), "Debug".into()],
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	let conc = |key: &str, ret: &str| {
		let mut c = mk(vec![], ret);
		c.key = key.into();
		c.owner = owner.into();
		c.canonical_path = format!("{owner}::downcast_chunks");
		c.impl_head = None;
		c.impl_bounds.clear();
		c.owner_generic = false;
		c.bucket = "mechanical".into();
		c
	};
	let view = |x: &str| format!("{CHUNKS_VIEW}<{x}>");
	let inv = Inventory {
		callables: vec![
			conc(
				"i8",
				&view("polars_arrow::array::primitive::PrimitiveArray<i8>"),
			),
			conc(
				"str",
				&view("polars_arrow::array::binview::BinaryViewArrayGeneric<str>"),
			),
			conc(
				"other",
				&view("polars_arrow::array::primitive::PrimitiveArray<i16>"),
			),
			conc(
				"optional",
				&format!(
					"core::option::Option<{}>",
					view("polars_arrow::array::primitive::PrimitiveArray<i8>")
				),
			),
			conc(
				"bare",
				"&polars_arrow::array::primitive::PrimitiveArray<i8>",
			),
			conc(
				"unscoped",
				&view("polars_arrow::array::primitive::PrimitiveArray<i8>"),
			),
		],
		supporting: vec![sup(owner)],
		provenance: None,
	};
	let world = World::new(&inv, &release, &["mechanical"]);
	let empty = || Emitted {
		from_names: BTreeMap::new(),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
		frozen_ids: Default::default(),
	};
	let emit = |key: &str, kind: Option<&str>| {
		let mut e = empty();
		world.active.set(
			"view_snapshot",
			(kind.map(|n| ("downcast_chunks".to_string(), n.to_string())))
				.map(|(a, b)| State::Two(a, b)),
		);
		emit_method(
			&world,
			&mut e,
			inv.callables.iter().find(|c| c.key == key).unwrap(),
			owner,
			None,
			false,
		);
		world.active.set("view_snapshot", None);
		(
			e.entries[0].status,
			e.entries[0].reason.clone().unwrap_or_default(),
			e.functions,
		)
	};
	for (key, kind, want) in [
		(
			"i8",
			"i8",
			"support::view_snapshot::<i8, _>(this.0.chunks(), __r.len(), |__i| __r.get(__i), \"downcast_chunks\", |__r| Ok::<_, Error>((__r as i64)))?",
		),
		(
			"str",
			"str",
			"support::view_snapshot_str(this.0.chunks(), __r.len(), |__i| __r.get(__i), \"downcast_chunks\")?",
		),
	] {
		let (status, reason, f) = emit(key, Some(kind));
		assert_eq!(status, "generated", "{key}: {reason}");
		assert!(
			f.contains(want) && f.matches(">::downcast_chunks(&this.0)").count() == 1,
			"{key}: one Polars call: {f}"
		);
	}
	for (key, kind) in [
		("other", Some("i8")),
		("optional", Some("i8")),
		("bare", Some("i8")),
		("unscoped", None),
		("str", Some("i8")),
	] {
		let (status, reason, f) = emit(key, kind);
		assert_eq!(status, "unsupported", "{key} must be refused, got {reason}");
		assert!(f.is_empty(), "{key}: no binding text: {f}");
	}
	assert!(!world.active.is_set("view_snapshot"));
	println!("view-snapshot self-test: ok");
}

/// Record 0103 controls: an `[[iter_snapshots]]` entry admits only
/// `&self -> impl DoubleEndedIterator<Item = &T::Array>` and fails closed,
/// naming the fault; with the scope set only the pair's own borrowed array
/// item binds, handing the copier the receiver's chunks, while an owned,
/// other-array or plain-Iterator item and an unscoped return stay
/// unsupported with no text.
pub(crate) fn iter_snapshot_self_test() {
	let ca = "polars_core::chunked_array::ChunkedArray";
	let mk = |params: Vec<Param>, ret: &str| Callable {
		key: "k".into(),
		kind: "inherent".into(),
		krate: "polars_core".into(),
		owner: ca.into(),
		name: "downcast_iter".into(),
		canonical_path: format!("{ca}::downcast_iter"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: "&self".into(),
		params,
		ret: None,
		ret_canonical: Some(ret.into()),
		generics_canonical: vec![],
		impl_for: None,
		impl_bounds: vec![("T".into(), "polars_core::datatypes::PolarsDataType".into())],
		impl_head: Some(format!("{ca}<T>")),
		impl_where: vec![],
		impl_assoc: vec![],
		docs_first: None,
		owner_generic: true,
		is_unsafe: false,
		is_async: false,
		deprecated: false,
		hidden: false,
		implementors: vec![],
		trait_reachable: false,
		derived: false,
		bucket: "generic".into(),
		rules: vec![],
		trait_lifetimes: vec![],
	};
	let pairs = |p: Vec<(&str, &str)>| {
		p.into_iter()
			.map(|(a, b)| (format!("polars_core::datatypes::{a}"), b.to_string()))
			.collect::<Vec<_>>()
	};
	let entry = |p: Vec<(String, String)>, cite: &str| IterSnapshot {
		key: "k".into(),
		path: format!("{ca}::downcast_iter"),
		pairs: p,
		cite: cite.into(),
	};
	let good_c = mk(vec![], DOWNCAST_ITER_RETURN);
	let good = entry(
		pairs(vec![
			("Int8Type", "i8"),
			("UInt64Type", "u64"),
			("StringType", "str"),
		]),
		"t",
	);
	assert!(good.check(&good_c).is_ok());
	let mut owned = good_c.clone();
	owned.receiver = "self".into();
	for (label, c, e, why) in [
		(
			"blank citation",
			good_c.clone(),
			entry(pairs(vec![("Int8Type", "i8")]), " "),
			"no citation",
		),
		(
			"an owned receiver",
			owned,
			good.clone(),
			"without parameters or method generics",
		),
		(
			"a parameter",
			mk(
				vec![Param {
					name: "idx".into(),
					ty: "usize".into(),
					ty_canonical: "usize".into(),
				}],
				DOWNCAST_ITER_RETURN,
			),
			good.clone(),
			"without parameters or method generics",
		),
		(
			"an owned item",
			mk(
				vec![],
				"impl core::iter::traits::double_ended::DoubleEndedIterator<Item = T::Array>",
			),
			good.clone(),
			"is not impl",
		),
		(
			"a plain Iterator",
			mk(
				vec![],
				"impl core::iter::traits::iterator::Iterator<Item = &T::Array>",
			),
			good.clone(),
			"is not impl",
		),
		(
			"an optional return",
			mk(
				vec![],
				&format!("core::option::Option<{DOWNCAST_ITER_RETURN}>"),
			),
			good.clone(),
			"is not impl",
		),
		(
			"a mispaired kind",
			good_c.clone(),
			entry(pairs(vec![("BooleanType", "u8")]), "t"),
			"nor a listed scalar owner and its kind",
		),
		(
			"List",
			good_c.clone(),
			entry(pairs(vec![("ListType", "list")]), "t"),
			"nor a listed scalar owner and its kind",
		),
		(
			"a duplicate pair",
			good_c.clone(),
			entry(pairs(vec![("Int8Type", "i8"), ("Int8Type", "i8")]), "t"),
			"is listed twice",
		),
	] {
		let r = e.check(&c);
		assert!(r.as_ref().is_err_and(|m| m.contains(why)), "{label}: {r:?}");
	}
	let mut release = Release {
		name: "t".into(),
		source: "t".into(),
		provenance: ReleaseProvenance::default(),
		instantiation: InstantiationScope::default(),
		api_crates: vec!["polars_core".into()],
		unordered: vec![],
		excluded_oracle: vec![],
		refused: vec![],
		families: FamilyTables::default(),
	};
	assert!(iter_snapshot_entry(&release, &good_c).is_none());
	release.families.iter_snapshots = vec![good.clone(), good];
	assert!(
		matches!(iter_snapshot_entry(&release, &good_c), Some(Err(ref m)) if m == "listed twice")
	);
	let owner = "polars_core::datatypes::Int8Chunked";
	let sup = |path: &str| Supporting {
		trait_params: vec![],
		key: path.to_string(),
		kind: "struct".into(),
		canonical_path: path.to_string(),
		found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())],
		crate_paths: vec![path.to_string()],
		public_fields: 0,
		fields_canonical: vec![],
		variant_shapes: vec![],
		variant_payloads: vec![],
		generic: false,
		lifetime: false,
		hidden: false,
		derived: vec!["Clone".into(), "Debug".into()],
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	let conc = |key: &str, ret: &str| {
		let mut c = mk(vec![], ret);
		c.key = key.into();
		c.owner = owner.into();
		c.canonical_path = format!("{owner}::downcast_iter");
		c.impl_head = None;
		c.impl_bounds.clear();
		c.owner_generic = false;
		c.bucket = "mechanical".into();
		c
	};
	let it = |item: &str| {
		format!("impl core::iter::traits::double_ended::DoubleEndedIterator<Item = {item}>")
	};
	let inv = Inventory {
		callables: vec![
			conc(
				"i8",
				&it("&polars_arrow::array::primitive::PrimitiveArray<i8>"),
			),
			conc("bool", &it("&polars_arrow::array::boolean::BooleanArray")),
			conc(
				"other",
				&it("&polars_arrow::array::primitive::PrimitiveArray<i16>"),
			),
			conc(
				"owned_item",
				&it("polars_arrow::array::primitive::PrimitiveArray<i8>"),
			),
			conc(
				"unscoped",
				&it("&polars_arrow::array::primitive::PrimitiveArray<i8>"),
			),
		],
		supporting: vec![sup(owner)],
		provenance: None,
	};
	let world = World::new(&inv, &release, &["mechanical"]);
	let empty = || Emitted {
		from_names: BTreeMap::new(),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
		frozen_ids: Default::default(),
	};
	let emit = |key: &str, kind: Option<&str>| {
		let mut e = empty();
		world.active.set(
			"iter_snapshot",
			(kind.map(|n| ("downcast_iter".to_string(), n.to_string())))
				.map(|(a, b)| State::Two(a, b)),
		);
		emit_method(
			&world,
			&mut e,
			inv.callables.iter().find(|c| c.key == key).unwrap(),
			owner,
			None,
			false,
		);
		world.active.set("iter_snapshot", None);
		(
			e.entries[0].status,
			e.entries[0].reason.clone().unwrap_or_default(),
			e.functions,
		)
	};
	for (key, kind, want) in [
		(
			"i8",
			"i8",
			"support::iter_snapshot::<i8, _>(this.0.chunks(), __r, \"downcast_iter\", |__r| Ok::<_, Error>((__r as i64)))?",
		),
		(
			"bool",
			"bool",
			"support::iter_snapshot_bool(this.0.chunks(), __r, \"downcast_iter\")?",
		),
	] {
		let (status, reason, f) = emit(key, Some(kind));
		assert_eq!(status, "generated", "{key}: {reason}");
		assert!(
			f.contains(want) && f.contains(">::downcast_iter(&this.0)"),
			"{key}: one Polars call, the copier sees the receiver's chunks: {f}"
		);
	}
	for (key, kind) in [
		("other", Some("i8")),
		("owned_item", Some("i8")),
		("unscoped", None),
		("bool", Some("i8")),
	] {
		let (status, reason, f) = emit(key, kind);
		assert_eq!(status, "unsupported", "{key} must be refused, got {reason}");
		assert!(f.is_empty(), "{key}: no binding text: {f}");
	}
	assert!(!world.active.is_set("iter_snapshot"));
	println!("iter-snapshot self-test: ok");
}

/// Record 0102 controls: an `[[array_snapshots]]` entry admits only
/// `&self -> &T::Array` on `ChunkedArray<T: PolarsDataType>` and fails
/// closed, naming the fault; with the scope set only `&Array` of the pair's
/// own array binds (the scalar rule or the kind's copier), while an optional,
/// nested, other-array or unscoped return stays unsupported with no text.
pub(crate) fn array_snapshot_self_test() {
	let ca = "polars_core::chunked_array::ChunkedArray";
	let mk = |params: Vec<Param>, ret: &str| Callable {
		key: "k".into(),
		kind: "inherent".into(),
		krate: "polars_core".into(),
		owner: ca.into(),
		name: "downcast_as_array".into(),
		canonical_path: format!("{ca}::downcast_as_array"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: "&self".into(),
		params,
		ret: None,
		ret_canonical: Some(ret.into()),
		generics_canonical: vec![],
		impl_for: None,
		impl_bounds: vec![("T".into(), "polars_core::datatypes::PolarsDataType".into())],
		impl_head: Some(format!("{ca}<T>")),
		impl_where: vec![],
		impl_assoc: vec![],
		docs_first: None,
		owner_generic: true,
		is_unsafe: false,
		is_async: false,
		deprecated: false,
		hidden: false,
		implementors: vec![],
		trait_reachable: false,
		derived: false,
		bucket: "generic".into(),
		rules: vec![],
		trait_lifetimes: vec![],
	};
	let pairs = |p: Vec<(&str, &str)>| {
		p.into_iter()
			.map(|(a, b)| (format!("polars_core::datatypes::{a}"), b.to_string()))
			.collect::<Vec<_>>()
	};
	let entry = |p: Vec<(String, String)>, cite: &str| ArraySnapshot {
		key: "k".into(),
		path: format!("{ca}::downcast_as_array"),
		pairs: p,
		cite: cite.into(),
	};
	let good_c = mk(vec![], DOWNCAST_AS_ARRAY_RETURN);
	let good = entry(
		pairs(vec![
			("Int8Type", "i8"),
			("UInt64Type", "u64"),
			("StringType", "str"),
		]),
		"t",
	);
	assert!(good.check(&good_c).is_ok());
	let mut owned = good_c.clone();
	owned.receiver = "self".into();
	let mut generic = good_c.clone();
	generic.generics_canonical = vec![("F".into(), "".into())];
	let mut with_where = good_c.clone();
	with_where.impl_where = vec!["T: core::fmt::Debug".into()];
	for (label, c, e, why) in [
		(
			"blank citation",
			good_c.clone(),
			entry(pairs(vec![("Int8Type", "i8")]), " "),
			"no citation",
		),
		(
			"an owned receiver",
			owned,
			good.clone(),
			"without parameters or method generics",
		),
		(
			"a method generic",
			generic,
			good.clone(),
			"without parameters or method generics",
		),
		(
			"a where-clause",
			with_where,
			good.clone(),
			"is not ChunkedArray<T: PolarsDataType>",
		),
		(
			"a parameter",
			mk(
				vec![Param {
					name: "idx".into(),
					ty: "usize".into(),
					ty_canonical: "usize".into(),
				}],
				DOWNCAST_AS_ARRAY_RETURN,
			),
			good.clone(),
			"without parameters or method generics",
		),
		(
			"an optional return",
			mk(vec![], "core::option::Option<&T::Array>"),
			good.clone(),
			"is not &T::Array",
		),
		(
			"a fallible return",
			mk(vec![], "polars_error::PolarsResult<&T::Array>"),
			good.clone(),
			"is not &T::Array",
		),
		(
			"a mispaired kind",
			good_c.clone(),
			entry(pairs(vec![("StringType", "binary")]), "t"),
			"nor a listed scalar owner and its kind",
		),
		(
			"Struct",
			good_c.clone(),
			entry(pairs(vec![("StructType", "struct")]), "t"),
			"nor a listed scalar owner and its kind",
		),
		(
			"a duplicate pair",
			good_c.clone(),
			entry(pairs(vec![("Int8Type", "i8"), ("Int8Type", "i8")]), "t"),
			"is listed twice",
		),
	] {
		let r = e.check(&c);
		assert!(r.as_ref().is_err_and(|m| m.contains(why)), "{label}: {r:?}");
	}
	let mut release = Release {
		name: "t".into(),
		source: "t".into(),
		provenance: ReleaseProvenance::default(),
		instantiation: InstantiationScope::default(),
		api_crates: vec!["polars_core".into()],
		unordered: vec![],
		excluded_oracle: vec![],
		refused: vec![],
		families: FamilyTables::default(),
	};
	assert!(array_snapshot_entry(&release, &good_c).is_none());
	release.families.array_snapshots = vec![good.clone(), good];
	assert!(
		matches!(array_snapshot_entry(&release, &good_c), Some(Err(ref m)) if m == "listed twice")
	);
	let owner = "polars_core::datatypes::Int8Chunked";
	let sup = |path: &str| Supporting {
		trait_params: vec![],
		key: path.to_string(),
		kind: "struct".into(),
		canonical_path: path.to_string(),
		found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())],
		crate_paths: vec![path.to_string()],
		public_fields: 0,
		fields_canonical: vec![],
		variant_shapes: vec![],
		variant_payloads: vec![],
		generic: false,
		lifetime: false,
		hidden: false,
		derived: vec!["Clone".into(), "Debug".into()],
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	let conc = |key: &str, ret: &str| {
		let mut c = mk(vec![], ret);
		c.key = key.into();
		c.owner = owner.into();
		c.canonical_path = format!("{owner}::downcast_as_array");
		c.impl_head = None;
		c.impl_bounds.clear();
		c.owner_generic = false;
		c.bucket = "mechanical".into();
		c
	};
	let prim = |n: &str| format!("&polars_arrow::array::primitive::PrimitiveArray<{n}>");
	let inv = Inventory {
		callables: vec![
			conc("i8", &prim("i8")),
			conc("u64", &prim("u64")),
			conc(
				"bin",
				"&polars_arrow::array::binview::BinaryViewArrayGeneric<[u8]>",
			),
			conc("other", &prim("i16")),
			conc("optional", &format!("core::option::Option<{}>", prim("i8"))),
			conc(
				"owned",
				"polars_arrow::array::primitive::PrimitiveArray<i8>",
			),
			conc("unscoped", &prim("i8")),
		],
		supporting: vec![sup(owner)],
		provenance: None,
	};
	let world = World::new(&inv, &release, &["mechanical"]);
	let empty = || Emitted {
		from_names: BTreeMap::new(),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
		frozen_ids: Default::default(),
	};
	let emit = |key: &str, kind: Option<&str>| {
		let mut e = empty();
		world.active.set(
			"array_snapshot",
			(kind.map(|n| ("downcast_as_array".to_string(), n.to_string())))
				.map(|(a, b)| State::Two(a, b)),
		);
		emit_method(
			&world,
			&mut e,
			inv.callables.iter().find(|c| c.key == key).unwrap(),
			owner,
			None,
			false,
		);
		world.active.set("array_snapshot", None);
		(
			e.entries[0].status,
			e.entries[0].reason.clone().unwrap_or_default(),
			e.functions,
		)
	};
	for (key, kind, want) in [
		(
			"i8",
			"i8",
			"support::array_snapshot::<i8, _>(__r, \"downcast_as_array\", |__r| Ok::<_, Error>((__r as i64)))?",
		),
		(
			"u64",
			"u64",
			"support::widen::<u64>(__r, \"downcast_as_array\")?",
		),
		(
			"bin",
			"binary",
			"support::array_snapshot_binview(__r, \"downcast_as_array\")?",
		),
	] {
		let (status, reason, f) = emit(key, Some(kind));
		assert_eq!(status, "generated", "{key}: {reason}");
		assert!(
			f.contains(want) && !f.contains("-> Result<Option<"),
			"{key}: one vector, no second optional layer: {f}"
		);
	}
	for (key, kind) in [
		("other", Some("i8")),
		("optional", Some("i8")),
		("owned", Some("i8")),
		("unscoped", None),
		("bin", Some("str")),
	] {
		let (status, reason, f) = emit(key, kind);
		assert_eq!(status, "unsupported", "{key} must be refused, got {reason}");
		assert!(f.is_empty(), "{key}: no binding text: {f}");
	}
	assert!(!world.active.is_set("array_snapshot"));
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
pub(crate) fn indexed_chunk_self_test() {
	let ca = "polars_core::chunked_array::ChunkedArray";
	let idx = || {
		vec![Param {
			name: "idx".into(),
			ty: "usize".into(),
			ty_canonical: "usize".into(),
		}]
	};
	let mk = |params: Vec<Param>, ret: &str| Callable {
		key: "k".into(),
		kind: "inherent".into(),
		krate: "polars_core".into(),
		owner: ca.into(),
		name: "downcast_get".into(),
		canonical_path: format!("{ca}::downcast_get"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: "&self".into(),
		params,
		ret: None,
		ret_canonical: Some(ret.into()),
		generics_canonical: vec![],
		impl_for: None,
		impl_bounds: vec![("T".into(), "polars_core::datatypes::PolarsDataType".into())],
		impl_head: Some(format!("{ca}<T>")),
		impl_where: vec![],
		impl_assoc: vec![],
		docs_first: None,
		owner_generic: true,
		is_unsafe: false,
		is_async: false,
		deprecated: false,
		hidden: false,
		implementors: vec![],
		trait_reachable: false,
		derived: false,
		bucket: "generic".into(),
		rules: vec![],
		trait_lifetimes: vec![],
	};
	let pairs = |p: Vec<(&str, &str)>| {
		p.into_iter()
			.map(|(a, b)| (format!("polars_core::datatypes::{a}"), b.to_string()))
			.collect::<Vec<_>>()
	};
	let entry = |p: Vec<(String, String)>, cite: &str| IndexedChunkSnapshot {
		key: "k".into(),
		path: format!("{ca}::downcast_get"),
		pairs: p,
		cite: cite.into(),
	};
	let good_c = mk(idx(), DOWNCAST_GET_RETURN);
	let good = entry(
		pairs(vec![
			("Int8Type", "i8"),
			("UInt64Type", "u64"),
			("StringType", "str"),
			("BinaryOffsetType", "binary_offset"),
		]),
		"t",
	);
	assert!(good.check(&good_c).is_ok());
	let mut owned = good_c.clone();
	owned.receiver = "self".into();
	let mut generic = good_c.clone();
	generic.generics_canonical = vec![("F".into(), "".into())];
	for (label, c, e, why) in [
		(
			"blank citation",
			good_c.clone(),
			entry(pairs(vec![("Int8Type", "i8")]), " "),
			"no citation",
		),
		(
			"an owned receiver",
			owned,
			good.clone(),
			"not a `&self` method taking one usize",
		),
		(
			"a method generic",
			generic,
			good.clone(),
			"not a `&self` method taking one usize",
		),
		(
			"no parameter",
			mk(vec![], DOWNCAST_GET_RETURN),
			good.clone(),
			"not a `&self` method taking one usize",
		),
		(
			"an i64 index",
			mk(
				vec![Param {
					name: "idx".into(),
					ty: "i64".into(),
					ty_canonical: "i64".into(),
				}],
				DOWNCAST_GET_RETURN,
			),
			good.clone(),
			"not a `&self` method taking one usize",
		),
		(
			"a non-optional return",
			mk(idx(), "&T::Array"),
			good.clone(),
			"is not core::option::Option<&T::Array>",
		),
		(
			"a fallible return",
			mk(idx(), "polars_error::PolarsResult<&T::Array>"),
			good.clone(),
			"is not core::option::Option<&T::Array>",
		),
		(
			"a mispaired kind",
			good_c.clone(),
			entry(pairs(vec![("StringType", "binary")]), "t"),
			"nor a listed scalar owner and its kind",
		),
		(
			"List",
			good_c.clone(),
			entry(pairs(vec![("ListType", "list")]), "t"),
			"nor a listed scalar owner and its kind",
		),
		(
			"a duplicate pair",
			good_c.clone(),
			entry(pairs(vec![("Int8Type", "i8"), ("Int8Type", "i8")]), "t"),
			"is listed twice",
		),
	] {
		let r = e.check(&c);
		assert!(r.as_ref().is_err_and(|m| m.contains(why)), "{label}: {r:?}");
	}
	let mut release = Release {
		name: "t".into(),
		source: "t".into(),
		provenance: ReleaseProvenance::default(),
		instantiation: InstantiationScope::default(),
		api_crates: vec!["polars_core".into()],
		unordered: vec![],
		excluded_oracle: vec![],
		refused: vec![],
		families: FamilyTables::default(),
	};
	assert!(indexed_chunk_entry(&release, &good_c).is_none());
	release.families.indexed_chunk_snapshots = vec![good.clone(), good];
	assert!(
		matches!(indexed_chunk_entry(&release, &good_c), Some(Err(ref m)) if m == "listed twice")
	);
	let owner = "polars_core::datatypes::Int8Chunked";
	let sup = |path: &str| Supporting {
		trait_params: vec![],
		key: path.to_string(),
		kind: "struct".into(),
		canonical_path: path.to_string(),
		found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())],
		crate_paths: vec![path.to_string()],
		public_fields: 0,
		fields_canonical: vec![],
		variant_shapes: vec![],
		variant_payloads: vec![],
		generic: false,
		lifetime: false,
		hidden: false,
		derived: vec!["Clone".into(), "Debug".into()],
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	let conc = |key: &str, ret: &str| {
		let mut c = mk(idx(), ret);
		c.key = key.into();
		c.owner = owner.into();
		c.canonical_path = format!("{owner}::downcast_get");
		c.impl_head = None;
		c.impl_bounds.clear();
		c.owner_generic = false;
		c.bucket = "mechanical".into();
		c
	};
	let prim = |n: &str| {
		format!("core::option::Option<&polars_arrow::array::primitive::PrimitiveArray<{n}>>")
	};
	let utf8 = "core::option::Option<&polars_arrow::array::binview::BinaryViewArrayGeneric<str>>";
	let inv = Inventory {
		callables: vec![
			conc("i8", &prim("i8")),
			conc("u64", &prim("u64")),
			conc("str", utf8),
			conc("other_array", &prim("i16")),
			conc("nested", &format!("core::option::Option<{}>", prim("i8"))),
			conc(
				"borrow",
				"&polars_arrow::array::primitive::PrimitiveArray<i8>",
			),
			conc("unscoped", &prim("i8")),
		],
		supporting: vec![sup(owner)],
		provenance: None,
	};
	let world = World::new(&inv, &release, &["mechanical"]);
	let empty = || Emitted {
		from_names: BTreeMap::new(),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
		frozen_ids: Default::default(),
	};
	let emit = |key: &str, kind: Option<&str>| {
		let mut e = empty();
		world.active.set(
			"indexed_chunk",
			(kind.map(|n| ("downcast_get".to_string(), n.to_string())))
				.map(|(a, b)| State::Two(a, b)),
		);
		emit_method(
			&world,
			&mut e,
			inv.callables.iter().find(|c| c.key == key).unwrap(),
			owner,
			None,
			false,
		);
		world.active.set("indexed_chunk", None);
		(
			e.entries[0].status.clone(),
			e.entries[0].reason.clone().unwrap_or_default(),
			e.functions,
		)
	};
	for (key, kind, want) in [
		(
			"i8",
			"i8",
			"support::indexed_snapshot::<i8, _>(__r, \"downcast_get\", |__r| Ok::<_, Error>((__r as i64)))?",
		),
		(
			"u64",
			"u64",
			"support::indexed_snapshot::<u64, _>(__r, \"downcast_get\", |__r| Ok::<_, Error>(support::widen::<u64>(__r, \"downcast_get\")?))?",
		),
		(
			"str",
			"str",
			"support::indexed_snapshot_str(__r, \"downcast_get\")?",
		),
	] {
		let (status, reason, f) = emit(key, Some(kind));
		assert_eq!(status, "generated", "{key}: {reason}");
		assert!(
			f.contains(want) && f.contains("support::narrow::<usize>(idx, \"idx\")?"),
			"{key}: {f}"
		);
	}
	for (key, kind) in [
		("other_array", Some("i8")),
		("nested", Some("i8")),
		("borrow", Some("i8")),
		("unscoped", None),
		("str", Some("i8")),
	] {
		let (status, reason, f) = emit(key, kind);
		assert_eq!(status, "unsupported", "{key} must be refused, got {reason}");
		assert!(f.is_empty(), "{key}: no binding text: {f}");
	}
	assert!(!world.active.is_set("indexed_chunk"));
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
pub(crate) fn chunk_snapshot_self_test() {
	let ca = "polars_core::chunked_array::ChunkedArray";
	let mk = |ret: &str| Callable {
		key: "k".into(),
		kind: "inherent".into(),
		krate: "polars_core".into(),
		owner: ca.into(),
		name: "chunks".into(),
		canonical_path: format!("{ca}::chunks"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: "&self".into(),
		params: vec![],
		ret: None,
		ret_canonical: Some(ret.into()),
		generics_canonical: vec![],
		impl_for: None,
		impl_bounds: vec![("T".into(), "polars_core::datatypes::PolarsDataType".into())],
		impl_head: Some(format!("{ca}<T>")),
		impl_where: vec![],
		impl_assoc: vec![],
		docs_first: None,
		owner_generic: true,
		is_unsafe: false,
		is_async: false,
		deprecated: false,
		hidden: false,
		implementors: vec![],
		trait_reachable: false,
		derived: false,
		bucket: "generic".into(),
		rules: vec![],
		trait_lifetimes: vec![],
	};
	let pairs = |p: Vec<(&str, &str)>| {
		p.into_iter()
			.map(|(a, b)| (format!("polars_core::datatypes::{a}"), b.to_string()))
			.collect::<Vec<_>>()
	};
	let entry = |p: Vec<(String, String)>, cite: &str| ChunkSnapshot {
		key: "k".into(),
		path: format!("{ca}::chunks"),
		pairs: p,
		cite: cite.into(),
	};
	let good_c = mk(CHUNKS_RETURN);
	let good = entry(
		pairs(vec![
			("Int8Type", "i8"),
			("UInt64Type", "u64"),
			("Float32Type", "f32"),
		]),
		"t",
	);
	assert!(good.check(&good_c).is_ok());
	assert_eq!(
		good.native_for(&format!("{ca}<polars_core::datatypes::UInt64Type>")),
		Some("u64")
	);
	assert_eq!(
		good.native_for(&format!("{ca}<polars_core::datatypes::StringType>")),
		None
	);
	let mut other_bound = good_c.clone();
	other_bound.impl_bounds = vec![(
		"T".into(),
		"polars_core::datatypes::PolarsNumericType".into(),
	)];
	let mut with_where = good_c.clone();
	with_where.impl_where = vec!["T::Native: core::fmt::Debug".into()];
	let mut owned = good_c.clone();
	owned.receiver = "self".into();
	let mut with_param = good_c.clone();
	with_param.params = vec![Param {
		name: "x".into(),
		ty: "usize".into(),
		ty_canonical: "usize".into(),
	}];
	for (label, c, e, why) in [
		(
			"blank citation",
			good_c.clone(),
			entry(pairs(vec![("Int8Type", "i8")]), " "),
			"no citation",
		),
		(
			"another owner bound",
			other_bound,
			good.clone(),
			"is not ChunkedArray<T: PolarsDataType>",
		),
		(
			"a where-clause",
			with_where,
			good.clone(),
			"is not ChunkedArray<T: PolarsDataType>",
		),
		(
			"an owned receiver",
			owned,
			good.clone(),
			"not a `&self` method",
		),
		(
			"a parameter",
			with_param,
			good.clone(),
			"not a `&self` method",
		),
		(
			"a mutable borrow",
			mk("&mut alloc::vec::Vec<polars_arrow::array::ArrayRef>"),
			good.clone(),
			"is not &alloc::vec::Vec",
		),
		(
			"an owned vector",
			mk("alloc::vec::Vec<polars_arrow::array::ArrayRef>"),
			good.clone(),
			"is not &alloc::vec::Vec",
		),
		(
			"another Arrow return",
			mk("&polars_arrow::array::ArrayRef"),
			good.clone(),
			"is not &alloc::vec::Vec",
		),
		(
			"no pair",
			good_c.clone(),
			entry(vec![], "t"),
			"no listed pair",
		),
		(
			"an unlisted nonnumeric owner",
			good_c.clone(),
			entry(pairs(vec![("StructType", "struct")]), "t"),
			"is not a numeric type and its native",
		),
		(
			"a mispaired native",
			good_c.clone(),
			entry(pairs(vec![("Int8Type", "i64")]), "t"),
			"is not a numeric type and its native",
		),
		(
			"a duplicate pair",
			good_c.clone(),
			entry(pairs(vec![("Int8Type", "i8"), ("Int8Type", "i8")]), "t"),
			"is listed twice",
		),
		// record 0100: a scalar owner must carry its own kind
		(
			"String as binary",
			good_c.clone(),
			entry(pairs(vec![("StringType", "binary")]), "t"),
			"nor a listed scalar owner and its kind",
		),
		(
			"Boolean as a number",
			good_c.clone(),
			entry(pairs(vec![("BooleanType", "u8")]), "t"),
			"nor a listed scalar owner and its kind",
		),
		(
			"List",
			good_c.clone(),
			entry(pairs(vec![("ListType", "list")]), "t"),
			"nor a listed scalar owner and its kind",
		),
	] {
		let r = e.check(&c);
		assert!(r.as_ref().is_err_and(|m| m.contains(why)), "{label}: {r:?}");
	}
	let mut release = Release {
		name: "t".into(),
		source: "t".into(),
		provenance: ReleaseProvenance::default(),
		instantiation: InstantiationScope::default(),
		api_crates: vec!["polars_core".into()],
		unordered: vec![],
		excluded_oracle: vec![],
		refused: vec![],
		families: FamilyTables::default(),
	};
	let scalars = entry(
		pairs(vec![
			("BooleanType", "bool"),
			("StringType", "str"),
			("BinaryType", "binary"),
			("BinaryOffsetType", "binary_offset"),
		]),
		"t",
	);
	assert!(
		scalars.check(&good_c).is_ok(),
		"the four scalar owners with their kinds pass"
	);
	assert!(chunk_snapshot_entry(&release, &good_c).is_none());
	release.families.chunk_snapshots = vec![good.clone(), good];
	assert!(
		matches!(chunk_snapshot_entry(&release, &good_c), Some(Err(ref m)) if m == "listed twice")
	);
	// the emitter, with the pair scope set
	let owner = "polars_core::datatypes::Int8Chunked";
	let sup = |path: &str| Supporting {
		trait_params: vec![],
		key: path.to_string(),
		kind: "struct".into(),
		canonical_path: path.to_string(),
		found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())],
		crate_paths: vec![path.to_string()],
		public_fields: 0,
		fields_canonical: vec![],
		variant_shapes: vec![],
		variant_payloads: vec![],
		generic: false,
		lifetime: false,
		hidden: false,
		derived: vec!["Clone".into(), "Debug".into()],
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	let conc = |key: &str, ret: &str| {
		let mut c = mk(ret);
		c.key = key.into();
		c.owner = owner.into();
		c.canonical_path = format!("{owner}::chunks");
		c.impl_head = None;
		c.impl_bounds.clear();
		c.owner_generic = false;
		c.bucket = "mechanical".into();
		c
	};
	let boxed = "&alloc::vec::Vec<alloc::boxed::Box<dyn polars_arrow::array::Array>>";
	let inv = Inventory {
		callables: vec![
			conc("i8", boxed),
			conc("u64", boxed),
			conc(
				"owned",
				"alloc::vec::Vec<alloc::boxed::Box<dyn polars_arrow::array::Array>>",
			),
			conc("nested", &format!("core::option::Option<{boxed}>")),
			conc("one", "&alloc::boxed::Box<dyn polars_arrow::array::Array>"),
			conc("unscoped", boxed),
		],
		supporting: vec![sup(owner)],
		provenance: None,
	};
	let world = World::new(&inv, &release, &["mechanical"]);
	let empty = || Emitted {
		from_names: BTreeMap::new(),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
		frozen_ids: Default::default(),
	};
	let emit = |key: &str, native: Option<&str>| {
		let mut e = empty();
		world.active.set(
			"chunk_snapshot",
			(native.map(|n| ("chunks".to_string(), n.to_string()))).map(|(a, b)| State::Two(a, b)),
		);
		emit_method(
			&world,
			&mut e,
			inv.callables.iter().find(|c| c.key == key).unwrap(),
			owner,
			None,
			false,
		);
		world.active.set("chunk_snapshot", None);
		(
			e.entries[0].status.clone(),
			e.entries[0].reason.clone().unwrap_or_default(),
			e.functions,
		)
	};
	for (key, native, elem) in [
		("i8", "i8", "(__r as i64)"),
		("u64", "u64", "support::widen::<u64>(__r, \"chunks\")?"),
	] {
		let (status, reason, f) = emit(key, Some(native));
		assert_eq!(status, "generated", "{key}: {reason}");
		assert!(
			f.contains("-> Result<Vec<Vec<Option<i64>>>, Error>")
				&& f.contains(&format!(
					"support::chunk_snapshot::<{native}, _>(__r, \"chunks\", |__r| Ok::<_, Error>({elem}))?"
				)),
			"{key}: {f}"
		);
	}
	// record 0100: each scalar kind has its own copier and element type
	for (kind, copier, elem) in [
		("bool", "support::chunk_snapshot_bool", "bool"),
		("str", "support::chunk_snapshot_str", "String"),
		("binary", "support::chunk_snapshot_binview", "Vec<i64>"),
		(
			"binary_offset",
			"support::chunk_snapshot_binary_offset",
			"Vec<i64>",
		),
	] {
		let (status, reason, f) = emit("i8", Some(kind));
		assert_eq!(status, "generated", "{kind}: {reason}");
		assert!(
			f.contains(&format!("-> Result<Vec<Vec<Option<{elem}>>>, Error>"))
				&& f.contains(&format!("{copier}(__r, \"chunks\")?")),
			"{kind}: {f}"
		);
	}
	for (key, native) in [
		("owned", Some("i8")),
		("nested", Some("i8")),
		("one", Some("i8")),
		("unscoped", None),
	] {
		let (status, reason, f) = emit(key, native);
		assert_eq!(status, "unsupported", "{key} must be refused, got {reason}");
		assert!(f.is_empty(), "{key}: no binding text: {f}");
	}
	assert!(!world.active.is_set("chunk_snapshot"));
	println!("chunk-snapshot self-test: ok");
}

#[cfg(test)]
mod tests {
	#[test]
	fn chunk_snapshot() {
		super::chunk_snapshot_self_test();
	}
	#[test]
	fn indexed_chunk() {
		super::indexed_chunk_self_test();
	}
	#[test]
	fn array_snapshot() {
		super::array_snapshot_self_test();
	}
	#[test]
	fn iter_snapshot() {
		super::iter_snapshot_self_test();
	}
	#[test]
	fn view_snapshot() {
		super::view_snapshot_self_test();
	}
	#[test]
	fn owned_iter() {
		super::owned_iter_self_test();
	}
	#[test]
	fn layout() {
		super::layout_self_test();
	}
}
