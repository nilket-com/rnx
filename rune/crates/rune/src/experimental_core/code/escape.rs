//! Cold negative proof for program collection. No execution or heap-write hooks.
use super::*;

// A new opcode must be classified here before it can be proved non-escaping.
fn instruction_may_escape(op: Op, code: &Code) -> bool {
	match op {
		Op::Literal { value, .. } => value_may_escape(value),
		// All local bodies are inspected. Typed host output import has no code
		// argument or Function output kind: leaf/dynamic imports reject stock
		// functions, and opaque payloads cannot become executable script tokens.
		Op::Call { function, .. } => {
			if (function as usize) < code.functions.len() || code.host(function).is_some() {
				false
			} else if variant_test_id(function).is_some() {
				// Sealed predicates are descriptor-checked by validate_operands and
				// only return Bool. Staged/unbound IDs never get this proof.
				false
			} else {
				intrinsic_may_escape(function).unwrap_or(true)
			}
		}
		Op::Template { .. }
		| Op::Print { .. }
		| Op::TupleFieldGetMut { .. }
		| Op::TupleFieldSetMut { .. }
		| Op::TupleField { .. }
		| Op::SetTupleField { .. }
		| Op::Record { .. }
		| Op::FieldGetMut { .. }
		| Op::FieldSetMut { .. }
		| Op::Field { .. }
		| Op::SetField { .. }
		| Op::Variant { .. }
		| Op::Try { .. }
		| Op::CallValue { .. }
		| Op::Range { .. }
		| Op::IterStart { .. }
		| Op::IterNext { .. }
		| Op::String { .. }
		| Op::StringFrom { .. }
		| Op::Panic { .. }
		| Op::Clone { .. }
		| Op::AssignBinary { .. }
		| Op::Copy { .. }
		| Op::Release { .. }
		| Op::Unary { .. }
		| Op::Binary { .. }
		| Op::Cast { .. }
		| Op::Is { .. }
		| Op::Jump { .. }
		| Op::Branch { .. }
		| Op::Return { .. }
		| Op::List { .. }
		| Op::Get { .. }
		| Op::Set { .. }
		| Op::Len { .. }
		| Op::Push { .. }
		| Op::Drop { .. } => false,
	}
}

// Some(false) is an explicit reviewed scalar/forward/container effect;
// None is unclassified and must remain a possible producer. Forwarded tokens
// retain their original code ID; callbacks execute bodies censused separately.
fn intrinsic_may_escape(id: u32) -> Option<bool> {
	use crate::function::{iter_terminal, number, numeric, object, variant};
	if let Some(method) = number::Pair::from_id(id)
		.map(|p| p.method())
		.or_else(|| number::Method::from_dynamic_id(id))
	{
		use number::Method::*;
		return Some(match method {
			Sqrt | Floor | Ceil | Exp | Ln | Powi | Min | Max | PartialCmp | ToString | Pow
			| CheckedPow | WrappingPow | SaturatingPow | CheckedAdd | CheckedSub | CheckedMul
			| CheckedDiv | CheckedRem | WrappingAdd | WrappingSub | WrappingMul | WrappingDiv
			| WrappingRem | SaturatingAdd | SaturatingSub | SaturatingMul | CheckedAbs
			| WrappingAbs | SaturatingAbs | Signum | ToI64 | FromI64 => false,
		});
	}
	if let Some(effect) = numeric::Call::from_id(id) {
		use numeric::Call::*;
		return Some(match effect {
			MethodAbs | MethodRound | MethodPowf | F64Abs | F64Round | F64Powf | I64Abs
			| MethodCmp => false,
		});
	}
	if let Some(effect) = variant::Method::from_id(id) {
		use variant::Method::*;
		return Some(match effect {
			Unwrap | Expect | IsSome | IsNone | IsOk | IsErr => false,
		});
	}
	if let Some(effect) = object::Method::from_id(id) {
		use object::Method::*;
		return Some(match effect {
			Get | ContainsKey | Insert | Remove | IsEmpty => false,
		});
	}
	if let Some(effect) = iter_terminal::Intrinsic::from_id(id) {
		use iter_terminal::{Intrinsic::*, Number};
		return Some(match effect {
			Seed(Number::I64 | Number::U64 | Number::F64)
			| AdapterCount
			| Range
			| Sum
			| Product
			| Insert
			| IntoTuple
			| Append
			| OptionSome
			| RevEmpty
			| RevLast
			| RevAt
			| IntoIter => false,
		});
	}
	match id {
		CLOSURE_CREATE => Some(true),
		// INSTANCE_METHOD can synthesize an own-code token only from Code.methods;
		// may_escape checks that table before instructions. With no entry it fails.
		INSTANCE_METHOD => Some(false),
		// Scalar results and forwarding from captures, variants, ranges, iterators,
		// sort scratch or collection arguments; none manufactures a code ID.
		PRINT_RAW | VARIANT_SUCCESS | MATCH_VARIANT | SORT_BEGIN | SORT_LEN | SORT_LOAD
		| SORT_VALUE | SORT_STORE | SORT_COMMIT | SORT_ORDERING | ANY_READY | CAPTURE_TAKE
		| RANGE_FROM | RANGE_FULL | RANGE_INCLUSIVE | RANGE_TO_INCLUSIVE | RANGE_TO
		| OBJECT_HAS_FIELD | ITER_METHOD | NEXT_METHOD | VEC_NEW | VEC_WITH_CAPACITY
		| VEC_RESIZE | ANY_TYPE_OF | ANY_TYPE_NAME => Some(false),
		// Certified text functions: string/char/scalar results and text iterators.
		TEXT_SPLIT
		| TEXT_REPLACE
		| TEXT_STARTS_WITH
		| TEXT_ENDS_WITH
		| TEXT_CONTAINS
		| TEXT_TRIM
		| PIPELINE_CHARS
		| PIPELINE_UNWRAP
		| PIPELINE_SKIP
		| PIPELINE_TAKE
		| PIPELINE_COUNT
		| PIPELINE_NEXT
		| PIPELINE_COLLECT_VEC
		| PIPELINE_COLLECT_STRING
		| PIPELINE_CHILD
		| PIPELINE_SKIP_REMAINING
		| PIPELINE_TAKE_NEXT
		| PIPELINE_APPEND => Some(false),
		_ => None,
	}
}

fn value_may_escape(value: Value) -> bool {
	match value {
		Value::Function { .. } | Value::Heap(_) => true,
		// validate_operands rejects every Heap literal; strings use the string
		// pool instead. Keep malformed, unvalidated heap constants conservative.
		Value::Unit
		| Value::Bool(_)
		| Value::I64(_)
		| Value::U64(_)
		| Value::F64(_)
		| Value::Char(_)
		| Value::Ordering(_)
		| Value::Type(_) => false,
	}
}

impl Code {
	/// Only false proves that no value can name this code ID after an entry.
	#[cold]
	#[inline(never)]
	pub(crate) fn may_escape(&self) -> bool {
		// Native method-return decoding can synthesize a Function token from a
		// closed function-table entry without a literal at that return site.
		if !self.methods.is_empty() {
			return true;
		}
		self.functions.iter().any(|function| {
			function.constants.iter().copied().any(value_may_escape)
				|| function
					.cached
					.iter()
					.any(|instruction| instruction_may_escape(instruction.op, self))
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	fn compiled(context: &crate::Context, source: &str) -> Code {
		let mut sources = crate::Sources::new();
		sources
			.insert(crate::Source::memory(source).unwrap())
			.unwrap();
		super::super::super::frontend::compile(
			&mut sources,
			context,
			&mut crate::Diagnostics::new(),
			0,
		)
		.unwrap()
		.code
	}
	#[test]
	fn census_covers_all_bodies_and_local_call_results() {
		let context = crate::Context::with_default_modules().unwrap();
		assert!(!compiled(&context, "pub fn main(){7}").may_escape());
		assert!(!compiled(&context, "fn value(){7} pub fn main(){value()}").may_escape());
		assert!(compiled(&context, "fn value(){7} pub fn main(){value}").may_escape());
		assert!(compiled(&context, "pub fn unused(){let x=7;||x} pub fn main(){7}").may_escape());
	}
	#[test]
	fn native_method_table_is_a_separate_conservative_producer() {
		let context = crate::Context::with_default_modules().unwrap();
		let mut code = compiled(
			&context,
			&std::format!(
				"{} pub fn main(seed){{kernel(seed)}}",
				include_str!("../region/fixtures/method_fields.rn")
			),
		);
		assert!(!code.methods.is_empty());
		// Isolate the native-return table in this census-only fixture. Never run
		// the resulting incomplete Code through an executor.
		for function in &mut code.functions {
			function.cached.clear();
			function.constants.clear();
		}
		assert!(code.may_escape());
		code.methods.clear();
		assert!(!code.may_escape());
	}
	#[test]
	fn all_function_producer_kinds_are_conservative() {
		let context = crate::Context::with_default_modules().unwrap();
		let code = compiled(&context, "pub fn main(){7}");
		assert!(value_may_escape(Value::Function { code: 17, id: 0 }));
		assert!(instruction_may_escape(
			Op::Literal {
				out: 0,
				value: Value::Function { code: 17, id: 0 },
			},
			&code
		));
		for function in [CLOSURE_CREATE, FIRST_INTRINSIC_ID, u32::MAX] {
			assert!(instruction_may_escape(
				Op::Call {
					out: 0,
					function,
					args_start: 0,
					args_len: 0,
				},
				&code
			));
		}
		assert!(!instruction_may_escape(
			Op::Call {
				out: 0,
				function: 0,
				args_start: 0,
				args_len: 0,
			},
			&code
		));
		assert!(!value_may_escape(Value::I64(7)));
	}
	#[test]
	fn every_registered_intrinsic_has_a_reviewed_effect() {
		for entry in CERTIFIED_INTRINSICS {
			assert!(
				intrinsic_may_escape(entry.id).is_some(),
				"unclassified {}",
				entry.id
			);
		}
		for pair in crate::function::number::PAIRS {
			assert_eq!(intrinsic_may_escape(pair.id()), Some(false));
		}
		for method in crate::function::number::Method::ALL {
			if let Some(id) = method.dynamic_id() {
				assert_eq!(intrinsic_may_escape(id), Some(false));
			}
		}
		for intrinsic in crate::function::iter_terminal::Intrinsic::ALL {
			assert_eq!(intrinsic_may_escape(intrinsic.id()), Some(false));
		}
		assert_eq!(intrinsic_may_escape(u32::MAX), None);
		assert_eq!(intrinsic_may_escape(FIRST_INTRINSIC_ID), None);
	}
	#[test]
	fn common_calls_do_not_invent_a_program_token() {
		let context = crate::Context::with_default_modules().unwrap();
		for source in [
			"pub fn main(v){v.len()}",
			"pub fn main(v){v[0]+1}",
			"pub fn main(){let n=0;for i in 0..8{n+=i;}n}",
			"pub fn main(){println!(\"hello\");7}",
			"pub fn main(x){x.abs()}",
			"pub fn main(){[1,2].iter().collect::<Vec>()}",
		] {
			assert!(!compiled(&context, source).may_escape(), "{source}");
		}
	}
}
