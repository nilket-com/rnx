# 0067 gate 4: one installed rnx reaches the adapter session

Ready for review. This checkpoint changes plan/evidence text only. Product source,
Cargo files, notices, adapters, server and kernel are unchanged from accepted
Gate 3, `7ae8863`. The costs and regression gate remains open.

## Published and fixture origins

The accepted Gate 3 commits were pushed before this gate. The ordinary install
uses the actual published repository and full reviewed revision:

```sh
cargo install --git https://github.com/nilket-com/rnx \
  --rev 7ae886305aea33312a061cbafbebbb49ff8bc8c4 rnx --locked \
  --root <private-install> --target-dir <private-build-target>
```

The second observation is explicitly a fixture: a private bare origin whose
commit changes only the root package's repository URL to that origin. Its delta
is archived as a Git bundle against `7ae8863`. Both use real Cargo Git installs,
produce only `bin/rnx`, and have no `rnx-project` on their controlled PATHs. The
fixture working checkout is renamed before the first adapter build. Cargo's
bare origin remains available for acquisition; it is not a path dependency.

Both installed binaries describe their own acquired coordinates. Each consumer
starts with a separate empty Cargo Git cache, an empty assembly cache, no runtime
store, no runtime override and no project association. Registry sources are
pre-existing; this is not a cold registry-download measurement.

## Interactive result

Both origins pass the same journey:

- Declining `:dep polars` creates no scratch, assembly cache or runtime store and
  leaves a bound value usable. Consent fetches, locks, builds, probes and replaces
  the session in the same PID, with the prompt restarting at input 1.
- History persists without replay. Old bindings are absent. The caller's working
  directory is preserved, including the location of the CSV and Parquet output.
- A bound frame supports filter, group, sum, sort, collect, explicit preview and
  Parquet read-back. A missing-column error is catchable and the original frame
  remains usable.
- A mixed `:dep --offline polars postgres` request names PostgreSQL as the
  addition and Polars as already declared. After handover both work. A private
  PostgreSQL cluster returns an exact typed text parameter containing quotes,
  a backslash and Unicode, alongside an integer parameter.
- Repeated declarations are no-ops. Ctrl-C cancels an awaiting input while keeping
  the earlier frame; reset loses its binding and retains the extension. Quit,
  EOF and the exact shell-quoted scratch reopen command work.
- A second stock session attaches to the first Polars assembly with compilation
  trapped and networking disabled in a bubblewrap network namespace. Both Cargo
  and rustc traps have positive controls. A full file/connect trace shows no
  runtime-store access. The no-store checks happen before the separate explicit
  developer-install control.

The stale-store control uses the published executable. After installing a real
retained source, the fixture writes a selection naming a missing ID and confirms
`runtime show` refuses. Default stock `:dep` still uses its Git coordinates,
attaches without compilation/network, never accesses that store, and leaves the
selection byte-identical. Explicit `RNX_DEP_RUNTIME` to the retained source then
builds a real path-native Polars assembly successfully. That is an opt-in
developer path, not a step required by the default journey.

A real Jupyter kernel uses the published combined artifact as its worker under a
private kernelspec. A query error preserves the frame, preview works, bare frames
remain opaque, and restart loses a saved binding while allowing a fresh frame.
The saved notebook validates. Sessions, PostgreSQL postmasters, kernels and
workers are reaped; no user kernelspec is touched.

## Reproducible evidence and limits

Bench: [`2c4877f`](https://github.com/nilket-com/rnx-bench/tree/2c4877f/results/one-install-0067),
with `probes/one-install/README.md` giving the full fresh-directory replay. Raw
PTY transcripts, losslessly compressed traces, install logs, compilation logs,
source coordinates, executable evidence hashes, declarations, lock pairs,
receipts and the notebook are retained. The fixture bundle preserves its exact
source delta. The README names the whole target/results reset requirement.

Root/tool formatting and strict all-target tool clippy pass. Tool suites pass
serially in ordinary and test-support configurations: 51 and 52 tests, with two
ignored integration tests in each. No product change was needed. The full root
feature suites and matched stock/launch measurements belong to Gate 5.

The two origin journeys ran concurrently. Their first preparations took about
199 seconds, combined preparations about 206 seconds and traced second
attachments about 9.5 seconds. These are harness observations, not performance
results or new cost promises. The no-compilation claim comes from invocation
traps, not elapsed time. Installation reused a build target; registry sources
were cached. This gate proves the workflow, not first-download cost or a
Windows execution path.
