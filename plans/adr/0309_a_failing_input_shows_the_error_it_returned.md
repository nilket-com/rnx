# ADR 0309: A failing input shows the error it returned

Status: Proposed

Base: published 50a959a5c5c7e36e4b1d9d9c0029aed24bfb5d32 (ADR 0308).

## Problem

A `?` that fails at the top level of an execution-session input never
shows the error. The input's generated entry returns a `(delta, value)`
envelope, and an `Err` that `?` propagates replaces the envelope.
Neither consumer checks for that:
- `DomainSession` reads slot 0 of the output as the delta. A failing
  `polars::DataFrame::height()?` prints a Store type diagnostic:
  `Error { kind: Type, … operation: "store" … }`.
- One-shot `FreshValue::show` reads child 1 of the root before its own
  `Result` handling.

Every fallible call reachable at the prompt hits this: `read_csv`,
`height`, `http`, `process`. ADR 0308 pinned the current diagnostic as
a known defect.

File runs already report a returned string error well.
`src/runner/execution.rs` prints `error: <text>` with no position and
exits 1, through the bounded `format::execution::error_text`. That
function handles a string specially and otherwise renders the value's
shape, so a native error (an opaque host value) becomes a `<type>`
token, and its message is lost.

The old engine also hides the error, with a tuple/result type mismatch.
By the standing rule, the old engine is not maintained for this.

## Decision

1. Both execution consumers distinguish an outer `Err` from the normal
   `(delta, value)` envelope before reading either slot.
   - An `Err` returned as an ordinary expression inside a successful
     envelope keeps each frontend's current behavior.
   - The boundary cannot tell whether an outer `Err` came from `?` or
     from an explicit top-level `return Err(...)`, and no source-syntax
     tracking is added. Both get the same report if the explicit return
     is admitted. It is probed first: if it is refused today, the
     refusal stands.
2. An outer `Err` is reported in the form a file run uses:
   `error: <text>` on stderr, unplaced. A small execution-side formatter
   is used for the outer `Err` only:
   - For a checked host leaf, it uses the type's checked display
     (`display_host`) within the preview byte limit, finished through
     `present::finish` into bounded, terminal-safe text. If the display
     fails, it falls back the same way.
   - Otherwise it uses `format::execution::error_text`.
   - There is no presenter registry, and no script method such as
     `kind()` is invoked.
   - File runs and inner-`Err` rendering are unchanged.
3. In a session, the report is a distinct returned-error `Failure`,
   displayed as `error: ...` with no position. The worker keeps
   classifying it as a runtime failure. When cleanup also fails, the
   existing precedence applies. The failed branch is taken right after
   the run, before the envelope is read, so it publishes no delta or
   declarations and still releases the program.
4. A failing session input publishes no new bindings. Earlier bindings
   stay usable. Effects that ran before the `?` happen once and are not
   repeated. The session continues to the next input, as after any other
   input failure.
5. One-shot `rnx eval` on the execution engine prints the same line and
   exits non-zero, matching a file run's exit code for a returned error.
6. Execution only. The old engine, its diagnostics and its routing are
   unchanged, and no old-engine output is pinned. The old engine's
   output is measured once for the record.

## Constraints

- Presentation and session-envelope handling only. No interpreter,
  opcode, Store layout, frontend proof or host bridge change.
- If anything protected moves, it gets an exact source-condition
  transition, reviewed before its screen. Early fixed-route screen.
- No new crate. Tabs.

## Controls

- Real `rnx-polars repl --engine=execution`:
  - A retained `read_csv` frame, then a failing top-level `?` on
    `height()` of the 64-bit overflow fixture shows
    `error: height: 9223372036854775808 does not fit a script integer`.
  - The frame is usable in the next input.
  - A `let` in the failing input before the `?` is not bound afterwards.
  - A `println!` before the `?` runs exactly once.
  - The binding from an earlier input is still usable after the
    failure.
- One-shot `rnx eval` on the execution engine, with an admitted `String`
  error (`Err("control")?`): `error: control` on stderr, a non-zero exit
  equal to a file run's, and nothing on stdout.
- A successful input whose value is an `Err` (no `?`) keeps its current
  rendering in both the session and one-shot.
- A native error whose display is long, or contains control characters,
  is shown bounded and escaped. One whose display fails gets the
  fallback.
- An explicit top-level `return Err(...)`, if admitted, gets the same
  report. If refused, the refusal is unchanged. The shape of the
  returned root is checked on the execution engine only, if needed.
- 0308's pinned Store diagnostic is replaced by the new line. No
  old-engine golden.
- A mutation that drops the outer-`Err` check fails its control.
- One complete final.
