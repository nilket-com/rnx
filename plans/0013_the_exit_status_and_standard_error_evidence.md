# rnx 0013 evidence: the exit status and standard error

Implementation of `plans/0013_the_exit_status_and_standard_error.md`,
2026-09-08, on Linux x86_64 against upstream Rune 0.14.1 with no patch.
Status of the record stays proposed until reviewed.

## What landed

- `src/host.rs`: `host::exit(code)` and `host::eprint(text)`, and a flag
  saying whether rnx is running one thing or holding a prompt.
- `src/runner.rs` and `src/main.rs`: `run` and `eval` set that flag. The
  session does not, so `host::exit` is refused there.
- `src/complete.rs`, `src/inspect.rs`, `src/text.rs`: the counts moved from
  eight to ten and the two new paths joined the list whose description has to
  name a result.
- `tests/exit_status.rs`: eight gates, new file. `tests/repl.rs`: the session
  gate.
- ket: the summarizer exits 2 for a usage error and 1 with an empty standard
  error, and `compare.sh` lost the exception machinery it needed to describe
  a difference that no longer exists.

## Gates

| Gate | Result | Where |
| --- | --- | --- |
| 1 a chosen status reaches the shell | pass: 0, 1, 2, 7, and 255 each arrive as themselves and say nothing | `a_chosen_status_reaches_the_shell` |
| 2 a script can fail quietly | pass: a report on standard output, status 1, standard error empty | `a_script_can_fail_quietly` |
| 3 nothing printed is lost | pass: output with no trailing newline still arrives | `output_with_no_trailing_newline_is_not_lost` |
| 4 an out-of-range status is refused, not truncated | pass: 256 does not become 0, 300 does not become 44, and -1 does not become 255; each is refused, the message names both the number and what it would have become, the process exits nonzero, and what the script printed first is still there | `a_status_outside_the_range_is_refused_rather_than_truncated` |
| 5 the refusal does not depend on the script handling it | pass: an impossible status with the result discarded, and with it bound and ignored, still ends the script, still exits nonzero, and the line after the call does not run; the same from `eval` | `an_invalid_status_ends_the_script_even_when_the_result_is_discarded`, `an_invalid_status_from_eval_also_fails` |
| 6 lost output is never reported as success | pass: with standard output on a stream that refuses every write, a script printing and asking for 0 exits nonzero and names the lost output; one asking for 2 keeps 2 and names it too | `output_that_could_not_be_written_never_reports_success`, `a_status_that_was_already_failing_survives_a_failing_stream` |
| 7 nothing after it runs | pass: a print after the call does not appear | `nothing_after_the_exit_runs` |
| 8 refused at the prompt, and the prompt survives | pass: the session says it is a session and names `:quit`, for a valid status and for an impossible one alike; the next input typed is still the person's own; `host::eprint` is allowed there | `tests/repl.rs::gate_0013_the_exit_status_is_refused_at_the_prompt_and_the_prompt_survives` |
| 9 allowed from `eval` | pass: `rnx eval "host::exit(5)"` exits 5 and says nothing | `eval_may_choose_a_status` |
| 10 `eprint` writes exactly what it is given | pass: two calls, no newline added, braces and backslashes and quotes verbatim, standard output untouched | `eprint_writes_exactly_what_it_is_given` |
| 11 both can be found at the prompt | pass: the completion candidates are the registered paths and the test compares the two lists; `:help` describes each from its registration | `complete::host_source_tests`, the session gate, `text::description_tests` |
| 12 the second port closes record 0011's second finding | pass: nineteen of nineteen cases now agree on the exit code, with no exceptions, and ten expect nothing at all from the port | below |
| 13 nothing regresses | pass: 74 unit tests, 16 pseudo-terminal gates, 12 runner diagnostics, 8 standard-input gates, 12 exit-status gates, 3 run-output, 2 upstream reproducer, formatting clean. A script that never calls `host::exit` keeps the status record 0009 gave it | whole suite, `a_script_that_never_asks_keeps_the_status_it_always_had` |

## Controls

| Removed | Result |
| --- | --- |
| the range check | `a_status_outside_the_range_is_refused_rather_than_truncated` fails |
| the range refusal made an ordinary error again | the two gate 5 tests fail, and nothing else |
| the flush error ignored again | the two gate 6 tests fail, and nothing else |
| `eprint`'s verbatim writing, appending a newline instead | `eprint_writes_exactly_what_it_is_given` fails |
| the bare `error: ` line put back in the port | 12 checks fail in the comparison, 6 of them naming the port speaking where the original was silent |

### The control that did not fire, and the gate that was wrong

The first version of this cut ignored the result of the flush, and the
control for it did not fire: removing the flush entirely left gate 3 passing.
The conclusion drawn was that ending the process flushes standard output
anyway, so the explicit flush was insurance rather than mechanism. That was
checked the way record 0012's lesson demands, and both halves of the check
were sound. A standalone Rust program that prints without a newline and calls
`std::process::exit` does deliver its output, to a terminal and through a
pipe.

The conclusion was still wrong, because the gate was. Gate 3 asked whether
output arrives on a stream that works, and on a stream that works both
versions behave alike. The property worth holding is not that output arrives
but that losing it is never reported as success, and no gate asked that.
Codex asked it, with standard output pointed at a device that accepts an open
and refuses every write, and the answer was that a script printing a report
and asking for 0 exited 0 with the report gone.

So the flush is load-bearing after all, for a reason the first analysis
missed: ending the process does flush, and discards the error while doing so.
Doing it here is what makes the failure visible. Gate 6 is the missing
question, and removing the error handling fails it.

The lesson is not the one record 0012 recorded, which was about proving a
control landed. This is the next one along: a control that does not fire says
either that the code does not matter or that the gate does not ask enough,
and only the second was true here.

## Two defects this cut shipped and had to fix

Both were found in review, after the suite, the formatting, and all nineteen
comparisons passed. Neither was a mistake in the decisions; both were the
decisions not actually being delivered.

**An invalid status did not guarantee failure.** Decision 2 said a refused
status is itself a failure. The code returned an ordinary Rune error, and an
error is a value a script can discard. `host::exit(256)` followed by a
`println!` printed and exited 0, and `rnx eval "host::exit(256)"` printed the
error as a value and exited 0. Every test used `?`, so all of them tested
propagation and none tested the guarantee. The refusal now ends the script
itself, and gate 5 exists.

**A failed flush was ignored.** `let _ = stdout().flush()` allowed success
after losing output. Gate 6 exists, and the rule is in decision 5.

## Gate 12, the second port

| | Before | After |
| --- | --- | --- |
| Cases agreeing on the exit code | 15 of 19 | 19 of 19 |
| Recorded exit-code exceptions | 4 | 0 |
| Cases expecting nothing from the port | 4 | 10 |

The four exceptions were usage errors: `argparse` exits 2 and a Rune script
could only exit 1. The port now exits 2. The six other changes are the bare
`error: ` line, which existed only because a failing status required a printed
error; those cases now carry their whole report on standard output and say
nothing, as their original does.

The comparison lost the machinery that described the exception, and gained a
stronger check in its place: where the original says nothing, the port says
nothing. That is wider than the check it replaces, which looked only at
successful cases, and putting the bare `error: ` line back fails six cases on
it.

## Limits stated

- The exit is abrupt. Nothing unwinds and no destructor runs. `host::process`
  is synchronous and leaves no child behind, and nothing else in the host
  holds a resource whose release is observable, so the risk is theoretical
  today. It stops being theoretical the first time the host owns something
  that must be released.
- A status is refused rather than truncated, and there is no flag to turn that
  off. A truncated status is a silent wrong answer. The refusal ends the
  script rather than returning a value, so it cannot be discarded.
- Gate 6 needs a stream that accepts an open and refuses every write, and it
  uses `/dev/full`, which is a Linux device. A platform without one needs an
  equivalent before that gate means anything there.
- The session refuses `host::exit` and allows `host::eprint`. The rule is
  about whether rnx is running one thing or holding a prompt, not about which
  module is loaded, which is why `eval` is allowed.
- The port's diagnostics still differ from the original's in wording. Only
  the status, the report, and the silence are claimed to match.
