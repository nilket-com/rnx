pub(crate) mod emit;
pub(crate) mod fixtures;
use crate::emit::{Entry, OracleInfo, rune_path};
use crate::families::{
	Active, ORACLE_REF, ORACLE_SCALAR, ORACLE_TOP, OracleSite, SCRIPT_TUPLE, first,
};
use crate::oracle::fixtures::{FIXTURES, Recipe, TYPED_FIXTURES};
use crate::text::sanitize;
use crate::ty;
use crate::ty::{Bound, Ty, last};
use crate::world::mapping::TZ;
use crate::world::mapping::{INT_NARROW, RISKY_INTS, iterator_return};
use crate::world::{World, rune_name};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct Oracle<'a> {
	pub(crate) world: &'a World,
	pub(crate) debuggable: BTreeSet<String>,
	pub(crate) defaultable: BTreeSet<String>,
	/// Types whose `default_()` binding was generated: the Rune side of a
	/// `Default` fixture needs the binding, not only the Rust impl.
	pub(crate) default_bound: BTreeSet<String>,
	/// Wrapper types the tests show through an in-crate helper (canonical paths).
	pub(crate) shown: std::cell::RefCell<BTreeSet<String>>,
	/// Derived fixture recipes by canonical type (record 0075 gate 2).
	pub(crate) recipes: BTreeMap<String, Recipe>,
	/// Types that got no recipe, with the reason.
	pub(crate) no_recipe: BTreeMap<String, String>,
	/// Record 0086: the length of the Rust mask fixture for the argument
	/// being paired (3 for a receiver-length mask, 1 for a values-length one).
	pub(crate) mask_len: std::cell::Cell<usize>,
	/// Record 0115: the listed families' oracle state for the current case.
	pub(crate) active: Active,
}

impl<'a> Oracle<'a> {
	pub(crate) fn fixture(
		&self,
		canonical: &str,
	) -> Option<&'static (&'static str, &'static str, &'static str, &'static str)> {
		FIXTURES.iter().find(|f| f.0 == canonical)
	}

	/// Rune expression producing a value of this shape, if fixtures allow.
	pub(crate) fn rune_value(&self, shape: &str) -> Option<String> {
		match shape {
			"bool" => Some("true".into()),
			"int" => Some("2".into()),
			"float" => Some("1.5".into()),
			"string" => Some("\"x\"".into()),
			"tz" => Some("\"UTC\"".into()),
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
				if let Some(inner) = shape
					.strip_prefix("tuple(")
					.and_then(|s| s.strip_suffix(')'))
				{
					let parts: Option<Vec<String>> =
						inner.split(';').map(|p| self.rune_value(p)).collect();
					return parts.map(|p| format!("({})", p.join(", ")));
				}
				None
			}
		}
	}

	/// The disposition note for a case: the derived recipes whose Rune
	/// fixture text the case's script actually contains (receiver or any
	/// argument, including one reached through a generic parameter).
	pub(crate) fn recipe_note(&self, case_path: &str, texts: &[&str]) -> String {
		let mut used: Vec<String> = Vec::new();
		for (c, r) in &self.recipes {
			// a constructor's own case is not "via" itself
			let own = r
				.kind
				.split(' ')
				.nth(1)
				.map(|n| format!("{c}::{n}"))
				.as_deref() == Some(case_path);
			if !own && texts.iter().any(|t| t.contains(r.rune.as_str())) && !used.contains(&r.kind)
			{
				used.push(r.kind.clone());
			}
		}
		used.sort();
		if used.is_empty() {
			String::new()
		} else {
			format!(" (fixture via {})", used.join(", "))
		}
	}

	pub(crate) fn rune_wrapped(&self, canonical: &str) -> Option<String> {
		self.base_rune(canonical)
			.or_else(|| self.recipes.get(canonical).map(|r| r.rune.clone()))
	}

	/// The rules of record 0073: a core fixture, `Default`, or a unit variant.
	pub(crate) fn base_rune(&self, canonical: &str) -> Option<String> {
		if let Some(f) = self.fixture(canonical) {
			return Some(format!("fx::{}()", f.1));
		}
		let w = self.world.wrappers.get(canonical)?;
		let s = self.world.types.get(canonical)?;
		if s.kind == "enum" {
			let v = s.variant_shapes.iter().find(|(_, data)| !*data)?;
			return Some(format!(
				"{}::{}()",
				rune_path(w),
				rune_name(&sanitize(&v.0))
			));
		}
		if self.defaultable.contains(canonical) && self.default_bound.contains(canonical) {
			return Some(format!("{}::default_()", rune_path(w)));
		}
		None
	}

	pub(crate) fn base_rust(&self, canonical: &str) -> Option<String> {
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
	pub(crate) fn derive_recipes(&mut self, entries: &[Entry]) {
		// constructor candidates per owner: receiver none, returns Self or PolarsResult<Self>
		let mut ctors: BTreeMap<String, Vec<(String, OracleInfo, bool)>> = BTreeMap::new();
		// record 0078: a `From` conversion ranks after every inherent
		// constructor; its argument is a placeholder fixture (`Scalar::default()`
		// is a null scalar), where an inherent constructor's literal arguments
		// give the value the receiver's methods can act on
		let mut conversions: BTreeSet<(String, String)> = BTreeSet::new();
		for e in entries {
			if e.kind == "foreign_trait_impl" {
				if let Some(info) = &e.oracle {
					if let Some((owner, _)) = &info.owner {
						conversions.insert((owner.clone(), info.rune_name.clone()));
					}
				}
			}
		}
		for e in entries {
			let Some(info) = &e.oracle else { continue };
			let Some((owner, _)) = &info.owner else {
				continue;
			};
			if info.receiver != "none" || e.status != "generated" {
				continue;
			}
			let ret = info.ret_canonical.as_deref().unwrap_or("");
			let (target, fallible) = match ty::parse(ret) {
				Ty::Path { path, args }
					if (path == "polars_error::PolarsResult" || path == "core::result::Result")
						&& !args.is_empty() =>
				{
					(args[0].clone(), true)
				}
				other => (other, false),
			};
			let returns_self = match &target {
				Ty::Path { path, args } => args.is_empty() && (path == "Self" || path == owner),
				Ty::Generic(g) => g == "Self",
				_ => false,
			};
			if !returns_self {
				continue;
			}
			ctors.entry(owner.clone()).or_default().push((
				info.rune_name.clone(),
				info.clone(),
				fallible,
			));
		}
		for (owner, v) in ctors.iter_mut() {
			v.sort_by_key(|(n, _, _)| {
				(
					if conversions.contains(&(owner.clone(), n.clone())) {
						3
					} else if n == "new" {
						0
					} else if n.starts_with("from_") {
						1
					} else {
						2
					},
					n.clone(),
				)
			});
		}
		loop {
			let mut progress = false;
			let mut wanted: Vec<String> = self
				.world
				.wrappers
				.keys()
				.filter(|c| self.rune_wrapped(c).is_none())
				.cloned()
				.collect();
			wanted.sort();
			for canonical in wanted {
				let Some(w) = self.world.wrappers.get(&canonical) else {
					continue;
				};
				let Some(s) = self.world.types.get(&canonical) else {
					continue;
				};
				// rule 5a (record 0076): a producer on a typed source fixture
				// that names it comes first: it is a deliberately supplied
				// value, where a constructor's placeholder arguments may not be
				// valid for the type (`rand_bernoulli("x", 2, 1.5)`)
				let mut found: Option<Recipe> = self.producer_recipe(&canonical, entries, true);
				// rule 3: a constructor binding whose arguments all have fixtures
				for (name, info, fallible) in ctors.get(&canonical).cloned().unwrap_or_default() {
					if found.is_some() {
						break;
					}
					let mut rune_args = Vec::new();
					let mut rust_args = Vec::new();
					let mut ok = true;
					for (shape, ct) in &info.params {
						match (
							self.rune_value(shape),
							self.rust_value(&ty::parse(ct), &info.generics, Some(&canonical), 0),
						) {
							(Some(a), Some(b)) => {
								rune_args.push(a);
								rust_args.push(b);
							}
							_ => {
								ok = false;
								break;
							}
						}
					}
					if !ok {
						continue;
					}
					let rune_call = format!("{}::{}({})", rune_path(w), name, rune_args.join(", "));
					let rust_call = format!("{}({})", info.callee, rust_args.join(", "));
					// the Rune binding is fallible when the Rust return is a
					// Result or an argument conversion can fail; each side is
					// unwrapped where it is fallible, with a `fixture:` panic
					let rune = if info.fallible {
						format!(
							"match {rune_call} {{ Ok(v) => v, Err(e) => panic(`fixture: {} failed: ${{e}}`) }}",
							name
						)
					} else {
						rune_call
					};
					let rust = if fallible {
						format!(
							"match {rust_call} {{ Ok(v) => v, Err(e) => panic!(\"fixture: {} failed: {{e}}\") }}",
							name
						)
					} else {
						rust_call
					};
					found = Some(Recipe {
						rune,
						rust,
						kind: format!(
							"constructor {}{}",
							name,
							if fallible || info.fallible {
								" (fallible)"
							} else {
								""
							}
						),
					});
					break;
				}
				// rule 4: the first data-carrying variant whose payload has fixtures
				if found.is_none() && s.kind == "enum" {
					for (vname, payload) in &s.variant_payloads {
						if payload.iter().any(|x| x.contains(": ")) {
							continue;
						} // struct variants: not this stage
						let mut rune_args = Vec::new();
						let mut rust_args = Vec::new();
						let mut ok = true;
						for pt in payload {
							let t = ty::parse(pt);
							let shape = match &t {
								Ty::Path { path, args }
									if args.is_empty()
										&& self.world.wrappers.contains_key(path) =>
								{
									format!("W:{path}")
								}
								Ty::Path { path, .. } if path == "bool" => "bool".into(),
								Ty::Path { path, .. }
									if path == "i64" || INT_NARROW.contains(&path.as_str()) =>
								{
									"int".into()
								}
								Ty::Path { path, .. } if path == "f64" || path == "f32" => {
									"float".into()
								}
								Ty::Path { path, .. }
									if path == "alloc::string::String"
										|| path == "polars_utils::pl_str::PlSmallStr" =>
								{
									"string".into()
								}
								_ => String::new(),
							};
							match (
								self.rune_value(&shape),
								self.rust_value(&t, &BTreeMap::new(), Some(&canonical), 0),
							) {
								(Some(a), Some(b)) => {
									rune_args.push(a);
									rust_args.push(b);
								}
								_ => {
									ok = false;
									break;
								}
							}
						}
						if !ok {
							continue;
						}
						found = Some(Recipe {
							rune: format!(
								"{}::{}({})",
								rune_path(w),
								rune_name(&sanitize(vname)),
								rune_args.join(", ")
							),
							rust: format!("<{}>::{}({})", w.spell, vname, rust_args.join(", ")),
							kind: format!("variant {vname}"),
						});
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
			if !progress {
				break;
			}
		}
		for (canonical, w) in &self.world.wrappers {
			if self.rune_wrapped(canonical).is_none() {
				let s = &self.world.types[canonical];
				let why = if s.kind == "enum" {
					"no unit variant and no variant whose payload has fixtures"
				} else if ctors.contains_key(canonical) {
					"constructors exist but none has fixtures for all arguments"
				} else if self.defaultable.contains(canonical)
					&& !self.default_bound.contains(canonical)
				{
					"Default impl without a generated default_ binding (the impl is on the generic base), no constructor binding"
				} else if s.public_fields > 0 {
					"public fields, no Default, no constructor binding"
				} else {
					"no public fields, no Default, no constructor binding"
				};
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
	pub(crate) fn producer_recipe(
		&self,
		canonical: &str,
		entries: &[Entry],
		typed_only: bool,
	) -> Option<Recipe> {
		let mut cands: Vec<(&Entry, &OracleInfo, bool, bool)> = Vec::new();
		for e in entries {
			if e.status != "generated" {
				continue;
			}
			let Some(info) = &e.oracle else { continue };
			let Some((owner, _)) = &info.owner else {
				continue;
			};
			// record 0109: a consuming producer on a Clone owner (`Expr::str`)
			// projects a fixture too; the Rune source is cloned, Rust takes it by value
			let consuming = info.receiver == "self" && self.world.clonable.contains(owner);
			if !(info.receiver == "&self" || consuming) || !info.params.is_empty() {
				continue;
			}
			if owner == canonical {
				continue;
			}
			let ret = info.ret_canonical.as_deref().unwrap_or("");
			let (target, fallible) = match ty::parse(ret) {
				Ty::Path { path, args }
					if (path == "polars_error::PolarsResult" || path == "core::result::Result")
						&& !args.is_empty() =>
				{
					(args[0].clone(), true)
				}
				other => (other, false),
			};
			let (target, by_ref) = match target {
				Ty::Ref {
					inner,
					mutable: false,
				} => (*inner, true),
				other => (other, false),
			};
			let hits =
				matches!(&target, Ty::Path { path, args } if args.is_empty() && path == canonical);
			if !hits {
				continue;
			}
			cands.push((e, info, fallible, by_ref));
		}
		cands.sort_by_key(|(e, _, _, _)| (e.canonical_path.len(), e.canonical_path.clone()));
		for (e, info, fallible, by_ref) in cands {
			let (owner, _) = info.owner.as_ref().unwrap();
			let typed = TYPED_FIXTURES
				.iter()
				.find(|f| f.0 == owner && f.4.iter().any(|n| *n == info.rune_name));
			if typed_only && typed.is_none() {
				continue;
			}
			let (recv_rune, recv_rust) = match typed {
				Some(f) => (format!("fx::{}()", f.1), format!("{}()", f.1)),
				None => match (self.rune_wrapped(owner), self.rust_wrapped(owner)) {
					(Some(a), Some(b)) => (a, b),
					_ => continue,
				},
			};
			let rune_call = format!("{recv_rune}.{}()", info.rune_name);
			let rune = if info.fallible {
				format!(
					"match {rune_call} {{ Ok(v) => v, Err(e) => panic(`fixture: {} failed: ${{e}}`) }}",
					info.rune_name
				)
			} else {
				rune_call
			};
			let rust_call = format!(
				"{}({}{recv_rust})",
				info.callee,
				if info.receiver == "self" { "" } else { "&" }
			);
			let rust_call = if fallible {
				format!(
					"match {rust_call} {{ Ok(v) => v, Err(e) => panic!(\"fixture: {} failed: {{e}}\") }}",
					info.rune_name
				)
			} else {
				rust_call
			};
			let rust = if by_ref {
				format!("{rust_call}.clone()")
			} else {
				rust_call
			};
			let via = match typed {
				Some(f) => format!(" on {}", f.1),
				None => String::new(),
			};
			return Some(Recipe {
				rune,
				rust,
				kind: format!(
					"producer {}{via}",
					e.canonical_path
						.rsplit("::")
						.take(2)
						.collect::<Vec<_>>()
						.into_iter()
						.rev()
						.collect::<Vec<_>>()
						.join("::")
				),
			});
		}
		None
	}

	/// Rust expression producing the Polars value for a canonical parameter
	/// type. References are leaked so they outlive any call and container:
	/// this is test fixture code.
	pub(crate) fn rust_value(
		&self,
		t: &Ty,
		generics: &BTreeMap<String, String>,
		owner: Option<&str>,
		depth: u8,
	) -> Option<String> {
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
				// record 0116 (rule 6): the time zone the script's "UTC" parses to
				TZ => Some("chrono_tz::Tz::UTC".into()),
				"polars_arrow::bitmap::immutable::Bitmap" => Some(match self.mask_len.get() {
					1 => "polars_arrow::bitmap::Bitmap::from([false])".into(),
					2 => "polars_arrow::bitmap::Bitmap::from([true, false])".into(),
					_ => "polars_arrow::bitmap::Bitmap::from([true, false, true])".into(),
				}),
				"core::option::Option" if args.len() == 1 => self
					.rust_value(&args[0], generics, owner, depth + 1)
					.map(|v| format!("Some({v})")),
				"alloc::vec::Vec" if args.len() == 1 => self
					.rust_value(&args[0], generics, owner, depth + 1)
					.map(|v| format!("vec![{v}]")),
				"Self" => self.rust_value(
					&Ty::Path {
						path: owner?.to_string(),
						args: vec![],
					},
					generics,
					owner,
					depth + 1,
				),
				_ => self.rust_wrapped(path),
			},
			Ty::Ref { mutable, inner } => match &**inner {
				Ty::Path { path, .. } if path == "str" => Some("\"x\"".into()),
				Ty::Slice(e) => self.rust_value(e, generics, owner, depth + 1).map(|v| {
					if *mutable {
						format!("vec![{v}].leak()")
					} else {
						format!("&*vec![{v}].leak()")
					}
				}),
				other => {
					let v = self.rust_value(other, generics, owner, depth + 1)?;
					Some(if *mutable {
						format!("Box::leak(Box::new({v}))")
					} else {
						format!("&*Box::leak(Box::new({v}))")
					})
				}
			},
			Ty::Tuple(ts) => {
				let parts: Option<Vec<String>> = ts
					.iter()
					.map(|e| self.rust_value(e, generics, owner, depth + 1))
					.collect();
				parts.map(|p| format!("({})", p.join(", ")))
			}
			Ty::Impl(bounds) => self.rust_bounds(bounds, generics, owner, depth),
			Ty::Generic(g) => {
				if g == "Self" {
					return self.rust_value(
						&Ty::Path {
							path: "Self".into(),
							args: vec![],
						},
						generics,
						owner,
						depth + 1,
					);
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

	pub(crate) fn rust_bounds(
		&self,
		bounds: &[Bound],
		generics: &BTreeMap<String, String>,
		owner: Option<&str>,
		depth: u8,
	) -> Option<String> {
		for b in bounds {
			let l = last(&b.path);
			match l {
				"Into" | "Borrow" if b.args.len() == 1 => {
					return self.rust_value(&b.args[0], generics, owner, depth + 1);
				}
				"AsRef" if b.args.len() == 1 => {
					return match &b.args[0] {
						Ty::Path { path, .. } if path == "str" => Some("\"x\"".into()),
						Ty::Slice(e) => self
							.rust_value(e, generics, owner, depth + 1)
							.map(|v| format!("vec![{v}]")),
						other => self.rust_value(other, generics, owner, depth + 1),
					};
				}
				"IntoVec" if b.args.len() == 1 => {
					return self
						.rust_value(&b.args[0], generics, owner, depth + 1)
						.map(|v| format!("vec![{v}]"));
				}
				"IntoIterator" => {
					return self
						.rust_value(b.item.as_ref()?, generics, owner, depth + 1)
						.map(|v| format!("vec![{v}]"));
				}
				_ => {}
			}
		}
		None
	}

	pub(crate) fn rust_wrapped(&self, canonical: &str) -> Option<String> {
		self.base_rust(canonical)
			.or_else(|| self.recipes.get(canonical).map(|r| r.rust.clone()))
	}

	/// How to show a wrapped value of this type, as an expression over `v: &T`.
	pub(crate) fn show(&self, canonical: &str) -> Option<String> {
		if let Some(f) = self.fixture(canonical) {
			return Some(f.3.to_string());
		}
		// record 0076: an array alias is compared structurally, as the series
		// it converts to (name, dtype, every element, nulls), never by Debug,
		// whose formatting truncates and rounds
		if let Some(w) = self.world.wrappers.get(canonical) {
			if w.rule == "alias"
				&& w.base.as_deref().is_some_and(|b| {
					b == "polars_core::chunked_array::ChunkedArray"
						|| b == "polars_core::chunked_array::logical::Logical"
				}) {
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
	pub(crate) fn top_wrapped(&self, t: &Ty, owner: Option<&str>) -> Option<(String, bool)> {
		let (inner, fallible) = match t {
			Ty::Path { path, args }
				if (path == "polars_error::PolarsResult" || path == "core::result::Result")
					&& !args.is_empty() =>
			{
				(&args[0], true)
			}
			other => (other, false),
		};
		let path = match inner {
			Ty::Path { path, args } if args.is_empty() => {
				if path == "Self" {
					owner?.to_string()
				} else {
					path.clone()
				}
			}
			Ty::Generic(g) if g == "Self" => owner?.to_string(),
			_ => return None,
		};
		if self.world.wrappers.contains_key(&path) && self.show(&path).is_some() {
			Some((path, fallible))
		} else {
			None
		}
	}

	/// Rust: format the Polars value `__r` of canonical type `t` as a `Side`.
	/// A nested result that fails is `<<ERR:kind>>`, collapsed by the caller.
	/// Record 0115: the first active listed family's oracle framing at `site`.
	pub(crate) fn family_oracle(
		&self,
		site: OracleSite,
		t: &Ty,
		owner: Option<&str>,
		depth: u8,
	) -> Option<Option<String>> {
		let list = match site {
			OracleSite::Top => ORACLE_TOP,
			OracleSite::Ref => ORACLE_REF,
			OracleSite::Scalar => ORACLE_SCALAR,
		};
		first(&self.active, list, "oracle", |f, s| {
			f.oracle_fmt(self, s, site, t, owner, depth)
		})
	}

	/// Record 0115: the first active listed family's script framing.
	pub(crate) fn family_script(
		&self,
		r: &str,
		owner: Option<&str>,
		depth: u8,
	) -> Option<Option<String>> {
		first(&self.active, SCRIPT_TUPLE, "script", |f, s| {
			f.script_fmt(self, s, r, owner, depth)
		})
	}

	pub(crate) fn oracle_fmt(&self, t: &Ty, owner: Option<&str>, depth: u8) -> Option<String> {
		if depth > 6 {
			return None;
		}
		// record 0115: the listed families' result framing (`ORACLE_TOP`), then
		// the chunk list (`ORACLE_REF`): of the `match t` arms before it, the
		// tuple arms and the `&str` reference arm cannot match a chunk list
		if let Some(x) = self.family_oracle(OracleSite::Top, t, owner, depth) {
			return x;
		}
		if let Some(x) = self.family_oracle(OracleSite::Ref, t, owner, depth) {
			return x;
		}
		match t {
            Ty::Tuple(ts) if ts.is_empty() => Some("\"()\".to_string()".into()),
            Ty::Tuple(ts) => {
                let parts: Option<Vec<String>> = ts.iter().enumerate().map(|(i, e)| self.oracle_fmt(e, owner, depth + 1).map(|f| format!("{{ let __r = __t.{i}; {f} }}"))).collect();
                parts.map(|p| format!("{{ let __t = __r; format!(\"({{}})\", [{}].iter().map(|e: &String| format!(\"{{}}:{{e}}\", e.len())).collect::<Vec<_>>().join(\", \")) }}", p.join(", ")))
            }
            Ty::Ref { inner, .. } if matches!(&**inner, Ty::Path { path, .. } if path == "str") => self.oracle_fmt(inner, owner, depth + 1),
            Ty::Ref { inner, .. } => self.oracle_fmt(inner, owner, depth + 1).map(|f| format!("{{ let __r = (__r).clone(); {f} }}")),
            // record 0082: a borrowed slice frames its elements like a vector
            Ty::Slice(elem) => self.oracle_fmt(elem, owner, depth + 1).map(|f| format!("format!(\"[{{}}]\", __r.iter().map(|__r| {{ let __r = __r.clone(); let e: String = {f}; format!(\"{{}}:{{e}}\", e.len()) }}).collect::<Vec<_>>().join(\", \"))")),
            // record 0088: a `Cow` result frames as its owned value
            Ty::Path { path, args } if path == "alloc::borrow::Cow" && args.len() == 1 => self.oracle_fmt(&args[0], owner, depth + 1).map(|f| format!("{{ let __r = __r.into_owned(); {f} }}")),
            Ty::Path { .. } if self.world.iter_return_items.contains_key(&t.render()) => {
                // record 0087: a listed concrete iterator frames its items like a vector
                Some("format!(\"[{}]\", __r.map(|__r| { let e = format!(\"{}\", __r); format!(\"{}:{e}\", e.len()) }).collect::<Vec<_>>().join(\", \"))".into())
            }
            Ty::Path { path, args } => {
                // record 0115: a listed hash return (`ORACLE_SCALAR`); the literal
                // arms before it name other types
                if let Some(x) = self.family_oracle(OracleSite::Scalar, t, owner, depth) {
                    return x;
                }
                match path.as_str() {
                // record 0085: a validity bitmap frames its bits like a vector of bool
                "polars_arrow::bitmap::immutable::Bitmap" => Some("format!(\"[{}]\", __r.iter().map(|b| { let e = format!(\"{}\", b); format!(\"{}:{e}\", e.len()) }).collect::<Vec<_>>().join(\", \"))".into()),
                "bool" | "i64" | "f64" => Some("format!(\"{}\", __r)".into()),
                "f32" => Some("format!(\"{}\", __r as f64)".into()),
                TZ => Some("format!(\"{}\", __r.name())".into()),
                // record 0093: a risky integer that does not fit a script integer is
                // the adapter's ConversionError, so a wrapping binding mismatches
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
	pub(crate) fn script_fmt(
		&self,
		ret_rust: &str,
		ret_canonical: Option<&Ty>,
		owner: Option<&str>,
		depth: u8,
	) -> Option<String> {
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
                // record 0106: a listed layout's (tag, chunks) pair, framed as the
                // Rust side's tuple formatter frames it; other tuples stay uncompared
                // record 0115: a listed layout's tuple (`SCRIPT_TUPLE`)
                if let Some(x) = self.family_script(r, owner, depth) {
                    return x;
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

pub(crate) struct OracleCase {
	pub(crate) id: String,
	pub(crate) path: String,
	pub(crate) script: String,
	pub(crate) has_receiver: bool,
	/// Rows compared as a set: only for a listed operation under its listed options.
	pub(crate) unordered: bool,
	/// The recorded justification, or why the case is ordered.
	pub(crate) policy: String,
	/// Rust: turn the script's first (and second) value into a `Side`.
	pub(crate) fmt: String,
	/// Rust: run the Polars side and produce a `Side`.
	pub(crate) oracle: String,
}

/// The script's setup stage: every fixture the measured call needs, and
/// nothing else, returned as a vector that `main(__fx)` receives; the
/// measured call uses those prepared values and constructs nothing.
pub(crate) fn setup_fn(exprs: &[String]) -> String {
	format!("pub fn setup() {{ [{}] }}", exprs.join(", "))
}

/// The Rust oracle with its own staged setup: the named fixtures are built
/// under `catch_unwind` and a failure there is `Staged::SetupFailed`; only
/// then does the body run with the prepared values.
pub(crate) fn staged(fixtures: &[(String, String)], body: &str) -> String {
	let names: Vec<&str> = fixtures.iter().map(|(n, _)| n.as_str()).collect();
	let exprs: Vec<&str> = fixtures.iter().map(|(_, e)| e.as_str()).collect();
	let pat = if names.is_empty() {
		"()".to_string()
	} else {
		format!("({},)", names.join(", "))
	};
	let tup = if exprs.is_empty() {
		"()".to_string()
	} else {
		format!("({},)", exprs.join(", "))
	};
	format!(
		"{{ let {pat} = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {{ {tup} }})) {{ Ok(v) => v, Err(e) => return crate_oracle::Staged::SetupFailed(crate_oracle::panic_text(e)) }}; crate_oracle::Staged::Ran({body}) }}"
	)
}
