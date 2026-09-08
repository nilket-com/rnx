//! Text helpers, each earned by the first port and none of them speculative.
//!
//! These are pure functions of their arguments, which is why they sit beside
//! `host::` rather than inside it: a reader can tell at a glance which calls
//! can fail because of the world and which cannot. They exist only because
//! upstream Rune does not have them, and each goes when upstream offers the
//! same behaviour, not merely the same name.
use crate::host::HostFunction;
use rune::{Context, Module};

/// The byte index of the first occurrence of `needle`, as Rust's
/// `str::find` reports it: an empty needle is found at the start, and a
/// needle longer than the haystack is not found. The index is a byte index,
/// so it can be handed straight back to a range.
fn find(haystack: &str, needle: &str) -> Option<i64> {
	haystack.find(needle).map(|at| at as i64)
}

/// Split on runs of whitespace into at most `count` fields, the last
/// keeping the rest of the text verbatim. Counts fields, where Python
/// counts splits, so a count of three is Python's `split(None, 2)`.
///
/// A negative count is an error rather than a stand-in for unlimited. Zero
/// asks for no fields and gets none. One removes leading whitespace and
/// keeps everything after it, trailing whitespace included. Text that is
/// empty or all whitespace gives no fields at any count.
fn split_max(text: &str, count: i64) -> Result<Vec<String>, String> {
	if count < 0 {
		return Err(format!(
			"split_max needs a count of zero or more, not {count}"
		));
	}
	let mut fields = Vec::new();
	if count == 0 {
		return Ok(fields);
	}
	let mut rest = text.trim_start();
	while !rest.is_empty() {
		// The last field the count allows keeps the remainder verbatim,
		// trailing whitespace and all.
		if fields.len() as i64 == count - 1 {
			fields.push(rest.to_owned());
			break;
		}
		match rest.find(char::is_whitespace) {
			Some(at) => {
				fields.push(rest[..at].to_owned());
				rest = rest[at..].trim_start();
			}
			None => {
				fields.push(rest.to_owned());
				break;
			}
		}
	}
	Ok(fields)
}

/// An integer with a comma every three digits. The separator is fixed
/// because no caller has asked for another, and a parameter would invite a
/// locale question this does not answer. A negative number groups its
/// digits and keeps its sign outside the grouping; the magnitude is taken
/// without negating, so the smallest representable integer works like any
/// other.
fn group_digits(value: i64) -> String {
	let digits = value.unsigned_abs().to_string();
	let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
	if value < 0 {
		out.push('-');
	}
	for (index, digit) in digits.char_indices() {
		if index > 0 && (digits.len() - index) % 3 == 0 {
			out.push(',');
		}
		out.push(digit);
	}
	out
}

/// Install the text module and return every function it registered, path
/// and description recorded at the registration itself, so completion and
/// `:help` have no second list to keep in step.
pub fn install(context: &mut Context) -> super::Result<Vec<HostFunction>> {
	let mut module = Module::with_crate("text")?;
	let mut registered = Vec::new();
	macro_rules! register {
		($name:literal, $function:expr, $doc:literal) => {
			module.function($name, $function).build()?;
			registered.push(HostFunction {
				path: format!("text::{}", $name),
				doc: $doc,
			});
		};
	}
	register!(
		"find",
		find,
		"find(haystack, needle) -> Option<int>: the byte index of the first occurrence, or None"
	);
	register!(
		"split_max",
		split_max,
		"split_max(text, count) -> Result<Vec<String>>: split on whitespace into at most count fields, the last keeping the rest verbatim; Err on a negative count"
	);
	register!(
		"group_digits",
		group_digits,
		"group_digits(value) -> String: an integer with a comma every three digits"
	);
	context.install(module)?;
	Ok(registered)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn find_behaves_as_rusts_does() {
		assert_eq!(find("abcdef", "cd"), Some(2));
		assert_eq!(find("abcdef", "a"), Some(0));
		assert_eq!(find("abcdef", "f"), Some(5));
		assert_eq!(find("abcdef", "zz"), None);
		// An empty needle is found at the start, as in Rust.
		assert_eq!(find("abcdef", ""), Some(0));
		assert_eq!(find("", ""), Some(0));
		// A needle longer than the haystack is not found.
		assert_eq!(find("ab", "abc"), None);
		assert_eq!(find("", "a"), None);
	}

	#[test]
	fn find_returns_a_byte_index_a_range_accepts() {
		let haystack = "café au lait";
		let at = find(haystack, "au").unwrap() as usize;
		// The index counts bytes, so `é` occupies two of them.
		assert_eq!(at, 6);
		assert_eq!(&haystack[at..at + 2], "au");
		let at = find(haystack, "é").unwrap() as usize;
		assert_eq!(&haystack[at..at + "é".len()], "é");
	}

	#[test]
	fn split_max_follows_its_table() {
		// A negative count is an error naming it.
		let failure = split_max("a b", -1).unwrap_err();
		assert!(failure.contains("-1"), "{failure}");
		// Zero asks for no fields.
		assert_eq!(split_max("a b", 0).unwrap(), Vec::<String>::new());
		// One removes leading whitespace and keeps the rest verbatim.
		assert_eq!(split_max("  a  b  ", 1).unwrap(), vec!["a  b  "]);
		// Nothing to split gives nothing, at any count.
		assert_eq!(split_max("", 1).unwrap(), Vec::<String>::new());
		assert_eq!(split_max("   ", 1).unwrap(), Vec::<String>::new());
		assert_eq!(split_max("   ", 3).unwrap(), Vec::<String>::new());
	}

	#[test]
	fn split_max_keeps_the_remainder_verbatim() {
		// Runs of whitespace separate fields, and the remainder keeps its own.
		assert_eq!(
			split_max("1200000  ffff0001  a  symbol  ", 3).unwrap(),
			vec!["1200000", "ffff0001", "a  symbol  "]
		);
		// Fewer fields than the maximum.
		assert_eq!(split_max("a b", 3).unwrap(), vec!["a", "b"]);
		// Exactly the maximum.
		assert_eq!(split_max("a b c", 3).unwrap(), vec!["a", "b", "c"]);
		// Leading whitespace never appears in a field.
		assert_eq!(split_max("\t a b", 2).unwrap(), vec!["a", "b"]);
		// Tabs and newlines are whitespace too.
		assert_eq!(split_max("a\tb\nc", 2).unwrap(), vec!["a", "b\nc"]);
	}

	#[test]
	fn group_digits_groups_to_both_boundaries() {
		assert_eq!(group_digits(0), "0");
		assert_eq!(group_digits(7), "7");
		assert_eq!(group_digits(999), "999");
		assert_eq!(group_digits(1000), "1,000");
		assert_eq!(group_digits(1234567), "1,234,567");
		assert_eq!(group_digits(-1234567), "-1,234,567");
		assert_eq!(group_digits(-1), "-1");
		assert_eq!(group_digits(i64::MAX), "9,223,372,036,854,775,807");
		// The one value whose negation overflows, which is why the magnitude
		// is taken without negating.
		assert_eq!(group_digits(i64::MIN), "-9,223,372,036,854,775,808");
	}

	#[test]
	fn every_registered_function_carries_a_description() {
		let mut context = Context::with_default_modules().unwrap();
		let registered = install(&mut context).unwrap();
		assert_eq!(registered.len(), 3);
		for function in &registered {
			assert!(function.path.starts_with("text::"), "{}", function.path);
			assert!(!function.doc.trim().is_empty(), "{}", function.path);
			assert!(
				function
					.doc
					.starts_with(function.path.trim_start_matches("text::")),
				"{}: {}",
				function.path,
				function.doc
			);
			// Every registered path resolves in the context it was installed into.
			let source = format!("pub fn main() {{ {} }}", function.path);
			assert!(
				crate::compile(&context, &source).is_ok(),
				"{}",
				function.path
			);
		}
	}
}

#[cfg(test)]
mod description_tests {
	/// A description that names a success type where the function returns a
	/// result tells a reader they need not handle one. Every registered
	/// function that returns a result says so.
	#[test]
	fn a_description_names_a_result_when_the_function_returns_one() {
		let mut context = rune::Context::with_default_modules().unwrap();
		let mut registered = crate::host::install(&mut context).unwrap();
		registered.extend(super::install(&mut context).unwrap());
		let fallible = [
			"host::json_parse",
			"host::json_stringify",
			"host::read",
			"host::write_new",
			"host::mkdir",
			"host::absolute",
			"host::process",
			"text::split_max",
		];
		for path in fallible {
			let function = registered
				.iter()
				.find(|f| f.path == path)
				.unwrap_or_else(|| panic!("{path} is not registered"));
			assert!(
				function.doc.contains("-> Result<"),
				"{path} returns a result and its description does not say so: {}",
				function.doc
			);
		}
		// And one that does not return a result does not claim to.
		let infallible = registered
			.iter()
			.find(|f| f.path == "text::group_digits")
			.unwrap();
		assert!(!infallible.doc.contains("Result<"), "{}", infallible.doc);
	}
}
