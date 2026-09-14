# rnx 0039 evidence: colour belongs to the presentation

Measured on nano, Linux, Intel i7-14700, Rust 1.98.1, 2026-09-14.
Before is f9440eb, whose binary hash matches record 0038's after binary.
The benchmark and terminal material lives in rnx-bench under
`probes/colour` and `results/colour_0039`, commit `ce06fdd`.

## What was built

`presentation.rs` decides stdout and stderr once, from `--color`, terminal
status, `TERM` and `NO_COLOR`. Its SGR set is closed:

- `ESC[0m` reset, `ESC[1m` bold, `ESC[2m` dim comment;
- `ESC[35m` magenta keyword, `ESC[32m` green literal, `ESC[36m` cyan number;
- `ESC[1;31m` bold red error and `ESC[31m` red caret.

The renderer records UTF-8 ranges beside its plain, bounded text. Styles
are applied after rendering, and the inspection printer carries those
ranges through its own byte budget. A plain render retains ownership of
its String rather than copying it for a disabled style pass. Neither
truncation nor display-width calculation sees an ANSI byte. Opaque and
cycle markers remain foreground. Help styles its heading; completion
candidates keep the same foreground style as identifiers.

An implementation check caught that splitting the old `Some(` append
into `Some` and `(` changed a preview at a four-byte budget. The append
is atomic again, with only its label styled. A new unit gate pins both
sides of that boundary. No pre-existing test was changed.

The tolerant highlighter styles original input bytes, with Unicode XID
identifier boundaries, incomplete literals and nested block comments.
It answers yes to every edit. The only dependency change is making the
already-resolved unicode-ident a direct dependency. The notices check
remains unchanged: 124 packages, 87 texts, 13 fetched, one previously
unresolved licence.

## Gates

The default and test-support suites were run sequentially with
`TERM=xterm-256color`: 293 default tests and 331 test-support tests, zero
failures. Counts and commands are preserved in the bench's `tests.json`. The new pipe gates cover run, eval, help, piped sessions,
inspection, errors, escaping, cycles and truncation. Removing only the
SGR strings above gives exact plain bytes. Raw script print and eprint
are checked separately and remain raw. Flag placement and invalid values
are gated, including delivery to env::args after the script path.

The Linux PTY harness feeds actual captures to pyte. Always and never
produce identical screen contents and cursor positions at 17 checkpoints:
keyword formation and removal, quote deletion, Ctrl-C, a pasted tab with
combining and wide characters, multiline input, a diagnostic, reset and
a 10,000-character paste. It asserts token colours as they change, default
attributes after edits and cancellation, and that an unevaluated print
expression produces no output. The pasted block appears in one full
highlighted redraw; transport chunks do not define redraws.

Mixed stdout/stderr terminal and pipe arrangements preserve reader
selection and apply each stream's colour independently. Auto, forced,
never, nonempty/empty NO_COLOR and TERM=dumb are exercised. Captures retain
redraw traffic for diagnosis; only the resulting screen and cursor are
compared. `terminal.json` records these checks.

The Windows console branch and presentation unit tests type-check via the
isolated probe against x86_64-pc-windows-msvc. That excludes unrelated
reqwest/ring compilation and is not a full rnx cross-build. The PTY
harness is Linux-only: Windows screen, cursor and console-mode execution
remain unverified, not counted as passing gate 6.

## Cost

Hyperfine 1.20.0, no shell, CPU 4, 10 warmups, 100 runs per command.
Commands, hashes, exact output comparisons and raw timings are preserved.

| command | before mean (ms) | after mean (ms) |
| --- | ---: | ---: |
| version | 0.554 | 0.536 |
| help | 0.541 | 0.536 |
| eval 42 | 4.058 | 4.005 |
| bare run | 3.716 | 3.660 |
| JSON workload | 11.627 | 11.668 |

No detected regression at this resolution; small differences do not prove
a speedup. The benchmark README retains standard deviations.
Binary: 14,865,000 → 14,875,688 bytes (+10,688).
The source subsequently received comment-only documentation changes.

The actual highlighter, imported into a release probe, measured over a
10,000-character input on CPU 4: 100 warmups, then 1,000 passes including
building and dropping the styled String. Median 0.03123 ms, p95 0.03395 ms,
maximum 0.56221 ms. Every pass is below this gate's stated 5 ms bound.
The CSV preserves all samples. This measures highlighting, not a terminal's
transport, redraw or scheduling time.

## What it looks like

The captured specimen uses a loopback HTTP fixture, parses its JSON,
shows a non-ASCII/control-containing string, a multiline expression,
a diagnostic and a completion menu. Both PNGs render the same captured
screen, with Tango normal ANSI colours on dark and light backgrounds.
DejaVu Sans Mono and Droid Sans Fallback supply glyphs.

Viewed both: magenta separates keywords without colouring punctuation;
green strings and cyan numbers distinguish the HTTP object's data;
bold keys make its structure easier to scan. Errors and their caret are
red, and the caret remains in the same column. The images are illustrative
terminal palettes, not an RGB palette imposed by rnx; a user's terminal
controls the actual colours. The files are `specimen-dark.png` and
`specimen-light.png`, with the raw capture and escaped-byte rendering beside
them. Numbered prompts remain the next record, including a count reset
that preserves bindings.

## Continuous terminal smoke gate

The ordinary Linux PTY suite now also checks the bold prompt, keyword
recolouring when `le` becomes `let`, and the final SGR reset after Ctrl-C.
These byte-level checks run under `cargo test`; the Python emulator remains
the full screen/cursor gate. They do not compare redraw traffic wholesale.
The session caret span now starts after its leading newline, asserted by a
separate pipe test. Plain diagnostic text and caret columns are unchanged.

Validation of this follow-up: `TERM=xterm-256color cargo test --locked
--test colour --test repl`: 4 colour tests and 17 REPL tests pass. The full
suite counts, timings and specimen above describe the preceding implementation;
this follow-up adds two tests and moves one styling boundary without changing
visible output. No benchmark was repeated for it.
