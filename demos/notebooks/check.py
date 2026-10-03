"""Record 0156: checks the notebook showcase (plans/0156 sections 3, 4, 5a).

    python check.py --rnx /abs/rnx --kernel /abs/rnx-jupyter            verify (default)
    python check.py --rnx /abs/rnx --kernel /abs/rnx-jupyter --generate write the committed outputs
    python check.py --rnx /abs/rnx --kernel /abs/rnx-jupyter --controls run the comparison controls

Run it with the pinned environment in requirements.txt. A temporary kernelspec in a
temporary JUPYTER_PATH / JUPYTER_DATA_DIR points at the given executables; the user's
own kernel registry is never read or changed. Verification never writes a committed file.

Per notebook, in a fresh kernel, all cells run. Any `error` output, or any stderr stream,
fails, whatever the client's error setting. The outputs are compared with the committed
ones after coalescing only adjacent same-name stream records within one cell (transport
chunking); execution counts, timestamps, ids and metadata are not compared, and no
output text or order is normalized. The notebook's stdout, concatenated, must equal the
terminal demo run live with the notebook's data path, and that live run must equal the
committed transcript demos/out/NN.txt (for 02, except the data path on its first line).
Each notebook runs twice in fresh kernels; one also restarts its own kernel and reruns
from the first cell."""
import argparse, copy, json, os, pathlib, subprocess, sys, tempfile

import nbclient
import nbformat
from nbclient.util import run_sync

HERE = pathlib.Path(__file__).resolve().parent
DEMOS = HERE.parent
NOTEBOOKS = {"01_mortgage": [], "02_orders": ["../data/orders.json"], "03_report": ["../data/orders.json"]}


_TEMPORARY = []


class Mismatch(Exception):
    pass


def kernelspec(rnx, kernel):
    holder = tempfile.TemporaryDirectory(prefix="rnx-0156-jupyter-")
    _TEMPORARY.append(holder)  # removed when the checker exits
    root = pathlib.Path(holder.name)
    spec = root / "kernels" / "rnx-check"
    spec.mkdir(parents=True)
    spec.joinpath("kernel.json").write_text(json.dumps({
        "argv": [kernel, "--connection-file", "{connection_file}", "--rnx", rnx],
        "display_name": "Rune (rnx, check)", "language": "rune"}))
    os.environ["JUPYTER_PATH"] = str(root)
    os.environ["JUPYTER_DATA_DIR"] = str(root)
    os.environ["JUPYTER_CONFIG_DIR"] = str(root / "config")
    os.environ["JUPYTER_RUNTIME_DIR"] = str(root / "runtime")
    return "rnx-check"


def outputs(cell):
    """A cell's outputs as comparable records; only adjacent same-name streams coalesce."""
    out = []
    for o in cell.get("outputs", []):
        if o["output_type"] == "stream":
            if out and out[-1][0] == "stream" and out[-1][1] == o["name"]:
                out[-1] = ("stream", o["name"], out[-1][2] + o["text"])
            else:
                out.append(("stream", o["name"], o["text"]))
        elif o["output_type"] in ("execute_result", "display_data"):
            out.append((o["output_type"], json.dumps(o.get("data", {}), sort_keys=True)))
        elif o["output_type"] == "error":
            out.append(("error", o.get("ename", ""), o.get("evalue", "")))
        else:
            out.append((o["output_type"], json.dumps(o, sort_keys=True)))
    return out


def judge(nb):
    """Fails on any error output or stderr stream; returns the concatenated stdout."""
    stdout = ""
    for i, cell in enumerate(nb.cells):
        for rec in outputs(cell):
            if rec[0] == "error":
                raise Mismatch(f"cell {i}: error output {rec[1]}: {rec[2]}")
            if rec[0] == "stream" and rec[1] != "stdout":
                raise Mismatch(f"cell {i}: unexpected {rec[1]} output {rec[2]!r}")
            if rec[0] == "stream":
                stdout += rec[2]
    return stdout


def compare(stored, ran):
    if len(stored.cells) != len(ran.cells):
        raise Mismatch("cell count differs")
    for i, (a, b) in enumerate(zip(stored.cells, ran.cells)):
        if a.cell_type != b.cell_type or a.source != b.source:
            raise Mismatch(f"cell {i}: source differs")
        if a.cell_type == "code" and outputs(a) != outputs(b):
            raise Mismatch(f"cell {i}: outputs differ:\n  stored {outputs(a)}\n  ran    {outputs(b)}")


def execute(nb, kernel_name, restart=False):
    """Runs all cells in a fresh kernel; with restart, then restarts that same kernel and
    reruns from the first code cell, returning both runs."""
    first = copy.deepcopy(nb)
    for cell in first.cells:
        if cell.cell_type == "code":
            cell.outputs, cell.execution_count = [], None
    client = nbclient.NotebookClient(first, timeout=120, kernel_name=kernel_name, allow_errors=True,
                                     resources={"metadata": {"path": str(HERE)}})
    with client.setup_kernel():
        for i, cell in enumerate(first.cells):
            client.execute_cell(cell, i)
        if not restart:
            return [first]
        second = copy.deepcopy(nb)
        for cell in second.cells:
            if cell.cell_type == "code":
                cell.outputs, cell.execution_count = [], None
        # the same client-managed kernel is restarted (nbclient's manager is async)
        marker = nbformat.v4.new_code_cell("let restart_marker = 1;")
        first.cells.append(marker)
        client.execute_cell(marker, len(first.cells) - 1)
        first.cells.pop()
        if any(o["output_type"] == "error" for o in marker.outputs):
            raise Mismatch("the restart marker could not be set")
        before = client.km.provisioner.pid
        run_sync(client.km.restart_kernel)(now=False)
        run_sync(client.kc.wait_for_ready)(timeout=60)
        after = client.km.provisioner.pid
        if before is None or after is None or before == after:
            raise Mismatch(f"the kernel process did not restart (pid {before} -> {after})")
        client.nb = second
        probe = nbformat.v4.new_code_cell("restart_marker")
        second.cells.append(probe)
        client.execute_cell(probe, len(second.cells) - 1)
        second.cells.pop()
        if not any(o["output_type"] == "error" for o in probe.outputs):
            raise Mismatch("state from before the restart survived it")
        for i, cell in enumerate(second.cells):
            client.execute_cell(cell, i)
        return [first, second]


def terminal(rnx, name):
    args = NOTEBOOKS[name]
    r = subprocess.run([rnx, "run", f"../{name}.rn", *args], cwd=HERE, capture_output=True, text=True)
    if r.returncode != 0 or r.stderr:
        raise Mismatch(f"{name}: the terminal demo failed: exit {r.returncode}, stderr {r.stderr!r}")
    transcript = (DEMOS / "out" / f"{name}.txt").read_text()
    live = r.stdout
    if args:
        lines = live.split("\n")
        if args[0] in lines[0]:
            lines[0] = lines[0].replace(args[0], "demos/data/orders.json")
        live = "\n".join(lines)
    if live != transcript:
        raise Mismatch(f"{name}: the live terminal run differs from demos/out/{name}.txt beyond the data path")
    return r.stdout


def verify(rnx, kernel_name, name, restart):
    path = HERE / f"{name}.ipynb"
    stored = nbformat.read(path, as_version=4)
    reference = terminal(rnx, name)
    runs = execute(stored, kernel_name, restart=restart) + execute(stored, kernel_name)
    for run in runs:
        stdout = judge(run)
        if stdout != reference:
            raise Mismatch(f"{name}: the notebook's stdout differs from the terminal demo:\n{stdout!r}\n{reference!r}")
        compare(stored, run)
    return len(runs)


def generate(kernel_name, name):
    path = HERE / f"{name}.ipynb"
    nb = nbformat.read(path, as_version=4)
    ran = execute(nb, kernel_name)[0]
    judge(ran)
    for cell in ran.cells:
        cell.metadata = {}
        for o in cell.get("outputs", []):
            o.pop("metadata", None) if o.get("output_type") == "stream" else o.update(metadata={})
    ran.metadata = nb.metadata
    nbformat.write(ran, path)


def controls(rnx, kernel_name):
    """Each comparison control (plans/0156 5a, items 2 and 3) must give the stated verdict."""
    ok = True

    def expect(name, fn, should_pass):
        nonlocal ok
        try:
            fn()
            passed = True
            detail = "accepted"
        except Mismatch as e:
            passed = False
            detail = str(e).splitlines()[0]
        good = passed == should_pass
        ok &= good
        print(f"{'pass' if good else 'WRONG'}: {name}: {detail}")

    stored = nbformat.read(HERE / "02_orders.ipynb", as_version=4)
    ran = execute(stored, kernel_name)[0]
    printing = next(i for i, c in enumerate(ran.cells) if any(o.get("name") == "stdout" for o in c.get("outputs", [])))

    def split(nb):
        nb = copy.deepcopy(nb)
        cell = nb.cells[printing]
        o = cell.outputs[0]
        text = o["text"]
        cut = len(text) // 2
        cell.outputs[0:1] = [nbformat.v4.new_output("stream", name="stdout", text=text[:cut]),
                             nbformat.v4.new_output("stream", name="stdout", text=text[cut:])]
        return nb
    expect("a harmless same-stream split", lambda: compare(stored, split(ran)), True)

    def changed(nb):
        nb = copy.deepcopy(nb)
        o = nb.cells[printing].outputs[0]
        o["text"] = o["text"].replace("245.00", "245.01", 1)
        assert o["text"] != ran.cells[printing].outputs[0]["text"]
        return nb
    expect("one byte of an answer changed", lambda: compare(stored, changed(ran)), False)

    results = [i for i, c in enumerate(ran.cells) if any(o["output_type"] == "execute_result" for o in c.get("outputs", []))]
    a, b = results[0], results[1]
    assert outputs(ran.cells[a]) != outputs(ran.cells[b])

    def swapped(nb):
        nb = copy.deepcopy(nb)
        nb.cells[a].outputs, nb.cells[b].outputs = nb.cells[b].outputs, nb.cells[a].outputs
        return nb
    expect("two different results swapped between cells", lambda: compare(stored, swapped(ran)), False)

    def stderr(nb):
        nb = copy.deepcopy(nb)
        nb.cells[printing].outputs[0]["name"] = "stderr"
        return nb
    expect("stdout turned into stderr (comparison)", lambda: compare(stored, stderr(ran)), False)
    expect("stdout turned into stderr (judge)", lambda: judge(stderr(ran)), False)

    broken = copy.deepcopy(stored)
    broken.cells.append(nbformat.v4.new_code_cell("let x = [1, 2];\nx[5]"))
    expect("a cell that raises", lambda: judge(execute(broken, kernel_name)[0]), False)

    def wrong_line():
        reference = terminal(rnx, "02_orders")
        if judge(ran) != reference.replace("Still open: 4", "Still open: 5"):
            raise Mismatch("the stream no longer matches the terminal reference")
    expect("a stream line not matching the terminal reference", wrong_line, False)
    return ok


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--rnx", required=True)
    p.add_argument("--kernel", required=True)
    mode = p.add_mutually_exclusive_group()
    mode.add_argument("--generate", action="store_true")
    mode.add_argument("--controls", action="store_true")
    a = p.parse_args()
    for exe in (a.rnx, a.kernel):
        if not os.path.isabs(exe) or not os.access(exe, os.X_OK):
            sys.exit(f"{exe}: give an absolute path to an executable")
    kernel_name = kernelspec(a.rnx, a.kernel)
    if a.generate:
        for name in NOTEBOOKS:
            generate(kernel_name, name)
            print(f"generated {name}.ipynb")
        return
    if a.controls:
        sys.exit(0 if controls(a.rnx, kernel_name) else 1)
    failed = False
    for name in NOTEBOOKS:
        try:
            n = verify(a.rnx, kernel_name, name, restart=(name == "02_orders"))
            print(f"ok: {name}: {n} runs match the stored outputs and the terminal demo"
                  + (" (one kernel restarted: new pid, earlier state gone)" if name == "02_orders" else ""))
        except Mismatch as e:
            failed = True
            print(f"FAIL: {name}: {e}")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
