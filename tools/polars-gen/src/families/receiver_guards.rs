//! Record 0119: listed receiver guards. Some methods panic when an index
//! argument is out of range for the receiver (`Series::to_arrow`'s chunk
//! index, an Arrow array's `is_null(i)`, `sliced(offset, length)`); a
//! listed guard converts the argument first, checks it against the
//! receiver (`len()` or `n_chunks()`, overflow checked) and returns a named
//! `OutOfBounds` error before the Polars call.
use crate::model::Callable;

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReceiverGuard {
	pub(crate) path: String,
	/// `below_len`, `below_n_chunks`, `at_most_len`, `range_len`
	/// (`param` is the offset, `param2` the length; a zero length is not
	/// checked, as Arrow's `sliced` returns an empty array for it), or
	/// `range_len_all` (record 0120: the concrete arrays' inherent `sliced`
	/// asserts `offset + length <= len` for every length, zero included).
	pub(crate) check: String,
	pub(crate) param: String,
	#[serde(default)]
	pub(crate) param2: Option<String>,
	pub(crate) cite: String,
}

/// Record 0119 (review): the guards are a closed, required set, not an
/// optional match. Whenever `polars_arrow` is admitted, the release's rows
/// must be exactly these (path, check, parameter, second parameter); a
/// missing, extra, duplicated or differently checked row refuses generation.
pub(crate) const REQUIRED: &[(&str, &str, &str, Option<&str>)] = &[
	(
		"polars_core::series::Series::to_arrow",
		"below_n_chunks",
		"chunk_idx",
		None,
	),
	(
		"polars_arrow::array::Array::is_null",
		"below_len",
		"i",
		None,
	),
	(
		"polars_arrow::array::Array::is_valid",
		"below_len",
		"i",
		None,
	),
	(
		"polars_arrow::array::Array::sliced",
		"range_len",
		"offset",
		Some("length"),
	),
	(
		"polars_arrow::array::Array::split_at_boxed",
		"at_most_len",
		"offset",
		None,
	), // record 0120: the concrete arrays' index and range methods
	(
		"polars_arrow::array::primitive::PrimitiveArray::value",
		"below_len",
		"i",
		None,
	),
	(
		"polars_arrow::array::utf8::Utf8Array::value",
		"below_len",
		"i",
		None,
	),
	(
		"polars_arrow::array::binary::BinaryArray::value",
		"below_len",
		"i",
		None,
	),
	(
		"polars_arrow::array::list::ListArray::value",
		"below_len",
		"i",
		None,
	),
	(
		"polars_arrow::array::utf8::Utf8Array::get",
		"below_len",
		"i",
		None,
	),
	(
		"polars_arrow::array::binary::BinaryArray::get",
		"below_len",
		"i",
		None,
	),
	(
		"polars_arrow::array::primitive::PrimitiveArray::sliced",
		"range_len_all",
		"offset",
		Some("length"),
	),
	(
		"polars_arrow::array::utf8::Utf8Array::sliced",
		"range_len_all",
		"offset",
		Some("length"),
	),
	(
		"polars_arrow::array::binary::BinaryArray::sliced",
		"range_len_all",
		"offset",
		Some("length"),
	),
	(
		"polars_arrow::array::list::ListArray::sliced",
		"range_len_all",
		"offset",
		Some("length"),
	),
	(
		"polars_arrow::array::struct_::StructArray::sliced",
		"range_len_all",
		"offset",
		Some("length"),
	),
];

/// The release's guard rows against the required set and the inventory:
/// each row cited, unique, required, with its exact check, its parameters
/// `usize` on a callable that has a receiver. With `required` (the release
/// admits `polars_arrow`), every required path the inventory holds must be
/// guarded.
pub(crate) fn validate(
	rows: &[ReceiverGuard],
	callables: &[Callable],
	required: bool,
) -> Result<(), String> {
	let mut seen = std::collections::BTreeSet::new();
	for g in rows {
		if g.cite.trim().is_empty() {
			return Err(format!("{}: no citation", g.path));
		}
		if !seen.insert(g.path.clone()) {
			return Err(format!("{}: listed twice", g.path));
		}
		let Some((_, check, p, p2)) = REQUIRED.iter().find(|(path, ..)| *path == g.path) else {
			return Err(format!("{}: not a required receiver guard", g.path));
		};
		if g.check != *check || g.param != *p || g.param2.as_deref() != *p2 {
			return Err(format!(
				"{}: the guard is `{}` on {}{}; the required guard is `{check}` on {p}{}",
				g.path,
				g.check,
				g.param,
				g.param2
					.as_deref()
					.map(|x| format!(", {x}"))
					.unwrap_or_default(),
				p2.map(|x| format!(", {x}")).unwrap_or_default()
			));
		}
		let Some(c) = callables.iter().find(|c| c.canonical_path == g.path) else {
			return Err(format!("{}: not in the inventory", g.path));
		};
		if c.receiver == "none" {
			return Err(format!("{}: a guard needs a receiver", g.path));
		}
		for name in std::iter::once(p).chain(p2.iter()) {
			match c.params.iter().find(|q| q.name == *name) {
				Some(q) if q.ty_canonical == "usize" => {}
				Some(q) => {
					return Err(format!(
						"{}: `{name}` is `{}`, not usize",
						g.path, q.ty_canonical
					));
				}
				None => return Err(format!("{}: no parameter `{name}`", g.path)),
			}
		}
	}
	if required {
		// every required path the inventory holds must be guarded
		if let Some((path, ..)) = REQUIRED.iter().find(|(path, ..)| {
			!seen.contains(*path) && callables.iter().any(|c| c.canonical_path == *path)
		}) {
			return Err(format!("{path}: a required receiver guard is missing"));
		}
	}
	Ok(())
}

/// The guard's check on the hoisted locals, as generated Rust, or why the
/// row is malformed. `local(p)` names the hoisted local of parameter `p`.
pub(crate) fn guard_code(
	g: &ReceiverGuard,
	c: &Callable,
	op: &str,
	local: impl Fn(&str) -> Option<String>,
) -> Result<String, String> {
	if g.cite.trim().is_empty() {
		return Err(format!("{}: no citation", g.path));
	}
	let a = local(&g.param).ok_or_else(|| format!("{}: no parameter `{}`", g.path, g.param))?;
	let oob = |cond: String, msg: &str, args: String| {
		format!(
			"if {cond} {{ return Err(Error(\"OutOfBounds\".into(), format!(\"{op}: {msg}\", {args}))); }} "
		)
	};
	if c.receiver == "none" {
		return Err(format!("{}: a guard needs a receiver", g.path));
	}
	Ok(match g.check.as_str() {
		"below_len" => oob(
			format!("{a} >= this.0.len()"),
			"index {} is out of bounds for length {}",
			format!("{a}, this.0.len()"),
		),
		"at_most_len" => oob(
			format!("{a} > this.0.len()"),
			"offset {} is past length {}",
			format!("{a}, this.0.len()"),
		),
		"below_n_chunks" => oob(
			format!("{a} >= this.0.n_chunks()"),
			"chunk {} is out of bounds for {} chunks",
			format!("{a}, this.0.n_chunks()"),
		),
		"range_len" => {
			let p2 = g
				.param2
				.as_deref()
				.ok_or_else(|| format!("{}: range_len needs param2", g.path))?;
			let b = local(p2).ok_or_else(|| format!("{}: no parameter `{p2}`", g.path))?;
			oob(
				format!("{b} != 0 && {a}.checked_add({b}).is_none_or(|e| e > this.0.len())"),
				"offset {} + length {} is past length {}",
				format!("{a}, {b}, this.0.len()"),
			)
		}
		"range_len_all" => {
			let p2 = g
				.param2
				.as_deref()
				.ok_or_else(|| format!("{}: range_len_all needs param2", g.path))?;
			let b = local(p2).ok_or_else(|| format!("{}: no parameter `{p2}`", g.path))?;
			oob(
				format!("{a}.checked_add({b}).is_none_or(|e| e > this.0.len())"),
				"offset {} + length {} is past length {}",
				format!("{a}, {b}, this.0.len()"),
			)
		}
		other => return Err(format!("{}: unknown check `{other}`", g.path)),
	})
}

/// Record 0119: each check's emitted condition, and the malformed rows.
pub(crate) fn receiver_guards_self_test() {
	let c: Callable = serde_json::from_value(serde_json::json!({
		"key": "k", "kind": "inherent", "krate": "polars_core", "owner": "o", "name": "m",
		"canonical_path": "o::m", "found_paths": [], "crate_paths": [], "receiver": "&self",
		"params": [], "ret": null, "ret_canonical": null, "generics_canonical": [], "impl_for": null,
		"impl_bounds": [], "impl_head": null, "impl_where": [], "impl_assoc": [], "docs_first": null,
		"owner_generic": false, "is_unsafe": false, "is_async": false, "deprecated": false,
		"hidden": false, "implementors": [], "trait_reachable": false, "derived": false,
		"bucket": "mechanical", "rules": []
	}))
	.unwrap();
	let g = |check: &str, param: &str, param2: Option<&str>, cite: &str| ReceiverGuard {
		path: "o::m".into(),
		check: check.into(),
		param: param.into(),
		param2: param2.map(String::from),
		cite: cite.into(),
	};
	let local = |p: &str| {
		["i", "offset", "length"]
			.contains(&p)
			.then(|| format!("__guard_{p}"))
	};
	let code = |r: &ReceiverGuard| guard_code(r, &c, "polars::X::m", local);
	assert!(
		code(&g("below_len", "i", None, "c"))
			.unwrap()
			.starts_with("if __guard_i >= this.0.len() {")
	);
	assert!(
		code(&g("at_most_len", "i", None, "c"))
			.unwrap()
			.starts_with("if __guard_i > this.0.len() {")
	);
	assert!(
		code(&g("below_n_chunks", "i", None, "c"))
			.unwrap()
			.contains("__guard_i >= this.0.n_chunks()")
	);
	let range = code(&g("range_len", "offset", Some("length"), "c")).unwrap();
	// record 0120: the strict range checks a zero length too
	let strict = code(&g("range_len_all", "offset", Some("length"), "c")).unwrap();
	assert!(
		strict.starts_with(
			"if __guard_offset.checked_add(__guard_length).is_none_or(|e| e > this.0.len())"
		) && !strict.contains("!= 0"),
		"{strict}"
	);
	assert!(
		range.starts_with("if __guard_length != 0 && __guard_offset.checked_add(__guard_length)"),
		"a zero length is not checked, as Arrow's sliced: {range}"
	);
	assert!(range.contains("\"OutOfBounds\""), "{range}");
	for (r, want) in [
		(g("below_len", "i", None, " "), "no citation"),
		(g("below_len", "nope", None, "c"), "no parameter `nope`"),
		(
			g("range_len", "offset", None, "c"),
			"range_len needs param2",
		),
		(g("sideways", "i", None, "c"), "unknown check `sideways`"),
	] {
		let e = code(&r).unwrap_err();
		assert!(e.contains(want), "{e}");
	}
	let mut free = c.clone();
	free.receiver = "none".into();
	assert!(
		guard_code(&g("below_len", "i", None, "c"), &free, "op", local)
			.unwrap_err()
			.contains("needs a receiver")
	);
	// review of 0119: the closed, required set, fail closed
	let param = |name: &str, ty: &str| crate::model::Param {
		name: name.into(),
		ty: ty.into(),
		ty_canonical: ty.into(),
	};
	let callables: Vec<Callable> = REQUIRED
		.iter()
		.map(|(path, _, p, p2)| {
			let mut x = c.clone();
			x.canonical_path = path.to_string();
			x.params = std::iter::once(param(p, "usize"))
				.chain(p2.map(|q| param(q, "usize")))
				.collect();
			x
		})
		.collect();
	let row = |path: &str, check: &str, p: &str, p2: Option<&str>| ReceiverGuard {
		path: path.into(),
		check: check.into(),
		param: p.into(),
		param2: p2.map(String::from),
		cite: "c".into(),
	};
	let full: Vec<ReceiverGuard> = REQUIRED
		.iter()
		.map(|(path, check, p, p2)| row(path, check, p, *p2))
		.collect();
	assert!(validate(&full, &callables, true).is_ok());
	let refused = |rows: &[ReceiverGuard], cs: &[Callable], want: &str| {
		let e = validate(rows, cs, true).unwrap_err();
		assert!(e.contains(want), "{e}");
	};
	// a removed row
	refused(
		&full[1..],
		&callables,
		"a required receiver guard is missing",
	);
	// a swapped, otherwise valid check (below_len on the chunk index)
	let mut swapped = full.clone();
	swapped[0].check = "below_len".into();
	refused(
		&swapped,
		&callables,
		"the required guard is `below_n_chunks`",
	);
	// a duplicate, an extra and an uncited row
	let mut dup = full.clone();
	dup.push(full[0].clone());
	refused(&dup, &callables, "listed twice");
	let mut extra = full.clone();
	extra.push(row("o::m", "below_len", "i", None));
	refused(&extra, &callables, "not a required receiver guard");
	let mut uncited = full.clone();
	uncited[2].cite = " ".into();
	refused(&uncited, &callables, "no citation");
	// the parameter must be usize, on a callable with a receiver
	let mut narrow = callables.clone();
	narrow[1].params = vec![param("i", "i32")];
	refused(&full, &narrow, "is `i32`, not usize");
	let mut free = callables.clone();
	free[4].receiver = "none".into();
	refused(&full, &free, "a guard needs a receiver");
	// a release that does not admit polars_arrow requires nothing
	assert!(validate(&[], &callables, false).is_ok());
	println!("receiver-guards self-test: ok");
}

#[cfg(test)]
mod tests {
	#[test]
	fn receiver_guards() {
		super::receiver_guards_self_test();
	}
}
