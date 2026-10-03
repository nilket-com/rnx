# rnx 0157: a Polars notebook, one short analyst workflow on bundled CSV

**Status:** plan.

**The direction (the user and Codex, 2026-10-03):** the Polars notebook comes next. It uses bundled CSV data, a short analytics workflow, checked results and the existing text display. HTML tables follow later and improve this same example.

**The cut:**
- **One notebook:** CSV in → filter and derive a column → group and summarize → the resulting DataFrame shown with the existing bounded text presenter.
- **An assembled Polars executable as the Jupyter worker,** built with rnx's own project tool, with the assembly and kernel setup documented exactly.
- **A checker:** a fresh-kernel run-all compared with independent expected results, reusing 0156's conventions.

**Out of this record:** HTML or any rich display, `:dep` in a cell, Candle, and any rnx, adapter or kernel code change.

## 1. What a probe already showed (2026-10-03, before this plan)

**The setup:**
- a scratch `rnx project` (manifest below) built with the current `main` (7e8e030);
- `rnx project build` printed the shared assembly's path, `~/.cache/rnx/assemblies/entries/<id>/artifacts/<hash>`;
- a temporary kernelspec pointing `rnx-jupyter --rnx` at that artifact;
- nbclient in 0156's pinned environment.

**The cold build took 5 min 49 s on this machine** (measured once, empty assembly cache, warm Cargo registry).

**What worked, each cell in a fresh kernel, through nbclient:**
- `polars::read_csv(path, [("units","i64"), …])?` read a 5-row CSV with an explicit schema;
- `sales.lazy().filter(polars::col("returned").not_()).with_columns([revenue])?.collect()?` with `revenue = (polars::col("units") * polars::col("price_cents"))?.alias("revenue_cents")`;
- `group_by([polars::col("region")])?.agg([… .sum().alias(…), polars::len().alias("orders")])?`;
- `sort_by_exprs([polars::col("revenue_cents")], polars::SortMultipleOptions::new().with_order_descending(true))?`;
- `select_([… .sum(), … .mean()])?` for a one-row summary;
- **a frame as a cell's value** shows the bounded preview as `execute_result` `text/plain`, and `println!("{}", frame.preview()?)` prints the same text as a stream. A 9-column frame shows the column elision line and `…`.

**What the notebook must allow for, all in the current adapter, none fixed here:**
- `not` and `select` are Rune keywords, so the bindings are **`not_()` and `select_()`**;
- **column arithmetic is fallible:** `(a * b)?` before `.alias(…)`;
- **group order isn't stable,** so every grouped frame is sorted before it's shown, on data with no ties in the sort key;
- `polars::len()` counts are `u32`.

## 2. The assembly route

**The reader's route is rnx's own project tool,** not a hand-written worker crate. It's the documented way to assemble adapters (records 0057, 0067), and its build prints the executable's path.

`demos/polars/` holds:
- **`rnx.toml`**, with relative paths into the checkout (as rnx-bench's `examples/polars` does):
  ```toml
  format = 1
  [application]
  entry = "sales.rn"
  [runtime]
  path = "../.."
  [native.polars]
  path = "../../adapters/polars"
  package = "rnx-polars"
  builder = "build"
  hook = "plain"
  presentation = true
  ```
- **`sales.rn`**, the same workflow as a terminal script. `rnx project run` prints the notebook's answer lines, so the notebook has a terminal reference, as in 0156.

**Not committed:** `rnx.lock` and `rnx.Cargo.lock` (the lock records absolute paths) and `.rnx/`. They're added to `.gitignore` there.

**Pinning:** the Rust dependencies come from the committed `Cargo.lock` of rnx and of `adapters/polars`, which the lock step uses. The evidence records the commit, rustc and cargo, the artifact's SHA-256 and the lock's input digests. A byte-identical executable on another machine isn't claimed.

**Setup, once:**
1. `cargo install --path . --locked`, so the `rnx` on PATH has `project`;
2. `rnx project lock --manifest demos/polars/rnx.toml`, then `rnx project build --manifest demos/polars/rnx.toml` (the stated cold time; it prints the executable's path);
3. `jupyter/target/release/rnx-jupyter install --rnx <that path> --replace`.

**The kernel name:** `rnx-jupyter install` always registers the spec named "rnx", so step 3 replaces 0156's plain kernel.
- The README says so plainly, and says how to switch back: rerun `install` with the plain `rnx` and `--replace`.
- The assembled worker is a superset (the whole stdlib plus `polars::`), so the 0156 notebooks run under it too. The checker runs them once under the Polars worker to prove that claim.
- No `--name` option is added in this record.

**After setup, everything runs offline.**

## 3. The data

`demos/data/sales.csv`, committed: about 24 rows. Its columns:
- `month` (2026-07 to 2026-09);
- `region` (4);
- `product` (3);
- `units`;
- `price_cents` (integer cents, as 0155 does);
- `returned` (bool).

**Chosen so that:**
- some rows are returned;
- per-region revenue has **no ties**;
- the raw frame is longer than the 10-row preview, so the notebook shows the bound honestly ("first 10 of 24") and the later, smaller frames show whole.

## 4. The notebook

`demos/notebooks/04_polars_sales.ipynb`, a short walkthrough. Markdown cells explain each step in a line or two.
1. **What it answers,** that it needs the Polars kernel (section 2), and that the data is `../data/sales.csv` (the kernel runs in the notebook's directory).
2. **Load** with an explicit schema; the frame is the cell's value (the bounded preview).
3. **Keep** non-returned rows and **derive** `revenue_cents`; the frame is shown.
4. **Revenue by region:** units, revenue and order count, sorted by revenue descending; the frame is shown.
5. **Revenue by month:** sorted by month; the frame is shown.
6. **The summary:** total and mean revenue, as a one-row frame.
7. **The answer:** printed with `println!`: the top region, its share and the net total in dollars.
   - If the adapter can't read a scalar back out of a frame without a code change, this cell prints the region and summary previews instead.
   - The plan doesn't change the adapter for it.

## 5. The checker

**`check.py` is extended rather than copied:** a notebook table entry can name its worker and its terminal reference. 0156's three notebooks keep their behaviour, and `--rnx`/`--kernel` remain. A new `--polars /abs/artifact` selects the Polars worker, in its own temporary kernelspec.

**For `04_polars_sales`, all of 0156's rules hold:**
- fresh kernels, two runs, one with a restart of the same kernel and a probe that state is gone;
- any `error` output or stderr fails;
- adjacent same-name streams coalesce, and nothing else is normalized;
- the stored outputs are compared;
- `--generate` is separate from verification.

**The terminal reference:** the notebook's stdout must equal `rnx project run --manifest demos/polars/rnx.toml` stdout, using the same built assembly.

**Independent expected results.** The checker computes these from `sales.csv` with Python's `csv` module, in integer cents, sharing no code with the workflow:
- the kept row count;
- per-region units, revenue and count;
- the region order;
- per-month revenue;
- the total and mean;
- the printed answer line's numbers.

It then **parses the displayed previews.** The format is: a header line `DataFrame: R rows × C columns`, then an optional omission line, then a `"name": dtype` schema line, then ` | `-separated rows. It requires the parsed region and month frames and the summary to equal the expectations exactly, row order included. That ties every shown number to the data, not only to a stored copy.

**Controls,** each with its stated verdict:
- the four from 0156 (one byte of an answer changed; two different results swapped; stdout as stderr; a cell that raises), applied to this notebook;
- **a CSV value edited in a temporary copy** makes the independent expectation disagree with the stored outputs. This proves the expectation reads the data, not the notebook;
- **two rows swapped in a displayed region frame** fail (the order is checked);
- **the preview parser refuses a truncated frame,** where it's asked for a whole one (the omission line).

## 6. Copy

- **`demos/notebooks/README.md`:** a "Polars notebook" section with the setup from section 2, the stated cold build time, the kernel replacement and how to switch back, and the four Rune spellings from section 1.
- **`demos/README.md`:** a one-line pointer.
- **No speed claims** beyond the measured build time, labelled cold and this-machine.

## 7. Evidence

`plans/0157_polars_notebook_evidence.md` records:
- the commit and toolchain;
- the artifact's SHA-256, built from the clean commit;
- the checker's verify and controls output;
- the 0156 notebooks under the Polars worker;
- `cargo test` still Python-free and green;
- `git diff --check`.

## 8. Out of scope

- HTML tables or `display_data` (the next step, improving this notebook).
- `:dep` in a cell.
- Candle notebooks.
- A kernelspec name option.
- Any change to rnx, the adapter, the kernel or the preview format. A concrete blocker is reported before the scope widens.
- Non-Linux kernels.
