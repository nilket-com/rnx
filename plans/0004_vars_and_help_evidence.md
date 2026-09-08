# rnx 0004 evidence: vars and help

Implementation of `plans/0004_vars_and_help.md`, 2026-09-08, on Linux
x86_64 against upstream Rune 0.14.1 with no patch. Status of the record
stays proposed until reviewed.

## What landed

- `src/inspect.rs`: `vars` and `help`, both pure reads over the session and
  the host registration. A `Budget` accumulates output under one total
  byte budget per invocation, refuses anything that would cross it, and
  appends a marker naming what was cut. Every branch writes through it,
  the unknown-name message included, and stored text is escaped into the
  remaining budget rather than escaped whole and then cut. `Work` counts
  the bindings visited, the values cloned, the values handed to the
  renderer, and the bytes read by the escaper, so the bounds are checked
  structurally rather than by output length or elapsed time.
  `InspectLimits` carries that budget, the binding count, and the
  per-value display limits, which are set tighter than a result's because
  a listing shows many values at once.
- `src/host.rs`: `install` returns `HostFunction { path, doc }` per
  function, both recorded by the same registration macro, so a function
  cannot exist without its description.
- `src/session.rs`: `binding_count`, `visit_bindings`, `binding`, and
  `declaration` read the session's tables. The count is the number of
  published names, which the session already keeps, so the total costs
  nothing. `visit_bindings` walks lazily and stops the moment the caller
  says to, so nothing past the last line printed is looked up and no value
  is cloned; a declaration reports its kind and retained source.
- `src/format.rs`: `terminal_safe_into` escapes control characters, escape
  included, while keeping newlines and tabs, and stops at a byte budget,
  returning how much of its input it read; `type_name` is now shared, so a
  type reads the same in a value's rendering and in its help.
- `src/repl.rs`: `COMMANDS` pairs each command with its one-line
  description, which is what `:help` prints and what completion offers. A
  command is the first word of the input, so `:help <name>` takes the rest
  as its argument.

## Defaults

| Bound | Value | Why |
| --- | --- | --- |
| Invocation budget | 8 KiB | A screenful or two, not a scrollback flood |
| Bindings per `:vars` | 64 | The same count the formatter shows per collection |
| Per-value budget inside a listing | 1 KiB | A listing shows many values; a result shows one |

## Gates

| Gate | Result | Where |
| --- | --- | --- |
| 1 `:vars` after work | pass: four bindings listed in name order with type and value, in the terminal and in unit tests; a fresh session and a reset session both report none | `tests/repl.rs::gate_0004_*`, `src/inspect.rs` tests |
| 2 `:help` on each kind | pass: binding shows type and value; function and struct show kind and retained source; `host::write_new` shows its registered description; `:memory` shows the command description; an unknown name gets the not-found message; no argument lists the commands | both |
| 3 invocation budget | pass: 200 small bindings are cut at 64 with the count marker; under a 200-byte budget `:vars` stops with the byte marker; a single binding whose rendering exceeds the budget is cut; a long declaration is cut with the marker; the command listing is cut under a 60-byte budget; a 20,000-character unknown name is bounded and escaped. Every output is within its budget plus the marker | `the_invocation_budget_bounds_every_command`, `an_oversized_unknown_name_is_bounded_and_escaped` |
| 3b work is bounded, not only output | pass: with 200 bindings, the total is the published-name count and `:vars` visits 65, the 64 it shows plus the one that stops it, cloning none; under a tight byte budget it visits fewer than 12; a visitor that stops after 3 is honoured after 3; help for a 1,000-line declaration under a 300-byte budget reads at most 300 bytes of it; the escaper consumes 64 bytes of a one-megabyte string and 3 escapes in a 20-byte budget | `src/inspect.rs::budget_tests` |
| 3c help renders only what it can print | pass: binding help under a 4-byte budget prints the marker and calls the renderer zero times, against exactly one call for the same binding with room to print it. The check counts the call rather than timing it, so scheduling cannot distort it | `src/inspect.rs::short_circuit_tests` |
| 4 source is terminal-safe | pass: a declaration with an escape byte in a string literal and one with an escape in a comment both display `\u{1b}` with no control byte in the output, and their line breaks and indentation survive. Neither declaration is called | `src/inspect.rs::declaration_source_is_escaped_for_the_terminal` |
| 5 state is followed | pass: an input that pushes to an existing shared binding and then panics leaves `shared: Vec = [1, 2]` visible in `:vars` and in its help while `fresh` is absent from both; a name that is both a binding and a declaration reports as the binding and says the declaration exists; after `:reset` only commands and host functions have help | `src/inspect.rs` tests, `tests/repl.rs` |
| 6 available over the bound | pass: with the session past its memory threshold, so that evaluation is refused, `:vars` and `:help` still answer | `src/inspect.rs::both_commands_answer_when_the_session_is_over_its_bound` |
| 7 no execution | pass: a binding holding a closure that writes a file is listed and described, and the file does not exist afterwards | `src/inspect.rs::inspection_runs_no_code` |
| 8 nothing regresses | pass: the 0002 and 0003 gates are unchanged and green | whole suite |

Test counts: 39 unit tests, 7 pseudo-terminal tests, all green; `cargo fmt
--check` clean; the spike self-checks pass.

## Limits stated

- A type shown is the value's runtime type by its last path component,
  `Vec` for `::std::vec::Vec`. Rune exposes no declared or inferred type on
  a value, and no arity or signature on a function value, so a binding
  holding a function shows only that it is a function.
- Host descriptions are text rnx carries at each registration. Upstream
  offers `docs` and `args` on its module builder, but they feed Rune's own
  documentation system and cannot be read back, so they are not the source.
- `:vars` lists bindings only. Declarations are reached through `:help`,
  which is where their source belongs.
