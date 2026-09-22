//! Hand-written support for the generated bindings: the error value,
//! checked integer narrowing, and taking wrapped values out of Rune
//! containers. Not generated; lives beside the generated files because
//! only they use it.
#![allow(dead_code)]
use polars::prelude as p;
use rnx::rune;
use rune::runtime::Formatter;
use rune::Any;

/// A Polars error as a Rune value: `kind()` is the variant name Rust
/// matches on, display is Polars's message.
#[derive(Any, Debug, Clone)]
#[rune(item = ::polars)]
pub struct Error(pub(crate) String, pub(crate) String);

impl From<p::PolarsError> for Error {
	fn from(e: p::PolarsError) -> Self {
		let kind = format!("{e:?}");
		let kind = kind.split(['(', ' ']).next().unwrap_or("Unknown").to_string();
		Error(kind, e.to_string())
	}
}

/// Record 0077: iterator returns are materialized into vectors inside the
/// binding, under an item-count bound. The contract is inclusive: exactly
/// `limit` items succeed, `limit + 1` refuse with a `MaterializeLimit`
/// error and no prefix. A length is trusted only from `ExactSizeIterator`
/// (`len()`), never from a `size_hint`; an unknown-length iterator is
/// driven one item at a time, at most `limit` items are converted, and
/// excess is detected by one further `next` whose item is discarded
/// without conversion. Nothing is reserved from a hint.
pub(crate) const MATERIALIZE_LIMIT: usize = 1 << 20;

#[cfg(feature = "test-support")]
static TEST_LIMIT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The bound in force: the production constant, or under `test-support`
/// the override set by `polars::set_materialize_limit` (0 = production).
pub(crate) fn materialize_limit() -> usize {
	#[cfg(feature = "test-support")]
	{
		let t = TEST_LIMIT.load(std::sync::atomic::Ordering::SeqCst);
		if t > 0 {
			return t;
		}
	}
	MATERIALIZE_LIMIT
}

/// Test-support only: a low bound for the controls; 0 restores the
/// production bound.
#[cfg(feature = "test-support")]
#[rune::function(path = set_materialize_limit)]
pub(crate) fn set_materialize_limit(n: i64) {
	TEST_LIMIT.store(n.max(0) as usize, std::sync::atomic::Ordering::SeqCst);
}

/// Materialize an iterator whose length is known exactly: a longer one
/// refuses before any `next` call; the count guard stays in place after.
pub(crate) fn materialize_exact<I: ExactSizeIterator, T>(it: I, method: &str, conv: impl FnMut(I::Item) -> Result<T, Error>) -> Result<Vec<T>, Error> {
	materialize_exact_with(it, materialize_limit(), method, conv)
}

/// Materialize an iterator of unknown length: at most `limit` items are
/// converted; one further `next` decides, its item discarded.
pub(crate) fn materialize_unknown<I: Iterator, T>(it: I, method: &str, conv: impl FnMut(I::Item) -> Result<T, Error>) -> Result<Vec<T>, Error> {
	materialize_unknown_with(it, materialize_limit(), method, conv)
}

/// Materialize an iterator that exposes only `TrustedLen`, whose contract
/// makes `size_hint`'s upper bound the exact length or `None` when the
/// length is not representable: an upper bound over the limit, or no
/// upper bound, refuses before any `next` call; the count guard stays.
pub(crate) fn materialize_trusted<I: polars_core::utils::arrow::trusted_len::TrustedLen, T>(it: I, method: &str, conv: impl FnMut(I::Item) -> Result<T, Error>) -> Result<Vec<T>, Error> {
	materialize_trusted_with(it, materialize_limit(), method, conv)
}

pub(crate) fn materialize_trusted_with<I: polars_core::utils::arrow::trusted_len::TrustedLen, T>(it: I, limit: usize, method: &str, conv: impl FnMut(I::Item) -> Result<T, Error>) -> Result<Vec<T>, Error> {
	match it.size_hint().1 {
		Some(n) if n > limit => return Err(Error("MaterializeLimit".into(), format!("{method}: {n} items (TrustedLen upper bound), more than the bound of {limit}"))),
		Some(_) => {}
		None => return Err(Error("MaterializeLimit".into(), format!("{method}: the TrustedLen upper bound is not representable"))),
	}
	materialize_unknown_with(it, limit, method, conv)
}

pub(crate) fn materialize_exact_with<I: ExactSizeIterator, T>(it: I, limit: usize, method: &str, conv: impl FnMut(I::Item) -> Result<T, Error>) -> Result<Vec<T>, Error> {
	let n = it.len();
	if n > limit {
		return Err(Error("MaterializeLimit".into(), format!("{method}: {n} items, more than the bound of {limit}")));
	}
	materialize_unknown_with(it, limit, method, conv)
}

pub(crate) fn materialize_unknown_with<I: Iterator, T>(mut it: I, limit: usize, method: &str, mut conv: impl FnMut(I::Item) -> Result<T, Error>) -> Result<Vec<T>, Error> {
	let mut out: Vec<T> = Vec::new();
	loop {
		let Some(item) = it.next() else { return Ok(out) };
		if out.len() == limit {
			// the one item of lookahead: discarded without conversion
			drop(item);
			return Err(Error("MaterializeLimit".into(), format!("{method}: more than the bound of {limit} items")));
		}
		out.push(conv(item)?);
	}
}

impl Error {
	pub(crate) fn conversion(what: &str) -> Error {
		Error("ConversionError".into(), what.to_string())
	}
	/// The engine thread could not be started or joined.
	pub(crate) fn engine(what: String) -> Error {
		Error("EngineError".into(), what)
	}
	#[rune::function(instance, path = kind)]
	fn kind(&self) -> String {
		self.0.clone()
	}
	#[rune::function(instance, path = message)]
	fn message(&self) -> String {
		self.1.clone()
	}
	#[rune::function(instance, protocol = DISPLAY_FMT)]
	fn display(&self, f: &mut Formatter) -> rune::runtime::VmResult<()> {
		use rune::alloc::fmt::TryWrite;
		let s = &self.1;
		rune::vm_write!(f, "{s}")
	}
}

/// Narrow a Rune integer to the Rust integer a Polars signature wants.
pub(crate) fn narrow<T: TryFrom<i64>>(v: i64, name: &str) -> Result<T, Error> {
	T::try_from(v).map_err(|_| Error::conversion(&format!("{name}: {v} is out of range for {}", std::any::type_name::<T>())))
}

/// A one-character string for a `char` parameter.
pub(crate) fn one_char(s: &str, name: &str) -> Result<char, Error> {
	let mut it = s.chars();
	match (it.next(), it.next()) {
		(Some(c), None) => Ok(c),
		_ => Err(Error::conversion(&format!("{name}: expected exactly one character, got {s:?}"))),
	}
}

/// Take a wrapped value out of a Rune container element.
pub(crate) fn take<W: Any + Clone>(v: &rune::Value, name: &str) -> Result<W, Error> {
	v.borrow_ref::<W>()
		.map(|r| r.clone())
		.map_err(|_| Error::conversion(&format!("{name}: expected {}", std::any::type_name::<W>().rsplit("::").next().unwrap_or("value"))))
}

pub fn install(m: &mut rune::Module) -> Result<(), rune::ContextError> {
	m.ty::<Error>()?;
	m.function_meta(Error::kind)?;
	m.function_meta(Error::message)?;
	m.function_meta(Error::display)?;
	#[cfg(feature = "test-support")]
	m.function_meta(set_materialize_limit)?;
	Ok(())
}

#[cfg(all(test, feature = "test-support"))]
mod materialize_tests {
	use super::*;
	use std::cell::Cell;
	use std::rc::Rc;

	/// An iterator that counts its `next` calls and reports the size hint
	/// it is told to, of a given true length.
	struct Counting {
		i: usize,
		len: usize,
		hint: (usize, Option<usize>),
		nexts: Rc<Cell<usize>>,
	}
	impl Iterator for Counting {
		type Item = usize;
		fn next(&mut self) -> Option<usize> {
			self.nexts.set(self.nexts.get() + 1);
			if self.i < self.len { self.i += 1; Some(self.i) } else { None }
		}
		fn size_hint(&self) -> (usize, Option<usize>) { self.hint }
	}
	struct Exact(Counting);
	impl Iterator for Exact {
		type Item = usize;
		fn next(&mut self) -> Option<usize> { self.0.next() }
		fn size_hint(&self) -> (usize, Option<usize>) { (self.0.len - self.0.i, Some(self.0.len - self.0.i)) }
	}
	impl ExactSizeIterator for Exact {}
	fn counting(len: usize, hint: (usize, Option<usize>)) -> (Counting, Rc<Cell<usize>>) {
		let nexts = Rc::new(Cell::new(0));
		(Counting { i: 0, len, hint, nexts: nexts.clone() }, nexts)
	}

	#[test]
	fn unknown_length_boundaries() {
		const L: usize = 4;
		for (len, ok) in [(0usize, true), (L - 1, true), (L, true), (L + 1, false)] {
			let (it, nexts) = counting(len, (0, None));
			let convs = Rc::new(Cell::new(0));
			let c2 = convs.clone();
			let r = materialize_unknown_with(it, L, "m", move |x| { c2.set(c2.get() + 1); Ok(x) });
			if ok {
				let v = r.expect("within the bound");
				assert_eq!(v.len(), len);
				assert_eq!(nexts.get(), len + 1, "one next past the last item");
				assert_eq!(convs.get(), len);
			} else {
				let e = r.expect_err("over the bound");
				assert_eq!(e.0, "MaterializeLimit");
				assert_eq!(nexts.get(), L + 1, "exactly L + 1 next calls: the lookahead item is discarded");
				assert_eq!(convs.get(), L, "the lookahead item is not converted");
			}
		}
		// an unbounded iterator refuses at L + 1 calls
		let nexts = Rc::new(Cell::new(0));
		let n2 = nexts.clone();
		let unbounded = std::iter::repeat_with(move || { n2.set(n2.get() + 1); 1usize });
		assert_eq!(materialize_unknown_with(unbounded, L, "m", Ok).unwrap_err().0, "MaterializeLimit");
		assert_eq!(nexts.get(), L + 1);
		// a size hint that overstates is not a length: the items are driven and counted
		let (it, nexts) = counting(2, (0, Some(usize::MAX)));
		assert_eq!(materialize_unknown_with(it, L, "m", Ok).unwrap().len(), 2);
		assert_eq!(nexts.get(), 3);
	}

	#[test]
	fn known_length_over_the_limit_refuses_before_any_next() {
		const L: usize = 4;
		let (c, nexts) = counting(L + 1, (0, None));
		let e = materialize_exact_with(Exact(c), L, "m", Ok).unwrap_err();
		assert_eq!(e.0, "MaterializeLimit");
		assert_eq!(nexts.get(), 0, "a known excess takes no item");
		let (c, nexts) = counting(L, (0, None));
		assert_eq!(materialize_exact_with(Exact(c), L, "m", Ok).unwrap().len(), L);
		assert_eq!(nexts.get(), L + 1);
	}

	// `TrustedLen` without `ExactSizeIterator`: the trait as Polars defines it
	unsafe impl polars_core::utils::arrow::trusted_len::TrustedLen for Counting {}
	fn trusted_only(len: usize, hint: (usize, Option<usize>)) -> (impl polars_core::utils::arrow::trusted_len::TrustedLen<Item = usize>, Rc<Cell<usize>>) {
		counting(len, hint)
	}

	#[test]
	fn a_trusted_len_only_return_materializes_by_its_upper_bound() {
		const L: usize = 4;
		// compiles against an opaque TrustedLen-only return with a mappable item
		let (it, nexts) = trusted_only(3, (3, Some(3)));
		assert_eq!(materialize_trusted_with(it, L, "m", Ok).unwrap().len(), 3);
		assert_eq!(nexts.get(), 4);
		// an upper bound over the limit refuses before any next call
		let (it, nexts) = trusted_only(L + 1, (L + 1, Some(L + 1)));
		assert_eq!(materialize_trusted_with(it, L, "m", Ok).unwrap_err().0, "MaterializeLimit");
		assert_eq!(nexts.get(), 0);
		// an unrepresentable length refuses before any next call
		let (it, nexts) = trusted_only(2, (2, None));
		assert_eq!(materialize_trusted_with(it, L, "m", Ok).unwrap_err().0, "MaterializeLimit");
		assert_eq!(nexts.get(), 0);
		// the count guard stays: a bound that understates is still caught at L + 1
		let (it, nexts) = trusted_only(L + 1, (1, Some(1)));
		assert_eq!(materialize_trusted_with(it, L, "m", Ok).unwrap_err().0, "MaterializeLimit");
		assert_eq!(nexts.get(), L + 1);
		let _: Vec<usize> = materialize_trusted(trusted_only(2, (2, Some(2))).0, "m", Ok).unwrap();
	}

	#[test]
	fn a_conversion_failure_stops_and_returns_the_error() {
		let (it, nexts) = counting(3, (0, None));
		let e = materialize_unknown_with(it, 10, "m", |x| if x == 2 { Err(Error::conversion("two")) } else { Ok(x) }).unwrap_err();
		assert_eq!(e.0, "ConversionError");
		assert_eq!(nexts.get(), 2);
	}

	#[test]
	fn production_limit_is_the_default() {
		assert_eq!(materialize_limit(), MATERIALIZE_LIMIT);
		let v: Vec<usize> = materialize_unknown(0..10usize, "m", Ok).unwrap();
		assert_eq!(v.len(), 10);
	}

	#[test]
	fn a_routed_iterator_is_driven_on_the_engine_thread_under_a_tokio_runtime() {
		let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
		let names = rt.block_on(async {
			crate::engine::run(move || {
				let owner = [1usize, 2, 3];
				let it = owner.iter().map(|x| { let name = std::thread::current().name().map(|s| s.to_string()); (*x, name) });
				let out: Vec<(usize, Option<String>)> = materialize_unknown(it, "m", Ok).unwrap();
				// borrowed elements are detached into owned values before the owner drops
				out
			})
			.unwrap()
		});
		assert_eq!(names.len(), 3);
		assert!(names.iter().all(|(_, n)| n.as_deref() == Some("rnx-polars-engine")), "next must run on the engine thread: {names:?}");
	}
}
