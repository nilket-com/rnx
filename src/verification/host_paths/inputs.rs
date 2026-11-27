//! Script values supplied independently at each argument position.
use super::Result;
use std::collections::BTreeMap;

pub(super) fn kinds() -> Result<BTreeMap<String, &'static str>> {
	let source = include_str!("../../../rune/crates/rune/src/experimental_core/detail.rs");
	let types = syn::parse_file(source)?
		.items
		.into_iter()
		.find_map(|item| match item {
			syn::Item::Enum(e) if e.ident == "TypeName" => Some(e.variants),
			_ => None,
		})
		.ok_or("heap diagnostic type enumeration missing")?;
	let mut kinds = BTreeMap::new();
	for variant in types {
		let name = variant.ident.to_string();
		let expr = match name.as_str() {
			"Unit" => "()",
			"Bool" => "true",
			"I64" => "1",
			"U64" => "1u64",
			"F64" => "1.25",
			"Char" => "'x'",
			"Ordering" => "std::cmp::Ordering::Equal",
			"String" => "\"probe\"",
			"Vec" => "[1,2]",
			"Tuple" => "(1,2)",
			"Object" => "#{a:1}",
			"Option" => "Some(1)",
			"Result" => "Ok(1)",
			"Range" => "(1..3)",
			"RangeInclusive" => "(1..=3)",
			"VecIter" => "[1,2].iter()",
			"Function" => "probe_fn",
			"Struct" => "Probe {n:1}",
			"Type" => "i64",
			"RangeFrom" => "(1..)",
			"RangeFull" => "(..)",
			"RangeToInclusive" => "(..=2)",
			"RangeTo" => "(..2)",
			"OptionIter" => "Some(1).iter()",
			"StringSplit" => "\"a b\".split(\" \")",
			"CharSplit" => "\"a b\".split(' ')",
			"Chars" => "\"ab\".chars()",
			"Skip" => "\"ab\".chars().skip(1)",
			"Take" => "\"ab\".chars().take(1)",
			"ObjectIter" => "#{a:1}.iter()",
			"RangeIter" => "(1..3).iter()",
			"UnsignedRangeIter" => "(1u64..3u64).iter()",
			"CharRangeIter" => "('a'..'c').iter()",
			"RangeInclusiveIter" => "(1..=3).iter()",
			"UnsignedRangeInclusiveIter" => "(1u64..=3u64).iter()",
			"CharRangeInclusiveIter" => "('a'..='c').iter()",
			"RangeFromIter" => "(1..).iter()",
			"UnsignedRangeFromIter" => "(1u64..).iter()",
			"CharRangeFromIter" => "('a'..).iter()",
			_ => return Err(format!("no host-path witness for heap kind {name}").into()),
		};
		kinds.insert(name, expr);
	}
	for (name, expr) in [
		("Closure", "{let n=1;|x|x+n}"),
		("HostFunction", "std::io::print"),
		("EmptyVecIter", "[].iter()"),
		("FloatVecIter", "[1.25, 2.0].iter()"),
		("AdvancedVecIter", "{let it=[1,2,3].iter(); it.next(); it}"),
		("ExhaustedVecIter", "{let it=[1].iter(); it.next(); it}"),
		("MixedIntegerVecIter", "[2,3u64].iter()"),
		("BadSecondVecIter", "[1,true,3].iter()"),
		("OverflowVecIter", "[9223372036854775807,2,1].iter()"),
		("Future", "time::sleep(0)"),
	] {
		assert!(kinds.insert(name.into(), expr).is_none());
	}
	Ok(kinds)
}

pub(super) fn sample(ty: &str) -> &'static str {
	match ty {
		"::std::bool" => "false",
		"::std::char" => "'x'",
		"::std::i64" => "1",
		"::std::u64" => "1u64",
		"::std::f64" => "1.25",
		"::std::string::String" => "\"probe\"",
		"::std::vec::Vec" => "[1,2]",
		"::std::tuple::Tuple" => "(1,2)",
		"::std::object::Object" => "#{a:1}",
		"::std::result::Result" => "Ok(1)",
		"::std::option::Option" => "Some(1)",
		"::std::any::Type" => "i64",
		"::std::cmp::Ordering" => "std::cmp::Ordering::Equal",
		"::std::ops::Function" => "|x|x",
		"::std::string::Chars" => "\"ab\".chars()",
		"::std::ops::Range" => "(1..3)",
		"::std::ops::RangeFrom" => "(1..)",
		"::std::ops::RangeFull" => "(..)",
		"::std::ops::RangeInclusive" => "(1..=3)",
		"::std::ops::RangeTo" => "(..2)",
		"::std::ops::RangeToInclusive" => "(..=2)",
		"::std::object::Iter" => "#{a:1}.iter()",
		"::std::option::Iter" => "Some(1).iter()",
		"::std::slice::Iter" => "[1,2].iter()",
		"::std::iter::Skip" => "\"ab\".chars().skip(1)",
		"::std::iter::Take" => "\"ab\".chars().take(1)",
		"::std::string::Split::<::std::char>" => "\"a b\".split(' ')",
		ty if ty.starts_with("::std::string::Split::<") => "\"a b\".split(\" \")",
		"::std::ops::RangeIter::<::std::i64>" => "(1..3).iter()",
		"::std::ops::RangeIter::<::std::u64>" => "(1u64..3u64).iter()",
		"::std::ops::RangeIter::<::std::char>" => "('a'..'c').iter()",
		"::std::ops::RangeInclusiveIter::<::std::i64>" => "(1..=3).iter()",
		"::std::ops::RangeInclusiveIter::<::std::u64>" => "(1u64..=3u64).iter()",
		"::std::ops::RangeInclusiveIter::<::std::char>" => "('a'..='c').iter()",
		"::std::ops::RangeFromIter::<::std::i64>" => "(1..).iter()",
		"::std::ops::RangeFromIter::<::std::u64>" => "(1u64..).iter()",
		"::std::ops::RangeFromIter::<::std::char>" => "('a'..).iter()",
		_ => "()",
	}
}

pub(super) fn arguments(
	method: &str,
	owner: Option<&str>,
	signature: Option<Vec<String>>,
) -> Vec<String> {
	let receiver = sample(owner.unwrap_or(""));
	let mut args: Vec<&str> = match signature {
		Some(types) => types.iter().map(|name| sample(name)).collect(),
		None if owner.is_some() => {
			let mut args = vec![receiver];
			match method {
				"nth" | "nth_back" | "skip" | "take" => args.push("1"),
				"chain" => args.push("[3]"),
				"fold" => args.extend(["0", "|a,b|a+b"]),
				"reduce" => args.push("|a,b|a+b"),
				"any" | "all" | "find" | "filter" => args.push("|x|true"),
				"filter_map" => args.push("|x|Some(x)"),
				"flat_map" => args.push("|x|[x]"),
				"map" => args.push("|x|x"),
				"cmp" | "partial_cmp" | "min" | "max" | "eq" | "ne" | "lt" | "le" | "gt" | "ge" => {
					args.push(receiver)
				}
				_ => {}
			}
			args
		}
		None if method == "dbg" => vec!["1"],
		None => vec![],
	};
	if owner.is_some_and(|name| {
		name.contains("Iter")
			|| name.contains("::Split")
			|| name.ends_with("::Skip")
			|| name.ends_with("::Take")
	}) && let Some(first) = args.first_mut()
	{
		*first = receiver;
	}
	if owner.is_some() && args.len() > 1 {
		let last = args.last_mut().unwrap();
		match method {
			"any" | "find" | "position" | "filter" | "rposition" => *last = "|x|true",
			"all" => *last = "|x|false",
			"reduce" | "fold" => *last = "|a,b|a+b",
			"sort_by" => *last = "|a,b|a.cmp(b)",
			"map" | "for_each" => *last = "|x|x",
			"and_then" if owner == Some("::std::result::Result") => *last = "|x|Ok(x)",
			"and_then" => *last = "|x|Some(x)",
			"split" => *last = "\" \"",
			_ => {}
		}
		let len = args.len();
		if method == "resize" {
			args[len - 2..].copy_from_slice(&["1u64", "1"]);
		}
		if method == "fold" {
			args[len - 2] = "0";
		}
	}
	args.into_iter().map(str::to_owned).collect()
}
