use crate::families::bounds::{
	ExternalBound, HashToken, MethodScalarGeneric, NullAwareReturn, SizedSelfMethod,
};
use crate::families::callbacks::{
	CallbackInvocation, CallbackMutable, CallbackRecipe, CallbackSafe, CallbackSink,
};
use crate::families::free_instantiations::FreeInstantiation;
use crate::families::generic_inputs::BitmapInput;
use crate::families::returns::{BoundedReadback, CowReturn, IteratorReturn};
use crate::families::snapshots::{
	ArraySnapshot, ChunkSnapshot, IndexedChunkSnapshot, IterSnapshot, LayoutSnapshot,
	OwnedIterSnapshot, ViewSnapshot,
};
use crate::model;
use crate::model::Inventory;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Record 0115: declares a release-file schema struct together with the
/// list of its field names, so the accepted top-level keys of a release
/// file come from the same declaration as the fields (no second list).
macro_rules! release_schema {
	($(#[$outer:meta])* $vis:vis struct $name:ident { $($(#[$m:meta])* $fvis:vis $field:ident : $ty:ty,)* } $(flatten $flat:ident : $flat_ty:ty)?) => {
		$(#[$outer])*
		$vis struct $name {
			$($(#[$m])* $fvis $field: $ty,)*
			$(#[serde(flatten)] pub(crate) $flat: $flat_ty,)?
		}
		impl $name {
			/// The top-level keys this struct reads from a release file.
			pub(crate) const KEYS: &'static [&'static str] = &[$(stringify!($field)),*];
		}
	};
}

release_schema! {
/// Record 0075: the release-specific inputs, read from
/// `releases/<release>.toml` and recorded (name, source, digest) in
/// `surface.json`. Nothing about a release is a constant in this file.
#[derive(serde::Deserialize, Clone)]
pub(crate) struct Release {
	pub(crate) name: String,
	pub(crate) source: String,
	pub(crate) api_crates: Vec<String>,
	/// The inventory this policy belongs to: `release` for a crates.io
	/// release, `rev` for Git sources. Checked against the inventory's
	/// recorded provenance before anything is generated.
	#[serde(default)]
	pub(crate) provenance: ReleaseProvenance,
	#[serde(default)]
	pub(crate) unordered: Vec<Unordered>,
	#[serde(default)]
	pub(crate) excluded_oracle: Vec<ExcludedOracle>,
	/// Record 0084: operations the release refuses to emit although the
	/// mapping rules would admit them, each with the source contract that
	/// the binding would have to validate first.
	#[serde(default)]
	pub(crate) refused: Vec<RefusedOperation>,
	/// Record 0076: which alias families get instantiated bindings; empty
	/// means every family. The shipped set under the launch budget is
	/// recorded here, as the recipe that selected it.
	#[serde(default)]
	pub(crate) instantiation: InstantiationScope,
}
	flatten families: FamilyTables
}

release_schema! {
/// Record 0115: every family's release table, flattened into `Release`
/// (the file format is unchanged); a self-test lists only the tables it
/// uses and takes the rest from `Default`.
#[derive(serde::Deserialize, Clone, Default)]
pub(crate) struct FamilyTables {
	/// Record 0085: canonical paths whose validity `Bitmap` return (direct,
	/// optional or as iterator items) is copied into an owned `Vec<bool>`.
	#[serde(default)]
	pub(crate) bitmap_returns: Vec<String>,
	/// Record 0086: canonical paths whose one `Bitmap` parameter is built
	/// from a script `Vec<bool>`, with the length it must equal.
	#[serde(default)]
	pub(crate) bitmap_inputs: Vec<BitmapInput>,
	/// Record 0087: canonical paths whose concrete iterator return (a
	/// `Map` Polars names by alias) is materialized as an exact-size
	/// iterator of the given integer item, converted with a range check.
	#[serde(default)]
	pub(crate) iterator_returns: Vec<IteratorReturn>,
	/// Record 0088: callables (by inventory key and canonical path) whose
	/// `Cow<Wrapped>` return is made owned inside the call.
	#[serde(default)]
	pub(crate) cow_returns: Vec<CowReturn>,
	/// Record 0089: generic free functions instantiated once per listed
	/// concrete type, their `ChunkedArray<T>` argument spelled as the
	/// type's wrapper and the binding placed on that wrapper.
	#[serde(default)]
	pub(crate) free_instantiations: Vec<FreeInstantiation>,
	/// Record 0116 (rule 3): generic owners with no alias, instantiated per
	/// listed dtype as synthetic wrappers.
	#[serde(default)]
	pub(crate) dtype_instantiations: Vec<crate::families::dtype_owners::DtypeInstantiation>,
	/// Record 0118: the closed std-facts table (allowlisted keys, cited).
	#[serde(default)]
	pub(crate) std_facts: Vec<crate::families::std_facts::StdFact>,
	/// Record 0119: API crates whose types are always registered under
	/// their own Rune module (`polars_arrow` -> `polars::arrow`) and never
	/// take part in short-name collisions, so no existing path can move.
	#[serde(default)]
	pub(crate) namespaced_crates: Vec<String>,
	/// Record 0119: the bindings a previous record generated, which this
	/// generation must keep unchanged (a JSON file, relative to the release file).
	#[serde(default)]
	pub(crate) frozen_bindings: Option<String>,
	/// Record 0119: type-path prefixes of a namespaced crate whose types form
	/// no wrapper in this record (the concrete arrays, deferred to 0120).
	#[serde(default)]
	pub(crate) deferred_type_prefixes: Vec<DeferredPrefix>,
	/// Record 0119: index arguments checked against the receiver before the call.
	#[serde(default)]
	pub(crate) receiver_guards: Vec<crate::families::receiver_guards::ReceiverGuard>,
	/// Record 0092: family-census methods whose one scalar function generic
	/// is bound, per proven pair, to that pair's native type.
	#[serde(default)]
	pub(crate) method_scalar_generics: Vec<MethodScalarGeneric>,
	/// Record 0093: callables (by inventory key and canonical path) whose
	/// `usize` result is proven, from the pinned source, to be a length of
	/// or index into a `Vec`/`IndexMap`, so it fits `i64` and keeps a plain
	/// integer return; every other `usize`/`u64` read-back is checked.
	#[serde(default)]
	pub(crate) bounded_readbacks: Vec<BoundedReadback>,
	/// Record 0094: categorical hashes (by inventory key and canonical path)
	/// carried across the script boundary as exact 16-digit lowercase hex
	/// tokens, as a return or as one named `u64` parameter.
	#[serde(default)]
	pub(crate) hash_tokens: Vec<HashToken>,
	/// Record 0096: family methods (by inventory key and canonical path)
	/// whose exact `Either<Vec<T::Native>, Vec<Option<T::Native>>>` return,
	/// on the listed types, becomes one owned `Vec<Option<script value>>`,
	/// bounded before the Polars call.
	#[serde(default)]
	pub(crate) null_aware_returns: Vec<NullAwareReturn>,
	/// Record 0097: `ChunkedArray` methods whose only function-level
	/// generic is `Self: Sized`, admitted on every proven family pair with
	/// the exact listed signature, and guarded by the receiver length.
	#[serde(default)]
	pub(crate) sized_self_methods: Vec<SizedSelfMethod>,
	/// Record 0098: `ChunkedArray` methods whose only unprovable impl bound
	/// is a cited external trait on `T::Native` (`num_traits::float::Float`,
	/// and `Canonical`), discharged for the listed float pairs only.
	#[serde(default)]
	pub(crate) external_bounds: Vec<ExternalBound>,
	/// Record 0099: `ChunkedArray::chunks` on the listed numeric pairs,
	/// copied into owned nested option vectors that keep chunk boundaries.
	#[serde(default)]
	pub(crate) chunk_snapshots: Vec<ChunkSnapshot>,
	/// Record 0101: `ChunkedArray::downcast_get` on the listed pairs, the
	/// selected chunk copied into an owned optional vector of options.
	#[serde(default)]
	pub(crate) indexed_chunk_snapshots: Vec<IndexedChunkSnapshot>,
	/// Record 0102: `ChunkedArray::downcast_as_array` on the listed pairs,
	/// the one array copied into an owned vector of options.
	#[serde(default)]
	pub(crate) array_snapshots: Vec<ArraySnapshot>,
	/// Record 0103: `ChunkedArray::downcast_iter` on the listed pairs, the
	/// typed chunk iterator driven into owned nested option vectors.
	#[serde(default)]
	pub(crate) iter_snapshots: Vec<IterSnapshot>,
	/// Record 0104: `ChunkedArray::downcast_chunks` on the listed pairs, the
	/// indexed chunk view read into owned nested option vectors.
	#[serde(default)]
	pub(crate) view_snapshots: Vec<ViewSnapshot>,
	/// Record 0105: `ChunkedArray::downcast_into_iter` on the listed pairs,
	/// preflighted on the receiver before it is cloned and consumed.
	#[serde(default)]
	pub(crate) owned_iter_snapshots: Vec<OwnedIterSnapshot>,
	/// Record 0106: `ChunkedArray::layout` on the listed pairs, the variant
	/// reported by name with its payload copied into owned nested options.
	#[serde(default)]
	pub(crate) layout_snapshots: Vec<LayoutSnapshot>,
	/// Record 0079: mutable closure arguments with an audited read-back contract.
	#[serde(default)]
	pub(crate) callback_mutable: Vec<CallbackMutable>,
	#[serde(default)]
	pub(crate) callback_invocation: Vec<CallbackInvocation>,
	#[serde(default)]
	pub(crate) callback_sink: Vec<CallbackSink>,
	#[serde(default)]
	pub(crate) callback_safe: Vec<CallbackSafe>,
	#[serde(default)]
	pub(crate) callback_recipe: Vec<CallbackRecipe>,
}
}
#[derive(serde::Deserialize, Clone, Default)]
pub(crate) struct InstantiationScope {
	#[serde(default)]
	pub(crate) families: Vec<String>,
	/// Proven pairs the compiler refuses for a reason the inventory cannot
	/// see (a `no_call_const` in the pinned sources), excluded with a
	/// citation; each is a route exception, never a silent skip.
	#[serde(default)]
	pub(crate) exclude: Vec<InstantiationExclude>,
}
#[derive(serde::Deserialize, Clone)]
pub(crate) struct InstantiationExclude {
	pub(crate) alias: String,
	pub(crate) methods: Vec<String>,
	pub(crate) cite: String,
}
#[derive(serde::Deserialize, Clone, Default)]
pub(crate) struct ReleaseProvenance {
	#[serde(default)]
	pub(crate) release: Option<String>,
	#[serde(default)]
	pub(crate) rev: Option<String>,
	/// Record 0081: the documentation configuration (`cfg` in pins.json)
	/// and the Polars features it must have resolved. When named, an
	/// inventory documented under another configuration or without every
	/// listed feature is refused: the inventory is the coverage
	/// denominator, and a feature-only change must not be reported
	/// against an inventory that predates it.
	#[serde(default)]
	pub(crate) cfg: Option<String>,
	#[serde(default)]
	pub(crate) features: Vec<String>,
}
#[derive(serde::Deserialize, Clone)]
pub(crate) struct Unordered {
	pub(crate) path: String,
	/// The recognized configuration, one entry per parameter of the
	/// operation: the exact Rune fixture expression the case must pass, or
	/// `*` for a parameter that cannot affect row order. A case whose
	/// parameters or expressions differ stays ordered.
	#[serde(default)]
	pub(crate) args: BTreeMap<String, String>,
	pub(crate) options: String,
	pub(crate) cite: String,
}
#[derive(serde::Deserialize, Clone)]
pub(crate) struct ExcludedOracle {
	pub(crate) path: String,
	pub(crate) reason: String,
}
#[derive(serde::Deserialize, Clone)]
pub(crate) struct RefusedOperation {
	pub(crate) path: String,
	pub(crate) reason: String,
	pub(crate) cite: String,
}

impl Release {
	pub(crate) fn load(path: &Path) -> (Release, String) {
		let text = std::fs::read_to_string(path)
			.unwrap_or_else(|e| panic!("release file {}: {e}", path.display()));
		// record 0115: an unknown top-level key (a misspelled table) is refused
		// before generation instead of silently ignored
		if let Err(key) = Release::unknown_key(&text) {
			eprintln!(
				"refusing to generate: release file {}: unknown top-level key `{key}`",
				path.display()
			);
			std::process::exit(2);
		}
		let r: Release = toml::from_str(&text)
			.unwrap_or_else(|e| panic!("release file {}: {e}", path.display()));
		use sha2::Digest;
		let digest = format!("{:x}", sha2::Sha256::digest(text.as_bytes()));
		(r, digest)
	}
	/// Record 0115: the first top-level key neither `Release` nor
	/// `FamilyTables` declares; `Ok` when every key is known. A file that
	/// does not parse as TOML is left to the typed parse, which names the error.
	pub(crate) fn unknown_key(text: &str) -> Result<(), String> {
		let Ok(table) = toml::from_str::<toml::Table>(text) else {
			return Ok(());
		};
		match table.keys().find(|k| {
			!Release::KEYS.contains(&k.as_str()) && !FamilyTables::KEYS.contains(&k.as_str())
		}) {
			Some(k) => Err(k.clone()),
			None => Ok(()),
		}
	}
	/// Record 0119: a type (struct or enum path) under a listed deferred prefix.
	pub(crate) fn deferred_type(&self, path: &str) -> bool {
		self.families
			.deferred_type_prefixes
			.iter()
			.any(|d| path.starts_with(d.prefix.as_str()))
	}
	pub(crate) fn is_api(&self, krate: &str) -> bool {
		self.api_crates.iter().any(|c| c == krate)
	}
	/// The order policy for a case: unordered only when the canonical
	/// operation is listed and the case's parameters are exactly the
	/// listed ones, each passing the recorded expression (or anything,
	/// for a `*` parameter). Anything unrecognized is ordered.
	pub(crate) fn policy(
		&self,
		canonical_path: &str,
		params: &[(String, String)],
	) -> (bool, String) {
		let Some(u) = self.unordered.iter().find(|u| u.path == canonical_path) else {
			return (false, "ordered (operation not listed as unordered)".into());
		};
		let names: BTreeSet<&str> = params.iter().map(|(n, _)| n.as_str()).collect();
		let listed: BTreeSet<&str> = u.args.keys().map(|k| k.as_str()).collect();
		if names != listed {
			return (
				false,
				format!(
					"ordered (listed, but the case's parameters [{}] are not the recorded configuration's [{}])",
					names.into_iter().collect::<Vec<_>>().join(", "),
					listed.into_iter().collect::<Vec<_>>().join(", ")
				),
			);
		}
		for (n, expr) in params {
			let want = &u.args[n];
			if want != "*" && want != expr {
				return (
					false,
					format!("ordered (listed, but `{n}` is `{expr}`, not the recorded `{want}`)"),
				);
			}
		}
		(true, format!("unordered: {} [{}]", u.options, u.cite))
	}

	/// The release file must name the inventory it belongs to, and the
	/// inventory must record the same provenance.
	pub(crate) fn check_provenance(&self, inv: &Inventory) -> Result<String, String> {
		let Some(p) = &inv.provenance else {
			return Err("the inventory records no provenance (re-extract it with the record 0075 extractor)".into());
		};
		let ((Some(_), _) | (_, Some(_))) = (&self.provenance.release, &self.provenance.rev) else {
			return Err(format!(
				"release file `{}` names no provenance ([provenance] release = … or rev = …)",
				self.name
			));
		};
		if let Some(want) = &self.provenance.release {
			match &p.release {
				Some(have) if have == want => {}
				other => {
					return Err(format!(
						"release file `{}` is for release {want}; the inventory records {other:?}",
						self.name
					));
				}
			}
		}
		if let Some(want) = &self.provenance.rev {
			match &p.rev {
				Some(have) if have == want => {}
				other => {
					return Err(format!(
						"release file `{}` is for rev {want}; the inventory records {other:?}",
						self.name
					));
				}
			}
		}
		if let Some(want) = &self.provenance.cfg {
			match &p.cfg {
				Some(have) if have == want => {}
				other => {
					return Err(format!(
						"release file `{}` is for configuration {want}; the inventory records {other:?}",
						self.name
					));
				}
			}
		}
		if !self.provenance.features.is_empty() {
			// The complete resolved set, compared exactly: a missing base
			// feature or an extra one documents a different surface.
			let Some(have) = &p.features else {
				return Err(format!(
					"release file `{}` pins features {:?}; the inventory records no feature set (re-extract it with the record 0081 extractor)",
					self.name, self.provenance.features
				));
			};
			let want: std::collections::BTreeSet<&String> =
				self.provenance.features.iter().collect();
			let have: std::collections::BTreeSet<&String> = have.iter().collect();
			if want != have {
				let missing: Vec<&&String> = want.difference(&have).collect();
				let extra: Vec<&&String> = have.difference(&want).collect();
				return Err(format!(
					"release file `{}` pins the resolved feature set {:?}; the inventory's set lacks {missing:?} and adds {extra:?}",
					self.name, self.provenance.features
				));
			}
		}
		Ok(format!(
			"release {:?} rev {:?} cfg {:?} features {}",
			p.release,
			p.rev,
			p.cfg,
			p.features
				.as_ref()
				.map_or("unrecorded".to_string(), |f| format!(
					"{} resolved",
					f.len()
				))
		))
	}
}

pub(crate) fn policy_self_test() {
	let r: Release = toml::from_str(
		r#"
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
"#,
	)
	.unwrap();
	let p = |v: &[(&str, &str)]| {
		v.iter()
			.map(|(a, b)| (a.to_string(), b.to_string()))
			.collect::<Vec<_>>()
	};
	let join = "polars_lazy::frame::LazyFrame::join";
	let base = [
		("other", "fx::lf()"),
		("left_on", "[fx::expr()]"),
		("right_on", "[fx::expr()]"),
	];
	fn with<'a>(base: &[(&'a str, &'a str)], a: &'a str) -> Vec<(&'a str, &'a str)> {
		let mut v = base.to_vec();
		v.push(("args", a));
		v
	}
	assert!(
		r.policy(join, &p(&with(&base, "polars::JoinArgs::default_()")))
			.0,
		"the recorded recipe is unordered"
	);
	assert!(
		!r.policy(
			join,
			&p(&with(
				&base,
				"polars::JoinArgs::default_().with_maintain_order(polars::MaintainOrderJoin::Left())"
			))
		)
		.0,
		"an explicit order stays ordered"
	);
	assert!(!r.policy(join, &p(&with(&base, "polars::JoinArgs::default_().with_maintain_order(polars::MaintainOrderJoin::None()).with_maintain_order(polars::MaintainOrderJoin::Left())"))).0, "a chained explicit order stays ordered");
	assert!(
		!r.policy(join, &p(&with(&base, "unknown_nondefault_fixture()")))
			.0,
		"an unrecognized configuration stays ordered"
	);
	assert!(
		!r.policy(join, &p(&base)).0,
		"a case missing a recorded parameter stays ordered"
	);
	assert!(
		!r.policy(
			join,
			&p(&[
				("other", "fx::lf()"),
				("left_on", "[fx::expr()]"),
				("right_on", "[fx::expr()]"),
				("args", "polars::JoinArgs::default_()"),
				("extra", "1")
			])
		)
		.0,
		"a case with an unlisted parameter stays ordered"
	);
	assert!(
		r.policy("polars_lazy::frame::LazyFrame::inner_join", &p(&base))
			.0,
		"a listed operation with only order-irrelevant parameters is unordered"
	);
	assert!(
		!r.policy("polars_lazy::frame::LazyFrame::cross_join", &p(&base))
			.0,
		"an unlisted operation is ordered"
	);
	assert!(
		!r.policy(
			"other::LazyFrame::join",
			&p(&with(&base, "polars::JoinArgs::default_()"))
		)
		.0,
		"a same-named method elsewhere is not matched"
	);
	// provenance: the release file must belong to the inventory
	let inv = |release: Option<&str>, rev: Option<&str>| Inventory {
		callables: vec![],
		supporting: vec![],
		provenance: Some(model::Provenance {
			release: release.map(String::from),
			rev: rev.map(String::from),
			cfg: None,
			features: None,
		}),
	};
	assert!(r.check_provenance(&inv(Some("9.9.9"), None)).is_ok());
	assert!(
		r.check_provenance(&inv(Some("0.55.2"), None)).is_err(),
		"another release is refused"
	);
	assert!(
		r.check_provenance(&inv(None, Some("abc"))).is_err(),
		"a Git inventory is refused by a release-pinned file"
	);
	assert!(
		r.check_provenance(&Inventory {
			callables: vec![],
			supporting: vec![],
			provenance: None
		})
		.is_err(),
		"no provenance is refused"
	);
	let mut g = r.clone();
	g.provenance = ReleaseProvenance {
		release: None,
		rev: Some("abc".into()),
		cfg: None,
		features: vec![],
	};
	assert!(g.check_provenance(&inv(None, Some("abc"))).is_ok());
	assert!(
		g.check_provenance(&inv(None, Some("abd"))).is_err(),
		"another revision is refused"
	);
	let mut n = r.clone();
	n.provenance = ReleaseProvenance::default();
	assert!(
		n.check_provenance(&inv(Some("9.9.9"), None)).is_err(),
		"a release file without provenance is refused"
	);
	println!("policy self-test: ok");
}

#[cfg(test)]
mod tests {
	#[test]
	fn policy() {
		super::policy_self_test();
	}

	/// Record 0115: a misspelled family table is refused by name; the same
	/// file with the declared name loads; every shipped release file has
	/// only declared keys; and the accepted keys are exactly the fields.
	#[test]
	fn unknown_release_keys() {
		use super::{FamilyTables, Release};
		let head = "name = \"t\"\nsource = \"t\"\napi_crates = []\n";
		let entry = "key = \"k\"\npath = \"p\"\npairs = []\ncite = \"c\"\n";
		let typo = format!("{head}[[array_snapshot]]\n{entry}");
		assert_eq!(
			Release::unknown_key(&typo),
			Err("array_snapshot".to_string())
		);
		let valid = format!("{head}[[array_snapshots]]\n{entry}");
		assert_eq!(Release::unknown_key(&valid), Ok(()));
		let r: Release = toml::from_str(&valid).unwrap();
		assert_eq!(
			r.families.array_snapshots.len(),
			1,
			"the flattened table is read"
		);
		for f in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/releases")).unwrap() {
			let text = std::fs::read_to_string(f.unwrap().path()).unwrap();
			assert_eq!(Release::unknown_key(&text), Ok(()));
		}
		assert!(
			FamilyTables::KEYS.contains(&"layout_snapshots") && Release::KEYS.contains(&"refused")
		);
		assert!(
			!Release::KEYS.contains(&"families"),
			"the flattened field is not a file key"
		);
		let all: std::collections::BTreeSet<&str> = Release::KEYS
			.iter()
			.chain(FamilyTables::KEYS)
			.copied()
			.collect();
		assert_eq!(
			all.len(),
			Release::KEYS.len() + FamilyTables::KEYS.len(),
			"no key declared twice"
		);
	}
}

/// Record 0119: a deferred type prefix, with why and where it goes.
#[derive(serde::Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeferredPrefix {
	pub(crate) prefix: String,
	pub(crate) cite: String,
}
