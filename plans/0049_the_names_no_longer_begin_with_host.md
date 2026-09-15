# rnx 0049: the names no longer begin with host

Status: implemented 2026-09-15, pending review. Linux gates and migrated
notebook fixtures pass, with evidence beside this record. Root clippy has
an unchanged inherited diagnostic baseline; the full Windows build remains
blocked by missing MSVC tooling. Neither is reported as a passing clean
check. The forty-ninth record completes the script-visible namespace
migration: `host::` is removed, JSON and streams have domain names, and the
process compatibility functions are retired.

## Context

At `4652830`, `host::install` registers eight production functions:
`json_parse`, `json_stringify`, `stdin`, `exit`, `eprint`, `process`,
`process_bytes`, and `process_bytes_input`. Test-support adds three more.
`src/json.rs` already owns the guarded reader and writer, but it does not
register a Rune `json::` module. HTTP's examples consequently still call
`host::json_parse(response.body)?`.

Record 0031 section 5 chose domain namespaces. Record 0035 removed the old
file names outright; record 0033 section 6 retained the JSON names while
fixing their behavior, and record 0044 deliberately kept the process
compatibility names. This record supersedes those two naming decisions.
The project is unpublished, and the user has asked to finish the migration
rather than keep aliases. That is authorization for a breaking change,
not proof that nobody has copied an example into a script or notebook.

Inspection of the pinned Rune 0.14.2 source finds its I/O module at
`::std::io`, including printing functions, macros and an error type.
Installing an rnx crate named `io` is distinct from extending that module.
The existing rnx `process` module has `run` and `run_bytes`; `exit` is a
new registration there. Gates must prove these names coexist in an actual
context rather than infer that from module spelling alone.

Two implementation details make a blind rename insufficient.
`host::install` calls `platform::watch_for_interrupt`, so removing the
installer must preserve that initialization. The old process functions
take positional deadlines and bypass record 0044's launch-option parser;
migrating their callers changes argument shape and some validation and
path behavior, not only their prefixes.

## Decision

### 1. One public home per operation

| old call | replacement |
| --- | --- |
| `host::json_parse(text)` | `json::parse(text)` |
| `host::json_stringify(value)` | `json::stringify(value)` |
| `host::stdin()` | `io::stdin()` |
| `host::eprint(text)` | `io::eprint(text)` |
| `host::exit(code)` | `process::exit(code)` |
| `host::process(p, a, t)` | `process::run(p, a, #{timeout_ms: t})` |
| `host::process_bytes(p, a, t)` | `process::run_bytes(p, a, #{timeout_ms: t})` |
| `host::process_bytes_input(p, a, data, t)` | `process::run_bytes(p, a, #{input: data, timeout_ms: t})` |

Register exactly two functions in `json`, two in rnx's `io`, and add
`exit` to the two existing `process` functions. No `host` crate is installed
in either production or test-support contexts. Old calls fail compilation;
there are no forwarding aliases, runtime warnings or alternate JSON APIs.
Register help at the function registration, as the other modules do, so
completion and help expose the same inventory.

An explicit old deadline remains explicit in the migrated call. Do not
replace it with `#{}` unless it was already 30000 milliseconds. The new
calls retain all of record 0044, including validation, explicit relative
program-path resolution and its UTF-8 advice naming `process::run_bytes`.
The README must state those differences, rather than promise that every
possible legacy process call is behaviorally interchangeable.

### 2. JSON behavior is already decided

Register thin entry points over the existing `json::parse` and
`json::stringify` Rust implementation. Preserve record 0033's unsigned
integers, approximate out-of-range conversion, negative zero, duplicate
keys, null/unit conversion and document-position refusals. Preserve the
reader's 127/128 nested-container boundary and the writer's existing
256/257 boundary shared with the renderer.

The writer retains record 0019's cycle, depth and representation refusals.
There is still one guarded writer and one reader implementation. Internal
process reply construction may call the reader directly; this is not a
second script-visible API. HTTP still returns a text body and has no
JSON method. Its example becomes `json::parse(response.body)?`.
No dependency, Rune feature or serde_json feature changes here.

### 3. Stream and exit contracts move intact

`io::stdin()` keeps the existing whole-stream UTF-8 read, 8 MiB limit,
terminal refusal and once-per-process consumption flag. Renaming it does
not make it notebook input: the worker's stdin is still not Jupyter's
`input_request` protocol. Session reset does not replenish a consumed
stream. A failed read retains the existing consumption behavior.

`io::eprint(text)` writes the same bytes to stderr, appends nothing and
flushes. Script escape sequences remain raw; presentation escaping does
not apply to this function. Rune's existing print/println/dbg functions
and macros stay available and unchanged. This record adds no rnx
`io::print`, stream handles or asynchronous stream operations.

`process::exit(code)` keeps the current policy: run and eval terminate;
sessions and persistent workers refuse catchably. Valid script statuses
are 0 through 255; invalid statuses terminate with 1 as before. Preserve
stdout-flush failure handling and explicit terminal-title restoration.
The existing session refusal wording remains unchanged in notebooks too;
making it notebook-specific is outside this migration.

### 4. Remove the namespace without rebuilding the supervisor

The script-visible namespace and the Rust source layout are separate.
`src/host.rs` may remain as an internal supervisor/support implementation;
its misleading spike-only module comment must be corrected. Retaining
`crate::host` internally is not retaining a Rune `host::` API. Move small
registration wrappers as needed, but do not relocate or rewrite the
process-group, job, capture, delivery or cancellation algorithms merely
to remove the filename. Delete obsolete compatibility entry wrappers
when they have no callers; retain shared implementation used internally.

Preserve interrupt-handler initialization before execution in every
context-building entry point, including selfcheck and test contexts.
Removing `host::install` must not move it onto the version/help fast path.
The pure settings evaluator receives none of these modules, as before.

Under `test-support` only, move `test_pending`, `test_allocation_peak`
and `test_reset_allocation_peak` to `rnx_test::` with their existing
signatures and semantics. This is a fixture namespace, not a public
battery. Do not reuse `time::sleep` for the pending fixture: it returns a
different value and would change the async foundation's tests.

### 5. Migrate current callers and preserve historical evidence

Update current source-generated scripts, selfchecks, tests, `journey.rn`,
README examples/help, worker fixtures and kernel examples that call the
old names. Review raw-string fixtures and expected diagnostics carefully:
a changed call length legitimately moves source columns and carets.
Preserve each test's discriminating assertion; do not remove a test just
because its original surface is gone. Tests specifically proving legacy
registration become negative registration tests.

Inventory rnx-bench's executable scripts and probes before editing. Move
current-head workloads and Jupyter fixtures to the new names. For probes
that compare old and new binaries, keep explicitly separate old/new
source variants, with hashes and the namespace adaptation recorded; an
old binary cannot run the new source. Require equivalent work and output
before timing either variant.

Do not rewrite prior raw timing exports, saved notebooks, screenshots,
source snapshots or historical plan/evidence transcripts to make old
spellings disappear. Keep their original commits and provenance. Add
migration pointers where current documentation would otherwise mislead,
and save this record's reruns under a new results directory. An inventory
in the evidence distinguishes migrated executable callers from retained
historical material. Review notes stay gitignored.

## Acceptance gates

1. **Registration and discovery.** In normal contexts, `json` has exactly
   parse/stringify, rnx `io` stdin/eprint and process run/run_bytes/exit.
   Exercise run, eval, a session across reset, and the persistent worker.
   Help and completion show the new names and no old host functions.
   All eight old production calls fail to compile. Under test-support,
   the three old fixture calls also fail and `rnx_test` works; a default
   build refuses `rnx_test`. Verify coexistence with Rune's print/println
   and dbg facilities. The settings evaluator refuses the new host APIs.
2. **One JSON contract.** Migrate and run all 0019/0033 gates, including
   exact u64::MAX, both numeric overflow ends, negative zero, duplicates,
   document positions, cycles and both depth boundaries. Compare old and
   new successful outputs and refusal payloads using paired sources.
   Source review confirms a single reader/writer implementation; no
   unguarded companion JSON registration is added. Parse a fixture HTTP
   body through the new name, including u64::MAX.
3. **Streams and exits.** Retain stdin's terminal, repeated-read, UTF-8
   and cap tests, stderr byte/flush tests, and exit status, output delivery
   and title-restoration tests under the new names. A session and a worker
   survive a refused `process::exit` and execute another input. Worker
   stderr with no newline and a NUL reaches its barrier byte-exact; saved
   notebook error output remains valid. No stdin-request support is claimed.
4. **One process supervisor.** Migrate the legacy child fixtures with
   explicit options and retain timeout, cancellation, partial UTF-8,
   capture, input delivery and descendant-cleanup assertions. Existing
   process facade tests pass. List intentionally changed validation/path
   expectations individually. Source review establishes that shared
   supervision and reply construction were retained. Run selfcheck and
   exercise interruption after removing the old installer.
5. **Migration completeness.** Record a search inventory of remaining
   `host::` references, distinguishing Rust internals, negative tests,
   migration documentation and historical artifacts from active scripts.
   No current positive fixture requires a Rune host module. Rerun the
   worker and installed-kernel notebook smoke fixtures with migrated
   code, validate the saved notebook, and preserve the results in rnx-bench.
6. **Regression and cost.** Both root suites, the separate kernel suites,
   formatting and notices checks pass. Run clippy and compare any inherited
   diagnostics with the accepted baseline rather than claim a clean pass.
   Implementation found 35 emitted code-bearing diagnostics in both trees
   under the same command/toolchain, none added or removed; the evidence
   preserves both logs. Kernel clippy is clean. Run available cross-checks
   and report exactly which platforms executed; this record does not close
   0047's non-Linux supervision gates. Measure matched release version,
   eval, bare run, migrated JSON workload and session baseline; record
   binary size, source variants, versions and raw exports. No speedup or
   unchanged timing is asserted before those measurements.

## Guardrails and stop conditions

- No script-visible compatibility aliases and no host module even in a
  test-support build. No test-only capability in a production build.
- No second JSON conversion, process supervisor or stream state. No new
  dependencies and no change to the pure config evaluator's capabilities.
- If a migrated process test exposes a behavior difference beyond record
  0044's existing contract, stop and explain it before weakening the gate.
- Stop if removing the installer loses interrupt setup or exit cleanup,
  or if module coexistence requires replacing Rune's standard I/O module.
- Preserve historical measurements rather than relabeling them as runs
  made with the new API. Review this draft before the implementation commit.

## Risks and forward

Copied scripts and saved notebook cells using `host::` will stop compiling.
The README migration table is the remedy, not an automatic source rewriter.
The wider fixture migration is likely to be more work than registration;
review it for preserved assertions and provenance, not only passing counts.
The internal supervisor filename may remain historical terminology without
making that terminology the API again. Notebook stdin and broader standard
stream APIs remain separate decisions driven by use.
