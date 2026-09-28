use crate::families::snapshots::{CHUNKS_VIEW, LAYOUT_ENUM, SCALAR_CHUNKS, indexed_array};
use crate::text::sanitize;
use crate::ty;
use crate::ty::{Bound, Ty, last};
use crate::world::{World, Wrapper};
use std::collections::BTreeMap;

/// Record 0077 gate 1 control: the recognizer accepts each bound spelling
/// the inventory shows, tells known from unknown length, sees the wrapper,
/// and rejects an `impl Trait` that is not an iterator.
pub(crate) fn iterator_self_test() {
	let known = |t: &str| {
		iterator_return(&ty::parse(t)).map(|(item, known, wrap)| (item.render(), known, wrap))
	};
	assert_eq!(
		known(
			"impl '_ + core::marker::Send + core::marker::Sync + core::iter::traits::exact_size::ExactSizeIterator<Item = i64>"
		),
		Some(("i64".to_string(), IterLen::Exact, IterWrap::Plain))
	);
	assert_eq!(
		known(
			"impl polars_core::chunked_array::iterator::PolarsIterator<Item = core::option::Option<&[u8]>>"
		),
		Some((
			"core::option::Option<&[u8]>".to_string(),
			IterLen::Exact,
			IterWrap::Plain
		))
	);
	assert_eq!(
		known("impl core::iter::traits::double_ended::DoubleEndedIterator<Item = &[f64]>"),
		Some(("&[f64]".to_string(), IterLen::Unknown, IterWrap::Plain))
	);
	assert_eq!(
		known(
			"core::option::Option<impl core::iter::traits::double_ended::DoubleEndedIterator<Item = &[&[u8]]>>"
		),
		Some(("&[&[u8]]".to_string(), IterLen::Unknown, IterWrap::Option))
	);
	assert_eq!(
		known(
			"polars_error::PolarsResult<impl core::iter::traits::iterator::Iterator<Item = polars_core::series::Series>>"
		),
		Some((
			"polars_core::series::Series".to_string(),
			IterLen::Unknown,
			IterWrap::Result
		))
	);
	assert_eq!(
		known("impl polars_arrow::trusted_len::TrustedLen<Item = usize>"),
		Some(("usize".to_string(), IterLen::Trusted, IterWrap::Plain))
	);
	assert_eq!(
		known(
			"impl polars_arrow::trusted_len::TrustedLen<Item = usize> + core::iter::traits::exact_size::ExactSizeIterator"
		),
		Some(("usize".to_string(), IterLen::Exact, IterWrap::Plain)),
		"an exact bound beside TrustedLen wins"
	);
	assert_eq!(
		known("impl core::fmt::Display"),
		None,
		"a non-iterator impl return is not an iterator"
	);
	assert_eq!(
		known("impl core::iter::traits::iterator::Iterator"),
		None,
		"an iterator without an Item is not mapped"
	);
	println!("iterator self-test: ok");
}

pub(crate) const SCALARS: &[&str] = &[
	"bool", "i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128", "usize", "isize",
	"f32", "f64", "char", "str",
];

// ---------------------------------------------------------------- mapping

/// How one argument crosses from Rune into Rust.
pub(crate) struct Arg {
	/// Parameter type of the generated wrapper function.
	pub(crate) rust_ty: String,
	/// Expression converting the parameter `name` to the Polars argument.
	/// `?` inside means the wrapper is fallible.
	pub(crate) conv: String,
	pub(crate) fallible: bool,
	/// What the catalogue says the script passes.
	pub(crate) doc: String,
	/// Statements to run before the call (owned temporaries a borrow needs).
	pub(crate) pre: Vec<String>,
	/// 0 by value, 1 a reference, 2 a slice.
	pub(crate) borrow: u8,
	/// For a borrow: the expression producing the owned value it refers to.
	pub(crate) owned: Option<String>,
	/// What the script passes, for the oracle generator: `bool`, `int`,
	/// `float`, `string`, `W:<canonical>`, `opt(..)`, `vec(..)`, `tuple(..;..)`, `unit`.
	pub(crate) shape: String,
}

pub(crate) struct Ret {
	pub(crate) rust_ty: String,
	/// Expression converting `__r` to the wrapper's return.
	pub(crate) conv: String,
	pub(crate) fallible: bool,
	pub(crate) doc: String,
	/// Record 0077: the return is an iterator, materialized inside the
	/// call (inside the routed closure when routed) under the bound.
	pub(crate) materialize: Option<Materialize>,
}

/// How an iterator return is materialized: the element conversion (over
/// `__r`, the element by value), whether the length is known exactly
/// (`ExactSizeIterator`, `PolarsIterator`, `TrustedLen`), and the wrapper
/// around the iterator (`Option`, `PolarsResult`, or none).
#[derive(Clone)]
pub(crate) struct Materialize {
	pub(crate) elem_conv: String,
	pub(crate) known: IterLen,
	pub(crate) wrap: IterWrap,
}
/// How an iterator's length is known: exactly through `len()`
/// (`ExactSizeIterator`, `PolarsIterator`), through the `TrustedLen`
/// contract's `size_hint` upper bound, or not at all.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum IterLen {
	Exact,
	Trusted,
	Unknown,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum IterWrap {
	Plain,
	Option,
	Result,
}

/// The iterator traits a return may name, with how each fixes the
/// length. `TrustedLen` is not `ExactSizeIterator` (Polars 0.55.2 defines
/// `pub unsafe trait TrustedLen: Iterator {}`), so it gets its own route.
pub(crate) const ITERATOR_TRAITS: &[(&str, IterLen)] = &[
	("core::iter::traits::iterator::Iterator", IterLen::Unknown),
	(
		"core::iter::traits::double_ended::DoubleEndedIterator",
		IterLen::Unknown,
	),
	(
		"core::iter::traits::exact_size::ExactSizeIterator",
		IterLen::Exact,
	),
	(
		"polars_core::chunked_array::iterator::PolarsIterator",
		IterLen::Exact,
	),
	("polars_arrow::trusted_len::TrustedLen", IterLen::Trusted),
];

/// Recognize an iterator return: `impl` bounds naming an iterator trait
/// with an `Item`, possibly inside `Option` or `PolarsResult`.
pub(crate) fn iterator_return(t: &Ty) -> Option<(Ty, IterLen, IterWrap)> {
	let (inner, wrap) = match t {
		Ty::Path { path, args } if path == "core::option::Option" && args.len() == 1 => {
			(&args[0], IterWrap::Option)
		}
		Ty::Path { path, args }
			if (path == "polars_error::PolarsResult" || path == "core::result::Result")
				&& !args.is_empty() =>
		{
			(&args[0], IterWrap::Result)
		}
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
			known = match (known, *len) {
				(IterLen::Exact, _) | (_, IterLen::Exact) => IterLen::Exact,
				(IterLen::Trusted, _) | (_, IterLen::Trusted) => IterLen::Trusted,
				_ => IterLen::Unknown,
			};
			if let Some(i) = &b.item {
				item = Some((**i).clone());
			}
		}
	}
	if !is_iter {
		return None;
	}
	Some((item?, known, wrap))
}

#[derive(Debug)]
pub(crate) struct Unsupported(pub(crate) &'static str, pub(crate) String);

pub(crate) fn ok_arg(rust_ty: &str, conv: String, doc: &str) -> Result<Arg, Unsupported> {
	let shape = match rust_ty {
		"bool" => "bool".to_string(),
		"i64" => "int".to_string(),
		"f64" => "float".to_string(),
		"String" | "&str" => "string".to_string(),
		"()" => "unit".to_string(),
		_ => String::new(),
	};
	Ok(Arg {
		rust_ty: rust_ty.to_string(),
		fallible: conv.contains('?'),
		conv,
		doc: doc.to_string(),
		pre: vec![],
		borrow: 0,
		owned: None,
		shape,
	})
}

pub(crate) fn shaped(mut a: Arg, shape: String) -> Arg {
	a.shape = shape;
	a
}

pub(crate) fn borrowed(mut a: Arg, borrow: u8, owned: Option<String>) -> Arg {
	a.borrow = borrow;
	a.owned = owned;
	a
}

pub(crate) const INT_NARROW: &[&str] = &[
	"i8", "i16", "i32", "u8", "u16", "u32", "u64", "usize", "isize", "i128", "u128",
];
/// Record 0093: integer source types whose range can exceed `i64` (or, for
/// `isize`, would on another target); read back through `support::widen`.
/// The rest of `INT_NARROW`, and `IdxSize` (`u32` without `bigidx`, which
/// `support` asserts at compile time), fit and keep `as i64`.
pub(crate) const RISKY_INTS: &[&str] = &["u64", "usize", "isize", "i128", "u128"];

impl World {
	pub(crate) fn wrapper_for(&self, canonical: &str) -> Option<&Wrapper> {
		self.wrappers.get(canonical)
	}

	pub(crate) fn tmp(&self) -> String {
		let n = self.tmp.get();
		self.tmp.set(n + 1);
		format!("__t{n}")
	}

	/// Argument mapping. `generics` resolves a generic parameter's bounds.
	pub(crate) fn arg(
		&self,
		t: &Ty,
		name: &str,
		generics: &BTreeMap<String, String>,
		owner: Option<&str>,
		depth: u8,
	) -> Result<Arg, Unsupported> {
		if depth > 6 {
			return Err(Unsupported("nesting", t.render()));
		}
		// record 0086: a listed callable's validity bitmap from a script Vec<bool>
		if let Ty::Path { path, args } = t {
			if path == "polars_arrow::bitmap::immutable::Bitmap" && args.is_empty() {
				if let Some((op, length)) = self.bitmap_input.borrow().clone() {
					let (pre, expect, shape) = match length.as_str() {
						// the oracle's receiver fixtures have 3 rows, the List one 2
						"receiver" => (
							Some("let __mask_len = this.0.len();".to_string()),
							"Some(__mask_len)",
							if owner.is_some_and(|o| o.ends_with("::ListChunked")) {
								"mask2"
							} else {
								"mask3"
							},
						),
						"values" => (
							Some(
								"let __mask_len = support::vec_len(&values, \"values\")?;"
									.to_string(),
							),
							"Some(__mask_len)",
							"mask1",
						),
						_ => (None, "None", "mask3"),
					};
					let mut a = ok_arg(
						"rune::Value",
						format!("support::bitmap_from_bools(&{name}, \"{op}\", {expect})?"),
						"vector of bool (a validity mask, copied)",
					)?;
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
				"usize"
					if self
						.arg_guard
						.borrow()
						.as_ref()
						.is_some_and(|(_, param, _)| param == name) =>
				{
					let (op, _, check) = self.arg_guard.borrow().clone().unwrap();
					if check != "below_idx_max" {
						return Err(Unsupported("unknown argument guard", check));
					}
					let mut a = ok_arg(
						"i64",
						format!("__guarded_{name}"),
						"int (checked below the index maximum)",
					)?;
					a.pre.push(format!("let __guarded_{name} = support::below_idx_max(support::narrow::<usize>({name}, \"{name}\")?, \"{op}\", \"{name}\")?;"));
					Ok(a)
				}
				// record 0094: a listed hash parameter is a token, parsed before the call
				"u64"
					if self
						.hash_token
						.borrow()
						.as_ref()
						.is_some_and(|h| h.2.as_deref() == Some(name)) =>
				{
					let op = self.hash_token.borrow().as_ref().unwrap().0.clone();
					let mut a = ok_arg(
						"&str",
						format!("__hash_{name}"),
						"string (a hash token: 16 lowercase hex digits)",
					)?;
					a.pre.push(format!(
						"let __hash_{name} = support::hash_from_token({name}, \"{op}\")?;"
					));
					a.fallible = true;
					a.shape = "hash".into();
					Ok(a)
				}
				p if INT_NARROW.contains(&p) => ok_arg(
					"i64",
					format!("support::narrow::<{p}>({name}, \"{name}\")?"),
					"int",
				),
				"polars_utils::index::IdxSize" => ok_arg(
					"i64",
					format!("support::narrow::<p::IdxSize>({name}, \"{name}\")?"),
					"int",
				),
				"char" => ok_arg(
					"&str",
					format!("support::one_char({name}, \"{name}\")?"),
					"one-character string",
				),
				"alloc::string::String" => ok_arg("String", name.into(), "string"),
				"polars_utils::pl_str::PlSmallStr" => {
					ok_arg("&str", format!("p::PlSmallStr::from({name})"), "string")
				}
				"core::option::Option" if args.len() == 1 => {
					let inner = self.arg(&args[0], "v", generics, owner, depth + 1)?;
					let (ity, conv) = by_value(&inner, "v");
					let doc = format!("option of {}", inner.doc);
					if inner.borrow == 0 {
						let mut a = ok_arg(
							&format!("Option<{ity}>"),
							format!("match {name} {{ Some(v) => Some({conv}), None => None }}"),
							&doc,
						)?;
						a.pre = inner.pre.clone();
						a.shape = format!("opt({})", inner.shape);
						return Ok(a);
					}
					// A borrow inside an Option needs an owned temporary that
					// outlives the call, so it is hoisted into a statement.
					let t = self.tmp();
					let (pre, expr) = if inner.borrow == 2 {
						let owned = inner.owned.clone().unwrap();
						(
							format!(
								"let {t} = match {name} {{ Some(v) => Some({owned}), None => None }};"
							),
							format!("{t}.as_deref()"),
						)
					} else if inner.rust_ty == "&str" {
						(
							format!("let {t}: Option<String> = {name};"),
							format!("{t}.as_deref()"),
						)
					} else if let Some(w) = inner
						.rust_ty
						.strip_prefix('&')
						.filter(|w| self.wrappers.values().any(|x| x.rust == *w))
					{
						(
							format!(
								"let {t} = match {name} {{ Some(v) => Some(support::take::<{w}>(&v, \"{name}\")?), None => None }};"
							),
							format!("{t}.as_ref().map(|w| &w.0)"),
						)
					} else if let Some(owned) = &inner.owned {
						(
							format!(
								"let {t} = match {name} {{ Some(v) => Some({owned}), None => None }};"
							),
							format!("{t}.as_ref()"),
						)
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
					let typed = if ity == "rune::Value" {
						String::new()
					} else {
						format!("let v: {ity} = support::borrow_element(&v, \"{name}\")?; ")
					};
					Ok(shaped(
						ok_arg(
							"rune::Value",
							format!(
								"support::borrow_vec(&{name}, \"{name}\")?.into_iter().map(|v| {{ {typed}Ok::<_, Error>({conv}) }}).collect::<Result<Vec<_>, Error>>()?"
							),
							&format!("vector of {}", inner.doc),
						)?,
						format!("vec({})", inner.shape),
					))
				}
				"core::result::Result" => Err(Unsupported("result argument", t.render())),
				"polars_error::PolarsResult" => Err(Unsupported("result argument", t.render())),
				"Self" => match owner {
					Some(o) => self.arg(
						&Ty::Path {
							path: o.to_string(),
							args: vec![],
						},
						name,
						generics,
						owner,
						depth + 1,
					),
					None => Err(Unsupported("Self without owner", t.render())),
				},
				_ => {
					// an instantiation an alias wrapper holds exactly (record 0076)
					if !args.is_empty() {
						if let Some(c) = self.by_identity.get(&t.render()) {
							return self.arg(
								&Ty::Path {
									path: c.clone(),
									args: vec![],
								},
								name,
								generics,
								owner,
								depth + 1,
							);
						}
					}
					if let Some(w) = self.wrapper_for(path) {
						if !args.is_empty() {
							return Err(Unsupported("generic instantiation", t.render()));
						}
						if !self.clonable.contains(path) {
							// record 0109: moved out of the Rune value at the top
							// level; inside a container it would move out of a
							// shared element, which stays refused
							if depth > 0 {
								return Err(Unsupported(
									"by-value argument of a non-Clone type",
									t.render(),
								));
							}
							let doc =
								format!("{} (moved: the Rune value is consumed)", w.rune_name);
							return Ok(shaped(
								ok_arg(&w.rust, format!("{name}.0"), &doc)?,
								format!("W-move:{path}"),
							));
						}
						let doc = w.rune_name.clone();
						return Ok(shaped(
							ok_arg(&format!("&{}", w.rust), format!("{name}.0.clone()"), &doc)?,
							format!("W:{path}"),
						));
					}
					if let Some(s) = self.types.get(path) {
						// an unwrapped concrete alias stands for its target
						if s.kind == "type_alias" && args.is_empty() {
							if let Some(target) = &s.alias_target {
								return self.arg(
									&ty::parse(target),
									name,
									generics,
									owner,
									depth + 1,
								);
							}
						}
						let why = if s.generic {
							"generic type"
						} else if s.lifetime {
							"lifetime type"
						} else if s.hidden {
							"hidden type"
						} else if s.kind == "trait" {
							"trait object"
						} else {
							"unwrapped type"
						};
						return Err(Unsupported(why, t.render()));
					}
					if path.starts_with("polars") {
						return Err(Unsupported("unreachable polars type", t.render()));
					}
					Err(Unsupported("foreign type", t.render()))
				}
			},
			Ty::Ref { mutable, inner } => match &**inner {
				Ty::Path { path, args } if path == "str" => {
					Ok(borrowed(ok_arg("&str", name.into(), "string")?, 1, None))
				}
				Ty::Path { path, .. } if path == "alloc::string::String" => {
					Ok(borrowed(ok_arg("&str", name.into(), "string")?, 1, None))
				}
				Ty::Path { path, .. } if path == "polars_utils::pl_str::PlSmallStr" => {
					Ok(borrowed(
						ok_arg("&str", format!("&p::PlSmallStr::from({name})"), "string")?,
						1,
						Some(format!("p::PlSmallStr::from({name})")),
					))
				}
				Ty::Slice(elem) => {
					if *mutable {
						return Err(Unsupported("mutable slice", t.render()));
					}
					let inner = self.arg(elem, "v", generics, owner, depth + 1)?;
					if inner.borrow != 0 {
						return Err(Unsupported("slice of borrows", t.render()));
					}
					let (ity, conv) = by_value(&inner, "v");
					let typed = if ity == "rune::Value" {
						String::new()
					} else {
						format!("let v: {ity} = support::borrow_element(&v, \"{name}\")?; ")
					};
					let owned = format!(
						"support::borrow_vec(&{name}, \"{name}\")?.into_iter().map(|v| {{ {typed}Ok::<_, Error>({conv}) }}).collect::<Result<Vec<_>, Error>>()?"
					);
					let tmp = self.tmp();
					let mut a = ok_arg(
						"rune::Value",
						format!("&{tmp}[..]"),
						&format!("vector of {}", inner.doc),
					)?;
					a.fallible = true;
					a.pre = vec![format!("let {tmp} = {owned};")];
					a.shape = format!("vec({})", inner.shape);
					Ok(borrowed(a, 2, Some(owned)))
				}
				Ty::Path { path, args } if args.is_empty() => {
					let path = if path == "Self" {
						owner
							.ok_or_else(|| Unsupported("Self without owner", t.render()))?
							.to_string()
					} else {
						path.clone()
					};
					match self.wrapper_for(&path) {
						Some(w) => {
							if *mutable {
								Ok(shaped(
									borrowed(
										ok_arg(
											&format!("&mut {}", w.rust),
											format!("&mut {name}.0"),
											&w.rune_name,
										)?,
										1,
										None,
									),
									format!("W:{path}"),
								))
							} else {
								Ok(shaped(
									borrowed(
										ok_arg(
											&format!("&{}", w.rust),
											format!("&{name}.0"),
											&w.rune_name,
										)?,
										1,
										None,
									),
									format!("W:{path}"),
								))
							}
						}
						None => {
							let inner = self.arg(inner, name, generics, owner, depth + 1)?;
							if *mutable {
								return Err(Unsupported(
									"mutable reference to non-wrapped",
									t.render(),
								));
							}
							let owned = inner.conv.clone();
							let mut a =
								ok_arg(&inner.rust_ty, format!("&{}", inner.conv), &inner.doc)?;
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
				let mut a = ok_arg(
					&format!("({})", tys.join(", ")),
					format!("({})", convs.join(", ")),
					&format!("tuple of {}", docs.join(", ")),
				)?;
				a.pre = pre;
				a.shape = format!("tuple({})", shapes.join(";"));
				Ok(a)
			}
			Ty::Impl(bounds) => self.bounds(bounds, name, generics, owner, depth, t),
			Ty::Generic(g) => {
				if g == "Self" {
					return self.arg(
						&Ty::Path {
							path: "Self".into(),
							args: vec![],
						},
						name,
						generics,
						owner,
						depth + 1,
					);
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
			Ty::Other(s) => Err(Unsupported(
				if s.starts_with("dyn ") {
					"trait object"
				} else if s.starts_with("fn(") {
					"function pointer"
				} else if s.starts_with("&'static") {
					"static borrow"
				} else {
					"shape"
				},
				s.clone(),
			)),
		}
	}

	pub(crate) fn bounds(
		&self,
		bounds: &[Bound],
		name: &str,
		generics: &BTreeMap<String, String>,
		owner: Option<&str>,
		depth: u8,
		t: &Ty,
	) -> Result<Arg, Unsupported> {
		let neutral = [
			"core::marker::Send",
			"core::marker::Sync",
			"core::marker::Sized",
			"core::clone::Clone",
			"core::marker::Copy",
			"core::fmt::Debug",
			"core::marker::Unpin",
			"'static",
		];
		const ITERATOR_BOUNDS: &[&str] = &[
			"Iterator",
			"ExactSizeIterator",
			"DoubleEndedIterator",
			"TrustedLen",
			"PolarsIterator",
		];
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
					(Some(item), None) if target.is_none() => {
						iterator = Some((**item).clone());
						continue;
					}
					(None, Some(_)) => continue, // `+ TrustedLen`, `+ ExactSizeIterator` beside the item-bearing bound
					(None, None)
						if target.is_none()
							&& bounds.iter().any(|o| {
								ITERATOR_BOUNDS.contains(&last(&o.path)) && o.item.is_some()
							}) =>
					{
						continue;
					}
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
						Ty::Path { path, .. } if l == "AsRef" && path == "str" => Some(Ty::Ref {
							mutable: false,
							inner: Box::new(inner.clone()),
						}),
						Ty::Slice(e) if l == "AsRef" => Some(Ty::Path {
							path: "alloc::vec::Vec".into(),
							args: vec![(**e).clone()],
						}),
						_ if l == "IntoVec" => Some(Ty::Path {
							path: "alloc::vec::Vec".into(),
							args: vec![inner.clone()],
						}),
						_ if l == "AsRef" => Some(Ty::Ref {
							mutable: false,
							inner: Box::new(inner.clone()),
						}),
						// record 0084: `Into<(PlSmallStr, Field)>` and kin stay refused until a
						// tuple-and-wrapper input mapping is proven on a real binding
						Ty::Tuple(_) => return Err(Unsupported("tuple conversion", t.render())),
						_ => Some(inner.clone()),
					}
				}
				"IntoIterator" => match &b.item {
					Some(item) => Some(Ty::Path {
						path: "alloc::vec::Vec".into(),
						args: vec![(**item).clone()],
					}),
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
	pub(crate) fn iterator_input(
		&self,
		item: &Ty,
		name: &str,
		generics: &BTreeMap<String, String>,
		owner: Option<&str>,
		depth: u8,
		t: &Ty,
	) -> Result<Arg, Unsupported> {
		let u8_ = Ty::Path {
			path: "u8".into(),
			args: vec![],
		};
		let string = Ty::Path {
			path: "alloc::string::String".into(),
			args: vec![],
		};
		let vec_u8 = Ty::Path {
			path: "alloc::vec::Vec".into(),
			args: vec![u8_.clone()],
		};
		let opt = |x: Ty| Ty::Path {
			path: "core::option::Option".into(),
			args: vec![x],
		};
		let is_str = |x: &Ty| matches!(x, Ty::Ref { mutable: false, inner } if matches!(&**inner, Ty::Path { path, .. } if path == "str"));
		let is_bytes = |x: &Ty| matches!(x, Ty::Ref { mutable: false, inner } if matches!(&**inner, Ty::Slice(e) if **e == u8_));
		let inner_opt = |x: &Ty| match x {
			Ty::Path { path, args } if path == "core::option::Option" && args.len() == 1 => {
				Some(args[0].clone())
			}
			_ => None,
		};
		// (owned element type, how the temporary is borrowed per item)
		let (owned, borrow_map): (Ty, Option<&str>) = if is_str(item) {
			(string.clone(), Some("String::as_str"))
		} else if is_bytes(item) {
			(vec_u8.clone(), Some("Vec::as_slice"))
		} else if let Some(i) = inner_opt(item) {
			if is_str(&i) {
				(opt(string.clone()), Some("Option::as_deref"))
			} else if is_bytes(&i) {
				(opt(vec_u8.clone()), Some("|o| o.as_deref()"))
			} else if matches!(i, Ty::Ref { .. }) {
				return Err(Unsupported("iterator of borrowed items", t.render()));
			} else {
				(item.clone(), None)
			}
		} else if matches!(item, Ty::Ref { .. }) {
			return Err(Unsupported("iterator of borrowed items", t.render()));
		} else {
			(item.clone(), None)
		};
		let vec = Ty::Path {
			path: "alloc::vec::Vec".into(),
			args: vec![owned],
		};
		let mut a = self.arg(&vec, name, generics, owner, depth + 1)?;
		let hold = format!("__hold_{}", sanitize(name));
		match borrow_map {
			Some(map) => {
				a.pre.push(format!("let {hold} = {};", a.conv));
				a.conv = format!("{hold}.iter().map({map})");
				a.doc = format!(
					"{} (iterated, items borrowed from the script's values for the call)",
					a.doc
				);
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
	pub(crate) fn ret_iterator(
		&self,
		t: &Ty,
		owner: Option<&str>,
		depth: u8,
	) -> Result<Ret, Unsupported> {
		let Some((item, known, wrap)) = iterator_return(t) else {
			return Err(Unsupported("impl return", t.render()));
		};
		let elem = self
			.ret(&item, owner, depth + 1)
			.map_err(|Unsupported(why, what)| {
				Unsupported("iterator item", format!("{why}: {what}"))
			})?;
		if elem.materialize.is_some() {
			return Err(Unsupported("iterator item", "nested iterator".into()));
		}
		let vec_ty = format!("Vec<{}>", elem.rust_ty);
		let limit = 1usize << 20;
		let (rust_ty, doc) = match wrap {
			IterWrap::Plain => (
				vec_ty,
				format!(
					"vector of {} (materialized, at most {limit} items)",
					elem.doc
				),
			),
			IterWrap::Option => (
				format!("Option<{vec_ty}>"),
				format!(
					"option of vector of {} (materialized, at most {limit} items)",
					elem.doc
				),
			),
			IterWrap::Result => (
				vec_ty,
				format!(
					"result of vector of {} (materialized, at most {limit} items)",
					elem.doc
				),
			),
		};
		Ok(Ret {
			materialize: Some(Materialize {
				elem_conv: elem.conv,
				known,
				wrap,
			}),
			rust_ty,
			conv: "__r".into(),
			fallible: true,
			doc,
		})
	}

	pub(crate) fn ret(&self, t: &Ty, owner: Option<&str>, depth: u8) -> Result<Ret, Unsupported> {
		// record 0106: exactly `ChunkedArrayLayout<the pair's owner type>`, tagged and copied after the preflight
		if let Some((_, kind, owner_ty)) = self.layout.borrow().clone() {
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
					return Ok(Ret {
						materialize: None,
						rust_ty: format!("(String, Vec<Vec<Option<{elem}>>>)"),
						conv: format!("{copier}(this.0.chunks(), __total, __r, \"__OP__\")?"),
						fallible: true,
						doc: format!(
							"tuple of the layout variant's name and its chunks, each a vector of option of {k} values (bounded before the call)"
						),
					});
				}
				let e = self.ret(
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
				return Ok(Ret {
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
				});
			}
		}
		// record 0105: exactly the owned-item iterator of the pair's array, streamed after the preflight
		if let Some((_, kind)) = self.owned_iter.borrow().clone() {
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
					return Ok(Ret {
						materialize: None,
						rust_ty: format!("Vec<Vec<Option<{elem}>>>"),
						conv: format!("{copier}(this.0.chunks(), __total, __r, \"__OP__\")?"),
						fallible: true,
						doc: format!(
							"vector of chunks in order, each a vector of option of {k} values (bounded before the receiver is cloned)"
						),
					});
				}
				let e = self.ret(
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
				return Ok(Ret {
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
				});
			}
		}
		// record 0104: exactly `Chunks<the pair's array>`, read by index into owned chunks
		if let Some((_, kind)) = self.view_snapshot.borrow().clone() {
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
					return Ok(Ret {
						materialize: None,
						rust_ty: format!("Vec<Vec<Option<{elem}>>>"),
						conv: format!(
							"{copier}(this.0.chunks(), __r.len(), |__i| __r.get(__i), \"__OP__\")?"
						),
						fallible: true,
						doc: format!(
							"vector of chunks in index order, each a vector of option of {k} values (copied, bounded with payload bytes)"
						),
					});
				}
				let e = self.ret(
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
				return Ok(Ret {
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
				});
			}
		}
		// record 0103: exactly the pair's borrowed typed-array iterator, driven into owned chunks
		if let Some((_, kind)) = self.iter_snapshot.borrow().clone() {
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
					return Ok(Ret {
						materialize: None,
						rust_ty: format!("Vec<Vec<Option<{elem}>>>"),
						conv: format!("{copier}(this.0.chunks(), __r, \"__OP__\")?"),
						fallible: true,
						doc: format!(
							"vector of chunks in iterator order, each a vector of option of {k} values (copied, bounded with payload bytes)"
						),
					});
				}
				let e = self.ret(
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
				return Ok(Ret {
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
				});
			}
		}
		// record 0102: exactly `&T::Array` for the pair, the one array copied
		if let Some((_, kind)) = self.array_snapshot.borrow().clone() {
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
					return Ok(Ret {
						materialize: None,
						rust_ty: format!("Vec<Option<{elem}>>"),
						conv: format!("{copier}(__r, \"__OP__\")?"),
						fallible: true,
						doc: format!(
							"the single array as a vector of option of {k} values (copied, bounded with payload bytes)"
						),
					});
				}
				let e = self.ret(
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
				return Ok(Ret {
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
				});
			}
		}
		// record 0101: exactly `Option<&T::Array>` for the pair, the one selected chunk copied
		if let Some((_, kind)) = self.indexed_chunk.borrow().clone() {
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
					return Ok(Ret {
						materialize: None,
						rust_ty: format!("Option<Vec<Option<{elem}>>>"),
						conv: format!("{copier}(__r, \"__OP__\")?"),
						fallible: true,
						doc: format!(
							"option of the selected chunk as a vector of option of {k} values (copied, bounded with payload bytes)"
						),
					});
				}
				let e = self.ret(
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
				return Ok(Ret {
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
				});
			}
		}
		// record 0099: the exact chunk list, copied per chunk, for the listed pair's native
		if let Some((_, native)) = self.chunk_snapshot.borrow().clone() {
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
					return Ok(Ret {
						materialize: None,
						rust_ty: format!("Vec<Vec<Option<{elem}>>>"),
						conv: format!("{copier}(__r, \"__OP__\")?"),
						fallible: true,
						doc: format!(
							"vector of chunks, each a vector of option of {kind} values (copied, chunk boundaries kept, bounded with payload bytes)"
						),
					});
				}
				let e = self.ret(
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
				return Ok(Ret {
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
				});
			}
		}
		// record 0096: the exact null-aware shape, for the listed pair's native
		if let Ty::Path { path, args } = t {
			if path == "either::Either" {
				if let Some((_, native)) = self.null_aware.borrow().clone() {
					if depth > 0 {
						return Err(Unsupported(
							"null-aware return",
							format!("{} is nested in the return", t.render()),
						));
					}
					let elem = Ty::Path {
						path: native.clone(),
						args: vec![],
					};
					let vec_of = |inner: Ty| Ty::Path {
						path: "alloc::vec::Vec".into(),
						args: vec![inner],
					};
					let want = [
						vec_of(elem.clone()),
						vec_of(Ty::Path {
							path: "core::option::Option".into(),
							args: vec![elem.clone()],
						}),
					];
					if args.len() != 2
						|| args[0].render() != want[0].render()
						|| args[1].render() != want[1].render()
					{
						return Err(Unsupported(
							"null-aware return",
							format!(
								"{} is not Either<Vec<{native}>, Vec<Option<{native}>>>",
								t.render()
							),
						));
					}
					let e = self.ret(&elem, owner, depth + 1)?;
					if e.materialize.is_some() || e.rust_ty.starts_with("Vec") {
						return Err(Unsupported(
							"null-aware return",
							format!("element {native} is not a scalar"),
						));
					}
					let conv = format!(
						"__r.either(|__v| __v.into_iter().map(|__r| Ok::<_, Error>(Some({c}))).collect::<Result<Vec<_>, Error>>(), |__v| __v.into_iter().map(|__r| Ok::<_, Error>(match __r {{ Some(__r) => Some({c}), None => None }})).collect::<Result<Vec<_>, Error>>())?",
						c = e.conv
					);
					return Ok(Ret {
						materialize: None,
						rust_ty: format!("Vec<Option<{}>>", e.rust_ty),
						conv,
						fallible: true,
						doc: format!(
							"vector of option of {} (both Polars branches; bounded before the call)",
							e.doc
						),
					});
				}
			}
		}
		// record 0087: a listed callable's concrete `Map` iterator of integers
		// (reached through the `ChunkLenIter` alias, one level down)
		{
			if let (Some(item), Ty::Path { path, .. }) = (self.iter_return.borrow().clone(), t) {
				if path == "core::iter::adapters::map::Map" {
					let limit = 1usize << 20;
					return Ok(Ret {
						materialize: Some(Materialize {
							elem_conv: format!("support::widen::<{item}>(__r, \"__OP__\")?"),
							known: IterLen::Exact,
							wrap: IterWrap::Plain,
						}),
						rust_ty: "Vec<i64>".into(),
						conv: "__r".into(),
						fallible: true,
						doc: format!(
							"vector of int (materialized, at most {limit} items, each checked into range)"
						),
					});
				}
			}
		}
		if depth > 6 {
			return Err(Unsupported("nesting", t.render()));
		}
		let r = |rust_ty: &str, conv: String, doc: &str| {
			Ok(Ret {
				materialize: None,
				rust_ty: rust_ty.to_string(),
				fallible: false,
				conv,
				doc: doc.to_string(),
			})
		};
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
				Ok(Ret {
					materialize: None,
					rust_ty: format!("({})", tys.join(", ")),
					fallible,
					conv: format!("{{ let __t = __r; ({}) }}", convs.join(", ")),
					doc: format!("tuple of {}", docs.join(", ")),
				})
			}
			Ty::Ref { inner, mutable } => {
				// record 0082: a mutable slice needs an audited write-back contract; not admitted
				if *mutable && matches!(&**inner, Ty::Slice(_)) {
					return Err(Unsupported("mutable slice", t.render()));
				}
				if let Ty::Path { path, .. } = &**inner {
					let p = if path == "Self" {
						owner.unwrap_or("")
					} else {
						path.as_str()
					};
					if self.wrappers.contains_key(p) && !self.clonable.contains(p) {
						return Err(Unsupported(
							"borrowed return of a non-Clone type",
							t.render(),
						));
					}
				}
				let x = self.ret(inner, owner, depth + 1)?;
				// `&str` converts through `to_string`, and a slice is copied by
				// `copy_slice` (record 0082): cloning the reference is a no-op
				if matches!(&**inner, Ty::Path { path, .. } if path == "str")
					|| matches!(&**inner, Ty::Slice(_))
				{
					return Ok(x);
				}
				Ok(Ret {
					materialize: None,
					rust_ty: x.rust_ty,
					fallible: x.fallible,
					conv: format!("{{ let __r = (__r).clone(); {} }}", x.conv),
					doc: x.doc,
				})
			}
			// record 0088: a listed callable's `Cow<Wrapped>` becomes owned inside the call
			Ty::Path { path, args }
				if path == "alloc::borrow::Cow" && args.len() == 1 && self.cow_ok.get() =>
			{
				let wrapped = match &args[0] {
					Ty::Path { path: p, args: a } if a.is_empty() => {
						let p = if p == "Self" {
							owner.unwrap_or("")
						} else {
							p.as_str()
						};
						self.wrapper_for(p).is_some() && self.clonable.contains(p)
					}
					Ty::Generic(g) if g == "Self" => owner.is_some_and(|o| {
						self.wrapper_for(o).is_some() && self.clonable.contains(o)
					}),
					_ => false,
				};
				if !wrapped {
					return Err(Unsupported("cow of an unwrapped type", t.render()));
				}
				let x = self.ret(&args[0], owner, depth + 1)?;
				Ok(Ret {
					materialize: None,
					rust_ty: x.rust_ty,
					fallible: x.fallible,
					conv: format!("{{ let __r = __r.into_owned(); {} }}", x.conv),
					doc: format!("{} (owned)", x.doc),
				})
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
				"u64" if self.hash_token.borrow().as_ref().is_some_and(|h| h.1) => r(
					"String",
					"support::hash_token(__r)".into(),
					"string (a hash token: 16 lowercase hex digits)",
				),
				"usize" if self.bounded_ok.get() => r(
					"i64",
					"support::bounded_usize(__r)".into(),
					"int (a proven length or index)",
				),
				p if RISKY_INTS.contains(&p) => Ok(Ret {
					materialize: None,
					rust_ty: "i64".into(),
					fallible: true,
					conv: format!("support::widen::<{p}>(__r, \"__OP__\")?"),
					doc: "int (checked into range)".into(),
				}),
				p if INT_NARROW.contains(&p) => r("i64", "(__r as i64)".into(), "int"),
				"polars_utils::index::IdxSize" => r("i64", "(__r as i64)".into(), "int"),
				"char" => r("String", "__r.to_string()".into(), "string"),
				"str" | "alloc::string::String" | "polars_utils::pl_str::PlSmallStr" => {
					r("String", "__r.to_string()".into(), "string")
				}
				"Self" => self.ret(
					&Ty::Path {
						path: owner
							.ok_or_else(|| Unsupported("Self without owner", t.render()))?
							.to_string(),
						args: vec![],
					},
					owner,
					depth + 1,
				),
				"core::option::Option" if args.len() == 1 && matches!(args[0], Ty::Impl(_)) => {
					self.ret_iterator(t, owner, depth)
				}
				"core::option::Option" if args.len() == 1 => {
					let x = self.ret(&args[0], owner, depth + 1)?;
					Ok(Ret {
						materialize: None,
						rust_ty: format!("Option<{}>", x.rust_ty),
						fallible: x.fallible,
						conv: format!(
							"match __r {{ Some(__r) => Some({}), None => None }}",
							x.conv
						),
						doc: format!("option of {}", x.doc),
					})
				}
				"alloc::vec::Vec" if args.len() == 1 => {
					let x = self.ret(&args[0], owner, depth + 1)?;
					Ok(Ret {
						materialize: None,
						rust_ty: format!("Vec<{}>", x.rust_ty),
						fallible: x.fallible,
						conv: format!(
							"{{ let mut __v = Vec::new(); for __r in __r {{ __v.push({}); }} __v }}",
							x.conv
						),
						doc: format!("vector of {}", x.doc),
					})
				}
				"polars_error::PolarsResult"
					if args.len() == 1 && matches!(args[0], Ty::Impl(_)) =>
				{
					self.ret_iterator(t, owner, depth)
				}
				"polars_error::PolarsResult" if args.len() == 1 => {
					let x = self.ret(&args[0], owner, depth + 1)?;
					Ok(Ret {
						materialize: None,
						rust_ty: x.rust_ty,
						fallible: true,
						conv: format!("{{ let __r = __r.map_err(Error::from)?; {} }}", x.conv),
						doc: format!("result of {}", x.doc),
					})
				}
				"core::result::Result"
					if args.len() == 2
						&& args[1]
							== Ty::Path {
								path: "polars_error::PolarsError".into(),
								args: vec![],
							} =>
				{
					let x = self.ret(&args[0], owner, depth + 1)?;
					Ok(Ret {
						materialize: None,
						rust_ty: x.rust_ty,
						fallible: true,
						conv: format!("{{ let __r = __r.map_err(Error::from)?; {} }}", x.conv),
						doc: format!("result of {}", x.doc),
					})
				}
				"core::result::Result" => Err(Unsupported("result with foreign error", t.render())),
				_ => {
					if !args.is_empty() {
						if let Some(c) = self.by_identity.get(&t.render()) {
							return self.ret(
								&Ty::Path {
									path: c.clone(),
									args: vec![],
								},
								owner,
								depth + 1,
							);
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
						let why = if s.generic {
							"generic type"
						} else if s.lifetime {
							"lifetime type"
						} else if s.hidden {
							"hidden type"
						} else if s.kind == "trait" {
							"trait object"
						} else {
							"unwrapped type"
						};
						return Err(Unsupported(why, t.render()));
					}
					// record 0085: a validity bitmap, only for the release's listed paths
					if path == "polars_arrow::bitmap::immutable::Bitmap"
						&& args.is_empty() && self.bitmap_ok.get()
					{
						let limit = 1usize << 20;
						return Ok(Ret {
							materialize: None,
							rust_ty: "Vec<bool>".into(),
							fallible: true,
							conv: "support::copy_bits(&__r, \"__OP__\")?".into(),
							doc: format!(
								"vector of bool (validity bits copied from a bitmap, at most {limit} bits per call)"
							),
						});
					}
					if path.starts_with("polars") {
						return Err(Unsupported("unreachable polars type", t.render()));
					}
					Err(Unsupported("foreign type", t.render()))
				}
			},
			Ty::Generic(g) if g == "Self" => self.ret(
				&Ty::Path {
					path: "Self".into(),
					args: vec![],
				},
				owner,
				depth + 1,
			),
			Ty::Generic(_) => Err(Unsupported("generic return", t.render())),
			Ty::Impl(_) => self.ret_iterator(t, owner, depth),
			// Record 0082: an immutable borrowed slice is copied into an
			// owned vector under the materialize bound (cumulative over the
			// binding's slices); the script owns the result outright.
			Ty::Slice(elem) => {
				let x = self
					.ret(elem, owner, depth + 1)
					.map_err(|Unsupported(why, what)| {
						Unsupported("slice element", format!("{why}: {what}"))
					})?;
				if x.materialize.is_some() {
					return Err(Unsupported("slice element", "iterator".into()));
				}
				let limit = 1usize << 20;
				Ok(Ret {
					materialize: None,
					rust_ty: format!("Vec<{}>", x.rust_ty),
					fallible: true,
					conv: format!(
						"support::copy_slice(__r, \"__OP__\", |__r| Ok::<_, Error>({}))?",
						x.conv
					),
					doc: format!(
						"vector of {} (copied from a borrowed slice, at most {limit} elements)",
						x.doc
					),
				})
			}
			Ty::Other(s) => Err(Unsupported("shape", s.clone())),
		}
	}
}

/// By-value form of an argument inside a container: wrapper references
/// become owned `rune::Value`s taken by `support::take`.
pub(crate) fn by_value(a: &Arg, name: &str) -> (String, String) {
	if let Some(w) = a.rust_ty.strip_prefix("&mut ") {
		return (
			"rune::Value".into(),
			format!("support::take::<{w}>(&{name}, \"{name}\")?.0"),
		);
	}
	if let Some(w) = a.rust_ty.strip_prefix('&') {
		if w == "str" {
			// owned string; the borrow form (`&str`) is handled by the caller
			return (
				"String".into(),
				a.conv.replace(name, &format!("{name}.as_str()")),
			);
		}
		return (
			"rune::Value".into(),
			format!("support::take::<{w}>(&{name}, \"{name}\")?.0"),
		);
	}
	(a.rust_ty.clone(), a.conv.clone())
}

#[cfg(test)]
mod tests {
	#[test]
	fn iterator() {
		super::iterator_self_test();
	}
}
