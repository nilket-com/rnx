# Implementation plan 0309: A failing input shows the error it returned

Status: Implemented

Paired ADR: ../adr/0309_a_failing_input_shows_the_error_it_returned.md.

1. Boundary note, read-only. Cover:
   - where each consumer reads the envelope;
   - how an outer `Err` is recognized;
   - the `Failure` used for a session report and its display;
   - one-shot's exit path;
   - binding staging, and whether anything protected moves.
2. Probe whether an explicit top-level `return Err(...)` is admitted.
   Measure the old engine's outputs for each control once, as a record
   only.
3. The session and one-shot handling, then the early fixed-route screen.
4. Session, one-shot, inner-`Err`, binding, bounded native-display and
   explicit-return controls. Replace 0308's pin. Add the mutation.
5. fmt, Clippy, the suites, preflight and the commit checker. Sign.
   Source review, one complete final, Results.

## Results

On the execution engine, an input whose `?` fails at the top level,
or that returns `Err(...)` explicitly, now shows the error it returned.
It prints `error: <text>`, the form a file run uses, with no position.
Before this record the same input printed a Store type diagnostic. An
explicit `return Err(...)` was already admitted, and was measured to
reach the same diagnostic. The boundary cannot tell the two forms
apart, so both get the same report.

How the outer `Err` is found:
- `format::execution::entry_error` reads the entry result's root kind
  and takes the payload of a `Result` `Err`.
- `DomainSession` checks right after the run, before reading the
  envelope. The failure is `Failure::Returned`, displayed as
  `error: ...` with no position, and classified as a runtime failure
  with no origin in both the worker and transcripts.
- The early return goes through the existing program release and
  cleanup, so the failed input publishes no bindings or declarations.
- One-shot `rnx eval` checks before reading the envelope's value slot.
  It exits 1, with nothing on stdout, as a file run does.

How the text is produced:
- A checked host error uses its type's display within the preview byte
  limit, finished into bounded, terminal-safe text. A display that fails
  falls back to the type label. This helper is shared with the session's
  existing bare-value path.
- Other values use the existing bounded `error_text`.
- No script method is invoked.
- File runs, and an `Err` returned as an ordinary value, are unchanged.

What a session shows, in the real Polars REPL:
- `let kept = polars::read_csv(...)?` in one input.
- A later input binds `absent`, prints `once` and then fails on
  `height()?` of the 64-bit overflow frame. It shows
  `error: height: 9223372036854775808 does not fit a script integer`.
- `once` is printed exactly once, `absent` is unbound afterwards, the
  overflow frame's `width()` still works, and `kept.height()?` is 3.
- This replaces 0308's pinned Store diagnostic.
- In one-shot, `Err("control")?`, `return Err("control");` and a bare
  `Err("control")` each exit 1 with `error: control` and empty stdout.
- A session unit control shows a long native display with control
  characters escaped and bounded, and a failing display falling back
  to the type label. An inner `Err("inner")` still renders as a value,
  and an explicit return reports `error: explicit`.

Scope:
- Only the execution engine changed. The old engine, which still
  reports a tuple/result type mismatch, is unchanged, and none of its
  output is pinned.
- Nothing under `rune/` changed.

A mutation that disables the outer-`Err` check fails both the one-shot
control and the native-display session control. The early fixed-route
screen passed against 50a959a5, with both bands 32/32. The complete
final passed all 101 stages: 40.5 minutes of accepted run, 41.5 end to
end. Both fixed-route bands held, with no memory.high, memory.max or
OOM events.
