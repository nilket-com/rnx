# rnx 0036 evidence: what the process was given

Implemented and measured by Codex on nano, Linux, Intel i7-14700,
2026-09-13, Rust 1.98.1, Rune 0.14.2. Plan commit: `9ba1094`.
Windows execution is not claimed.

## Implementation

`src/env.rs` registers exactly four functions: args, var, vars, home_dir.
There is no setter. The command-line reader consumes OS strings, skips
argv[0] without decoding, and refuses the first non-Unicode argument with
its one-based position and exit 2 before any command is dispatched.

The existing run-flag parser selects the script argument slice once.
An immutable `Arc<[String]>` snapshot supplies both main's argument and
the env::args closure. Every conversion starts from cloned Rust strings,
so neither Rune vectors nor their strings are shared between consumers.
The old JSON round trip for main's argument is removed. Non-run entry
points register an empty snapshot. Version/help still create no context.

vars validates every name and value before ignoring duplicate keys, so a
later malformed value cannot hide behind a valid first entry. var delegates
to the OS after rejecting invalid names. home_dir delegates to Rust's
platform lookup and labels a non-Unicode result with its variable or fallback
source. It does not acquire a deadline for account-database lookup.

## Gates and validation

Children receive explicit argv and env_clear plus only the fixture's
variables. The hand-built Unix environment gate prepares all CStrings before
fork; the pre-exec hook only builds fixed-size pointer arrays and calls
execve. No allocation or environment mutation happens inside that hook.

Eight integration tests establish:

- Main/args agreement for empty, spaced, flag-like and empty-string arguments;
  run flags remain rnx's before the path. Popping vectors and modifying
  strings inside both main's argument and an accessor result leaves later
  accessor calls intact. Eval and the session return empty lists, including
  after a session reset. The accessor is discoverable through session help.
- Invalid command word, file path and script argument report positions 1, 2
  and 3. Eval, repl, help, version and unknown-command paths all refuse before
  dispatch; a non-Unicode argv[0] is ignored. Status, complete stdout and
  complete stderr are asserted, so a panic/backtrace notice cannot pass.
- Missing versus empty values, invalid lookup names, and invalid Unicode
  values. A valid lookup continues working when another variable is bad.
  Whole-environment output against A=1 and E=empty matches exactly.
- A raw non-Unicode environment name refuses the whole object. D=first,
  D=second retains first in both APIs. D=first followed by D=<FF> still
  returns first from var but refuses vars. These are actual execve environments,
  not a map that would have discarded a duplicate before launching rnx.
- Explicit HOME, invalid HOME, and empty HOME versus getpwuid_r called by
  the test. With HOME unset, this machine reported Some("/home/me"); the test
  does not generalize that observation to other account databases.
- No setter resolves, and a one-byte session ceiling still refuses evaluation
  while remaining visible to the memory command. A normal ceiling remains
  readable as a string through env::var.

The unit gate also pins the four registered names and their compilation.
Both full suites ran sequentially under TERM=xterm-kitty:

| check | result |
| --- | --- |
| cargo test --locked | 272 passed, zero failed |
| cargo test --locked --features test-support | 310 passed, zero failed |
| cargo build --release --locked | passed |
| scripts/third-party-notices.sh --check | unchanged; passed |
| new source/test rustfmt and diff whitespace | passed |

Notices remain 121 packages, 87 texts, 13 fetched, and the same one unresolved
item. Shipping dependencies and features did not change.

The existing rnx-bench probes/fs-portability crate now also includes the
actual environment source. Its Windows cargo check --tests, with test-support,
passes and is preserved as results/env_0036_windows_check.txt. The probe has
its own lockfile and a minimal registration record. This checks the module
and unit-gate types, not the complete application or Windows execution.
The Windows case-insensitive lookup and USERPROFILE integration gates are
authored but remain unexecuted here. Raw Unix argv/environment gates are
explicitly Unix-only.

## Cost and provenance

The before release was rebuilt from the committed plan tree `9ba1094`,
whose product code is 0035 including its accepted enum cleanup. The after
release is this implementation. The actual release behavior was checked
again with FF FE as argument 2: before exits 101 with a panic, after exits
2 with the exact escaped refusal. Raw outputs and suite summaries are in
rnx-bench results/env_0036_tests.json.

The reproducible runner is scripts/measure_env_0036.py in rnx-bench,
preserved with the results at commit `eda3c34`.
results/env_0036_startup.json preserves every hyperfine sample;
results/env_0036_conditions.json preserves commands, outputs, versions,
binary sizes and hashes, and memory-command output. Ten warmups and fifty
runs, hyperfine -N, core 4 pinned, with PATH and TERM=dumb as the controlled
benchmark environment. All measured commands exit successfully with matching
full stdout and stderr before and after (help outputs are preserved too).
The argument workload passes 100 arguments containing spaces, a flag-like
value, an empty string and Unicode, and returns the count.

| workload | before ms, mean ± sd | after ms, mean ± sd |
| --- | --- | --- |
| version | 0.562 ± 0.103 | 0.512 ± 0.015 |
| help | 0.512 ± 0.014 | 0.511 ± 0.018 |
| eval 42 | 4.002 ± 0.015 | 3.960 ± 0.011 |
| bare run | 3.659 ± 0.010 | 3.668 ± 0.100 |
| 10k JSON | 11.473 ± 0.081 | 11.403 ± 0.079 |
| run with 100 arguments | 3.690 ± 0.010 | 3.650 ± 0.018 |

No startup regression was detected at this resolution; small differences
are not attributed to this change. Every timing includes process startup.

Binary: 14,263,016 to 14,288,904 bytes (+25,888).
Before SHA-256: `de454494ce277e998fdd91ba3cc037a5defe1763ae3f4e56f76eb99737f18481`.
After SHA-256: `2d23944c9f950389e2b9885fb3c6094704a9914e384b1d6849485615f275cbe6`.

Session startup allocation reference: 1,813,041 to 1,814,852 bytes (+1,811).
Live allocation at the first memory command: 1,821,631 to 1,823,442 (+1,811).
These are allocator request counts, not RSS. No host call changes the ceiling.

## Remaining scope

Windows execution remains outstanding. home_dir's native fallback may block.
The earlier async CPU-loop interruption clause remains open; this record
changes neither execution budgeting nor process environment inheritance.
No review notes are committed and nothing has been sent upstream.
