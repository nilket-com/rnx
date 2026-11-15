//! Same installed API through both runtimes, including a legacy-construction
//! counter with a positive legacy control. No production routing fallback.
use super::*;
use rune::experimental_core::{
	execute::Executor,
	frontend,
	heap::{Limits, Store, ViewKind, ViewLimits},
	value::{Root, Scalar},
};

fn context() -> rune::Context {
	let mut context = rune::Context::with_default_modules().unwrap();
	install(&mut context).unwrap();
	context
}
fn execute(context: &rune::Context, source: &str) -> (Store, Root) {
	let mut sources = rune::Sources::new();
	sources
		.insert(rune::Source::memory(source).unwrap())
		.unwrap();
	let program =
		frontend::compile(&mut sources, context, &mut rune::Diagnostics::new(), 1).unwrap();
	let mut executor = Executor::new().unwrap();
	executor
		.set_host_runtime(Some(context.execution_runtime().unwrap()))
		.unwrap();
	let mut store = Store::new(Limits::default()).unwrap();
	let output = executor
		.invoke(
			&program,
			&mut store,
			program.function(rune::Hash::type_hash(["main"])).unwrap(),
			&[],
			2_000_000,
		)
		.unwrap();
	assert!(executor.is_idle());
	(store, output)
}
fn legacy(context: &rune::Context, source: &str) -> Value {
	let unit = crate::compile(context, source).unwrap();
	let mut vm = rune::Vm::new(
		rune::sync::Arc::try_new(context.runtime().unwrap()).unwrap(),
		rune::sync::Arc::try_new(unit).unwrap(),
	);
	vm.call(["main"], ()).unwrap()
}
fn parse_source(document: &str, body: &str) -> String {
	format!(
		"pub fn main() {{ let parsed = json::parse({}); {body} }}",
		serde_json::to_string(document).unwrap()
	)
}
fn zero_legacy_nodes() {
	LEGACY_DOCUMENT_NODES.with(|n| n.set(0));
}
fn legacy_nodes() -> usize {
	LEGACY_DOCUMENT_NODES.with(std::cell::Cell::get)
}

#[test]
fn direct_parser_matches_legacy_values_numbers_and_error_text_without_legacy_nodes() {
	let context = context();
	let mut documents: Vec<String> = [
		"null",
		"true",
		"false",
		"0",
		"-9223372036854775808",
		"9223372036854775807",
		"9223372036854775808",
		"18446744073709551615",
		"18446744073709551616",
		"-0",
		"-0.0",
		"1.5",
		"1e2",
		"[]",
		"{}",
		"[1,null,true,18446744073709551615,-0.0]",
		r#"{"z":[{"a":null}],"a":1,"a":2,"\u0061":3,"é":"🦀","a\u0000":4}"#,
		r#""\ud83e\udd80""#,
		r#""\u0000\n\t\"\\""#,
		"1e400",
		"1 2",
		"{",
		"nul",
		"[1,]",
		r#""\ud800""#,
		r#""\udc00""#,
		r#""\x""#,
		"[\n1,\n]",
	]
	.into_iter()
	.map(str::to_owned)
	.collect();
	for (open, close) in [("[", "]"), ("{\"a\":", "}")] {
		for depth in [127, 128, 2048] {
			documents.push(format!("{}0{}", open.repeat(depth), close.repeat(depth)));
		}
	}
	for document in documents {
		// Debug output shares the established formatter; the script wraps every
		// parse error into a normal string so it can be compared in full.
		let source = parse_source(
			&document,
			"match parsed { Ok(v) => json::stringify(v), Err(e) => Err(e) }",
		);
		zero_legacy_nodes();
		let (store, output) = execute(&context, &source);
		assert_eq!(
			legacy_nodes(),
			0,
			"execution built legacy nodes: {document}"
		);
		let view = store
			.root_view(
				&output,
				ViewLimits {
					nodes: usize::MAX,
					depth: usize::MAX,
				},
			)
			.unwrap();
		let node = view.root().unwrap();
		let ViewKind::Result { ok } = node.kind().unwrap() else {
			panic!("lost result");
		};
		let ViewKind::String(text) = node.child(0).unwrap().kind().unwrap() else {
			panic!("lost text");
		};
		let observed = (ok, text.to_owned());
		let old = legacy(&context, &source);
		let result = old.borrow_ref::<Result<Value, Value>>().unwrap();
		let expected = match &*result {
			Ok(v) => (true, v.borrow_string_ref().unwrap().to_string()),
			Err(v) => (false, v.borrow_string_ref().unwrap().to_string()),
		};
		assert_eq!(observed, expected, "{document}");
		if parse_document(&document).is_ok() {
			assert!(legacy_nodes() > 0, "counter has no teeth");
		}
		let mut store = store;
		store.root_release(output).unwrap();
		assert_eq!((store.live_entries(), store.payload_bytes()), (0, 0));
	}
}

#[test]
fn direct_parser_preserves_number_bits_and_sorted_object_iteration() {
	let context = context();
	for (document, expected) in [
		("0", Scalar::I64(0)),
		("9223372036854775808", Scalar::U64(1u64 << 63)),
		("18446744073709551615", Scalar::U64(u64::MAX)),
		("-9223372036854775808", Scalar::I64(i64::MIN)),
		("-0", Scalar::F64(-0.0)),
		("-0.0", Scalar::F64(-0.0)),
		("1.5", Scalar::F64(1.5)),
	] {
		let (mut store, root) = execute(&context, &parse_source(document, "parsed?"));
		let scalar = store.root_scalar(&root).unwrap().unwrap();
		match (scalar, expected) {
			(Scalar::F64(a), Scalar::F64(b)) => assert_eq!(a.to_bits(), b.to_bits()),
			(a, b) => assert_eq!(a, b),
		}
		store.root_release(root).unwrap();
	}
	let document = r#"{"z":0,"a":1,"é":2,"a\u0000":3,"a":4}"#;
	let source = parse_source(
		document,
		"let keys=[]; for (key, _) in parsed? { keys.push(key); } json::stringify(keys)?",
	);
	let (mut store, root) = execute(&context, &source);
	let expected = "[\"a\",\"a\\u0000\",\"z\",\"é\"]";
	assert_eq!(store.root_text(&root).unwrap(), expected);
	assert_eq!(
		legacy(&context, &source)
			.borrow_string_ref()
			.unwrap()
			.as_ref(),
		expected
	);
	store.root_release(root).unwrap();
}

#[test]
fn direct_parser_nesting_boundary_on_two_mib_stack() {
	std::thread::Builder::new()
		.stack_size(2 * 1024 * 1024)
		.spawn(|| {
			let context = context();
			for (open, close) in [("[", "]"), ("{\"a\":", "}")] {
				for depth in [127, 128] {
					let text = format!("{}0{}", open.repeat(depth), close.repeat(depth));
					let source =
						parse_source(&text, "match parsed { Ok(_) => true, Err(_) => false }");
					let (mut store, root) = execute(&context, &source);
					assert_eq!(
						store.root_scalar(&root).unwrap(),
						Some(Scalar::Bool(depth == 127))
					);
					assert_eq!(legacy(&context, &source).as_bool().unwrap(), depth == 127);
					store.root_release(root).unwrap();
					assert_eq!((store.live_entries(), store.payload_bytes()), (0, 0));
				}
			}
		})
		.unwrap()
		.join()
		.unwrap();
}
