# rnx 0156: a notebook showcase, the 0155 demos as checked Jupyter notebooks

**Status:** plan.

**The user (2026-10-03):** Polariton showcase work moves to the Polariton team; the Jupyter examples stay with us ("work with Codex and have fun"). The scope was agreed with Codex over chatd.

**The cut:** a pure-rnx Jupyter checkpoint.
- **Re-verify rnx's own kernel** (record 0047) on the current `main`.
- **Add three notebooks** derived from the 0155 terminal demos: plain Rune, text output.
- **Check them automatically,** as 0155's transcripts are checked.

**Out of this record:** adapter notebooks, an HTML display, and `:dep` inside a cell.

## 1. What a probe already showed (2026-10-03, before this plan)

**The setup:**
- the kernel built from `jupyter/`;
- a **temporary kernelspec** in a scratch `JUPYTER_PATH` and `JUPYTER_DATA_DIR`, so the user's registry was untouched;
- rnx-bench's pinned environment (`probes/jupyter-transport/.venv`: jupyterlab 4.6.3, nbclient 0.11.0, nbformat 5.11.1, jupyter_client 8.10.0, pyzmq 27.2.0).

**What worked:**
- `1 + 2` gives `execute_result 3`;
- `println!` arrives as a `stream`;
- a function defined in one cell is callable in the next;
- a sorted vector displays as a value.

**What the notebooks must allow for:**
- **A string result displays quoted** (`"1798.65"`), so the notebooks print their answers with `println!`.
- **`:dep polars` is a parse error in a cell.** That isn't a blocker for adapter notebooks: the existing route is an **assembled executable** used as the worker (`rnx-jupyter install --rnx /absolute/path/to/app`), whose Extensions reach worker mode (Codex's correction). Adapter notebooks are deferred to keep 0156 small.
- **`execute_result` is `text/plain` only,** and the worker already uses rnx's top-level native presenters. No rich output is needed here.

## 2. Kernel acceptance on the current `main`

**The run:** rnx-bench's `bash probes/jupyter-supervision/run.sh`, 0047's Linux acceptance, against the current `rnx` and `rnx-jupyter`. It's unchanged, in the same pinned environment, and installs no user kernelspec.
- **It covers:** real clients, the queue, byte streams, a blocked handoff, Linux descendants and nbclient notebook fixtures, each run twice, plus both transport fixtures.
- **The evidence records** the outcome and the environment line the script prints: rustc, cargo, uname, the pinned packages and both commits.
- **A failure is reported before anything else proceeds,** as a blocker. No core change is made without coming back first.

## 3. The notebooks

They live in `demos/notebooks/`, with the terminal demos as the reference.
- `01_mortgage.ipynb`
- `02_orders.ipynb`
- `03_report.ipynb`

**Each notebook exercises kernel state.** It isn't one cell wrapping a whole script; it reads as a short walkthrough:
1. A markdown cell says what the notebook answers.
2. A cell defines the helpers: `payment` and `money`, or `dollars` and `add`.
3. A cell loads or sets the inputs: the loan terms, or the orders read from JSON.
4. Cells compute in steps. A value is left as a cell's result where it reads well, such as a count or a sorted vector.
5. A cell prints the answer with `println!`.

Short markdown cells explain each step in a line or two.

**The data path:** the kernel's working directory is the notebook's directory, so the order notebooks read `../data/orders.json`. The notebook says so in its first cell.

**Tied to the checked answers:** each notebook's `stream` output, concatenated in order, must equal its terminal transcript `demos/out/NN_name.txt` byte for byte.
- So a notebook proves the same answers that `tests/demos.rs` verifies independently, through the kernel, without a second set of expected numbers.
- Its `execute_result` values are compared with the stored outputs.

## 4. The checker

`demos/notebooks/check.py` runs under the pinned Python environment. It takes the paths to `rnx` and `rnx-jupyter`.
- **The kernelspec:** it writes one into a fresh temporary `JUPYTER_PATH` and `JUPYTER_DATA_DIR`. It never touches the user's registry and never runs `rnx-jupyter install`.
- **Per notebook:**
  1. start a **fresh kernel** and run all cells with nbclient, with errors not allowed and a bounded timeout;
  2. **fail on any `error` output,** not merely on a failed process exit;
  3. compare every cell's outputs with the stored outputs;
  4. concatenate the streams and compare them with the terminal transcript.
- **Normalization is narrow:** only execution counts, timestamps, message and cell ids, and kernel metadata. **Result text and output order are never normalized.**
- **Restart and fresh state:** each notebook runs twice, each run in a new kernel. A second check runs one notebook, restarts the kernel through the client, and runs all again. Both runs must match.
- **Controls, each of which must be refused:**
  - a stored output edited by one character;
  - a cell that raises an error;
  - two cells' outputs swapped;
  - a stream line changed so it no longer matches the transcript.
- **The stored outputs** come from a clean run of the checker itself. Only the normalized fields are cleared, so the committed notebooks show real output when opened.

**The tests:** `cargo test` stays Python-free. The notebook check is a documented command in the evidence, and in `demos/notebooks/README.md` for maintainers. It isn't a cargo test, because it needs the pinned Python environment and a built kernel.

## 5. Copy and setup

`demos/notebooks/README.md` covers:
- **The status:** the kernel is **Linux-only and unpublished,** and non-Linux supervision is unfinished.
- **Setup, once,** kept apart from running a notebook:
  1. a checkout, then build `rnx` and `jupyter/`'s `rnx-jupyter`;
  2. `rnx-jupyter install --rnx /absolute/path/to/rnx`, the kernel's own documented install;
  3. JupyterLab, choosing **Rune (rnx)**.
- **The working directory and data path.**
- **One line per notebook** on what it shows, with no speed claims.

`demos/README.md` gets a short pointer to the notebooks.

## 5a. Folded in from Codex's acceptance (no new review cycle)

1. **A reproducible Python setup.**
   - `demos/notebooks/requirements.txt` is the pinned environment: a full freeze of a fresh venv with `nbclient`, `nbformat`, `jupyter_client` and `pyzmq` at the probe's versions.
   - The README documents creating a fresh venv from it and running `check.py` through it. The public checker never depends on rnx-bench's `.venv`.
   - The evidence records the complete tested environment. rnx-bench's environment is used only for the legacy acceptance in section 2.
2. **Stream chunks:** comparison coalesces only **adjacent** `stream` records with the **same name** within the **same cell**, concatenating their text verbatim.
   - Stream names (stdout and stderr), all text, and the order relative to `execute_result`, `error` or any other output kind are kept.
   - Nothing merges across cells or across other kinds. This is transport-chunk handling, not normalization of answer text.
   - **The controls:**
     - a harmless split of one stdout record into two passes;
     - changed bytes fail;
     - swapping two outputs **with genuinely different content** fails;
     - changing an output from stdout to stderr fails.
3. **The answer transcript uses stdout only.**
   - Any stderr output is a failure, so an error line can't complete a matching string.
   - `error` outputs are rejected by the checker itself, whatever the client's error setting.
4. **Generation is separate from verification:**
   - `check.py --generate` explicitly writes the committed notebooks' outputs;
   - the default mode only verifies, and never writes a committed file.
   - The committed outputs are generated once. A separate verification run then executes and compares them.
5. **The legacy acceptance** runs against a freshly built root `rnx` and the current kernel, with both hashes recorded (the script writes `source-and-binary-sha256.json`).
   - It runs in a scratch worktree of rnx-bench, so the tracked baseline in `results/jupyter-0047-supervision` is never overwritten.
   - The script ends with a Windows type-check. A missing cross-target or environment prerequisite is reported apart from any Linux kernel regression, and before any scope change.
   - No production fixes are made in this record.
6. **The restart check** restarts **the same client-managed kernel** (`KernelManager.restart_kernel`) and reruns from the first code cell. Creating a second fresh client doesn't count.

## 6. Out of scope

- Adapter notebooks (an assembled worker for Polars or Candle).
- An HTML display.
- `:dep` in a cell.
- Any rnx or kernel code change. A concrete blocker found here is reported before the scope widens.
- Non-Linux kernels.
