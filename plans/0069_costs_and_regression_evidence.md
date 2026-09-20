# 0069 gate 4: costs, regression and documentation

Status: passes on nano; the record closes on Linux once the user's journey
on slim — `:dep polars` then `:dep postgres` in a stock session, then a
fresh session's `:dep polars` — confirms the second wait and the attach on
Rust 1.95, which the committed gate requires. The product is `6acde0b` (gate 3 plus the documentation pass); the baseline is
`c6d8a1c`, the product before this record. The probe is
`rnx-bench/probes/shared-build-final/`; results and sample journals are in
`rnx-bench/results/shared-build-final-0069/`.

## The matrix, interleaved (`matrix-summary.json`, `matrix-samples.jsonl`)

Both stock binaries are built from matched sources. Every Git-source project
addresses a bare origin of the baseline tree, so both binaries compile the
same runtime and adapters and only the tool differs; each binary authors
its projects with its own `project add` (the product writes
`shared_build = true`; the baseline cannot). Three repeats, each in a fresh
cache, alternating which binary runs first; every executable's behaviour
verified through the notebook worker, every recorded row of every repeat
(24 rows). Whole `project build` times:

| step | baseline (three runs) | product (three runs) | Compiling entries | saved (median) |
|---|---|---|---|---|
| first Polars | 113.5, 114.3, 114.3 s | 112.7, 113.2, 113.7 s | 308 / 308 | 1.2 s |
| second wrapper over the same natives | 114.5, 115.1, 116.4 s | **4.9, 5.0, 5.0 s** | 308 / **1** | **110.1 s** |
| Polars + PostgreSQL | 114.7, 116.7, 117.1 s | 39.5, 39.5, 40.7 s | 326 / 31 | 77.3 s |
| the first declaration again (attach) | 2.43, 2.48, 2.51 s | 2.43, 2.51, 2.56 s | 0 / 0 | 0.0 s |

The first build costs the same: sharing does not make Polars compile faster,
it makes the next assembly not compile it. Behaviour: the second wrapper's
frames are opaque and the others present; `postgres::query` exists only in
the combined executable; every product assembly recorded `shared`.

Storage after a repeat's four preparations — three unique assemblies, the
fourth attaching to the first (medians): baseline 5.18 GB in three private
entries; product 0.65 GB in three entries plus 1.88 GB in one shared
directory — 2.53 GB against 5.18 GB for the same three executables.

## Stock startup gate (`stock-summary.json`, `stock-gate.json`)

Matched release binaries, one pinned core, five warm-ups, 100 samples per
command per binary in two randomized interleaved repeats, 200 persistent
prompt cells per binary per repeat: 2,400 samples. Medians baseline →
product: `version` 1.91 → 1.93 / 1.91 → 1.90 ms (+1.1%, −0.5%); `eval`
5.36 → 5.38 / 5.39 → 5.39 ms (+0.3%, 0.0%); `run json.rn` 13.00 → 12.97 /
12.99 → 12.97 ms (−0.2%, −0.1%); first prompt 4.09 → 4.08 / 4.14 → 4.15 ms
(−0.2%, +0.1%); prompt cell 0.34 → 0.34 ms (−1.3%, −0.9%). Nothing
reproduces above the 5% gate; the stock binary's launch path is untouched
by this record.

## Regression (`regression.json`, logs)

On the product checkout: `cargo fmt --check` in root, tool and Polars
adapter; root suites default, test-support and runner-only
(`--no-default-features --features count-allocations,project-sources`),
serially; tool suites and strict clippy in both configurations; Polars
suites and strict clippy in both configurations, serially; root, tool,
Polars, PostgreSQL and http-postgres notices; `selfcheck`; the PostgreSQL
adapter and server suites; the packaged-manifest test. Root strict clippy
against a real baseline invocation on the `c6d8a1c` worktree: the same
fifteen diagnostic sites, none added, none removed.

Feature audits (`feature-checks.json`): the stock, runner-only and
server-runtime dependency graphs carry no Polars and no PostgreSQL; the
management crate is present with the default feature and with
server-runtime; the product's four generated artifacts link no
`rnx_project::` symbol.

## Documentation

The tool guide's "The shared build directory" section states the
eligibility rule as decided (a Git runtime; every native a Git declaration
carrying `shared_build = true`), the listing and `remove build-<key>`
commands with their busy refusal and interrupted-removal naming, and the
conditional guarantee; "Add a known adapter" states what the declaration
means and who is bound by it; both adapter READMEs carry the contract
statement and the fork caveat.

## Qualifications

Timing is one machine (nano, Rust 1.98.1); the Polars numbers are three
interleaved runs per binary, not a distribution. Sharing applies to Git
declarations; path-source assemblies keep private directories, as measured
by the baseline column. The shared directory is Cargo's mutable output and
the record's guarantee rests on the `shared_build` declaration (gate 3).
The user's `:dep polars` then `:dep postgres` on slim (Rust 1.95) is the
remaining observation, outside this gate's fixtures. Windows is not claimed.
