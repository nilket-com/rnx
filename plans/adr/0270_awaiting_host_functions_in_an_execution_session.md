# ADR 0270: awaiting a host function in an execution session

Status: Proposed

Base: published ee4cf69ed5950aa62e9f83f3f54d16de1964933e (ADR 0269).

## Problem

Five of the ten host functions an execution session still refuses are
refused for one reason, "async": `time::sleep`, `http::get`,
`http::get_bytes`, `http::request` and `http::request_bytes`. Each is
registered as a function that returns a future, used as
`time::sleep(5).await`. The execution engine has no await: the session
refuses the syntax as "async input" and each function by name.
A session that cannot fetch a URL or pause is not usable for the work
sessions are for.

## Goal

In an execution session, awaiting a call to an asynchronous host
function, written `f(args).await`, runs the function to completion and
yields its result, with the same observable behavior as the legacy
session.

## Required behavior

- Scope is the direct form only: `.await` applied to a call of an
  asynchronous host function. A future as a value (bound, stored,
  passed, returned), asynchronous blocks and functions, and `select`
  stay refused, each by its own name. A call to an asynchronous host
  function that is not awaited is refused at compile time, with a
  message that says to await it. The message is not worse than the
  legacy one for the same input.
- The form works wherever an expression of the input itself does: at
  the top of an input, inside a block, a branch or a loop, as an
  operand, with `?` applied to its result. Inside an ordinary function
  the legacy compiler rejects `.await`, and that diagnostic is kept.
- Results equal legacy for: `time::sleep(0).await`, a sleep outside
  its range, a pause of some tens of milliseconds (elapsed time
  compared by a committed property, as the clock rows are), `http::get`
  and `http::request` against a listener the test starts on the
  loopback interface, a refused connection, a malformed URL, and the
  session's request bounds. No test reaches another host.
- The two functions that return bytes stay refused, now for the reason
  "bytes". If `http::request` cannot take its options object without
  the same executor change the other object inputs wait for, it stays
  refused for that reason and the note says so.
- Interrupting while a call is pending ends the input in bounded time.
  Bindings, the store and the session's request scope are left as the
  legacy session leaves them, and the next input runs. The delay from
  interrupt to return is measured and stated.
- Arguments are evaluated once, before the wait. Nothing borrowed from
  the store is held by the pending operation. A failure or interrupt
  during the wait releases what the call owned, once.
- Waiting spends no work units and is not cut by the work slice. The
  note states how the wait relates to the slicing that an earlier
  investigation found unsafe for nested asynchronous calls, and why
  this design cannot resume a pending operation in the wrong context.
- No thread and no runtime is created per await.
- A program that awaits nothing pays nothing. The fixed-route bands
  show it, with no exception and no policy edit.
- No path reachable from user input panics.
- The note names every inventoried item the design would touch. If any
  is touched, send the note and stop: that is the project owner's
  decision and it is still open.

## Not in this record

Futures as values, asynchronous blocks and functions (so no await
inside a user function), `select`, concurrent requests, bytes, and the
object and vector inputs of the other refused host functions.
