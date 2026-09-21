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
	Ok(())
}
