# 0068 gate 4: costs, regression and the walkthrough

Status: passes on Linux; the record closes on Linux with this gate. The
product is `80d40d2` (the seam is unchanged since the gate 2 port); the
baseline is `73532b5`, the last product before the seam. The probe is
`rnx-bench/probes/frame-final/`; results are `rnx-bench/results/frame-final-0068/`.
Sample journals are retained.

## Stock startup gate (`stock-summary.json`, `stock-gate.json`)

Matched release binaries built from the baseline worktree and the checkout,
one pinned core, five warm-ups, then 100 samples per command per binary in
two randomized interleaved repeats (`version`, `eval 42`, `run json.rn`,
PTY create-to-first-prompt), and 200 persistent prompt cells per binary per
repeat: 2,400 samples. Medians, baseline → product, with the change:

| command | repeat 0 | repeat 1 |
|---|---|---|
| version | 1.90 → 1.89 ms (−0.5%) | 1.89 → 1.88 ms (−0.7%) |
| eval | 5.36 → 5.37 ms (+0.2%) | 5.36 → 5.33 ms (−0.6%) |
| run json.rn | 12.93 → 13.03 ms (+0.8%) | 12.91 → 13.01 ms (+0.8%) |
| first prompt | 4.12 → 4.11 ms (−0.2%) | 4.13 → 4.14 ms (+0.1%) |
| prompt cell | 0.35 → 0.35 ms (−0.2%) | 0.35 → 0.35 ms (+0.2%) |

p10–p90 spreads are within ±0.2 ms of the medians for every process
command and ±0.03 ms for cells. Nothing reproduces above the 5% gate; the
stock binary carries an empty registry and one `BTreeMap` probe per
top-level prompt result.

## Polars costs (`polars-summary.json`)

Through the generated application's notebook worker (the exact wrapper the
tool builds with `presentation = true`), one pinned core, one Polars thread:
each cell is timed from the request write to the settled reply, ten cells ×
30 samples × two randomized interleaved repeats. `sales.csv` (5 × 4) reads
in 0.93 ms; `big.csv` (200,000 × 12, 15.5 MB) in 95 ms, once.

| cell | median, repeats 0 / 1 |
|---|---|
| `42` | 0.278 / 0.283 ms |
| `let _s = sales;` (no render) | 0.290 / 0.297 ms |
| `sales` (present) | 0.362 / 0.357 ms |
| `sales.preview()?` (explicit) | 0.374 / 0.388 ms |
| `format!("{sales}").len()` | 0.318 / 0.321 ms |
| `let _b = big;` (no render) | 0.292 / 0.298 ms |
| `big` (present, bounded) | 0.447 / 0.444 ms |
| `big.preview()?` | 0.450 / 0.463 ms |
| filter `c0 > 1,000,000` and collect, bound | 0.748 / 0.749 ms |
| the same collect, presented (116,666 rows) | 0.990 / 0.992 ms |

Derived: presenting the small frame costs 0.06–0.07 ms over suppressing
it, the large frame 0.15 ms — twice the small one because it renders 80
cells of wider integers to 569 bytes against 20 cells to 227, not because it
has 40,000 times the rows; the explicit preview costs a few hundredths of
a millisecond more than the automatic one (0.01–0.03 ms across the repeats); collecting the filter costs 0.47 ms and
presenting its 116,666-row result 0.24 ms more, separately. No work
proportional to the frame appears anywhere, which is the stop condition
this gate names.

Spawn to first prompt, PTY create-to-prompt, 100 samples × two repeats each,
interleaved: the presenting application 5.02 / 5.02 ms, the same
application without the field 5.01 / 5.02 ms. The difference (0.01 ms and
0.001 ms) is within the run-to-run noise of the measurement: registration
is not distinguishable from zero at the prompt, which is the claim, not an
isolated measurement of the registrar's work.

## Regression (`regression.json`, logs)

On the product checkout: `cargo fmt --check` in root, tool and adapter;
root tests default (390), test-support (435) and runner-only
`--no-default-features --features count-allocations,project-sources` (403),
serially; tool tests and strict clippy in both configurations (54, 55);
Polars tests and strict clippy in both configurations (9, 12, serially);
root, tool, Polars, PostgreSQL and http-postgres notices; `selfcheck`; the
PostgreSQL adapter and server suites; the packaged-manifest test. Root
strict clippy is compared with a real baseline invocation on the `73532b5`
worktree: the same fifteen diagnostic sites, none added, none removed.

Feature audits (`feature-checks.json`): the stock, runner-only and
server-runtime dependency graphs carry no Polars and no PostgreSQL; the
management crate is present only with the default feature; both generated
artifacts link no `rnx_project::` symbol.

## Walkthrough and documentation

`rnx-bench/examples/polars/SESSION.md` now types frame names bare, keeps
`preview()?` as the portable string-producing method (section 7 binds it,
compares it and prints it through `println!`/`format!`), and says what stays
opaque. `walkthrough.py` runs its 22 lines at the presenting application's
prompt: no diagnostic except the one section 5 provokes on purpose, bare
frames print tables, `:vars` lists frames by type and the bound preview
string quoted. `examples/polars/rnx.toml` declares `presentation = true` and
its README shows bare frames. The adapter and tool READMEs were updated in
gate 2, with the review's two wording nits folded in gate 3.

## Qualifications

Timing is one machine (nano, Rust 1.98.1) and one core; the Polars numbers
are the worker's wall clock including the Python client's observation, not
raw render time. The pre-existing parallel flake in the adapter's
test-support engine test (gate 2 evidence) is run serially here and stays a
follow-up. The top-level `?` diagnostic wording (gate 3) is a separate
record. Windows is not claimed.
