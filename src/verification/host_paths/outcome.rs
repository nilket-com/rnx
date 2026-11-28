//! Primary observations at the script boundary. A process exit has no returned diagnostic.
use rune::experimental_core::error::{Detail, ErrorClass, ErrorKind, ErrorProjection};
use rune::experimental_core::{execute::Failure, frontend::Program};
use rune::runtime::VmError;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Outcome {
	Complete,
	Exit(i32),
	CompileError,
	Fault {
		message: String,
		offset: Option<usize>,
		cause: Cause,
	},
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Cause {
	Other,
	StockArity,
	CallableArity { given: u32, expected: u32 },
	Budget,
}

impl Outcome {
	pub(super) fn stock_error(error: &VmError, source: &str, offset: Option<usize>) -> Self {
		let message = offset
			.and_then(|_| crate::method::named(error, source))
			.unwrap_or_else(|| error.to_string());
		let cause = if error.is_budget_exhausted() {
			Cause::Budget
		} else if matches!(
			error.error_class(),
			ErrorProjection::Class(ErrorClass::ArgumentCount)
		) {
			Cause::StockArity
		} else {
			Cause::Other
		};
		Self::Fault {
			message,
			offset,
			cause,
		}
	}

	pub(super) fn budget(fuel: usize) -> Self {
		Self::Fault {
			message: crate::runner::budget_message(fuel),
			offset: None,
			cause: Cause::Budget,
		}
	}

	pub(super) fn execution_error(failure: &Failure, program: &Program, fuel: usize) -> Self {
		if failure.error.kind == ErrorKind::FuelExhausted {
			return Self::budget(fuel);
		}
		let message = crate::method::named_execution(failure, program)
			.unwrap_or_else(|| failure.message(program).to_string());
		let offset = failure.error.location.and_then(|at| {
			program
				.sources()
				.iter()
				.find(|s| {
					s.id == at.source && s.revision == at.revision && s.name == "host-path.rn"
				})
				.map(|_| at.start as usize)
		});
		let cause = match failure.error.detail {
			Detail::Arity { given, expected } => Cause::CallableArity { given, expected },
			_ => Cause::Other,
		};
		Self::Fault {
			message,
			offset,
			cause,
		}
	}

	/// Returns the reviewed reason for a difference, or an error for an unexpected one.
	pub(super) fn compare(
		&self,
		actual: &Self,
		input: &str,
		fuel: usize,
		call_offset: Option<usize>,
	) -> Result<Option<&'static str>, ()> {
		use Outcome::Fault;
		let (
			Fault {
				message: old,
				offset: a,
				cause: stock,
			},
			Fault {
				message: new,
				offset: b,
				cause: execution,
			},
		) = (self, actual)
		else {
			return if self == actual { Ok(None) } else { Err(()) };
		};
		if old == new && a == b {
			return Ok(None);
		}
		match (stock, execution) {
			(Cause::Budget, Cause::Budget)
				if old == &VmError::budget_exhausted().to_string()
					&& a.is_some() && *a == call_offset
					&& b.is_none() && new
					== &format!("{fuel} work units exceeded; --budget N raises it") =>
			{
				Ok(Some("budget advice"))
			}
			// The generated capturing closure has one explicit parameter. The VM
			// counts its environment as an extra argument on both sides.
			(Cause::StockArity, Cause::CallableArity { given, expected: 1 })
				if input == "Closure"
					&& a.is_some() && a == b
					&& old
						== &format!(
							"Wrong number of arguments {}, expected 2",
							u64::from(*given) + 1
						) && new == &format!("Wrong number of arguments {given}, expected 1") =>
			{
				Ok(Some("closure arity"))
			}
			_ => Err(()),
		}
	}
}
