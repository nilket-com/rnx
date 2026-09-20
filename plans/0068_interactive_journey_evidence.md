# 0068 gate 3: the interactive and notebook journey

Status: passes on Linux against the product at `a8e0f4e` (the gate 2 port).
Gate 4 remains open. The probe is `rnx-bench/probes/frame-journey/`; results
are `rnx-bench/results/frame-journey-0068/`, including the saved notebook.

## Setup

`prepare.py` archives the product with `git archive`, commits it as its own
repository (the tool fingerprints a path runtime from tracked files), and
builds the stock `rnx` and `rnx-jupyter` (46 s). A project with only a
runtime path is given Polars by the tool's own `rnx project add polars`, which
writes the declaration with `presentation = true`; lock and build take 112 s
into a private cache, and the lock's generated `main` carries
`.present("polars", native_0::present)`. Nothing is written outside the
probe's `target/` and the results directory; the notebook fixture uses a
temporary HOME and Jupyter directories, and the user's kernelspecs are
compared before and after.

## The prompt (`journey.json`, `target/journey/session.pty`)

`rnx project session --manifest … --no-splash` at a pseudo-terminal pinned to
`xterm-256color`, 30 rows × 120 columns; the executable is the cached
artifact. In order, with the input numbers the prompt printed:

- `let sales = polars::read_csv("sales.csv", …)?;` — nothing printed.
- `sales` — `[2] DataFrame: 5 rows × 4 columns`, the header and five rows;
  no `<::polars::DataFrame>`.
- `let big = sales.lazy().filter(qty > 2).collect()?;` then `big` —
  `[4] DataFrame: 3 rows × 4 columns` with `east | apple | 5` and no `pear`.
- the query error `sales.lazy().filter(col("missing") > 1).collect()?` —
  reported as a runtime error and the session continues: `sales` presents
  again at `[6]` and `big` at `[7]`. The text is the known top-level `?`
  diagnostic (`Expected type ::std::tuple::Tuple but found
  ::std::result::Result`, "position not in your input"); its wording is the
  open follow-up already recorded, not this record's, and recovery is what
  gate 3 asks for.
- `wide` (15 × 12) — `[9] DataFrame: 15 rows × 12 columns`, `[5 rows and 4
  columns omitted by display limits]`, columns `c0`…`c7`, rows to `900 |
  901 | …`; no `c8`, no row 1000.
- `let x = sales;` — nothing printed; `println!("{sales}")` prints the table
  without a number; `x` presents at `[12]`: numbers count inputs, as before.
- `:vars` lists `big`, `sales`, `wide`, `x` as `DataFrame =
  <::polars::DataFrame>` — its policy is unchanged.
- `:reset` — `session reset`; `sales` is `No local variable` at `[1]`; a
  fresh read presents at `[2]`.
- `:quit` exits 0; the history file holds `sales`, `let x = sales;` and the
  rest; a second session recalls `:quit` and then the read with two
  up-arrows, and Enter presents the frame at `[1]`. No artifact process
  remains after either session.

## The notebook (`notebook.json`, `journey.ipynb`)

`rnx-jupyter install --rnx <artifact>` into the private data directory
produces a kernelspec whose `argv` names the artifact. Driven with
`jupyter_client` from the record 0047 environment (kernel_dirs limited to the
private directory):

- cell 1 `let sales = …;` — no outputs; cell 2 `sales` — one
  `execute_result` whose `text/plain` is the table (`DataFrame: 5 rows × 4
  columns` … `"north" | "apple" | 4 | 1.5`), and no `text/html`.
- cell 3, the query error — status `error`, `RuntimeError`, the same
  diagnostic text as at the prompt.
- cell 4 `sales` — the identical table; cell 5 `[sales]` —
  `[<::polars::DataFrame>]`.
- `restart_kernel` — the worker process changes (pids 3613528 → 3613540, the
  old one gone), `sales` is a `CompileError` at execution count 1 (bindings
  are gone), and a fresh read presents the same table at count 2: the new
  worker owns a registry of its own.
- The seven cells with their outputs are written with `nbformat`, validated,
  reopened and validated again; the reopened cell 2 carries the table.
- Shutdown reaps the kernel and the worker: no `<artifact> worker` and no
  kernel process remains, and the user's `~/.local/share/jupyter/kernels`
  listing is unchanged.

## Qualifications and next gate

The kernel is driven through `jupyter_client`, not a browser; the 0047
browser acceptance is unchanged and not repeated here. The top-level `?`
diagnostic wording is a separate record. Costs, the walkthrough and the
notices are gate 4. No Windows claim is made.
