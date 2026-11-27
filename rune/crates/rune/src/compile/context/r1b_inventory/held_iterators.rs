//! Join installed trait implementations and method specializations to the
//! execution receiver universe. Compile the same held call against both VMs.
use super::*;
use crate::experimental_core::{
	execute::Executor,
	heap::{Limits, Store},
};
use crate::runtime::Protocol;

/// ADR 0264 requires overflow before yielding the maximum of an open range.
struct TerminalCase {
	owner: Hash,
	literal: &'static str,
	stock_release_value: Option<&'static str>,
}

fn compare(context: &Context, source: &str, terminal: Option<&TerminalCase>) -> bool {
	let mut sources = Sources::new();
	sources.insert(Source::memory(source).unwrap()).unwrap();
	let mut vm = crate::prepare(&mut sources)
		.with_context(context)
		.build_vm()
		.unwrap_or_else(|e| panic!("{source}: {e:?}"));
	let program = match frontend::compile(&mut sources, context, &mut Diagnostics::new(), 1) {
		Ok(p) => p,
		Err(e) if e.refusal.is_some() => return false,
		Err(e) => panic!("{source}: {e:?}"),
	};
	let mut stock = || {
		vm.call(["main"], ())
			.map(|v| {
				crate::from_value::<crate::alloc::String>(v)
					.unwrap()
					.to_string()
			})
			.map_err(|e| e.to_string())
	};
	let expected = if let Some(case) = terminal {
		use std::panic::{catch_unwind, AssertUnwindSafe};
		let observed = catch_unwind(AssertUnwindSafe(stock)).unwrap_or_else(|panic| {
			let message = panic
				.downcast_ref::<&str>()
				.copied()
				.or_else(|| panic.downcast_ref::<String>().map(String::as_str))
				.expect("stock panic text");
			Err(format!("Rust panic: {message}"))
		});
		let pinned = match case.stock_release_value {
			Some(value) if !cfg!(debug_assertions) => Ok(value.to_owned()),
			Some(_) => Err("Rust panic: attempt to add with overflow".to_owned()),
			None => Err("Rust panic: overflow in `Step::forward`".to_owned()),
		};
		assert_eq!(observed, pinned, "ADR 0264 stock witness: {source}");
		observed
	} else {
		stock()
	};
	let mut store = Store::new(Limits::default()).unwrap();
	let mut executor = Executor::new().unwrap();
	executor
		.set_host_runtime(Some(context.execution_runtime().unwrap()))
		.unwrap();
	let actual = match executor.invoke(
		&program,
		&mut store,
		program.function(Hash::type_hash(["main"])).unwrap(),
		&[],
		100000,
	) {
		Ok(root) => {
			let text = store.root_text(&root).unwrap().to_string();
			store.root_release(root).unwrap();
			Ok(text)
		}
		Err(failure) => Err(failure.message(&program).to_string()),
	};
	if terminal.is_some() {
		assert_eq!(
			actual,
			Err("Numerical overflow".to_owned()),
			"ADR 0264: {source}"
		);
		assert_ne!(actual, expected, "stale ADR 0264 difference: {source}");
	} else {
		assert_eq!(actual, expected, "{source}");
	}
	assert_eq!(
		(store.live_entries(), store.payload_bytes()),
		(0, 0),
		"{source}"
	);
	true
}

#[test]
fn installed_methods_on_held_iterators_agree_or_refuse() {
	let context = Context::with_default_modules().unwrap();
	let iterator = Hash::type_hash(crate::item!(::std::iter::Iterator));
	let owners: BTreeSet<Hash> = crate::function::methods::RECEIVERS
		.iter()
		.copied()
		.filter(|h| {
			context
				.implemented_traits
				.get(h)
				.is_some_and(|ts| ts.contains(&iterator))
		})
		.collect();
	use crate::runtime::range_from::RangeFromIter;
	let terminal = [
		TerminalCase {
			owner: RangeFromIter::<i64>::HASH,
			literal: "9223372036854775807",
			stock_release_value: Some("Some(9223372036854775807)/Some(-9223372036854775808)"),
		},
		TerminalCase {
			owner: RangeFromIter::<u64>::HASH,
			literal: "18446744073709551615u64",
			stock_release_value: Some("Some(18446744073709551615)/Some(0)"),
		},
		TerminalCase {
			owner: RangeFromIter::<char>::HASH,
			literal: r#"'\u{10ffff}'"#,
			stock_release_value: None,
		},
	];
	let mut checked_terminal = BTreeSet::new();
	let mut pairs = BTreeSet::new();
	let mut totals = BTreeMap::new();
	for &owner in &owners {
		let mut receiver =
			receiver(&context, owner).expect("held iterator requires a source witness");
		match receiver.name.as_str() {
			"::std::iter::Skip" => receiver.source = "\"abc\".chars().skip(1)".into(),
			"::std::iter::Take" => receiver.source = "\"abc\".chars().take(2)".into(),
			_ => (),
		}
		let mut types = Sources::new();
		types
			.insert(Source::memory(format!("pub fn main(){{{}}}", receiver.source)).unwrap())
			.unwrap();
		let v = crate::prepare(&mut types)
			.with_context(&context)
			.build_vm()
			.unwrap()
			.call(["main"], ())
			.unwrap();
		assert_eq!(v.type_hash(), owner, "{}", receiver.source);
		for meta in &context.meta {
			let Kind::Function {
				container: Some(hash),
				associated: Some(associated),
				signature,
				parameter_types,
				..
			} = &meta.kind
			else {
				continue;
			};
			if *hash != owner || !context.functions.contains_key(&meta.hash) {
				continue;
			}
			assert!(
				pairs.insert((owner, meta.hash)),
				"duplicate specialization {:?}",
				meta.hash
			);
			let name = match associated {
				AssociatedKind::Instance(n) => n.as_ref(),
				AssociatedKind::Protocol(p) => p.method.unwrap_or(p.name),
				_ => panic!("unexpected iterator registration {:?}", meta.hash),
			};
			let mut args = arguments(&context, signature, Some(&receiver), name);
			if name == "all" {
				args = vec!["|x|false".into()];
			}
			let expr = if matches!(associated,AssociatedKind::Protocol(p) if p.hash == Protocol::INTO_ITER.hash)
			{
				"{let entered=false;for x in it{entered=true;break;} entered}".to_owned()
			} else {
				expression(
					&receiver,
					associated,
					name,
					&args,
					&generic(&context, parameter_types),
					"it",
				)
			};
			// Successful iterator-valued calls are observed by advancing the
			// result, without comparing opaque handle text or object addresses.
			let observation=if in_names(name,"clone into_iter rev chain enumerate filter filter_map flat_map map peekable skip take") {
				"value.next()"
			} else {"value"};
			// A complete reduction of an open integer range has no finite
			// success witness. Its moved receiver gives a terminating error
			// at this same installed operation, including collect::<Vec>.
			let drop = if receiver.name == "::std::ops::RangeFromIter"
				&& in_names(name, "count collect fold reduce sum product")
			{
				"drop(it);"
			} else {
				""
			};
			let source=format!("pub fn main(){{let it={};{drop}let value={expr};format!(\"{{:?}}\",{observation})}}",receiver.source);
			let admitted = compare(&context, &source, None);
			*totals.entry(admitted).or_insert(0usize) += 1;
			if let Some(case) = terminal.iter().find(|case| {
				case.owner == owner
					&& meta.hash == Hash::associated_function(owner, Hash::ident("next"))
			}) {
				assert!(admitted, "finite next prefix must run: {source}");
				let edge = format!(
					r#"pub fn main(){{let it=({}..).iter();let a=it.next();let b=it.next();format!("{{:?}}/{{:?}}",a,b)}}"#,
					case.literal
				);
				assert!(
					compare(&context, &edge, Some(case)),
					"terminal next must run"
				);
				assert!(checked_terminal.insert(owner));
			}
		}
	}
	assert_eq!(
		checked_terminal.len(),
		terminal.len(),
		"unvisited ADR 0264 entry"
	);
	assert!(!owners.is_empty() && totals.contains_key(&true) && totals.contains_key(&false));
	std::eprintln!(
		"held iterator owners={} registrations={} admitted={} refused={}",
		owners.len(),
		pairs.len(),
		totals[&true],
		totals[&false]
	);
}
