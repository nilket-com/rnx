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

## 5a. Folded in from Codex's acceptance and one route departure (no new review cycle)

1. **The assembly route changed: a standalone worker crate replaces the project tool** (Codex accepted it over chatd before implementation).
   - `rnx project lock` refuses a project inside a native package root it fingerprints, and the rnx checkout is one: "project lock would be inside native package root; move the project" (`tools/project/src/workflow/shared.rs`). So section 2's in-repo `rnx.toml` can't work. That is a guard, not something to bypass.
   - `demos/polars/` is a small crate with its own `[workspace]`, `publish = false`, relative path dependencies on rnx (`default-features = false, features = ["count-allocations"]`) and `adapters/polars`, a committed `Cargo.lock`, and `--locked` builds.
   - Its `main` registers the adapter the way the project tool's generated executable does (Extensions with `build` and `present`). It isn't the identical assembly: its own package identity, and no `project-sources`, which it doesn't use.
   - It's an example executable built from the checkout, not a general adapter installer. The tested setup is the checkout revision plus its committed worker lock.
   - The terminal reference is the same executable: `demos/polars/target/release/rnx-polars-demo run demos/polars/sales.rn <csv>`, from the checkout root. The notebook, the terminal reference and the checks all use the very same worker artifact.
2. **Dependency pinning, corrected.** The project tool seeds resolution from the runtime's `Cargo.lock` as a preference and resolves without `--locked`; it doesn't import `adapters/polars/Cargo.lock`. The worker's committed lock is instead the resolution itself, built with `--locked`. It was seeded the same way, from the root `Cargo.lock`. The evidence records its digest and compares it with the project tool's generated lock for the same sources. No claim that a fresh resolution on another date gives the same versions.
3. **Every displayed frame is checked against the CSV,** not only the summaries.
   - The raw and kept frames are intentionally partial: the exact header, the schema in order with dtypes, the omission count, and the visible first ten rows.
   - The region, month and summary frames must be whole: exact dimensions, schema and rows in order, and no omission, elision or byte-limit marker.
   - The parser fails on extra, missing or duplicate rows or columns and on non-finite numbers. It is scoped to this example's simple cells, not a general preview parser.
4. **The mean is an `f64`.** For this data it is exactly representable (122,750 / 20 = 6137.5), so the displayed value must equal total ÷ count exactly, with no tolerance. Cents become dollars only in the answer, explicitly.
5. **The scalar read-back is the shipped form:** `frame.column(name)?.i64()?.get(row)?` and `.str()?.get(0)?`. No fallback and no new binding.
6. **Paths and cwd:** the notebook reads `../data/sales.csv` from `demos/notebooks/`; the checker runs the terminal script with absolute script and data paths, from the checkout root. The script prints no path, so no path-line adjustment is allowed or needed.
7. **Backward-compatible selection:** without `--polars`, the checker verifies 0156's notebooks on plain rnx exactly as before. With `--polars`, it also verifies the Polars notebook (with the restart check) and the plain three on the Polars worker. Verification writes no committed file; `--generate --polars` writes only the Polars notebook.
8. **Controls:**
   - the CSV edit must change a checked region result, and is asserted to;
   - the omission control is applied to the region frame, where a whole frame is required;
   - a duplicated region row and a non-finite mean are also refused.

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
