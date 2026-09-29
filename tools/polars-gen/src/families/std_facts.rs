//! Record 0118: a closed table of std facts. The inventory records no std
//! trait impls, and the generator models neither core traits with arguments
//! nor auto traits; in-memory I/O needs a handful of them for exactly the
//! two types it instantiates (`Cursor<Vec<u8>>` and the adapter's `Sink`)
//! and the `Vec<u8>` a cursor owns. A release row is admitted only if its
//! (type, trait) key is on the fixed allowlist below, spelled canonically,
//! unduplicated and cited; anything else refuses generation. `holds`
//! consults the admitted set before its core-trait rules, and every
//! shipped fact is emitted as a compile-time assertion, so a false row fails
//! the adapter build at that pin. Nothing is inferred.
use crate::ty;
use std::collections::BTreeSet;

/// One release row: `[[std_facts]]` with `type`, `trait` and `cite`.
#[derive(serde::Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct StdFact {
	#[serde(rename = "type")]
	pub(crate) ty: String,
	#[serde(rename = "trait")]
	pub(crate) tr: String,
	pub(crate) cite: String,
}

pub(crate) const VEC_U8: &str = "alloc::vec::Vec<u8>";
pub(crate) const CURSOR: &str = "core::io::cursor::Cursor<alloc::vec::Vec<u8>>";
pub(crate) const SINK: &str = "support::Sink";

/// Record 0118: the two types an I/O listing may instantiate a reader or a
/// writer at, as generated Rust spells them; `None` for any other.
pub(crate) fn io_type_spelling(t: &str) -> Option<&'static str> {
	match t {
		CURSOR => Some("std::io::Cursor<Vec<u8>>"),
		SINK => Some("super::support::Sink"),
		_ => None,
	}
}

/// The fixed allowlist: (canonical type, canonical trait with its
/// arguments, the type and trait as generated Rust names them).
pub(crate) const ALLOWED: &[(&str, &str, &str, &str)] = &[
	(
		VEC_U8,
		"core::convert::AsRef<[u8]>",
		"Vec<u8>",
		"AsRef<[u8]>",
	),
	(VEC_U8, "core::marker::Send", "Vec<u8>", "Send"),
	(VEC_U8, "core::marker::Sync", "Vec<u8>", "Sync"),
	(
		CURSOR,
		"alloc::io::read::Read",
		"std::io::Cursor<Vec<u8>>",
		"std::io::Read",
	),
	(
		CURSOR,
		"core::io::seek::Seek",
		"std::io::Cursor<Vec<u8>>",
		"std::io::Seek",
	),
	(
		CURSOR,
		"alloc::io::buf_read::BufRead",
		"std::io::Cursor<Vec<u8>>",
		"std::io::BufRead",
	),
	(
		CURSOR,
		"core::marker::Send",
		"std::io::Cursor<Vec<u8>>",
		"Send",
	),
	(
		CURSOR,
		"core::marker::Sync",
		"std::io::Cursor<Vec<u8>>",
		"Sync",
	),
	(
		SINK,
		"core::io::write::Write",
		"support::Sink",
		"std::io::Write",
	),
	(SINK, "core::marker::Send", "super::support::Sink", "Send"),
	(SINK, "core::marker::Sync", "super::support::Sink", "Sync"),
];

/// The admitted (type, trait) keys, or the first bad row's reason.
pub(crate) fn validate(rows: &[StdFact]) -> Result<BTreeSet<(String, String)>, String> {
	let mut out = BTreeSet::new();
	for r in rows {
		let key = format!("`{}: {}`", r.ty, r.tr);
		if r.cite.trim().is_empty() {
			return Err(format!("{key}: no citation"));
		}
		for part in [&r.ty, &r.tr] {
			if ty::parse(part).render() != *part {
				return Err(format!("{key}: `{part}` is not canonical"));
			}
		}
		if !ALLOWED
			.iter()
			.any(|(t, tr, _, _)| *t == r.ty && *tr == r.tr)
		{
			return Err(format!("{key}: not on the std-facts allowlist"));
		}
		if !out.insert((r.ty.clone(), r.tr.clone())) {
			return Err(format!("{key}: listed twice"));
		}
	}
	Ok(out)
}

/// One compile-time assertion per admitted fact, in generated Rust.
pub(crate) fn assertions(facts: &BTreeSet<(String, String)>) -> String {
	let mut s = String::from(
		"\n// record 0118: every shipped std fact, asserted at compile time at this pin\n",
	);
	for (t, tr) in facts {
		let (_, _, ts, trs) = ALLOWED
			.iter()
			.find(|(a, b, _, _)| a == t && b == tr)
			.unwrap();
		s.push_str(&format!(
			"const _: () = {{ fn holds<T: ?Sized + {trs}>() {{}} let _ = holds::<{ts}>; }};\n"
		));
	}
	s
}

/// Record 0118: the table refuses what it must and admits only its rows.
pub(crate) fn std_facts_self_test() {
	let row = |t: &str, tr: &str, cite: &str| StdFact {
		ty: t.into(),
		tr: tr.into(),
		cite: cite.into(),
	};
	let ok = validate(&[
		row(VEC_U8, "core::convert::AsRef<[u8]>", "std"),
		row(SINK, "core::io::write::Write", "support.rs"),
	])
	.unwrap();
	assert_eq!(ok.len(), 2);
	for (rows, want) in [
		(
			vec![row(VEC_U8, "core::marker::Unpin", "std")],
			"not on the std-facts allowlist",
		),
		(
			vec![row("alloc::vec::Vec<u16>", "core::marker::Send", "std")],
			"not on the std-facts allowlist",
		),
		(
			vec![row("alloc::vec::Vec< u8 >", "core::marker::Send", "std")],
			"is not canonical",
		),
		(vec![row(VEC_U8, "core::marker::Send", " ")], "no citation"),
		(
			vec![
				row(VEC_U8, "core::marker::Send", "a"),
				row(VEC_U8, "core::marker::Send", "b"),
			],
			"listed twice",
		),
	] {
		let e = validate(&rows).unwrap_err();
		assert!(e.contains(want), "{e}");
	}
	// every allowed key is canonical, and its assertion names it in Rust
	let all: BTreeSet<(String, String)> = ALLOWED
		.iter()
		.map(|(a, b, _, _)| (a.to_string(), b.to_string()))
		.collect();
	for (a, b) in &all {
		assert_eq!(ty::parse(a).render(), *a);
		assert_eq!(ty::parse(b).render(), *b);
	}
	let text = assertions(&all);
	assert_eq!(text.matches("const _: ()").count(), ALLOWED.len());
	assert!(text.contains("holds::<support::Sink>") && text.contains("std::io::Write"));
	println!("std-facts self-test: ok");
}

#[cfg(test)]
mod tests {
	#[test]
	fn std_facts() {
		super::std_facts_self_test();
	}
}
