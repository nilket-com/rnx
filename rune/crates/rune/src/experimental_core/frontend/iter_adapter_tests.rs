//! C17e: closure adapter certificates, function value arity and the
//! direct-chain boundary.
use super::*;
use crate::experimental_core::error::Refusal;
use crate::function::iter_terminal::{closure_provided, extended_next_provided, extended_provided};
use crate::function::iter_terminal::{Closure, Terminal};

fn compile(source: &str, context: &Context) -> Result<Program> {
	let mut sources = Sources::new();
	sources.insert(crate::Source::memory(source)?).unwrap();
	super::compile(&mut sources, context, &mut Diagnostics::new(), 14)
}

fn refusal(source: &str, context: &Context) -> Option<Refusal> {
	compile(source, context).err().map(|e| e.refusal.unwrap())
}

#[test]
fn default_modules_certify_every_adapter_and_consumer() {
	let context = Context::with_default_modules().unwrap();
	for closure in Closure::ALL {
		assert!(closure_provided(&context, closure), "{closure:?}");
	}
	for terminal in Terminal::ALL {
		assert!(extended_provided(&context, terminal), "{terminal:?}");
	}
	assert!(extended_next_provided(&context));
	let empty = Context::new();
	assert!(Closure::ALL
		.into_iter()
		.all(|c| !closure_provided(&empty, c)));
	assert!(!extended_next_provided(&empty));
}

#[test]
fn direct_chains_accept_stored_callbacks_and_check_known_arities() {
	let context = Context::with_default_modules().unwrap();
	for source in [
		"pub fn main(){[1,2,3].iter().map(|x|x*2).filter(|x|x>2).count()}",
		"fn g(x){x+1} pub fn main(){let f=|x|x>1;([1].iter().map(g).collect::<Vec>(),[2].iter().filter(f).count())}",
		"pub fn main(){let k=2;(0..).iter().filter_map(|x|if x>k{Some(x)}else{None}).take(2).sum::<i64>()}",
		"pub fn main(){[[1],[2]].iter().flat_map(|v|v).enumerate().collect::<Vec>()}",
		"pub fn main(){[1].iter().chain([2]).fold(0,|a,x|a+x)}",
		"pub fn main(){([1,2].iter().reduce(|a,x|a*x),[1].iter().find(|x|x>0),[1].iter().map(|x|x).all(|x|x>0))}",
		"pub fn main(){(1..=4).iter().map(|x|x).rev().next()}",
		"pub fn main(){[1,2,3].iter().take(2).rev().collect::<Vec>()}",
		"pub fn main(x){x.iter().rev().count()}",
		"pub fn main(){\"a b\".split(\" \").map(|s|s+s).collect::<Vec>()}",
		"pub fn main(){let n=0;for (i,x) in [5].iter().enumerate(){n+=x;}n}",
		"struct T(a) pub fn main(){[1].iter().map(T).count()}",
		// The callback can be supplied by the caller or selected at run time.
		"fn h(f){[1].iter().map(f).count()} pub fn main(){h(|x|x)}",
		"pub fn main(){let f=|x|x;f=|x|x+1;[1].iter().map(f).count()}",
		"pub fn main(){let o=#{f:|x|x};[1].iter().map(o.f).count()}",
	] {
		assert!(compile(source, &context).is_ok(), "{source}");
	}
	for source in [
		// Stored, passed and returned iterators, and iterator arguments.
		"pub fn main(){let it=[1].iter().map(|x|x);it.count()}",
		"fn f(it){it.count()} pub fn main(){f([1].iter().map(|x|x))}",
		"pub fn main(){[1].iter().chain([2].iter()).count()}",
		"pub fn main(){[1].iter().map(5).count()}",
		// `rev` outside its admitted shapes.
		"pub fn main(x){x.iter().map(|y|y).rev().count()}",
		"pub fn main(){[1].iter().enumerate().rev().count()}",
		"pub fn main(){\"ab\".chars().rev().count()}",
		"pub fn main(){(1..=3).iter().skip(1).rev().count()}",
		"pub fn main(){[1].iter().rev().rev().count()}",
		// Outside the admitted surface.
		"pub fn main(){[1].iter().map(|x|x).nth(0)}",
		"pub fn main(){[1].iter().map::<i64>(|x|x).count()}",
		"pub fn main(){[1].iter().map().count()}",
	] {
		assert_eq!(refusal(source, &context), Some(Refusal::Method), "{source}");
	}
	for source in [
		"pub fn main(){[1].iter().map(|a,b|a).count()}",
		"fn g(a,b){a} pub fn main(){[1].iter().filter(g).count()}",
		"pub fn main(){let f=|a|a;[].iter().fold(0,f)}",
		"pub fn main(){let k=1;[1].iter().map(move ||k).count()}",
		"struct T(a,b) pub fn main(){[1].iter().map(T).count()}",
		"pub fn main(){[1].iter().fold(|a,x|a)}",
	] {
		assert_eq!(refusal(source, &context), Some(Refusal::Arity), "{source}");
	}
	for source in [
		"pub fn main(f){[1].iter().any(f)}",
		"pub fn main(f){[1].iter().skip(0).any(f)}",
	] {
		assert!(compile(source, &context).is_ok(), "{source}");
	}
}

/// A rival registration at a C17e provider hash either fails or leaves the
/// std iterator modules uncertified.
#[test]
fn foreign_provider_registrations_fail_or_stay_uncertified() {
	let mut context = Context::with_default_modules().unwrap();
	let mut module = crate::Module::new();
	module
		.associated_function("map", |_: &crate::runtime::slice::Iter, _: i64| 0u64)
		.unwrap();
	assert!(context.install(module).is_err());
	assert!(closure_provided(&context, Closure::Map));
	// A provider name on a receiver no chain reaches closes nothing.
	let mut module = crate::Module::new();
	module
		.associated_function("fold", |this: bool| this)
		.unwrap();
	context.install(module).unwrap();
	assert!(closure_provided(&context, Closure::Fold));
	assert!(compile("pub fn main(){[1].iter().fold(0,|a,x|a+x)}", &context).is_ok());
	// A native `rev` on an iterator stock gives none closes `rev`.
	let mut context = Context::with_default_modules().unwrap();
	let mut module = crate::Module::new();
	module
		.associated_function("rev", |_: &crate::runtime::object::RuneIter| 0u64)
		.unwrap();
	context.install(module).unwrap();
	assert_eq!(
		refusal("pub fn main(){[1].iter().rev().count()}", &context),
		Some(Refusal::Method)
	);
}

#[test]
fn escaped_adapters_refuse_before_a_generic_advance() {
	let context = Context::with_default_modules().unwrap();
	for owner in [
		r#""abc".chars()"#,
		r#""a b".split(" ")"#,
		r#""a b".split(' ')"#,
		"[1,2].iter()",
		"(1..4).iter()",
	] {
		for adapter in ["skip(1)", "take(2)", "skip(1).take(2)"] {
			let cursor = std::format!("{owner}.{adapter}");
			for consume in ["CUR.next()", "{for x in CUR {} 1}"] {
				for (prefix, value) in [
					("fn make(){CURSOR}", "make()"),
					("fn pass(it){it}", "pass(CURSOR)"),
					("", "[CURSOR][0]"),
					("", "(CURSOR,).0"),
					("", "#{it:CURSOR}.it"),
					("", "{let it=CURSOR;let f=||it;f()}"),
					("", "if true{CURSOR}else{CURSOR}"),
					("", "match true {true=>CURSOR,_=>CURSOR}"),
				] {
					let body = consume.replace("CUR", value);
					let source =
						std::format!("{prefix} pub fn main(){{{body}}}").replace("CURSOR", &cursor);
					assert_eq!(
						refusal(&source, &context),
						Some(Refusal::Method),
						"{source}"
					);
				}
			}
		}
	}
}

#[test]
fn held_iterator_methods_compare_with_stock_on_dynamic_receivers() {
	use super::vector_sort_and_held::check;
	for body in [
		r#"let a="abc".chars().skip(1);a.collect::<Vec>().len() as i64"#,
		r#"let a="abc".chars().take(2);a.collect::<String>().len() as i64"#,
		"let a=(0..5).iter();a.next();size(a)",
		"let a=(1u64..=5u64).iter();a.next();size(a)",
		"let a=(3..1).iter();size(a)",
		"let a=#{x:1,y:2}.iter();a.next();size(a)",
		"let a=[1,2].iter();size(a)",
		"let a=Some([1]).iter();let c=copy(a);c.next().unwrap().push(2);a.next().unwrap().len() as i64",
		"let a=None.iter();if std::clone::clone(a).next()==None{1}else{0}",
		"let a=(0..3).iter();std::clone::clone(a);0",
		"let a=\"abc\".chars().take(0);let b=a.skip(2);b.len() as i64",
		"let a=\"abc\".chars().skip(1);a.len() as i64",
		"let a=\"abc\".chars().take(1);let b=a.skip(1);drop(a);b.len() as i64",
	] {
		check(&std::format!("fn size(x){{x.len() as i64}} fn copy(x){{x.clone()}} pub fn main(){{{body}}}"));
	}
}

#[test]
fn held_iterator_len_and_clone_keep_the_concrete_receiver_after_advance() {
	use super::vector_sort_and_held::check;
	for advance in ["", "it.next();"] {
		for receiver in [
			"(0..5).iter()",
			"(0u64..5u64).iter()",
			"(0..=5).iter()",
			"(0u64..=5u64).iter()",
			"#{x:1,y:2}.iter()",
		] {
			check(&std::format!(
				"fn size(x){{x.len()}} pub fn main(){{let it={receiver};{advance}size(it) as i64}}"
			));
		}
		for receiver in ["None.iter()", "Some(1).iter()"] {
			check(&std::format!("fn copy(x){{x.clone()}} pub fn main(){{let it={receiver};{advance}if copy(it).next()==it.next(){{1}}else{{0}}}}"));
		}
		for receiver in [
			r#""abc".chars()"#,
			r#""a,b".split(",")"#,
			r#""a,b".split(',')"#,
			r#""abc".chars().skip(1)"#,
			r#""abc".chars().take(1)"#,
			"[1,2].iter()",
			"#{x:1}.iter()",
			"(0..5).iter()",
			"(0u64..5u64).iter()",
			"('a'..'d').iter()",
			"(0..=5).iter()",
			"(0u64..=5u64).iter()",
			"('a'..='d').iter()",
			"(0..).iter()",
			"(0u64..).iter()",
			"('a'..).iter()",
		] {
			check(&std::format!(
				"pub fn main(){{let it={receiver};{advance}std::clone::clone(it);0}}"
			));
		}
	}
}
