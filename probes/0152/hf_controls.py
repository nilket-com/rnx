"""Record 0152: controls for the retrieval semantic check (hf_retrieval.py)
and its completion validation (hf_result.py --dims), on the already-spent
DEVELOPMENT data. Each corruption is applied to a copy of a validated
retrieval-only directory (or its queries file); the check runs, then
hf_result.py --require-pass --dims; a case is refused when the driver
would stop. Which layer refused is shown: validation (exit 1) or a
completed FAIL (exit 3). The unmodified copy must pass.

  hf_controls.py HF_PYTHON MODEL_DIR U8_DIR DEV_TSV D1"""
import json, os, pathlib, shutil, subprocess, sys, tempfile

hfpy, model, src, dev, d1 = sys.argv[1:6]
here = pathlib.Path(__file__).resolve().parent
N = 3461
FILES = ["u8-passages.json", "u8-retrieval.tsv", "u8-pool.tsv"]


def matrix_edit(fn):
    def go(d, q):
        p = d / "u8-retrieval.tsv"
        rows = [l.split("\t") for l in p.read_text().splitlines()]
        fn(rows)
        p.write_text("".join("\t".join(r) + "\n" for r in rows))
    return go


def lowest(rows, k):
    """The row index (1-based in rows) of query column k's lowest score."""
    return min(range(1, len(rows)), key=lambda i: float(rows[i][k]))


def bump(rows):
    i = lowest(rows, 1)
    rows[i][1] = repr(float(rows[i][1]) + 2e-4)


def dup_row(rows):
    i = lowest(rows, 1)
    rows[i] = [rows[i][0]] + rows[i - 1][1:]


def swap_rows(rows):
    rows[5], rows[6] = rows[6], rows[5]


def swap_cols(rows, header=True):
    for r in rows[0 if header else 1:]:
        r[1], r[2] = r[2], r[1]


def dup_col(rows):
    for r in rows[1:]:
        r[2] = r[1]


def queries_swap(d, q):
    lines = q.read_text().splitlines()
    a, b = lines[1].split("\t"), lines[2].split("\t")
    a[2], b[2] = b[2], a[2]
    lines[1], lines[2] = "\t".join(a), "\t".join(b)
    q.write_text("".join(l + "\n" for l in lines))


def passages_swap(d, q):
    p = d / "u8-passages.json"
    v = json.loads(p.read_text())
    # the last two passages of the last document (order within D1 is checked)
    v["texts"][-1], v["texts"][-2] = v["texts"][-2], v["texts"][-1]
    p.write_text(json.dumps(v))


def nonfinite(rows):
    i = lowest(rows, 1)
    rows[i][1] = "inf"


cases = {
    "unmodified": None,
    "a matrix row missing": matrix_edit(lambda r: r.pop()),
    "an extra matrix row": matrix_edit(lambda r: r.append([str(len(r) - 1)] + r[-1][1:])),
    "a duplicated matrix row (pid kept)": matrix_edit(dup_row),
    "two matrix rows reordered": matrix_edit(swap_rows),
    "a query column missing": matrix_edit(lambda r: [x.pop() for x in r]),
    "an extra query column": matrix_edit(lambda r: [x.append(x[-1] if i else "T99") for i, x in enumerate(r)]),
    "a duplicated query column (header kept)": matrix_edit(dup_col),
    "two query columns reordered with their header": matrix_edit(swap_cols),
    "two query columns' values reordered (wrong query binding)": matrix_edit(lambda r: swap_cols(r, header=False)),
    "two query texts swapped in the queries file (wrong query binding)": queries_swap,
    "two passage texts swapped (wrong passage binding)": passages_swap,
    "a non-finite retained score": matrix_edit(nonfinite),
    "a genuine over-tolerance difference (+2e-4, outside the pool)": matrix_edit(bump),
    "the expected passage count not met (3,462 required)": "shape",
}
ok = True
for name, edit in cases.items():
    with tempfile.TemporaryDirectory() as t:
        d, q = pathlib.Path(t, "u8"), pathlib.Path(t, "dev.tsv")
        d.mkdir()
        for f in FILES:
            shutil.copy(pathlib.Path(src, f), d / f)
        shutil.copy(dev, q)
        if callable(edit):
            edit(d, q)
        art = pathlib.Path(t, "hf.json")
        n = N + 1 if edit == "shape" else N
        r = subprocess.run([hfpy, here / "hf_retrieval.py", model, d, q, d1, str(n), art], capture_output=True, text=True)
        out = [l for l in (r.stdout + r.stderr).strip().splitlines() if "Loading weights" not in l]
        g = subprocess.run([sys.executable, here / "hf_result.py", "hf-retrieval", art, str(r.returncode),
                            "--require-pass", "--dims", str(N), dev], capture_output=True, text=True)
        gate = (g.stdout + g.stderr).strip().splitlines()[-1]
        layer = {0: "PASS", 1: "validation", 3: "completed FAIL"}.get(r.returncode, f"exit {r.returncode}")
        detail = next((l for l in reversed(out) if l.startswith(("FAIL", "semantic")) or "max |rnx" in l), out[-1] if out else "")
        if edit is None:
            good = r.returncode == 0 and g.returncode == 0
            print(f"{'pass' if good else 'WRONG'}: {name}: {detail}; {gate}")
        else:
            good = g.returncode != 0 and r.returncode != 0
            print(f"{'refused' if good else 'ACCEPTED'}: {name}: {layer}: {detail[:160]}; driver: {gate[:120]}")
        ok &= good

# the completion validation itself
with tempfile.TemporaryDirectory() as t:
    ids = [l.split("\t")[0] for l in open(dev).read().splitlines()[1:] if l]
    base = {"check": "hf-retrieval", "completed": True, "result": "PASS", "passages": N, "queries": len(ids),
            "values": N * len(ids), "query_ids": ids}
    arts = {
        "a well-formed PASS artifact": (base, "0", True),
        "a missing artifact": (None, "0", False),
        "a malformed artifact": ("{not json", "0", False),
        "another check's artifact": ({**base, "check": "hf-pool"}, "0", False),
        "not completed": ({**base, "completed": False}, "0", False),
        "PASS recorded with exit 3": (base, "3", False),
        "an exception (exit 1) with a PASS artifact": (base, "1", False),
        "a completed FAIL": ({**base, "result": "FAIL"}, "3", False),
        "the wrong passage count": ({**base, "passages": N - 1, "values": (N - 1) * len(ids)}, "0", False),
        "the wrong value count": ({**base, "values": N * len(ids) - 1}, "0", False),
        "a query missing": ({**base, "queries": len(ids) - 1, "values": N * (len(ids) - 1), "query_ids": ids[:-1]}, "0", False),
        "the query ids reordered": ({**base, "query_ids": [ids[1], ids[0]] + ids[2:]}, "0", False),
    }
    for name, (art, status, want) in arts.items():
        p = pathlib.Path(t, "a.json")
        if p.exists():
            p.unlink()
        if isinstance(art, dict):
            p.write_text(json.dumps(art))
        elif isinstance(art, str):
            p.write_text(art)
        g = subprocess.run([sys.executable, here / "hf_result.py", "hf-retrieval", p, status, "--require-pass",
                            "--dims", str(N), dev], capture_output=True, text=True)
        line = (g.stdout + g.stderr).strip().splitlines()[-1]
        good = (g.returncode == 0) == want
        ok &= good
        print(f"{('pass' if want else 'refused') if good else 'WRONG'}: hf_result, {name}: {line[:140]}")
print("all semantic-check controls behave" if ok else "CONTROLS FAILED")
sys.exit(0 if ok else 1)
