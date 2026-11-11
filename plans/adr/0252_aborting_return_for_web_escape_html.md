# ADR 0252: aborting string return for web::escape_html

Status: Proposed

Base: published 18c82c4da3cd00bcc45b8acdd62457dab762f7fd (ADR 0251).

## Problem

web::escape_html stays refused in an execution session. It is the function
a template calls on every interpolated value, so a session that builds
markup meets the refusal at once.

Its input is a string, which the bridge already carries, and its successful
result is a string, which the bridge already imports. The refusal comes
from the failure side. The function has one data-dependent failure, the
size cap, and reports it by aborting execution, deliberately, so that
interpolation needs no Result handling. In Rust that is a return of the
string or a runtime error. The host output declarations cover a plain
value, an Option, and a Result whose error is a script value. None of them
describes "a string, or execution stops".

Enrolling the checked helper that returns a Result with a string error
would change what the script sees: a value it must unwrap instead of a
string. That is not acceptable.

## Goal

web::escape_html is callable from an execution session, returns a plain
string, and stops execution on the cap exactly where legacy does.

## Required behavior

- Scope is the one function and one new sealed output declaration: a
  supported scalar, or a runtime error that stops execution. The script
  never observes a Result. It is not a general declaration for arbitrary
  error types.
- Success returns the same string as legacy for every input, through the
  existing string import.
- Over the input cap or the output cap, execution stops in both engines.
  The message has one source. The control states the two diagnostics
  exactly; if the execution engine adds its source position, the row is
  classed by the existing rule for that, not by a new one.
- A stop leaves the session usable: bindings from earlier inputs keep
  their values, the failed input commits nothing, and the next input runs.
- The host receives the existing owned string. No new input mechanism.
- The declaration does not let a host function that returns a script
  Result be enrolled as if it aborted, or the reverse. A compile-time
  control shows each.
- The ten functions that stay refused keep their specific reasons.

## Fixed route

This record does not edit any inventoried item and does not change the
policy file. Runtime errors from a host body already travel through the
existing call path; the record declares the shape and does not add a
path. If that is not so, stop and bring the finding.

The Ir bands pass per record and cumulatively, as for every record.

## Out of scope

- time::from_parts, web::response, process::run, process::run_bytes,
  fs::read_bytes, the async functions.
- Changing escape_html to return a Result, in either engine.
- Manifest and lock files.

## Acceptance

- session_parity.json: the row leaves "Refused for a host function" and
  becomes Equal with an executed witness; host refusals fall from 11 to 10
  and Equal rises from 138 to 139.
- Controls, each compared between engines: empty text; each escaped
  character; already escaped text escaped again; Unicode and NUL; input at
  and over the cap; output over the cap from input under it; text from a
  local, a branch and an earlier input; use inside a template string; the
  session usable after a stop.
- API controls with compiling twins for the two negatives above.
- Miri only if the declaration adds an ownership or unwinding path the
  existing tests do not exercise; otherwise the record states why existing
  coverage holds.
- The fixed-route source condition and bands pass. Required stages follow
  the changed paths.
