# ADR 0308: A frame reports its height on the new engine

Status: Proposed

Base: published aae80905b55cea5cbe611791f9f8c866c6e6c6d7 (fix after ADR 0307).

## Problem

ADRs 0306 and 0307 let a new-engine session load a real CSV with
`polars::read_csv(path, schema)?` and keep the frame, but the only checked
read on it is `width()`. The sales notebook's next step reads the row
count: `kept.height()?` and `sales.height()? - orders`. The model demo
does the same. `height()` still takes the old route.

The generated binding returns `Result<i64, polars::Error>`, and both
failures are real: the engine thread's `EngineError` or `CallbackError`,
and a row count that does not fit a script integer (`widen`'s conversion
error). A frame whose columns are broadcast scalars can have more rows
than `i64::MAX` without allocating them. `width()`, by contrast, is
already a checked read (0305), because a column count is a proven length
and cannot fail.
`polars::Error` (`adapters/polars/src/generated/support.rs`) is the
adapter's own Rune type: `kind()` is the Polars variant name, and its
display is Polars's message. The checked path cannot carry a native error
type yet. 0306's fallible producer carries only `Result<T, String>`, and
0305's checked reads return closed scalars. Turning the error into a
string, or moving it to the execution-error channel, would change what
scripts observe.

## Decision

1. `polars::Error` is enrolled as a checked native type for the new
   engine. Its charge covers its inline size and both strings' capacity.
   Its display is the existing message, registered once, and `kind()` is
   a checked shared read returning a string. `message()` stays on the
   old route.
2. A checked shared read may return `Result<S, E>`, where `S` is a closed
   scalar and `E` an enrolled native error type. `Ok` is the scalar. `Err`
   imports a uniquely owned `E`. The registration carries separate
   pre-erasure metadata certifying `&T -> Result<S, E>`: a closed `S`,
   and the registered receiver and error type hashes and Rust `TypeId`s,
   checked before insertion, with one effective handler. Enrollment is
   carried into the binding's sole registration, by 0305's rule. The
   cold host output builds the `Ok` scalar or the owned `E` inside its
   `Result`, and rolls back if the wrapper cannot be allocated.
3. The typed error is admitted in two places only, both in the session
   input itself: `?` on the checked call, and an exhaustive `match` on the checked call whose `Ok` arm binds the
   scalar and whose `Err` arm binds exactly `E`, for `kind()` and
   display. A `?` on a typed-error read inside an ordinary script
   helper is refused at admission, before effects, including a helper
   that obtains its own frame. The helper's implicit return could carry
   `Err(E)`, and a later input could put that return in a container or
   pass it to a host function, which a proof of the call alone cannot
   guard. A helper that catches the error and returns a plain value
   stays eligible. Propagating the typed error through script helpers
   needs return proofs across calls, function values and retained
   programs, and is a later record.
   A failing top-level `?` keeps today's session diagnostic on each
   engine. On neither engine does it display the contained error: the
   old engine reports a tuple/result type mismatch from its entry
   wrapper, the execution engine a Store type diagnostic. 0308 does not
   change session error handling. Showing the contained error is a
   later record on the execution engine only; the old engine is not
   maintained for it.
4. The first use is the generated `DataFrame::height()`, under its
   existing name, enrolled exactly once. The generator emits the
   registration attribute, as 0305 did for `width()`. The witness is
   real: `read_csv` in one input, then `frame.height()?` in a later one.
   The `Err` path uses the real method: on the 64-bit test host, a
   test-only factory builds a zero-column frame with Polars' public
   `DataFrame::empty_with_height(i64::MAX as usize + 1)`, and the
   generated `height()` returns its deterministic conversion error.
5. `StoreCapacity` stays a distinct terminal error, never a
   `polars::Error`. Shape, column names and other outputs are later
   records. Outside sessions, the old route is unchanged.

## Constraints

- Registration-time, cold host-output and frontend-proof changes only,
  as in 0305 and 0306. No interpreter, opcode or Store layout change. Any
  inventoried item touched needs an exact source-condition transition,
  reviewed before its screen. Early fixed-route screen.
- No new crate. Tabs.

## Controls

- In the real `rnx-polars repl --engine=execution`, against explicit
  legacy, with stderr equal:
  - `height()?` on a retained `read_csv` frame;
  - the direct exhaustive `match` on the conversion error, with
    `kind()`, the bare session rendering and explicit display of the
    `Err` binding, as in 0305;
  - reset.
- A failing top-level `?` on `height()`, in the execution REPL only:
  its current diagnostic is pinned, and the retained frame is still
  usable in the next input. The old engine's output for it is a
  measurement only, never a pinned control.
- A `?` on `height()` inside a script helper, including one that builds
  its own frame, is refused at admission before any effect. A
  retained helper defined in one input and called from a later one is
  refused the same way. A helper that catches and returns a plain value
  is admitted.
- A capacity miss is `StoreCapacity`, never a `polars::Error`.
- Rejected registration shapes: a non-enrolled error type, a non-scalar
  `Ok`, an owned or mutable receiver.
- The wrapped `Result` stays out of containers, closures and host
  arguments before effects, as in 0306.
- File routing for `height` stays on the old route with one effect.
- The inventory admits exactly the new sites, each with a live witness.
- A mutation per rule fails its control.
- One complete final.
