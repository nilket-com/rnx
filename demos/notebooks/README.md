# The demos as notebooks

The three terminal demos as short Jupyter walkthroughs: helpers first, then the inputs, then
each step in its own cell, then the answer. They run on rnx's own kernel, in plain Rune, with
text output and no downloads.

| notebook | what it shows |
| --- | --- |
| `01_mortgage.ipynb` | what a 30-year mortgage costs, one step per cell |
| `02_orders.ipynb` | JSON in, answers out: revenue by region, top customer, what is still open |
| `03_report.ipynb` | the same orders as an aligned table, built from format strings |

Each notebook prints exactly what its terminal demo prints, and `check.py` verifies that
(below).

## The kernel's status

rnx's Jupyter kernel is **Linux-only and unpublished**; supervision on other platforms is
unfinished (record 0047). Notebook cells don't take `:dep`; adapter notebooks would use an
assembled executable as the kernel's worker, and aren't part of these examples.

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

## Checking the notebooks (maintainers)

`check.py` runs each notebook in a fresh kernel, twice. For one notebook it also restarts that
same kernel and runs everything again. Then it requires:

- no error output and no stderr, whatever Jupyter's error setting;
- outputs equal to the committed ones. Only adjacent same-stream chunks within one cell are
  merged; output text and order are compared exactly;
- stdout equal to the terminal demo run live with the notebook's data path, and that run
  equal to the checked transcript in `demos/out/`, apart from the data path on its first line.

It uses a temporary kernelspec, so it never touches your installed kernel, and it never writes
a committed file unless asked to with `--generate`.

```sh
python3 -m venv /tmp/rnx-nb && /tmp/rnx-nb/bin/pip install -r demos/notebooks/requirements.txt
/tmp/rnx-nb/bin/python demos/notebooks/check.py \
  --rnx "$PWD/target/release/rnx" --kernel "$PWD/jupyter/target/release/rnx-jupyter"
```

`--controls` runs the comparison's own controls, and `--generate` rewrites the committed outputs.
