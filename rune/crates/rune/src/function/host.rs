//! Closed, synchronous new-host signatures. Enrollment is a trusted host contract.
#![allow(missing_docs)]
use crate::alloc::clone::TryClone;
use crate::function::{Function, Plain, Ref};
use crate::runtime::FunctionHandler;
use crate::Hash;

mod sealed {
	pub trait Argument {}
	pub trait Scalar {}
	pub trait Output {}
	pub trait Arguments {}
	pub trait Callable<A> {}
	pub trait StringCallable<A> {}
}

/// Scalar representation used by the unstable host bridge.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HostType {
	Unit,
	Bool,
	I64,
	U64,
	F64,
	Char,
	String,
}
/// Closed scalar/optional/result shapes and two concrete dynamic output certificates.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostOutputKind {
	Scalar(HostType),
	Option(HostType),
	Result(HostType, HostType),
	/// Owned snapshot of the closed builtin dynamic payload graph.
	Dynamic,
	/// Exactly Rust Result<Value, std::string::String>.
	ResultDynamicString,
}
/// One node in a static output-shape graph. Every declared node must be importable.
/// Indices refer to nodes in the same certificate; recursive containers describe
/// arbitrarily large finite graphs. A union promises that every alternative is safe.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum OutputShape {
	Scalar(HostType),
	Ordering,
	Vec(usize),
	Tuple(&'static [usize]),
	Object(usize),
	Option(usize),
	Result(usize, usize),
	Union(&'static [usize]),
	/// Declare a type outside the closed importer domain. The execution core refuses the call.
	Unsupported(Hash),
}

/// A trusted host promise about all outputs, bound to the installed handler.
/// Outputs must be readable, acyclic owned snapshots. Internal DAGs are allowed;
/// externally mutable and cross-call aliases are excluded. A broken promise is a
/// host bug, never a reason to retry execution on stock after effects have occurred.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputCertificate {
	pub nodes: &'static [OutputShape],
	pub root: usize,
}
impl OutputCertificate {
	fn importable(self) -> bool {
		let valid = |i: usize| i < self.nodes.len();
		valid(self.root)
			&& self.nodes.iter().all(|node| match node {
				OutputShape::Scalar(_) | OutputShape::Ordering => true,
				OutputShape::Vec(i) | OutputShape::Object(i) | OutputShape::Option(i) => valid(*i),
				OutputShape::Tuple(items) => items.iter().all(|i| valid(*i)),
				OutputShape::Union(items) => !items.is_empty() && items.iter().all(|i| valid(*i)),
				OutputShape::Result(ok, err) => valid(*ok) && valid(*err),
				OutputShape::Unsupported(_) => false,
			})
	}
	fn certifies(self, output: HostOutputKind) -> bool {
		if !self.importable() {
			return false;
		}
		match output {
			HostOutputKind::Dynamic => true,
			HostOutputKind::ResultDynamicString => {
				matches!(self.nodes[self.root], OutputShape::Result(_, err) if self.nodes[err] == OutputShape::Scalar(HostType::String))
			}
			_ => false,
		}
	}

	/// JSON trees wrapped in Rust `Result<Value, String>`.
	pub const JSON_RESULT: Self = Self {
		nodes: &[
			OutputShape::Result(1, 7),
			OutputShape::Union(&[2, 3, 4, 5, 6, 7, 8, 9]),
			OutputShape::Scalar(HostType::Unit),
			OutputShape::Scalar(HostType::Bool),
			OutputShape::Scalar(HostType::I64),
			OutputShape::Scalar(HostType::U64),
			OutputShape::Scalar(HostType::F64),
			OutputShape::Scalar(HostType::String),
			OutputShape::Vec(1),
			OutputShape::Object(1),
		],
		root: 0,
	};
}

#[derive(Clone, Copy)]
pub(crate) struct HostEnrollment {
	pub(crate) signature: HostSignature,
	pub(crate) certificate: Option<OutputCertificate>,
}
impl TryClone for HostEnrollment {
	fn try_clone(&self) -> crate::alloc::Result<Self> {
		Ok(*self)
	}
}
impl HostEnrollment {
	pub(crate) fn admitted(self) -> bool {
		match self.signature.output {
			HostOutputKind::Dynamic | HostOutputKind::ResultDynamicString => self
				.certificate
				.is_some_and(|c| c.certifies(self.signature.output)),
			_ => self.certificate.is_none(),
		}
	}
}

/// Immutable signature created only by the sealed typed API.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostSignature {
	pub(crate) arity: u8,
	pub(crate) arguments: [HostType; 4],
	pub(crate) output: HostOutputKind,
}
impl TryClone for HostSignature {
	fn try_clone(&self) -> crate::alloc::Result<Self> {
		Ok(*self)
	}
}
impl HostSignature {
	pub(crate) fn valid(self) -> bool {
		self.arity <= 4
			&& self.arguments[self.arity as usize..]
				.iter()
				.all(|kind| *kind == HostType::Unit)
	}
}

/// Sealed argument marker. Owned strings and mutable/reference aggregate inputs are excluded.
#[doc(hidden)]
pub trait HostArgument: sealed::Argument {
	type Rust<'a>;
	const KIND: HostType;
}
/// Sealed scalar return type.
#[doc(hidden)]
pub trait HostScalar: sealed::Scalar {
	const KIND: HostType;
}
/// Sealed return signature; dynamic payloads require an explicit output certificate.
#[doc(hidden)]
pub trait HostOutput: sealed::Output {
	const SHAPE: HostOutputKind;
}
/// Sealed tuple of zero through four argument markers.
#[doc(hidden)]
pub trait HostArguments: sealed::Arguments {
	const ARITY: u8;
	const TYPES: [HostType; 4];
}
/// A Rust Fn with the closed synchronous signature; doc metadata cannot certify one.
#[doc(hidden)]
pub trait HostFunction<A>: Function<A, Plain> + sealed::Callable<A> {
	fn signature() -> HostSignature;
}

macro_rules! scalar {
	($rust:ty, $kind:ident) => {
		impl sealed::Scalar for $rust {}
		impl HostScalar for $rust {
			const KIND: HostType = HostType::$kind;
		}
		impl sealed::Output for $rust {}
		impl HostOutput for $rust {
			const SHAPE: HostOutputKind = HostOutputKind::Scalar(HostType::$kind);
		}
	};
}
macro_rules! argument {
	($rust:ty, $kind:ident) => {
		impl sealed::Argument for $rust {}
		impl HostArgument for $rust {
			type Rust<'a> = $rust;
			const KIND: HostType = HostType::$kind;
		}
	};
}
scalar!((), Unit);
scalar!(bool, Bool);
scalar!(i64, I64);
scalar!(u64, U64);
scalar!(f64, F64);
scalar!(char, Char);
scalar!(std::string::String, String);
argument!((), Unit);
argument!(bool, Bool);
argument!(i64, I64);
argument!(u64, U64);
argument!(f64, F64);
argument!(char, Char);
impl sealed::Argument for Ref<str> {}
impl HostArgument for Ref<str> {
	type Rust<'a> = &'a str;
	const KIND: HostType = HostType::String;
}
impl<T: HostScalar> sealed::Output for Option<T> {}
impl<T: HostScalar> HostOutput for Option<T> {
	const SHAPE: HostOutputKind = HostOutputKind::Option(T::KIND);
}
impl<T: HostScalar, E: HostScalar> sealed::Output for Result<T, E> {}
impl<T: HostScalar, E: HostScalar> HostOutput for Result<T, E> {
	const SHAPE: HostOutputKind = HostOutputKind::Result(T::KIND, E::KIND);
}

// These concrete certificates deliberately do not implement HostScalar.
impl sealed::Output for crate::runtime::Value {}
impl HostOutput for crate::runtime::Value {
	const SHAPE: HostOutputKind = HostOutputKind::Dynamic;
}
impl sealed::Output for Result<crate::runtime::Value, std::string::String> {}
impl HostOutput for Result<crate::runtime::Value, std::string::String> {
	const SHAPE: HostOutputKind = HostOutputKind::ResultDynamicString;
}

// Concrete owned trees use the existing transactional importer, and still require
// an explicit output certificate at registration. No blanket ToValue admission.
impl sealed::Output for std::vec::Vec<std::string::String> {}
impl HostOutput for std::vec::Vec<std::string::String> {
	const SHAPE: HostOutputKind = HostOutputKind::Dynamic;
}
impl sealed::Output for Result<std::vec::Vec<std::string::String>, std::string::String> {}
impl HostOutput for Result<std::vec::Vec<std::string::String>, std::string::String> {
	const SHAPE: HostOutputKind = HostOutputKind::Dynamic;
}
impl sealed::Output for Result<Option<std::string::String>, std::string::String> {}
impl HostOutput for Result<Option<std::string::String>, std::string::String> {
	const SHAPE: HostOutputKind = HostOutputKind::Dynamic;
}

macro_rules! arguments {
	($n:expr; $($a:ident : $index:expr),*) => {
		impl<$($a: HostArgument),*> sealed::Arguments for ($($a,)*) {}
		impl<$($a: HostArgument),*> HostArguments for ($($a,)*) {
			const ARITY: u8 = $n;
			const TYPES: [HostType; 4] = {
				let mut types = [HostType::Unit; 4];
				$(types[$index] = $a::KIND;)*
				types
			};
		}
		impl<F, R, $($a: HostArgument),*> sealed::Callable<($($a,)*)> for F
		where F: for<'a> Fn($($a::Rust<'a>),*) -> R + Function<($($a,)*), Plain, Return = R>, R: HostOutput {}
	};
}
impl sealed::Arguments for () {}
impl HostArguments for () {
	const ARITY: u8 = 0;
	const TYPES: [HostType; 4] = [HostType::Unit; 4];
}
impl<F, R> sealed::Callable<()> for F
where
	F: Fn() -> R + Function<(), Plain, Return = R>,
	R: HostOutput,
{
}
arguments!(1; A:0);
arguments!(2; A:0,B:1);
arguments!(3; A:0,B:1,C:2);
arguments!(4; A:0,B:1,C:2,D:3);
impl<F, A> HostFunction<A> for F
where
	F: Function<A, Plain> + sealed::Callable<A>,
	A: HostArguments,
	F::Return: HostOutput,
{
	fn signature() -> HostSignature {
		HostSignature {
			arity: A::ARITY,
			arguments: A::TYPES,
			output: F::Return::SHAPE,
		}
	}
}

/// Certificate retains the allocation of the installed handler, never a code-address token.
#[derive(Clone)]
pub(crate) struct HostBinding {
	pub(crate) hash: Hash,
	pub(crate) handler: FunctionHandler,
	pub(crate) signature: HostSignature,
	pub(crate) certificate: Option<OutputCertificate>,
}
impl TryClone for HostBinding {
	fn try_clone(&self) -> crate::alloc::Result<Self> {
		Ok(self.clone())
	}
}
#[cfg(target_pointer_width = "64")]
const _: () = {
	assert!(core::mem::size_of::<HostSignature>() == 7);
	assert!(core::mem::size_of::<HostBinding>() == 56);
};
impl HostBinding {
	pub(crate) fn same(&self, other: &Self) -> bool {
		self.hash == other.hash
			&& self.signature == other.signature
			&& self.certificate == other.certificate
			&& self.handler.same_allocation(&other.handler)
	}
}

#[cfg(test)]
mod tests;

/// Owned text or a payload-free rejection, for a final dynamic input only.
/// This deliberately does not implement HostArgument: it cannot appear in an
/// ordinary signature or before another argument.
#[doc(hidden)]
pub struct StringOrRejected(Option<crate::alloc::String>);
impl StringOrRejected {
	pub fn as_str(&self) -> Option<&str> {
		self.0.as_deref()
	}
}
impl crate::runtime::FromValue for StringOrRejected {
	fn from_value(value: crate::runtime::Value) -> Result<Self, crate::runtime::RuntimeError> {
		if value.type_hash() != <crate::alloc::String as crate::runtime::TypeHash>::HASH {
			return Ok(Self(None));
		}
		// A real string must be readable and copied successfully. Neither access
		// nor allocation failure is a wrong-type rejection.
		let text = value.borrow_string_ref()?;
		Ok(Self(Some(crate::alloc::String::try_from(&*text)?)))
	}
}
impl crate::runtime::MaybeTypeOf for StringOrRejected {
	fn maybe_type_of() -> crate::alloc::Result<crate::compile::meta::DocType> {
		<crate::runtime::Value as crate::runtime::MaybeTypeOf>::maybe_type_of()
	}
}
/// Sealed supported arrangements: one final rejection-bearing argument.
#[doc(hidden)]
pub trait StringInputFunction<A>: Function<A, Plain> + sealed::StringCallable<A> {
	fn string_signature() -> HostSignature;
}
impl<F, R> sealed::StringCallable<(StringOrRejected,)> for F
where
	F: Fn(StringOrRejected) -> R + Function<(StringOrRejected,), Plain, Return = R>,
	R: HostOutput,
{
}
impl<F, R> StringInputFunction<(StringOrRejected,)> for F
where
	F: Fn(StringOrRejected) -> R + Function<(StringOrRejected,), Plain, Return = R>,
	R: HostOutput,
{
	fn string_signature() -> HostSignature {
		HostSignature {
			arity: 1,
			arguments: [
				HostType::String,
				HostType::Unit,
				HostType::Unit,
				HostType::Unit,
			],
			output: R::SHAPE,
		}
	}
}
impl<F, R> sealed::StringCallable<(Ref<str>, StringOrRejected)> for F
where
	F: Fn(&str, StringOrRejected) -> R + Function<(Ref<str>, StringOrRejected), Plain, Return = R>,
	R: HostOutput,
{
}
impl<F, R> StringInputFunction<(Ref<str>, StringOrRejected)> for F
where
	F: Fn(&str, StringOrRejected) -> R + Function<(Ref<str>, StringOrRejected), Plain, Return = R>,
	R: HostOutput,
{
	fn string_signature() -> HostSignature {
		HostSignature {
			arity: 2,
			arguments: [
				HostType::String,
				HostType::String,
				HostType::Unit,
				HostType::Unit,
			],
			output: R::SHAPE,
		}
	}
}

#[derive(Clone)]
pub(crate) struct AlternateHost {
	pub handler: FunctionHandler,
	pub enrollment: HostEnrollment,
}
impl TryClone for AlternateHost {
	fn try_clone(&self) -> crate::alloc::Result<Self> {
		Ok(self.clone())
	}
}
#[derive(Clone)]
pub(crate) struct InstalledAlternate {
	pub original: FunctionHandler,
	pub binding: HostBinding,
}
impl TryClone for InstalledAlternate {
	fn try_clone(&self) -> crate::alloc::Result<Self> {
		Ok(self.clone())
	}
}

/// An execution-only snapshot. Legacy RuntimeContext cannot be passed in its place.
/// The compiling twin differs only in the runtime supplied to the setter.
/// ```
/// let c = rune::Context::new();
/// let mut e = rune::experimental_core::execute::Executor::new().unwrap();
/// e.set_host_runtime(Some(c.execution_runtime().unwrap())).unwrap();
/// ```
/// ```compile_fail,E0308
/// let c = rune::Context::new();
/// let mut e = rune::experimental_core::execute::Executor::new().unwrap();
/// e.set_host_runtime(Some(rune::sync::Arc::try_new(c.runtime().unwrap()).unwrap())).unwrap();
/// ```
#[derive(Clone)]
#[doc(hidden)]
pub struct ExecutionRuntime {
	pub(crate) inner: crate::sync::Arc<crate::runtime::RuntimeContext>,
}

#[cfg(test)]
mod string_input_controls {
	use super::*;
	use crate::{FromValue, Value};
	#[test]
	fn string_input_rejection_does_not_borrow_payload_and_string_errors_propagate() {
		let value = crate::to_value(crate::runtime::Vec::new()).unwrap();
		let guard = value.borrow_mut::<crate::runtime::Vec>().unwrap();
		assert!(StringOrRejected::from_value(value.clone())
			.unwrap()
			.as_str()
			.is_none());
		drop(guard);
		let value = crate::to_value("text").unwrap();
		let guard = value.borrow_mut::<crate::alloc::String>().unwrap();
		assert!(StringOrRejected::from_value(value.clone()).is_err());
		drop(guard);
		let owned = StringOrRejected::from_value(value.clone()).unwrap();
		assert_eq!(owned.as_str(), Some("text"));
		value.borrow_mut::<crate::alloc::String>().unwrap().clear();
		assert_eq!(owned.as_str(), Some("text"));
		assert!(StringOrRejected::from_value(Value::from(42i64))
			.unwrap()
			.as_str()
			.is_none());
	}
	#[test]
	fn string_input_installation_failure_and_invalid_output_do_not_admit() {
		let mut c = crate::Context::new();
		let mut m = crate::Module::with_item(["x"]).unwrap();
		m.function("f", |_: Value| 0i64).build().unwrap();
		c.install(m).unwrap();
		let mut m = crate::Module::with_item(["x"]).unwrap();
		m.host_function_string_input("f", |_: Value| 0i64, |_: StringOrRejected| 1i64, None)
			.unwrap();
		assert!(c.install(m).is_err());
		assert!(c.host(Hash::type_hash(["x", "f"])).is_none());
		let mut m = crate::Module::with_item(["y"]).unwrap();
		m.host_function_string_input(
			"f",
			|_: Value| Value::from(()),
			|_: StringOrRejected| Value::from(()),
			None,
		)
		.unwrap();
		c.install(m).unwrap();
		assert!(c.host(Hash::type_hash(["y", "f"])).is_none());
	}
}
