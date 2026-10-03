# The demos as notebooks

The three terminal demos as short Jupyter walkthroughs: helpers first, then the inputs, then
each step in its own cell, then the answer. They run on rnx's own kernel, in plain Rune, with
text output and no downloads. A fourth notebook does a short analysis with Polars.

| notebook | what it shows |
| --- | --- |
| `01_mortgage.ipynb` | what a 30-year mortgage costs, one step per cell |
| `02_orders.ipynb` | JSON in, answers out: revenue by region, top customer, what is still open |
| `03_report.ipynb` | the same orders as an aligned table, built from format strings |
| `04_polars_sales.ipynb` | Polars on a bundled CSV: filter, derive, group, summarize; needs the Polars kernel (below) |
| `05_candle_model.ipynb` | Candle: a frame becomes a tensor, a tiny fixed model scores it, the scores come back as a column; needs the Candle kernel |
| `06_candle_search.ipynb` | Candle: semantic search with a pretrained model over short help articles; needs the Candle kernel and a one-time model download |

Each notebook prints exactly what its terminal demo prints, and `check.py` verifies that
(below).

## The kernel's status

rnx's Jupyter kernel is **Linux-only and unpublished**; supervision on other platforms is
unfinished (record 0047). Notebook cells don't take `:dep`; an adapter notebook uses an
executable with the adapter assembled in as the kernel's worker (the Polars notebook, below).

## Setup (once)

You need Rust, Git and Python 3 (with `venv`). From a checkout of the rnx repository:

```sh
# 1. rnx and its kernel
cargo install --path . --locked
cargo build --release --locked --manifest-path jupyter/Cargo.toml

# 2. a JupyterLab to open the notebooks in (the version these were tested with)
python3 -m venv ~/.venvs/rnx-lab
. ~/.venvs/rnx-lab/bin/activate
pip install jupyterlab==4.6.3

# 3. register the Rune (rnx) kernel with that Jupyter (its `jupyter` must be on PATH)
jupyter/target/release/rnx-jupyter install --rnx "$(command -v rnx)"
```

Then, with that environment active:

```sh
jupyter lab demos/notebooks
```

Open a notebook, choose the **Rune (rnx)** kernel, and run all cells. This JupyterLab
environment is for reading and running the notebooks; the checker below uses its own,
smaller environment.

## Working directory and data

A notebook's kernel runs in the notebook's own directory, `demos/notebooks/`, so the order
notebooks read `../data/orders.json`.

## The Polars notebook

`04_polars_sales.ipynb` reads `demos/data/sales.csv` (24 orders, money in integer cents),
leaves out returns, derives each order's revenue, groups by region and by month, and
summarizes. Frames show as tables in Polars' style, bounded: past 10 rows the first and last 5
with a `…` row between, past 8 columns the first and last 4. In Jupyter each frame also comes
as an HTML table (record 0159), built from the same bounded cells, with the text table kept
beside it as the fallback. rnx sends no styling, classes, attributes or scripts: the table takes
JupyterLab's own look, in light and dark themes alike.

**Its kernel is a different worker:** rnx with the Polars adapter assembled in. `demos/polars/`
is a small example executable built from this checkout, not a general adapter installer. It
registers the adapter the way `rnx project build`'s generated executable does, and its
`Cargo.lock` is committed, so the build uses the tested dependency versions. From the checkout
root, after the setup above:

```sh
# 4. the Polars worker: one cold build took about 6 minutes on our machine, with the
#    crates already downloaded; it downloads them first if they aren't
cargo build --release --locked --manifest-path demos/polars/Cargo.toml

# 5. point the Rune (rnx) kernel at it instead of plain rnx
jupyter/target/release/rnx-jupyter install --rnx "$PWD/demos/polars/target/release/rnx-polars-demo" --replace
```

The kernel is always registered as **Rune (rnx)**, so step 5 replaces the plain one. The Polars
worker is plain rnx plus `polars::`, so the other three notebooks run on it unchanged. To switch
back: `jupyter/target/release/rnx-jupyter install --rnx "$(command -v rnx)" --replace`. After
setup, nothing needs the network.

The same analysis runs in the terminal, from the checkout root:

```sh
demos/polars/target/release/rnx-polars-demo run demos/polars/sales.rn demos/data/sales.csv
```

A few Rune spellings in the notebook, as the adapter has them today:

- `not` and `select` are Rune keywords, so the bindings are `not_()` and `select_()`;
- column arithmetic can fail: `(polars::col("units") * polars::col("price_cents"))?` before `.alias(…)`;
- group order isn't stable, so every grouped frame is sorted before it's shown;
- `polars::len()` counts are `u32`.

## The Candle notebooks

`05_candle_model.ipynb` takes 64 rows of three numbers from a frame into an `f32` tensor, runs
a tiny model (3 inputs, 8 hidden units, 1 output) on the CPU, and puts the scores back in the
frame. Its weights are **fixed values chosen to show the plumbing, not trained on anything**.
Nothing is downloaded: the data and the model are in `demos/data`.

`06_candle_search.ipynb` embeds 24 short help articles and 6 questions with
sentence-transformers' all-MiniLM-L6-v2, and shows the three articles closest in meaning to
each question. The articles and questions were written for this demo: it's a showcase, not a
benchmark of search quality, and the notebook points out where the small model ranks a less
useful article first.

**Their kernel is another worker:** rnx with the Polars and Candle adapters assembled in.
`demos/candle/` is a small example executable built from this checkout, like `demos/polars/`,
with its `Cargo.lock` committed. It is a superset of the Polars worker, so all six notebooks run
on it. From the checkout root:

```sh
# 6. the Candle worker: one cold build took about 6 minutes on our machine, with the crates
#    already downloaded
cargo build --release --locked --manifest-path demos/candle/Cargo.toml

# 7. the model for 06, once: sentence-transformers/all-MiniLM-L6-v2 (Apache-2.0) at a pinned
#    revision, about 90 MB, every file's SHA-256 checked; it goes to demos/models/ (ignored by git)
sh demos/candle/fetch-model.sh

# 8. point the Rune (rnx) kernel at the Candle worker
jupyter/target/release/rnx-jupyter install --rnx "$PWD/demos/candle/target/release/rnx-candle-demo" --replace
```

rnx itself makes no network call; after step 7 everything runs offline. On our machine each
notebook runs in under two seconds, kernel start included. The same workflows run in the
terminal, from the checkout root:

```sh
demos/candle/target/release/rnx-candle-demo run demos/candle/model.rn demos/data
demos/candle/target/release/rnx-candle-demo run demos/candle/search.rn demos/data demos/models/all-MiniLM-L6-v2
```

Tensors show their dtype, shape and first 8 × 8 corner: as text at the prompt, and in Jupyter
also as a small table with row and column indices (record 0161). Frames show as tables, in HTML
in Jupyter. Both HTML forms carry the same values as the text beside them.

## Checking the notebooks (maintainers)

`check.py` runs each notebook in a fresh kernel, twice. For one notebook it also restarts that
same kernel and runs everything again. Then it requires:

- no error output and no stderr, whatever Jupyter's error setting;
- outputs equal to the committed ones. Only adjacent same-stream chunks within one cell are
  merged; output text and order are compared exactly;
- stdout equal to the terminal demo run live with the notebook's data path, and that run
  equal to the checked transcript in `demos/out/`, apart from the data path on its first line.
  For the Polars notebook, stdout equal to `sales.rn` run live by the same worker, and every
  displayed frame and the answer equal to what `check.py` computes from `sales.csv` itself.
- for `05_candle_model`, every shown frame, tensor and the answer equal to the model run again
  in plain Python from the bundled weights. Exact equality is established for these fixed
  weights only, whose every intermediate sum is a float32 exactly; it isn't a claim about
  models in general;
- for `06_candle_search`, every ranking equal in order and identity to a committed
  sentence-transformers (PyTorch) reference, made by rnx-bench's `probes/candle-search-0160`
  from the same CSVs and model files (both pinned by hash), with scores within `1e-5`. The six
  model files are checked by hash before the notebook runs, and a missing or changed model fails
  with the setup command.

It uses a temporary kernelspec, so it never touches your installed kernel, and it never writes
a committed file unless asked to with `--generate`.

```sh
python3 -m venv /tmp/rnx-nb && /tmp/rnx-nb/bin/pip install -r demos/notebooks/requirements.txt
/tmp/rnx-nb/bin/python demos/notebooks/check.py \
  --rnx "$PWD/target/release/rnx" --kernel "$PWD/jupyter/target/release/rnx-jupyter"
```

Add `--polars "$PWD/demos/polars/target/release/rnx-polars-demo"` to also check the Polars
notebook, and the plain three again on the Polars worker; add
`--candle "$PWD/demos/candle/target/release/rnx-candle-demo"` to check the Candle notebooks and
every other notebook on the Candle worker. `--only 05_candle_model` checks just that notebook,
without the model download. `--controls` runs the comparison's own controls, and `--generate`
rewrites the committed outputs (with `--polars` or `--candle`, only that worker's notebooks).
