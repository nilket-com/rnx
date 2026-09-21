# 0070 gate 3: costs, regression and documentation

Status: passes on nano; the record closes on Linux once the user runs
`:dep polars` and `:dep postgres` on slim and reports what the screen
showed. The product is this record's impl commit; the
baseline is `39beeaa`, the product before this record. The probe is
`rnx-bench/probes/quiet-dep-final/`; results, sample journals and the
recorded screens are in `rnx-bench/results/quiet-dep-final-0070/`.

## Stock startup gate (`stock-summary.json`, `stock-gate.json`)

Matched release binaries from the two revisions, one pinned core, five
warm-ups, 100 samples per command per binary in two randomized interleaved
repeats, 200 persistent prompt cells per binary per repeat: 2,400 samples.
Medians baseline → product: `version` 1.91 → 1.88 / 1.91 → 1.90 ms (−1.6%,
−0.4%); `eval` 5.36 → 5.34 / 5.34 → 5.36 ms (−0.3%, +0.4%); `run json.rn`
12.95 → 12.94 / 12.95 → 12.93 ms (−0.1%, −0.2%); first prompt 4.07 → 4.07 /
4.07 → 4.07 ms (−0.1%, 0.0%); prompt cell 0.349 → 0.349 / 0.347 → 0.349 ms
(−0.2%, +0.8%). Nothing reproduces above the 5% gate. The capture lives in
the tool's transition path and the session's changes are a request field
and two command-table entries; the launch path is untouched.

## The transition with and without capture (`transition-summary.json`, `transition-samples.jsonl`, `screens/`)

The product installed as stock `rnx` from a private bare origin (0067's
fixture: `repository` names the origin, so the Git runtime and the Polars
adapter come from this machine). Each preparation is a fresh session in a
PTY; the clock runs from the line being written to the replacement's
first prompt, and for `:depv` includes the driver answering `Continue?`.
Only the stock gate pins a core; this driver does not: a pinned driver
hands its affinity to Cargo, which serialised a first build to seventeen
minutes in a discarded run. Each home has its own project cache, state,
history and Cargo home (registry shared read-only).

Cold, one first Polars build each in a fresh home: `:dep` 113.1 s, `:depv`
118.0 s. The quiet screen for the whole two minutes is

```
[1] > :dep polars
[1] >
```

and the verbose screen ends in Cargo's `Finished` line, the phases, the
restart line and the reopen command, as before.

Warm, in one home whose entry already exists (each preparation then is a
resolution, an attach, a startup check and a restart), 10 samples per
spelling in two randomized interleaved repeats, 40 samples:

| spelling | repeat 0 median (p10–p90) | repeat 1 median (p10–p90) |
|---|---|---|
| `:dep` (captured, log written) | 7.60 s (7.08–7.97) | 7.24 s (7.12–7.65) |
| `:depv` (streams to the terminal) | 7.44 s (7.14–9.15) | 7.46 s (7.27–7.82) |

The difference changes sign between repeats (quiet +0.17 s, then −0.22 s)
and is inside either spelling's own spread: no reproducible overhead at
this resolution, which is not a proof of zero cost. An earlier run of this driver, discarded when the
Polars assertion was added, had shown quiet 0.15 s slower in both repeats;
an `strace -f` timeline of one preparation each, `cargo metadata` timed
with a terminal against a pipe on stderr (0.216 against 0.219 s), and the
drain's EOF (the pipe's write end is dropped after the `dup2`s) had found
nothing to attribute it to, which this rerun explains.

After each timed preparation, outside the clock, the driver evaluates
`polars::lit(42).is_ok()` at the replacement's prompt and requires `true`,
and for `:dep` requires the screen to be the echoed line and the new
prompt and nothing else: a refused request also ends at a prompt, and
these assertions keep a future run from timing a refusal (Codex's gate 3
nit). All 42 preparations passed both.

## Regression (`regression.json`, logs)

On the product checkout: `cargo fmt --check` in root, tool and Polars
adapter; root suites default, test-support and runner-only
(`--no-default-features --features count-allocations,project-sources`),
serially; tool suites and strict clippy in both configurations; Polars
suites and strict clippy in both configurations, serially; root, tool,
Polars, PostgreSQL and http-postgres notices; `selfcheck`; the PostgreSQL
adapter and server suites; the packaged-manifest test. All green. Root
strict clippy against a real baseline invocation on the `39beeaa`
worktree: the same fifteen diagnostic sites, none added, none removed.

Feature audits (`feature-checks.json`): the stock, runner-only and
server-runtime dependency graphs carry no Polars and no PostgreSQL; the
management crate is present with the default feature and with
server-runtime and absent from the runner-only graph and the adapters.

## Documentation

`README.md` states the two spellings in the installation section: `:dep`
prints only what varies — a failure, with the error, the last lines of
Cargo's output and the path of the log that holds it up to a size limit,
with a marker naming what was left out; `:depv` is the same preparation
with the notice, a confirmation and Cargo's output on the terminal; the
reopen command is the log's first line. The tool guide's "Requesting
adapters from a live session" states the log's location
(`<project>/.rnx/dep.log`), its contents and truncation marker, when it is
created and replaced, that it is not an identity input, the silent no-op,
and that an installed tool without quiet mode refuses `:dep` by name and
still serves `:depv`. `:help` lists both spellings.

## Qualifications

Timing is one machine (nano, Rust 1.98.1). The warm comparison is 20
samples per spelling in one home, not a distribution across machines; the
cold comparison is one build each. The transition fixture installs from a
private origin, so what it measures is the tool's path, not GitHub. The
user's `:dep polars` and `:dep postgres` on slim are the remaining
observation; Windows is not claimed.
