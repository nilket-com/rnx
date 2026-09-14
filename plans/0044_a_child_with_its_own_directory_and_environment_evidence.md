# rnx 0044 evidence: launch settings over the existing supervisor

2026-09-14, Codex, nano, Linux x86-64, Rust 1.98.1, Rune 0.14.2.
Plan commit `a0be64a`; baseline parent `7877fdc9f350f40ce26d525e834dc714cc8536a1`.
Measurements and build provenance are in rnx-bench commit `46dd57e`,
`results/process_0044/`, with the runnable harness in `probes/process/`.
No new dependency or dependency feature was added.

## What changed

`process::run` and `process::run_bytes` parse and validate the five options,
then call the host supervisor with prepared launch settings. The supervisor's
body is now `run_child_with`; the original `run_child` is a compatibility
wrapper passing no launch settings. Inside the supervisor only command
construction and the contextual launch refusal change. Pipe delivery,
capture, deadlines, interruption, reader collection and group/job cleanup
retain their existing implementation.

Text reply construction is shared. The decoder takes a sibling name:
`process::run_bytes` for new calls and `host::process_bytes` for old calls.
The old three registrations retain their signatures and runtime behaviour;
their help descriptions now point to the new module. No runtime warning is
emitted. The README documents the options and explicit-path origin rule.

Environment key collisions are tested using a temporary `Command`'s own
`get_envs()` enumeration before and after inserting a key. This invokes
the same native key comparison the eventual command uses, including on
Windows, without spawning or mutating the process environment. Inherited
entries never pass through Unicode conversion. Path preparation keeps OS
paths internally; launch errors use escaped debug spelling for a resolved
working directory instead of losing non-Unicode characters through display.

## Gates

The default suite passed **337 tests**, and the test-support suite passed
**378**, sequentially, with zero failures. After changing only the new
session fixture's absent-config path to a platform-native absolute path and
isolating its inherited ceiling, the process suite was rerun under both
configurations: **11 and 13 tests**, zero failures. The full suites include
the existing process, notices, diagnostics, session and allocation gates.
The final release build succeeded with `cargo build --release --locked`.

`tests/process_module.rs` compiles a small native child from
`tests/harness/process_child.rs` with the installed Rust toolchain. It
observes actual executable identity, cwd, environment, stdin and output;
settings are not judged only by inspecting a `Command` object.

- **Surface/reply:** the unit gate checks exactly the two new registrations.
  Text replies equal the old reply for the same child; byte streams and code
  agree across byte forms. All five flags are asserted. A child writing
  stderr and exiting 7 is `Ok`, while a missing executable is `Err`.
- **Input/validation:** exact non-ASCII text, embedded NUL and non-UTF-8 bytes,
  no added newline, empty and absent stdin, byte input to the text form,
  simultaneous input/output pressure and early reader exit. Invalid options,
  environment entries and arguments refuse before a marker child runs.
  New UTF-8 refusals name the new sibling; old UTF-8 gates remain unchanged.
- **Program/cwd:** two copies of a fixture report their own executable path,
  distinguishing the parent-relative executable from the child-directory
  copy. Relative and absolute directories, no-cwd explicit paths, missing
  and non-directory cwd, spaces/non-ASCII paths, and controlled bare lookup
  are exercised. Parent cwd is unchanged. Unit gates preserve lexical `..`.
- **Environment:** set, empty, remove, clear then set, subsequent inheritance,
  and unchanged `env::var`. A Unix invalid-UTF-8 value is inherited intact.
  A build-shaped fixture observes cwd and a setting in the same call.
- **Supervisor:** both forms hit the deadline and 2 MiB capture limit;
  cancellation waits for the fixture's started marker before signalling.
  Test-support reuses the cleanup announcement and held-reader hooks to
  place SIGINT inside cleanup, and the existing escaped-descendant stdin
  fixture plus late-write-failure hook proves delivery failures survive
  cleanup through both forms. An injected unreadable capture reaches both.
  Teardown for cancellation/descendant cases is the existing bounded harness.
- **Examples/integration:** temporary Git checkout with explicit config
  isolation and inherited `GIT_*` overrides removed; EOF-reading fixture;
  directory-plus-environment build fixture; help across reset and session
  allocation-ceiling admission refusal with the module present.

The first ceiling test incorrectly asked the file runner, then eval, to
perform the session's admission refusal. It failed and was replaced by a
session fixture, matching the existing contract. No production memory policy
was changed to satisfy that test.

## Windows limits

The actual process module and its unit tests type-check for
`x86_64-pc-windows-msvc` in the existing `probes/fs-portability` crate. That
crate stubs the call into the supervisor to avoid the application's existing
reqwest/ring MSVC tooling requirement. It is not a whole-application build
and does not execute Windows tests. The command and output are preserved
in `results/process_0044/windows-typecheck.txt`.

Windows unit rows cover rooted-without-prefix and verbatim path preparation;
integration rows cover drive-relative refusal and case-insensitive override
collisions. They remain **unexecuted**. No Windows gate is marked passed by
Linux execution or by this isolated type check.

## Cost and provenance

Both binaries built with `--release --locked` and the same toolchain. The
baseline was rebuilt from the plan's parent in an isolated worktree; the
implementation binary was preserved while doing so. Binary SHA-256 hashes,
source-file hashes, commands, full stdout/stderr checks and versions are
preserved in the bench repository. All compared one-shot outputs were equal,
including exit status, before timing. The child case also checks equality
between old and new facades on the implementation binary.

Pinned to core 4, hyperfine `-N`, ten warmups and 100 runs, whole-process
elapsed time with startup included:

| command | before mean ms | after mean ms |
| --- | ---: | ---: |
| version | 0.555 | 0.519 |
| eval 42 | 4.057 | 3.937 |
| run bare file | 3.732 | 3.588 |
| 10k JSON workload | 11.595 | 11.511 |
| old host call, `/bin/true` | 4.680 | 4.547 |
| new facade, `/bin/true` | — | 4.547 |

This run detects no added penalty in these workloads. The small before/after
reductions are **not attributed speedups**: commands ran sequentially, not
interleaved, and drift and binary layout remain confounders. Raw samples and
standard deviations are retained. An earlier run overlapped tests and was
noisy; `preliminary-*` preserves it separately. The table uses the rerun
with this task's tests/builds finished.

Binary: 14,984,320 to 15,028,912 bytes, **+44,592 bytes**. The reported session
startup reference was 1,823,240 before and 1,823,136 after; these are tracked
allocation requests, not RSS. The raw `:memory` output is preserved without
claiming a module-specific allocation saving.

Record 0031's async CPU-loop interruption limitation is unchanged. This
module remains synchronous and its deadline still starts after spawn.
