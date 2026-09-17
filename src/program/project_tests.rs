use super::*;
use rune::{Context, Diagnostics, Vm};
use std::sync::{
	Arc,
	atomic::{AtomicUsize, Ordering},
};

struct Tree(PathBuf);
impl Tree {
	fn new() -> Self {
		static NEXT: AtomicUsize = AtomicUsize::new(0);
		let root = std::env::temp_dir().join(format!(
			"rnx-mounts-{}-{}",
			std::process::id(),
			NEXT.fetch_add(1, Ordering::Relaxed)
		));
		std::fs::create_dir_all(&root).unwrap();
		Self(root.canonicalize().unwrap())
	}
	fn write(&self, path: &str, text: &str) -> PathBuf {
		let path = self.0.join(path);
		std::fs::create_dir_all(path.parent().unwrap()).unwrap();
		std::fs::write(&path, text).unwrap();
		path
	}
	fn loader(&self, entries: &[(&str, &str)]) -> Loader {
		Loader::with_mounts(
			Mounts::new(
				entries
					.iter()
					.map(|(prefix, path)| {
						(
							prefix.split("::").map(String::from).collect(),
							self.0.join(path),
						)
					})
					.collect(),
			)
			.unwrap(),
		)
	}
}
impl Drop for Tree {
	fn drop(&mut self) {
		std::fs::remove_dir_all(&self.0).unwrap();
	}
}

fn compile(
	entry: &Path,
	loader: &mut Loader,
) -> (Sources, Result<rune::Unit, String>, Diagnostics) {
	let mut sources = Sources::new();
	sources.insert(loader.entry(entry).unwrap()).unwrap();
	let context = Context::with_default_modules().unwrap();
	let mut diagnostics = Diagnostics::new();
	let unit = rune::prepare(&mut sources)
		.with_context(&context)
		.with_source_loader(loader)
		.with_diagnostics(&mut diagnostics)
		.build()
		.map_err(|e| e.to_string());
	(sources, unit, diagnostics)
}
fn value(entry: &Path, loader: &mut Loader) -> rune::Value {
	let (_, unit, diagnostics) = compile(entry, loader);
	let context = Context::with_default_modules().unwrap();
	Vm::new(
		Arc::new(context.runtime().unwrap()),
		Arc::new(unit.unwrap_or_else(|e| panic!("{e}: {diagnostics:?}"))),
	)
	.call(["main"], ())
	.unwrap()
}
fn number(entry: &Path, loader: &mut Loader) -> i64 {
	rune::from_value(value(entry, loader)).unwrap()
}

#[test]
fn external_nested_inline_renamed_and_ancestry() {
	let t = Tree::new();
	t.write("pkg/mod.rn", "pub mod nested; pub mod inline { pub mod leaf; } pub fn marker() { 7 } pub fn value() { self::nested::value() + self::inline::leaf::value() + super::marker() + crate::marker() }");
	t.write(
		"pkg/nested.rn",
		"pub mod deep; pub fn value() { self::deep::value() + super::marker() }",
	);
	t.write("pkg/nested/deep.rn", "pub fn value() { 10 }");
	t.write("pkg/inline/leaf.rn", "pub fn value() { 100 }");
	for alias in ["atlas", "renamed"] {
		let entry = t.write(
			"app/main.rn",
			&format!(
				"pub mod {alias}; pub fn marker() {{ 99 }} pub fn main() {{ {alias}::value() }}"
			),
		);
		assert_eq!(number(&entry, &mut t.loader(&[(alias, "pkg")])), 315);
	}
	t.write("pkg/nested/deep/mod.rn", "pub fn value() { 20 }");
	assert_eq!(
		number(
			&t.0.join("app/main.rn"),
			&mut t.loader(&[("renamed", "pkg")])
		),
		325
	);
}

#[test]
fn transitive_mount_wins_and_missing_never_falls_back() {
	let t = Tree::new();
	let entry = t.write(
		"app/main.rn",
		"pub mod left; pub fn main() { left::value() }",
	);
	t.write("app/left.rn", "pub fn value() { 999 }");
	t.write(
		"left/mod.rn",
		"pub mod codec; pub fn value() { self::codec::value() }",
	);
	t.write("left/codec.rn", "pub fn value() { 888 }");
	t.write(
		"codec/mod.rn",
		"pub mod deep; pub fn value() { self::deep::value() }",
	);
	t.write("deep/mod.rn", "pub fn value() { 42 }");
	let mounts = [
		("left", "left"),
		("left::codec", "codec"),
		("left::codec::deep", "deep"),
	];
	assert_eq!(number(&entry, &mut t.loader(&mounts)), 42);
	std::fs::remove_file(t.0.join("codec/mod.rn")).unwrap();
	let (_, unit, errors) = compile(&entry, &mut t.loader(&mounts));
	assert!(unit.is_err());
	assert!(format!("{errors:?}").contains("codec/mod.rn"));
}

#[test]
fn diamonds_have_distinct_types_and_charge_both_reads() {
	let t = Tree::new();
	let entry = t.write(
		"app/main.rn",
		"pub mod left; pub mod right; pub fn main() { [left::make(), right::make()] }",
	);
	for dir in ["left", "right"] {
		t.write(
			&format!("{dir}/mod.rn"),
			"pub mod shared; pub fn make() { self::shared::Thing { field: 42 } }",
		);
	}
	let shared = t.write("shared/mod.rn", "pub struct Thing { field }");
	let mounts = [
		("left", "left"),
		("right", "right"),
		("left::shared", "shared"),
		("right::shared", "shared"),
	];
	let mut loader = t.loader(&mounts);
	let result = value(&entry, &mut loader);
	let mut fields = crate::declared::Fields::default();
	for text in &loader.texts {
		crate::declared::into_fields(&text.text, &mut fields);
	}
	let rendered = crate::format::render_complete(&result, Some(&fields)).unwrap();
	assert!(rendered.contains("field: 42"), "{rendered}");
	let values = rune::from_value::<Vec<rune::Value>>(result).unwrap();
	assert_ne!(values[0].type_hash(), values[1].type_hash());
	assert_eq!(loader.texts.iter().filter(|t| t.path == shared).count(), 2);
	let total = loader.texts.iter().map(|t| t.text.len()).sum::<usize>();
	let mut exact = t.loader(&mounts);
	exact.remaining = total;
	exact.limit = total;
	assert!(compile(&entry, &mut exact).1.is_ok());
	assert_eq!(exact.remaining, 0);
	let mut short = t.loader(&mounts);
	short.remaining = total - 1;
	short.limit = total - 1;
	assert!(compile(&entry, &mut short).1.is_err());
	assert!(short.exhausted);
	let opens = short.opens;
	assert!(
		short
			.load(
				&entry,
				&rune::ItemBuf::with_item(["right", "shared"]).unwrap(),
				&rune::ast::Span::empty()
			)
			.is_err()
	);
	assert!(short.entry(&entry).is_err());
	assert_eq!(opens, short.opens);
}

#[test]
fn undeclared_mapping_does_not_load_and_inline_stays_inline() {
	let t = Tree::new();
	let entry = t.write("main.rn", "pub fn main() { mapped::value() }");
	t.write("pkg/mod.rn", "pub fn value() { 42 }");
	let mut loader = t.loader(&[("mapped", "pkg")]);
	assert!(compile(&entry, &mut loader).1.is_err());
	assert_eq!(loader.opens, 1);
	t.write(
		"main.rn",
		"pub mod mapped { pub fn value() { 7 } } pub fn main() { mapped::value() }",
	);
	let mut loader = t.loader(&[("mapped", "pkg")]);
	assert_eq!(number(&entry, &mut loader), 7);
	assert_eq!(loader.opens, 1);
}

#[test]
fn faults_and_named_fields_keep_the_dependency_source() {
	let t = Tree::new();
	let entry = t.write(
		"main.rn",
		"pub mod pkg; pub fn main() { pkg::nested::value() }",
	);
	t.write("pkg/mod.rn", "pub mod nested;");
	let fault = t.write("pkg/nested.rn", "pub fn value() {\n  1.missing()\n}");
	let mut loader = t.loader(&[("pkg", "pkg")]);
	let (sources, unit, errors) = compile(&entry, &mut loader);
	let unit = Arc::new(unit.unwrap_or_else(|e| panic!("{e}: {errors:?}")));
	let context = Context::with_default_modules().unwrap();
	let error = Vm::new(Arc::new(context.runtime().unwrap()), unit)
		.call(["main"], ())
		.unwrap_err();
	let location = error.first_location().unwrap();
	let inst = location
		.unit
		.debug_info()
		.unwrap()
		.instruction_at(location.ip)
		.unwrap();
	let text = loader.get(&sources, inst.source_id).unwrap();
	assert_eq!(text.path, fault);
	assert_eq!(
		crate::session::position(&text.text, inst.span.range().start).0,
		2
	);
	let named = crate::method::named(&error.to_string(), &text.text).unwrap();
	assert!(named.contains("missing"), "{named}");
	t.write("pkg/nested.rn", "pub fn value() {\n let x = ;\n}");
	let mut loader = t.loader(&[("pkg", "pkg")]);
	let (sources, result, errors) = compile(&entry, &mut loader);
	assert!(result.is_err());
	let rune::diagnostics::Diagnostic::Fatal(fatal) = &errors.diagnostics()[0] else {
		panic!()
	};
	assert_eq!(loader.get(&sources, fatal.source_id()).unwrap().path, fault);
	t.write(
		"pkg/nested.rn",
		"pub enum E { Named { answer } } pub fn value() { E::Named { answer: 42 } }",
	);
	let mut loader = t.loader(&[("pkg", "pkg")]);
	let result = value(&entry, &mut loader);
	let mut fields = crate::declared::Fields::default();
	for text in &loader.texts {
		crate::declared::into_fields(&text.text, &mut fields);
	}
	let rendered = crate::format::render_complete(&result, Some(&fields)).unwrap();
	assert!(rendered.contains("answer: 42"), "{rendered}");
}

#[test]
fn mount_table_limits_and_component_boundaries() {
	let t = Tree::new();
	assert!(Mounts::new(vec![(vec!["x".into()], PathBuf::from("relative"))]).is_err());
	assert!(Mounts::new(vec![(vec![], t.0.clone())]).is_err());
	assert!(Mounts::new(vec![(vec!["x".into(); 17], t.0.clone())]).is_err());
	assert!(Mounts::new(vec![(vec!["x".into()], t.0.clone()); 2]).is_err());
	let entries: Vec<_> = (0..256)
		.map(|i| (vec![format!("a{i}")], t.0.clone()))
		.collect();
	assert!(Mounts::new(entries.clone()).is_ok());
	let mut over = entries;
	over.push((vec!["extra".into()], t.0.clone()));
	assert!(Mounts::new(over).is_err());
	for alias in [
		"../x", "x::y", "self", "super", "crate", "", "fn", " a", "a ",
	] {
		assert!(
			Mounts::new(vec![(vec![alias.into()], t.0.clone())]).is_err(),
			"{alias}"
		);
	}
	let mounts = Mounts::new(vec![(vec!["a".into()], t.0.clone())]).unwrap();
	assert!(
		mounts
			.resolve(&rune::ItemBuf::with_item(["ab"]).unwrap())
			.is_none()
	);
}

#[test]
fn real_allowance_counts_each_alias_and_stops_before_a_later_open() {
	let t = Tree::new();
	let entry = t.write(
		"main.rn",
		"pub mod left; pub mod right; pub fn main() { left::value() + right::value() }",
	);
	let shared = t.write(
		"pkg/mod.rn",
		&format!(
			"pub fn value() {{ 1 }} /*{}*/",
			" ".repeat(SOURCE_ALLOWANCE / 2)
		),
	);
	assert!(std::fs::metadata(&shared).unwrap().len() < SOURCE_ALLOWANCE as u64);
	let mut loader = t.loader(&[("left", "pkg"), ("right", "pkg")]);
	let (_, result, errors) = compile(&entry, &mut loader);
	assert!(result.is_err());
	assert!(loader.exhausted);
	assert!(format!("{errors:?}").contains("8388608 bytes"));
	assert_eq!(loader.opens, 3);
	let opens = loader.opens;
	assert!(
		loader
			.load(
				&entry,
				&rune::ItemBuf::with_item(["left"]).unwrap(),
				&rune::ast::Span::empty()
			)
			.is_err()
	);
	assert_eq!(loader.opens, opens);
}
