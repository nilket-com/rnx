# rnx 0049 evidence: the names no longer begin with host

Measured on nano, Linux, 2026-09-15 with Rust/Cargo 1.98.1 and Rune 0.14.2.
Plan commit `57cd19f`; accepted pre-migration code `4652830`.
The probes, raw exports, migration inventory and saved notebooks are in
**rnx-bench `06fbc7e`, `results/namespaces-0049/`**, with reproducible drivers
under `probes/namespaces/`. This is implementation evidence for review,
not a claim of an independent review or Windows execution.

## What changed

No context installs a Rune `host` crate, including test-support builds.
The new production paths are `json::parse`, `json::stringify`, `io::stdin`,
`io::eprint`, and `process::exit`, alongside existing `process::run` and
`process::run_bytes`. The three fixture paths moved to `rnx_test`, compiled
only with test-support. There are no compatibility aliases or warnings.

A shared `install_core` replaces every former host installer call and arms
interrupt handling before installing the domain modules. It remains after
version/help dispatch, and the pure config context does not call it.
`src/host.rs` remains an internal supervisor/support module; internal Rust
`crate::host` references do not register script names.

The JSON reader/converter and guarded serializer are retained. New wrappers
register them directly; the internal process text reply calls the reader
directly instead of its removed forwarding function. `run_child_with`
and the capture/supervision region preceding `decode` are byte-identical to
the accepted source, proved by `supervisor-unchanged.json`. Unused legacy
entry wrappers are removed. Stdin state, stderr writes and exit cleanup
retain their implementation and only change registration visibility.

## Migration and registration

`inventory-before.txt` preserves original active references.
`inventory-after.json` classifies the remaining ones as Rust internals,
negative tests or migration documentation. `migrated-rnx-files.txt` lists
the changed active files. Historical records, raw exports, notebooks,
snapshots and review notes were not rewritten to erase old API spellings.

- Ordinary run/eval, a session after reset, and the worker exercise the new
  names. The exact core inventory is seven functions, plus three fixtures
  under test-support. All eight old production names and all three old
  fixture names fail compilation. The default build refuses `rnx_test`.
- Completion now lists the JSON domain and recovers it after reset. Help
  follows the registered inventory. Old names no longer resolve in help.
- A file with `use std::io;` uses Rune's printing functions/macros and
  explicitly calls rnx's `::io::eprint`; this coexistence was executed.
  README explains the import shadowing and gives a full migration table.
- The settings fixture rejects all five newly named operations while still
  reaching the prompt. No config capability or startup policy was widened.
- Legacy process calls were rewritten with explicit `timeout_ms` and, where
  applicable, `input`. Their existing assertions survived: capture flags,
  deadlines, cancellation, delivery, encoding and program-naming errors.
  The previous compatibility comparison now checks explicit versus default
  timeout options. No process behavior assertion was relaxed.
- Validation and relative-path behavior are the existing 0044 facade's,
  not a promise to reproduce every possible legacy call. The text-output
  refusal requires `process::run_bytes`. Help counts and prefix-specific
  completion expectations were updated deliberately.

Six active benchmark/fixture files migrated. Two timing drivers remain
explicitly historical: the 0044 comparison now has its own preserved JSON
source, and the 0047 startup check still requires its recorded old binary
hash. Their documentation points current work to the new namespace driver.
New comparisons preserve separate before/after sources and their hashes.
No old benchmark output was relabeled as a measurement of the new API.

## Gates run

| check | outcome |
| --- | --- |
| root `cargo test --locked` | 347 passed, zero failures |
| root `cargo test --locked --features test-support` | 388 passed, zero failures |
| kernel suites, default and transport-probe | 23 passed each |
| release selfcheck and worker protocol/lifecycle checks | passed |
| four existing supervision fixtures | passed |
| isolated installed-kernel namespace notebook | passed; strict saved nbformat validation |
| actual JupyterLab browser journey | passed execute/reconnect/interrupt/restart/save/reopen |
| root formatting and both notices checks | passed |
| kernel clippy, all targets, transport-probe, warnings denied | passed |
| root clippy, all targets, test-support, warnings denied | unchanged inherited failures; see below |
| isolated Windows modules/unit tests and 16 integration-test targets | type checks passed; not execution |
| whole-root Windows check | stopped at missing MSVC `lib.exe` while building ring |
| kernel Windows check | passed, with inherited unused-mut warning |

The JSON suites retain exact u64 values, both overflow ends, approximate
float behavior, negative zero, duplicate keys, document locations, cycles,
and the reader/writer depth gates. Paired release probes additionally
compare 17 successful command outputs, full stdout/stderr and exit status;
refusals are wrapped as values to compare their payload without asserting
that renamed source excerpts and carets are unchanged.

The worker refuses `process::exit`, continues with another input and delivers
stderr containing a NUL without a newline at its barrier. Stdin consumption
still survives reset. `namespaces.ipynb` executes the new JSON/stream/process
calls, catches exit refusal, rejects an old host name and retains its earlier
binding. Error outputs have the required notebook schema. `Journey.ipynb`
and the actual screenshots record the separate existing browser workflow.
No notebook input_request support is claimed.

### Inherited check qualifications

The accepted source was extracted at `57cd19f` and checked with the same
Rust/Cargo and clippy command as the implementation. Both emit **35
code-bearing diagnostics**, counting the bin/test duplicates, with no added
or removed `(lint code, message)` entries. Both JSON streams and the multiset
comparison are in the evidence. These are existing completion, presentation,
session and test-code findings. They were not suppressed or repaired through
unrelated edits; root clippy is not described as clean. The record's gate
wording now states this measured qualification.

The Windows module probe uses signature-only supervisor/stream stubs. The
integration-test compilation probe uses an intentionally unusable executable
placeholder to supply Cargo's binary-path variable and explicitly enables
Win32_Security for the existing console fixture. These check test source,
Rust format strings and Windows types, not generated Rune execution or the
whole root feature graph. Neither closes the existing portability gates.
The root/kernel manifests, lockfiles and notices remain unchanged.

### Existing exit-propagation behavior

While writing the notebook fixture, a top-level `?` on the refused exit
produced `Expected type Tuple but found Result` from the session wrapper.
The accepted and changed release binaries produce byte-identical diagnostics
for their respective names; `exit-propagation-observation.json` preserves
that comparison. Catching with `.is_err()` works and is what the notebook
gate tests. This record does not silently fix or promise better propagation.

## Cost and provenance

Pinned core 4, hyperfine 1.20.0, 10 warmups and 100 runs. The before binary
matches the hash saved in the accepted 0047 notebook evidence. Both use the
same locked release/toolchain conditions. Raw commands, hashes, size and
memory reports are in `conditions.json`; outliers remain in `timings.json`
and the warning log. Runs are sequential, so drift is not isolated and the
lower means are not a speedup claim.

| whole process | before mean | after mean |
| --- | ---: | ---: |
| version | 0.582 ms | 0.551 ms |
| eval 42 | 4.211 ms | 4.027 ms |
| bare run | 3.975 ms | 3.686 ms |
| 10k JSON | 12.298 ms | 11.918 ms |
| supervised /bin/true | 4.804 ms | 4.695 ms |

Release binary: 15,102,752 to 15,095,392 bytes, **7,360 fewer bytes**.
Session startup allocation reference: 1,823,011 to 1,824,426 bytes,
**1,415 more bytes**. These are tracked allocator requests, not RSS; full
current-live and reference samples are preserved together.

The README and probe documentation describe reproduction from the accepted
revision and current tree. Review notes stay on disk and out of git.
