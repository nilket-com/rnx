//! Record 0141: the value contract. `DataFrame`, `LazyFrame`, `Series` and `Expr` are never consumed; Rune names alias one value, so an in-place method changes every name; `clone()` gives an independent value; builders are consumed by their by-value methods. Configuration: default features (`generated`) plus `test-support`; the no-`generated` build is covered by the library's `clone_identity` unit tests.
#![cfg(all(feature = "generated", feature = "test-support"))]
use rnx::rune::{self, Context, Module, Source, Sources, Vm};
use std::collections::BTreeMap;
use std::sync::Arc;

fn run(script: &str) -> Result<rune::Value, String> {
	let mut polars = Module::with_crate("polars").unwrap();
	rnx_polars::build(&mut polars).unwrap();
	let mut fixtures = Module::with_crate("fx").unwrap();
	rnx_polars::generated::fixtures::install(&mut fixtures).unwrap();
	let mut context = Context::with_default_modules().unwrap();
	context.install(polars).unwrap();
	context.install(fixtures).unwrap();
	let runtime = Arc::new(context.runtime().unwrap());
	let mut sources = Sources::new();
	sources.insert(Source::memory(script).unwrap()).unwrap();
	let mut diagnostics = rune::Diagnostics::new();
	let built = rune::prepare(&mut sources)
		.with_context(&context)
		.with_diagnostics(&mut diagnostics)
		.build();
	if built.is_err() {
		panic!("diagnostics: {:?}", diagnostics.diagnostics());
	}
	let mut vm = Vm::new(runtime, Arc::new(built.unwrap()));
	vm.call(["main"], ()).map_err(|e| e.to_string())
}

fn strings(script: &str) -> Vec<String> {
	rune::from_value(run(script).unwrap()).unwrap()
}

/// Instance receivers of the generated bindings per owner: (by
/// reference, by mutable reference, by value).
fn census() -> BTreeMap<String, [usize; 3]> {
	let source = include_str!("../src/generated/functions.rs");
	let mut out: BTreeMap<String, [usize; 3]> = BTreeMap::new();
	let lines: Vec<&str> = source.lines().collect();
	for pair in lines.windows(2) {
		// methods only: protocol handlers (`CLONE`, `DEBUG_FMT`, …) are not
		// called by name, and free functions have no receiver
		let (attr, line) = (pair[0], pair[1]);
		if !attr.starts_with("#[rune::function(instance, path = ") {
			continue;
		}
		let Some(at) = line.find("(this: ") else {
			continue;
		};
		let rest = &line[at + "(this: ".len()..];
		let (kind, ty) = if let Some(t) = rest.strip_prefix("&mut ") {
			(1, t)
		} else if let Some(t) = rest.strip_prefix('&') {
			(0, t)
		} else {
			(2, rest)
		};
		let ty = ty.split([',', ')']).next().unwrap().to_string();
		out.entry(ty).or_default()[kind] += 1;
	}
	out
}

#[test]
fn the_four_value_types_are_never_consumed() {
	let c = census();
	assert_eq!(c["DataFrame"], [83, 22, 0]);
	assert_eq!(c["LazyFrame"], [75, 1, 0]);
	assert_eq!(c["W_polars_core__series__Series"], [163, 8, 0]);
	assert_eq!(c["Expr"], [118, 0, 0]);
	let total = c
		.values()
		.fold([0; 3], |t, v| [t[0] + v[0], t[1] + v[1], t[2] + v[2]]);
	assert_eq!(total, [3394, 468, 131]);
	// the four hand-written clones are not generated bindings
	assert!(!include_str!("../src/generated/functions.rs").contains("path = clone)"));
}

#[test]
fn every_consuming_receiver_is_marked_and_nothing_else_is() {
	use rnx_polars::generated::catalogue::{CATALOGUE, CONSUMING};
	assert_eq!(CONSUMING.len(), 131, "one path per by-value binding");
	// the same methods as the census's by-value bindings
	let source = include_str!("../src/generated/functions.rs");
	let lines: Vec<&str> = source.lines().collect();
	let mut by_value: Vec<&str> = lines
		.windows(2)
		.filter(|w| {
			w[0].starts_with("#[rune::function(instance, path = ")
				&& w[1].contains("(this: ")
				&& !w[1].contains("(this: &")
		})
		.map(|w| w[0].split("path = ").nth(1).unwrap().trim_end_matches(")]"))
		.collect();
	let mut named: Vec<&str> = CONSUMING
		.iter()
		.map(|(k, _)| k.rsplit("::").next().unwrap())
		.collect();
	by_value.sort();
	named.sort();
	assert_eq!(by_value, named);
	let mut m = Module::with_crate("polars").unwrap();
	let catalogue: BTreeMap<String, &str> =
		rnx_polars::build(&mut m).unwrap().into_iter().collect();
	let marker = "(consumes the receiver";
	assert!(
		CONSUMING.windows(2).all(|w| w[0].0 < w[1].0),
		"sorted for the binary search"
	);
	for (k, marked) in CONSUMING {
		let (_, text) = CATALOGUE
			.iter()
			.find(|(c, _)| c == k)
			.expect("a catalogue entry");
		assert_eq!(
			*marked,
			format!("{text} {marker}: a later use of it is an access error)")
		);
		assert_eq!(catalogue[*k], *marked, "{k} is marked in the catalogue");
	}
	let marked = catalogue.values().filter(|v| v.contains(marker)).count();
	assert_eq!(marked, CONSUMING.len());
	// `&mut`, not by value: mutated, never consumed
	assert!(!catalogue["polars::SeriesBuilder::freeze_reset"].contains(marker));
}

const DESC: &str = "polars::SortMultipleOptions::default_().with_order_descending(true)";

#[test]
fn an_alias_sees_an_in_place_sort_and_a_clone_does_not() {
	let alias = strings(&format!(
		"pub fn main() {{ let a = fx::df(); let b = a; b.sort_in_place([\"x\"], {DESC}).unwrap(); [polars::oracle_repr(a).unwrap(), polars::oracle_repr(b).unwrap()] }}"
	));
	let cloned = strings(&format!(
		"pub fn main() {{ let a = fx::df(); let before = polars::oracle_repr(a).unwrap(); let b = a.clone(); b.sort_in_place([\"x\"], {DESC}).unwrap(); [before, polars::oracle_repr(a).unwrap(), polars::oracle_repr(b).unwrap()] }}"
	));
	let original = &cloned[0];
	assert_ne!(original, &cloned[2], "the sort changed the data");
	assert_eq!(
		&cloned[1], original,
		"the original is untouched by its clone's sort"
	);
	assert_eq!(alias[0], alias[1], "both names see the alias's sort");
	assert_eq!(alias[1], cloned[2], "the alias saw the same sort");
	// and the reverse: sorting the original leaves an earlier clone alone
	let reverse = strings(&format!(
		"pub fn main() {{ let a = fx::df(); let b = a.clone(); a.sort_in_place([\"x\"], {DESC}).unwrap(); [polars::oracle_repr(a).unwrap(), polars::oracle_repr(b).unwrap()] }}"
	));
	assert_eq!(&reverse[1], original);
	assert_eq!(reverse[0], cloned[2]);
}

#[test]
fn an_alias_sees_an_in_place_append_and_a_clone_does_not() {
	let v = strings(
		"pub fn main() { let a = fx::series_i64(); let b = a.clone(); b.append(fx::series_i64()).unwrap(); let c = a; c.append(fx::series_i64()).unwrap(); [polars::oracle_repr(a).unwrap(), polars::oracle_repr(b).unwrap(), polars::oracle_repr(c).unwrap(), `${a.len().unwrap()} ${b.len().unwrap()} ${c.len().unwrap()}`] }",
	);
	// b (the clone) and c (an alias of a) each appended once; a sees c's
	assert_eq!(v[3], "6 6 6");
	assert_eq!(v[0], v[2]);
	let fresh = strings(
		"pub fn main() { let a = fx::series_i64(); let b = a.clone(); b.append(fx::series_i64()).unwrap(); [polars::oracle_repr(a).unwrap(), `${a.len().unwrap()} ${b.len().unwrap()}`] }",
	);
	assert_eq!(
		fresh[1], "3 6",
		"the clone's append left the original at three"
	);
	assert_eq!(
		fresh[0],
		strings("pub fn main() { [polars::oracle_repr(fx::series_i64()).unwrap()] }")[0]
	);
}

#[test]
fn a_lazy_frame_and_an_expression_serve_repeated_chains() {
	// the same chain twice on one value, on its clone, and after the one
	// in-place LazyFrame method (`collect_schema`) on the clone
	let v = strings(
		"pub fn main() {
			let e = fx::expr();
			let lf = fx::lf();
			let view = |plan, ex| polars::oracle_repr(plan.select_([ex.alias(\"a\"), ex.alias(\"b\")]).unwrap()).unwrap();
			let first = view(lf, e);
			let second = view(lf, e);
			let copy = lf.clone();
			let schema = `${copy.collect_schema().unwrap()}`;
			[first, second, view(copy, e.clone()), view(lf, e), polars::oracle_repr(lf).unwrap(), polars::oracle_repr(fx::lf()).unwrap(), schema, `${lf.collect_schema().unwrap()}`]
		}",
	);
	assert_eq!(v[0], v[1]);
	assert_eq!(v[0], v[2]);
	assert_eq!(v[0], v[3]);
	assert_eq!(v[4], v[5], "the plan is unchanged after every view");
	assert_eq!(v[6], v[7], "the clone's schema is the original's");
}

#[test]
fn a_consumed_builder_is_an_access_error_on_second_use() {
	let first =
		run("pub fn main() { let j = fx::lf().join_builder(); let done = j.with(fx::lf()); done }");
	assert!(first.is_ok());
	let second = run(
		"pub fn main() { let j = fx::lf().join_builder(); let done = j.with(fx::lf()); j.with(fx::lf()) }",
	)
	.unwrap_err();
	eprintln!("second use: {second}");
	// Rune's own access error for a taken value, shown and not changed
	assert!(second.starts_with("Cannot take, value is"), "{second}");
}

#[test]
fn the_clone_protocol_and_the_method_agree() {
	// record 0111 registered Polars' Clone as Rune's CLONE protocol, reached
	// by `std::clone::clone(v)`; the method form is this record's
	let v = strings(&format!(
		"pub fn main() {{ let a = fx::df(); let p = std::clone::clone(a); let m = a.clone(); p.sort_in_place([\"x\"], {DESC}).unwrap(); m.sort_in_place([\"x\"], {DESC}).unwrap(); [polars::oracle_repr(a).unwrap(), polars::oracle_repr(p).unwrap(), polars::oracle_repr(m).unwrap(), polars::oracle_repr(fx::df()).unwrap()] }}"
	));
	assert_eq!(v[0], v[3], "neither copy's sort reached the original");
	assert_eq!(v[1], v[2]);
	assert_ne!(v[1], v[0]);
}
