# rnx 0009 evidence: diagnostics for the file runner

Implementation of `plans/0009_diagnostics_for_the_file_runner.md`,
2026-09-08, on Linux x86_64 against upstream Rune 0.14.1 with no patch.
Status of the record stays proposed until reviewed.

## What landed

- `src/session.rs`: the line and column computation is now `position`, a
  free function the session and the runner share, so gate 8 holds by
  construction rather than by two implementations agreeing today. It counts
  characters, not bytes, and returns the line's text with it.
- `src/runner.rs`: the whole of `run`. It executes under the two million
  instruction budget `run` has always had, which is the only thing that
  can stop a file, since nothing interrupts one from the keyboard. It
  reads the file, compiles it, reports a compile error at a place,
  executes `main`, reports a runtime error at the instruction that
  raised it, and prints what the script returned. Every diagnostic goes
  to standard error; script output is never touched. It returns an exit
  code rather than an error, so the shape of what it prints is decided
  in one place.
- `src/main.rs`: flags are read only before the script path, and everything
  after the path is passed to the script verbatim.

## Gates

| Gate | Result | Where |
| --- | --- | --- |
| 1 a compile error is located | pass: file, line 3, column 10, the source line, and a caret; no `Diagnostics {` and no source dump in the output | `a_compile_error_names_the_file_line_column_and_marks_the_column` |
| 2 a runtime error is located | pass: file, line, column, source line, caret | `a_runtime_error_names_the_place_it_happened` |
| 3 an error inside a called function | pass: `main` calls `outer` calls `inner`, and the diagnostic names line 3 inside `inner`, not line 5 or line 7 where the calls are | `an_error_inside_a_called_function_reports_that_function_not_the_call` |
| 4 the streams stay separated | pass: a script that prints and then fails puts `mine` on standard output and the diagnostic on standard error, captured separately | `the_streams_stay_separated` |
| 5 a returned error prints plainly and claims no position | pass: `error: plain words`, with no wrapper, no backslash, no line, and no caret; a vector, an object, and a struct print through the bounded renderer, and a five-thousand element vector is elided | `a_returned_error_prints_plainly_and_claims_no_position`, `a_returned_error_that_is_not_a_string_prints_through_the_bounded_renderer` |
| 6 the source dump is asked for and cannot be triggered by an argument | pass: absent without the flag; before the path it prints to standard error; after the path it reaches the script as `args[0]` and prints nothing extra | `the_source_dump_is_asked_for_and_cannot_be_triggered_by_an_argument` |
| 7 the port's error names its line | pass: the mixed arithmetic that took five rounds of print statements reports line 6 and shows the assignment | `the_error_that_took_five_rounds_of_printing_to_find_now_names_its_line` |
| 8 positions survive tabs and characters beyond ASCII | pass: a doubly tab-indented line reports column 11, and a line containing `café` before the fault reports column 13; the shared `position` is unit-tested on the same two cases | `a_column_counts_characters_through_tabs_and_beyond_ascii`, `src/session.rs::position_tests` |
| 9 a failure with no span says so | pass: a path that does not exist names the path and the reason, with no line, column, or caret, and exits 1 | `a_file_that_cannot_be_read_names_it_without_inventing_a_place` |
| 10 the budget still stops a runaway script | pass: a file containing an endless loop is halted, says how many instructions it exceeded, and exits 1, within seconds; a loop of a thousand iterations is untouched and prints its result | `a_script_that_loops_for_ever_is_halted_by_the_budget` |
| 11 a fault with no resolvable place says so | pass: a file with no `main` reports that no source position is available; a returned error and an unreadable file, which never had one, do not | `a_fault_with_no_resolvable_place_says_so` |
| 12 nothing regresses | pass: 64 unit tests, 13 pseudo-terminal gates, 12 runner diagnostics, 3 run-output, 2 upstream reproducer, formatting clean, spike self-checks pass. The first port still produces output identical to the Python original, with an empty standard error and exit 0 | whole suite, and the fixture comparison re-run |

## Before and after, on the port's own failure

Before, the whole of what a person got:

```
Error: "Unsupported binary operation `SUB` on `::std::f64` and `::std::i64`"
```

After:

```
runtime error at /path/to/script.rn, line 6, column 2: Unsupported binary
operation `ADD` on `::std::i64` and `::std::f64`
  counter["a"] = get(counter, "a") + 1.5;
  ^
```

The message is Rune's either way. What changed is that it now says where.

## A regression this cut introduced and then fixed

The first version of `src/runner.rs` called the virtual machine directly and
lost the instruction budget the old path had, so a script containing an
endless loop ran until something outside killed it. The budget is restored,
the record now states it as a preserved guarantee beside the streams, and a
regression test covers both a runaway loop and a loop that finishes inside
the budget. It was caught in review, not by the suite, because nothing had
ever tested it.

## Limits stated

- The caret is placed by column, so on a line indented with tabs it does not
  sit under the character when the terminal renders a tab as several
  columns. The session has always done this and the two now agree by
  sharing the code; making the caret account for tab width is a separate
  question for both.
- A compile error reports the first fatal diagnostic. Rune may have found
  more, and the rest are not shown.
- `eval` is unchanged and still runs through the session, which the record
  named as forward work rather than covering here.
