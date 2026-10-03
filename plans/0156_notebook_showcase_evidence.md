# rnx 0156: the notebook showcase, the evidence

`demos/notebooks/` holds the three 0155 demos as stateful Jupyter walkthroughs on rnx's own kernel: `01_mortgage.ipynb`, `02_orders.ipynb` and `03_report.ipynb`. It also holds the checker (`check.py`), its pinned environment (`requirements.txt`) and a README. `demos/README.md` points to them.

**No rnx or kernel code changed.** Everything below was run from the clean committed tree **4dc1422**, with root `rnx` at sha256 `acddc23c…713e`.

## 1. Kernel acceptance on the current `main` (record 0047, unchanged)

- **The run:** rnx-bench's `probes/jupyter-supervision/run.sh`, in a **scratch worktree** of rnx-bench (63531f1) beside the rnx checkout, with the pinned bench venv linked in. rnx-bench's tracked baseline, `results/jupyter-0047-supervision`, was not touched.
- **The result** (`probes/0156/acceptance/`): **exit 0.** All 12 runs pass, and so do the third-party notice check and the Windows type-check:
  - probe, extended, boundaries and notebook, each twice;
  - the wire probe and wire extended, each twice.
- **The binaries,** as recorded by the script:
  - **root `rnx`:** `acddc23c…713e`, the same before and after the run;
  - **`rnx-jupyter`:** `240a42ae…291f`. The script builds it with the `transport-probe` feature, as 0047 did.
- **Toolchain:** rustc and cargo 1.98.1. Everything else is in `versions.txt`.

**An earlier acceptance run,** before the impl commit, also passed. It's superseded: `build.rs` consults git, so the root binary built from a dirty tree had a different hash (`45b3177a…`). The rerun from the clean commit is the record.

## 2. The notebook check (`probes/0156/notebooks/check.txt`)

**The binaries:**
- **root `rnx`:** `acddc23c…713e`, the same binary as the acceptance run;
- **the plain kernel,** `rnx-jupyter` built without features as users install it: `05aedaea…f8b5`.

**The environment:** a **brand-new venv** created from `demos/notebooks/requirements.txt`, Python 3.14.4. Its `pip freeze` equals the file exactly, and the full freeze is in the log.

**Verification: exit 0.**
- **`01_mortgage` and `03_report`:** 2 runs each in fresh kernels.
- **`02_orders`:** 3 runs. One restarts **the same client-managed kernel**; the check confirms a new process id, and that a variable set before the restart is unknown after it.
- **Every run:** no error output and no stderr; outputs equal the committed ones; stdout equals the terminal demo run live with the notebook's data path. That live run equals `demos/out/NN.txt`, apart from the data path on 02's first line.
- **Verification writes no files.** Checksums of the notebooks were unchanged across a run.

**Controls: exit 0, every verdict as intended.**

| case | verdict |
|---|---|
| a harmless same-stream split | passes |
| one byte of an answer changed | refused |
| two genuinely different results swapped between cells | refused |
| stdout turned into stderr, by the comparison | refused |
| stdout turned into stderr, by the stderr rule | refused |
| a cell that raises | refused |
| a stream line not matching the terminal reference | refused |

**The terminal demos are unchanged.** `cargo test --test demos` still passes, 6 of 6.

## 3. Found and fixed during the implementation

- **The restart check didn't restart the kernel.** nbclient's kernel manager and client are async, so `restart_kernel` and `wait_for_ready` were called without being awaited, which Python warned about. The process-id guard passed vacuously, because both ids were `None`. **The fix:**
  - `nbclient.util.run_sync` for both calls;
  - a provisioner pid that must change and must not be `None`;
  - a state probe: `restart_marker`, set before the restart, must raise after it.

  The final runs use `-W error::RuntimeWarning`, so an unawaited coroutine would fail them.
- **The data path:** the order notebooks read `../data/orders.json`, because the kernel runs in the notebook's directory, so 02's first stdout line names that path. The checker therefore compares the notebook with the terminal demo run live with that same path, and requires that run to equal the committed transcript except for the path on that one line.

## 4. What the notebooks found about the kernel (no change made)

- **A string result displays quoted,** so the notebooks print their answers with `println!`.
- **`:dep` in a cell is a parse error.** Adapter notebooks would use an assembled worker, and are deferred.
- **`execute_result` is `text/plain` only,** which is enough for these examples.

## 5. Review round (Codex): the reader's JupyterLab setup, and a cleanup

**The README now gives the full setup.** The prerequisites are Rust, Git and Python 3 with `venv`. Then:
1. `cargo install` rnx, and build the kernel;
2. create a **separate JupyterLab environment** (`python3 -m venv ~/.venvs/rnx-lab`, then `pip install jupyterlab==4.6.3`, the tested version) and activate it, so its `jupyter` is on `PATH`;
3. run `rnx-jupyter install --rnx "$(command -v rnx)"`;
4. run `jupyter lab demos/notebooks`.

It says plainly that this environment is for reading and running the notebooks, and that the checker uses its own smaller pinned one. The Linux-only and unpublished wording stays.

**Verified in isolation,** with temporary `JUPYTER_DATA_DIR`, `JUPYTER_CONFIG_DIR`, `JUPYTER_PATH` and `JUPYTER_RUNTIME_DIR`, so the user's registry wasn't touched:
- a fresh venv installed JupyterLab 4.6.3;
- `rnx-jupyter install` reported "Installed Rune (rnx)";
- `jupyter kernelspec list` showed `rnx`;
- a headless `jupyter lab demos/notebooks` served `/api/kernelspecs` with `rnx` and listed the three notebooks.

**The cleanup:** `check.py`'s temporary kernelspec is now a `TemporaryDirectory`, removed when the checker exits. A verification run left no `rnx-0156-jupyter-*` directory behind, and still passes: 2, 3 and 2 runs.
