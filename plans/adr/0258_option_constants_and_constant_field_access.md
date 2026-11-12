# ADR 0258: Option constants and constant field access in an execution session

Status: Proposed

Base: published dffb48c148c6618227d9e4500ab468704422a64a (ADR 0257).

## Problem

Three constant expressions are still refused when an input is admitted to
an execution session. One of them is plain data:

	const A = Some(1);

The bounded evaluator refuses every call, and Option construction is
written as a call. The constant value representation already has a place
for an Option, with its variant and zero or one field. What is missing is
the evaluator recognizing the two constructors and the lowering turning
such a value into a fresh Option when the constant is used.

ADR 0257 admitted indexing and left field access refused by name:

	const T = (1, [2]); const B = T.1;
	const O = #{x: [3]}; const C = O.x;

Legacy admits both. The selection and the charged copy they need exist
since ADR 0257.

Legacy does not admit Ok and Err as constants; it reports that a Result
cannot be converted to a constant value. Run-time support for a type does
not make it constant data.

## Goal

Some and None are constant data in an execution session, and a constant
tuple or object can be read by field, each with the same value as legacy.

## Required behavior

- Scope is the two Option constructors and field access on a constant
  tuple or object. Constant functions, general calls and byte-string
  constants stay refused with their present reasons. Ok and Err stay
  refused, with a reason that names Result, as legacy refuses them.
- The evaluator recognizes Some and None by their resolved identity and
  arity, never by spelling. An alias of a constructor is admitted. A user
  function or constant named Some or None is not treated as the
  constructor. No call is dispatched, to a host or to anything else.
- The payload of Some is evaluated once, by the same evaluator and under
  the same shared budget. Options nest, and may sit inside and contain
  vectors, tuples and objects.
- Each use of a constant yields a fresh Option. Changing the payload
  obtained from one use never changes another use or the constant.
- Lowering validates the family, the variant and the field count of an
  Option constant before emitting anything, then materializes the payload
  and emits the existing variant instruction. A malformed value is an
  internal error, not a guess. No new instruction, store operation or
  schema.
- A named field reads an object. A numbered field reads the values legacy
  treats as tuple-like: a tuple, a vector and the payload of an Option.
  A constant has the value the same expression has at run time, so the
  receivers admitted are exactly those on which the two engines agree at
  run time; if the engines differ at run time for a field form, that is
  sent before it is implemented. Out of range, missing key, a missing
  numbered field such as on None or unit, and a wrong receiver are
  refused at admission with legacy's diagnostic for the field form and at
  the location legacy reports, which is the field, not the whole
  expression. Other field forms are refused by name.
- The selected field is copied through the charged copy of ADR 0257. A
  field that is an Option is materialized by the path above.
- The limits on nodes, bytes and depth apply to the new forms. Cycles are
  detected as today.

## Fixed route

This record does not edit any inventoried item and does not change the
policy file. The evaluator and the constant lowering are in the execution
core, so code layout may move; the Ir bands pass per record and
cumulatively, as for every record, with no exception.

## Out of scope

- Constant functions and general constant calls.
- Destructuring bindings, pattern conditions and match in constants; they
  share one pattern evaluator and are their own record.
- Result constants, bytes.
- Changes to legacy constant evaluation.
- Manifest and lock files.

## Acceptance

- session_parity.json: the Some row leaves "Refused at admission" and
  becomes Equal; that class falls from 3 to 2. New rows for None, a tuple
  field and an object field are measured, and the record states their
  class. The two remaining refusals keep their reasons. Any row that does
  not come out Equal is sent before the table changes.
- Controls, each compared with legacy: Some and None alone, nested, and
  inside each container; an aliased constructor; a user item with a
  constructor's name; wrong arity; Ok and Err refused; an Option constant
  used twice with one payload changed; an Option reached by index, by
  field and by reference; each field form with its out-of-range, missing
  and wrong-receiver diagnostics and locations; a numbered field on a
  vector, on Some, on None and on unit; chained fields; each field form
  beside its run-time form in both engines; each limit reached through
  the new forms; a failed admission leaving earlier bindings unchanged.
- Miri covers Option payload copying, repeated fresh materialization and
  release, nesting, and cleanup after a failure.
- The fixed-route source condition and bands pass. Required stages follow
  the changed paths.
