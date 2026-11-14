# ADR 0273: async functions awaited directly in an execution session

Status: Proposed

Base: published f7a8e42fb57b035fe2fbe59cb2c02a885d485a6c (ADR 0272).

## Problem

ADR 0270 let an execution session await `time::sleep`, `http::get` and
`http::request`, but only in the input expression itself. A user cannot
put a wait inside a function:

	async fn fetch(url) { http::get(url).await }
	fetch("http://127.0.0.1:8080/").await

The declaration is refused as an async function, so any helper around
a request or a pause forces the legacy session. This is the ordinary
way such code is written.

## Decision

An execution session accepts `async fn` declarations and the direct
form `f(args).await`, in the input and inside other async functions.
Inside an async function, the direct waits of ADR 0270 and direct
awaits of other async functions are accepted.

A future is still not a value. Calling an async function without
awaiting it at the call, binding its result, passing it, returning it
unawaited, or taking the function as a value where it could be called
without an await is refused by name, with the remedy to await the call
directly, on the model of the ADR 0270 refusals. `.await` in an
ordinary function keeps the legacy compiler diagnostic. Async blocks,
async closures, `select` and joining several futures stay refused.

Because the call and its await are one expression, the body runs at
that point, once, with the arguments evaluated first, left to right.
That is what the legacy session observes for the same expression. The
implementer establishes this by measurement before relying on it, and
reports any observable difference (order of effects, errors, moved
arguments, interruption) for a ruling.

## Constraints

- Interruption stays terminal and prompt inside nested async
  functions: a pending wait three calls deep is interrupted within the
  existing cadence, no catch observes it, earlier bindings survive.
- An async function defined in an earlier input is callable from later
  inputs, like any function; redefinition follows the existing rules.
- Depth is bounded by the existing call and nesting limits, with a
  named refusal or error, never a stack overflow. Async recursion is
  measured beside legacy and the difference stated.
- No per-call runtime, thread or task is created. Work slicing between
  instructions is unchanged and no pending wait is retried.
- No inventoried executor item or policy file changes. If one must,
  send that and stop.
- Every refusal names the function and a remedy that works, through
  the remedy module of ADR 0272 where a switch of engine is advised.
- Parity rows are observed and classified by measurement.

## Controls

- The example above against a loopback listener; nested async
  functions; an async function across inputs; arguments with effects;
  a script error and a `?` inside the body; early return.
- Each refused form (bare call, bound, passed, returned, function
  value, async block, async closure) refused before any effect, with
  its exact text and location.
- Interruption at depth with a real signal; the bound on depth at its
  exact frontier on the 2 MiB stack and the headroom stack.
- The session ordinary-function path: frames and stack bytes of
  ordinary call lowering unchanged beside the base.
- Miri over the new ownership paths at small sizes.

## Out of scope

Futures as values, concurrency of any kind, async closures and blocks,
bytes.
