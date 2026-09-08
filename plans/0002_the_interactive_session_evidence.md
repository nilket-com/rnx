# rnx 0002 evidence: the interactive session

Implementation of `plans/0002_the_interactive_session.md`, 2026-09-08, on
Linux x86_64 against upstream Rune 0.14.1 with no patch. Status of the
record stays proposed until reviewed.

## What landed

- `src/session.rs`: the adapter builds each unit through a `Generated`
  source that records a `SourceMap` segment for every piece of user text,
  keyed by input number and byte offset. Declarations carry the input they
  were last entered in. Every unit is retained with its map before it runs,
  so a runtime error inside a function or closure from an earlier input maps
  through that unit's debug info to the earlier input; retention precedes
  the run because a failed or interrupted input can leave closures over the
  unit reachable through shared handles. Execution runs in slices of 10,000 instructions; between
  slices the interrupt flag is checked; the budget for one input is two
  billion instructions.
- `src/format.rs`: a bounded renderer over `Value::as_type_value` and typed
  borrows. Limits are applied before anything is copied or descended into,
  wrapper types included: a million-element vector copies 64 items, a
  hundred-thousand-entry object examines 64 entries, and a ten-megabyte key
  copies no more than the string limit, with the omitted count taken from
  the key's byte length rather than inferred from the copy, so a multibyte
  character at the cut cannot hide the marker; each is counted by the
  renderer's own work counters in tests rather than timed. An object within the length
  limit renders with its keys sorted; a larger one previews the first
  entries in the map's own order, sorted among themselves, since choosing
  the globally first keys would mean examining the whole object. Object
  keys are quoted and escaped like strings. It holds
  no VM and calls no protocol.
- `src/repl.rs`: rustyline 18.0.1 (MIT) with a `Validator` that decides
  completeness on the original input, history persisted as text, and the
  session commands `:quit`, `:reset`, `:memory`, `:debug`. `:begin` and
  `:end` are gone.
- `src/host.rs`: the interrupt flag is read by the session and cleared before
  each evaluation; `host::process` no longer clears it itself.
- `tests/repl.rs`: the gates below, driven through a pseudo-terminal opened
  with `openpty`, with control sequences stripped before comparison.

## Mechanisms, with what the pinned source supports

**Slices.** `VmExecution::resume` under `budget::with(n, ..)` returns an
error on budget exhaustion and leaves the execution resumable; a probe ran a
300,000-iteration loop to the correct result across 211 slices at the same
speed as one unbudgeted call (9.6 ms against 9.7 ms), while single-stepping
took 93.9 ms. The halt is recognised structurally: the error carries no
location (a halt is constructed without unwinding), and the slice's budget
guard reads exhausted. Message text is never consulted. Completion is a
slice boundary too, so an interrupt that arrived during a host call is
observed when the input completes.

**Completeness.** `compile::Error::kind` is crate-private in 0.14.1, so the
closed list is enforced by the shape of the first error's span on the
original input: a zero-width span at the end is an expected-token-at-eof or
unexpected-eof; a span reaching the end that still reaches the end when a
newline is appended is an unterminated string, template, or block comment;
everything else is a diagnostic. A real token at the end, such as the `;` in
`let x = ;`, keeps its span when the input grows and is a diagnostic. A
`let` without its `;` is incomplete under this rule, which is Rune's grammar
speaking; the test names it.

**Interrupt latency.** Pure Rune: one slice of 10,000 instructions, about
50 µs at the measured 200 million instructions a second; the unit test
bounds the observed stop at 500 ms from the flag and passes in far less.
Host calls: `host::process` has its own cancellation path and is cancelled
by the same flag; `host::read`, `host::write_new`, `host::mkdir`,
`host::absolute`, `host::json_parse`, and `host::json_stringify` run to
completion first. The pseudo-terminal gate stops a 30-second child in well
under 2 seconds.

## Gates

| Gate | Result | Where |
| --- | --- | --- |
| 1 ordinary session | pass: vector and struct, three-line function through the continuation, recall with the up arrow and edit of the body, struct displayed by fields, syntax error marked at line 1 column 9, infinite loop stopped by Ctrl-C, function called again; the five incomplete forms continue and the two invalid controls diagnose at once | `tests/repl.rs::gate_1_the_ordinary_session` |
| 2 history across sessions | pass: inputs recallable after reopen, function undefined, a `host::write_new` entry in history produces no file on restore | `gate_2_history_survives_and_state_does_not` |
| 3 display limits | pass: 10,000-element vector cut at 64 with `+9936 more`; 20-deep object cut at depth 8 with the field count; 10 MB string cut at 4,096 bytes with the byte count; total output refused past 16,384 bytes with a marker naming the budget, not a count; self-containing vector prints `<cycle>`; a range prints `<::std::ops::Range>`; a host type with a live `DEBUG_FMT` protocol renders as an opaque marker with the protocol never invoked, while the control through the VM invokes it | `src/format.rs` tests |
| 4 position mapping | pass: a closure retained by an input that then failed maps to that input; compile error on line 2 column 9 of a three-line input reports it; a runtime error in a function defined two inputs earlier reports input 1, line 2, column 3; an error through a retained closure reports the closure's input; ten refusal and failure forms all carry a user position and never generated text | `src/session.rs` tests |
| 5 interruption | pass: a loop is interrupted at a slice boundary with the shared push before it surviving and the `let` unpublished; a Ctrl-C at the prompt does not cancel the next input; a waited child is cancelled from the terminal; the input after each interruption runs to completion. The interrupt flag is process-global, so these run through the terminal harness, one process per session, never in the unit tests | `tests/repl.rs::gate_5_*` |
| 6 Ctrl-C at the prompt | pass: an open three-line input is cleared and the next input evaluates on its own | `gate_6_ctrl_c_clears_an_open_input` |
| 7 nothing regresses | pass: the spike's session checks run unchanged under the default command, with the budget set to the spike's two million for its infinite-loop check | `rnx` with no arguments |

Test counts: 19 unit tests, 5 pseudo-terminal tests, all green; the
formatter is pinned by `rustfmt.toml` and `cargo fmt --check` is clean.

## Findings against upstream, not defects here

- A struct literal assigns values by position, not by field name: with
  `struct P { y, x }`, `P { x: 1, y: 2 }` gives `p.x == 2`, and Rune's own
  debug output agrees. The renderer reads slots by declared name, matching
  field access. Worth an upstream issue.
- `panic!` does not capture template variables: `panic!(`bad ${x}`)` fails
  to compile with "No local variable `x`"; `panic!("bad {}", x)` works.
- A script `impl P { fn debug_fmt(self, f) }` is not consulted by `{:?}`;
  only host types can define the protocol, which is why gate 3's control is
  a host type.
- A function value exposes no name or arity, so functions render as
  `<function>`, as the record allowed.
- Object iteration order is the map's and is not stable across processes;
  keys are sorted for display within the previewed subset.

## Not done, by the record's own scope

Completion, `:vars`, `:help`, highlighting. Mac and Windows: the line
editor and the pseudo-terminal harness are exercised on Linux only, as the
record states.

**The first release record's memory gate is open.** `:memory` reports
source and map storage only: inputs, declaration sources, generated sources
and their maps. Values held by bindings and compiled unit storage are not
measured; a retained 1.6 MB string reports a few hundred bytes. The bound
is therefore a bound on that storage, not on retained memory, and gate 3 of
the first release record is not met by this cut.
