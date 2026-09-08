# rnx 0012 evidence: standard input

Implementation of `plans/0012_standard_input.md`, 2026-09-08, on Linux x86_64
against upstream Rune 0.14.1 with no patch. Status of the record stays
proposed until reviewed.

## What landed

- `src/host.rs`: `stdin_read`, registered as `host::stdin`. It refuses a
  terminal before reading anything, refuses a second read, reads to the end
  under the same eight mebibyte limit as `host::read`, and refuses input that
  is not UTF-8 in the same words.
- `src/complete.rs` and `src/inspect.rs`: the two assertions that count the
  registered host functions moved from seven to eight. Neither needed a new
  name added by hand, because completion and `:help` both come from the
  registration; the counts are what noticed the change.
- `src/text.rs`: `host::stdin` joined the list of registered functions whose
  description has to name a result.
- `tests/stdin.rs`: seven gates, new file.
- `tests/repl.rs`: the session gate.
- `nilket/crates/toron/scripts/bench/summarize_runtime_filter_perft.rn` in
  ket: reads standard input when given no path, as the original does.

## Gates

| Gate | Result | Where |
| --- | --- | --- |
| 1 a pipe is read | pass: `a\nb\n` arrives whole; a stream with no final newline keeps not having one; `café` survives as characters | `a_pipe_is_read_exactly` |
| 2 a redirected file is read | pass: the same text `host::read` gives for the same file | `a_redirected_file_reads_as_the_same_text_host_read_gives` |
| 3 an empty stream is not an error | pass: exit 0, an empty string, nothing on standard error | `an_empty_stream_is_not_an_error` |
| 4 a terminal is refused, and immediately | pass: under a pseudo-terminal a script exits 1 naming the terminal and what to do instead, inside a bounded wait; at the session prompt the same, and the next input typed is still the person's own | `a_terminal_is_refused_and_does_not_block`, `tests/repl.rs::gate_0012_standard_input_is_refused_at_the_prompt_and_the_prompt_survives` |
| 5 a second read is refused | pass: the first read prints, the second fails naming that it was already read, and an empty stream does not carry that message | `a_second_read_is_refused_and_is_not_the_empty_stream` |
| 6 the limit holds, and the read stops at it | pass: exactly 8 MiB reads and reports its length; one byte more is refused in `host::read`'s words; and a pipe carrying one byte past the limit with its write end held open is refused before any end-of-file arrives, which a file cannot show because a file ends | `the_limit_holds_at_its_boundary`, `the_read_is_bounded_and_does_not_wait_past_the_limit` |
| 7 input that is not UTF-8 is refused | pass: the message says invalid UTF-8, the script printed nothing, and no replacement character appears | `input_that_is_not_utf8_is_refused_rather_than_replaced` |
| 8 it can be found at the prompt | pass: the completion candidates are the registered paths and the test compares the two lists, so `host::stdin` is a candidate by construction; `:help host::stdin` names it and shows `-> Result<String>` | `complete::host_source_tests`, the session gate, `text::description_tests` |
| 9 the second port closes its gap | pass: the summarizer reads standard input when given no path; the comparison went from sixteen cases to nineteen and all nineteen agree | below |
| 10 nothing regresses | pass: 74 unit tests, 15 pseudo-terminal gates, 12 runner diagnostics, 8 standard-input gates, 3 run-output, 2 upstream reproducer, formatting clean. The first port is still byte-identical to its original | whole suite |

## Every decision has a control that fires

A gate that cannot fail is not a gate. Each of the four refusals was removed
in turn and the suite re-run.

| Removed | Result |
| --- | --- |
| the terminal check | `a_terminal_is_refused_and_does_not_block` fails after its bounded wait |
| the second-read refusal, returning an empty string instead | `a_second_read_is_refused_and_is_not_the_empty_stream` fails |
| the length check | `the_limit_holds_at_its_boundary` fails |
| the `take` that bounds the read | `the_read_is_bounded_and_does_not_wait_past_the_limit` fails, after its bounded wait |
| the UTF-8 refusal, replacing invalid bytes instead | `input_that_is_not_utf8_is_refused_rather_than_replaced` fails |

The terminal gate is written with a bounded wait rather than an ordinary one
for exactly this reason: the behaviour it guards is a script waiting for an
end-of-file that will never come, so an unbounded wait would hang the suite
instead of failing the test.

### The bound is observable, and finding that out took two mistakes

The first attempt at a control for the `take` did not fire, and the first
conclusion drawn from that was wrong twice over.

The wrong conclusion was that the bound is not observable from a script, on
the reasoning that the limit is enforced by the length check afterwards and
`take` only decides how much is ever buffered. It is observable, and the way
to see it is the difference between a file and a stream. A file ends, so a
read that stops at the bound and a read that waits for the end finish alike.
A pipe whose write end stays open separates them: with the bound the read
returns as soon as the limit is passed, and without it `read_to_end` waits
for an end-of-file that nothing is going to send. The gate above is written
that way and the control fails in ten seconds without the bound.

The mistake underneath was worse and is the one to remember. `file_read` and
`stdin_read` contain the same two lines, a `take` at the same limit followed
by a `read_to_end`, so a textual replacement of the first occurrence removed
the bound from `host::read` rather than from `host::stdin`. The control then
tested a function it was not aiming at, the standard-input tests passed
because nothing about standard input had changed, and the passing suite was
read as evidence about the wrong thing. A control has to be shown to have
landed where it was aimed before its result means anything: the corrected one
matches on the surrounding `.lock()`, and the count of bounds left in the
file was checked before the suite was run.

## Gate 9, the second port

| | Before | After |
| --- | --- | --- |
| Comparison cases | 16 | 19 |
| Cases reading standard input | 0 | 3 |

The three new cases pipe a file in and pass no path: all the rows, the
complete rows, and an empty file. Standard output, the exit code, and the
recorded diagnostic agree on each. Breaking the port's new branch put all
three in the failing column, so they are checks rather than decoration.

Record 0011's first finding is answered. Its second, an exit status the
script chooses, is not: the port still prints a bare `error: ` line to get
exit 1 out of a run that has a complete report on standard output. The
fixture's recorded expectation says so in as many words.

## Limits stated

- A terminal is refused, not read. That is the decision, and there is no flag
  to turn it off. If refusing one turns out to block a use that matters, the
  record says to record it rather than to add the flag.
- The stream is read once. A script that reads twice is told rather than
  handed the empty string that a consumed stream would give, which would be
  indistinguishable from an empty stream.
- Eight mebibytes, because that is the limit `host::read` already has. A
  larger stream is a streaming problem and this cut has no streaming. The
  read stops one byte past the limit rather than draining what follows, so
  refusing a stream costs the same whatever is behind it.
- Nothing here touches how the session reads a line. The line editor keeps
  the terminal; this only refuses to fight it.
