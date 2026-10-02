"""Record 0152: corruption controls for validate_retrieval.py. Each
corruption is applied to BOTH producers alike (agreement can't rescue it),
except the last, which is one-sided. Every one must be refused; the
unmodified pair must pass. Run on the spent development data.

  validate_controls.py SCRIPT_DIR TWIN_DIR QUERIES_TSV D1"""
import json, pathlib, struct, subprocess, sys, tempfile
sys.path.insert(0, str(pathlib.Path(__file__).parent))
from validate_retrieval import text

here = pathlib.Path(__file__).parent
a_dir, b_dir, qfile, d1 = sys.argv[1:5]
FILES = ["u8-pool.tsv", "u8-passages.json", "u8-retrieval.tsv"]
data = {n: {f: text(d, f) for f in FILES} for n, d in (("a", a_dir), ("b", b_dir))}


def run(da, db):
    with tempfile.TemporaryDirectory() as t:
        dirs = []
        for name, files in (("a", da), ("b", db)):
            p = pathlib.Path(t, name)
            p.mkdir()
            for f, body in files.items():
                (p / f).write_text(body)
            dirs.append(p)
        r = subprocess.run([sys.executable, here / "validate_retrieval.py", *dirs, qfile, d1], capture_output=True, text=True)
        return r.returncode, (r.stdout + r.stderr).strip().splitlines()[-1]


def edit(name, fn):
    def go(files):
        files = dict(files)
        lines = files[name].splitlines()
        fn(lines)
        files[name] = "".join(l + "\n" for l in lines)
        return files
    return go


def field(line, i, v):
    f = line.split("\t")
    f[i] = v
    return "\t".join(f)


def step(x):
    return repr(struct.unpack("<f", struct.pack("<I", struct.unpack("<I", struct.pack("<f", float(x)))[0] + 1))[0])


def set_(l, i, v):
    l[i] = v


def swap(l, i, j):
    l[i], l[j] = l[j], l[i]


def passages_edit(files):
    files = dict(files)
    v = json.loads(files["u8-passages.json"])
    v["texts"][5] = v["texts"][5] + " invented"
    files["u8-passages.json"] = json.dumps(v)
    return files


def lowest_cell(lines):
    """(row index, value) of the first query column's lowest score."""
    i = min(range(1, len(lines)), key=lambda k: float(lines[k].split("\t")[1]))
    return i, lines[i].split("\t")[1]


pool = lambda fn: edit("u8-pool.tsv", fn)
cases = {
    "a missing pool row": pool(lambda l: l.pop(3)),
    "an extra pool row": pool(lambda l: l.append(l[-1])),
    "a duplicate pool row": pool(lambda l: set_(l, 3, l[2])),
    "two pool rows reordered": pool(lambda l: swap(l, 1, 2)),
    "a forged candidate rank": pool(lambda l: set_(l, 1, field(l[1], 1, "2"))),
    "another passage's pid": pool(lambda l: set_(l, 1, field(l[1], 4, str(int(l[1].split("\t")[4]) + 1)))),
    "a wrong document path": pool(lambda l: set_(l, 1, field(l[1], 2, "9999_nonexistent.md"))),
    "a pool retrieval score one f32 step away": pool(lambda l: set_(l, 1, field(l[1], 5, step(l[1].split("\t")[5])))),
    "a cross-encoder column added": pool(lambda l: [set_(l, i, l[i] + ("\tce" if i == 0 else "\t0.5")) for i in range(len(l))]),
    "a passage text not in its document": passages_edit,
    "a retained retrieval score changed (the pool no longer recomputes)":
        edit("u8-retrieval.tsv", lambda l: set_(l, 1 + int(data["a"]["u8-pool.tsv"].splitlines()[1].split("\t")[4]),
             field(l[1 + int(data["a"]["u8-pool.tsv"].splitlines()[1].split("\t")[4])], 1,
                   step(l[1 + int(data["a"]["u8-pool.tsv"].splitlines()[1].split("\t")[4])].split("\t")[1])))),
    "a non-finite retained score": edit("u8-retrieval.tsv", lambda l: set_(l, lowest_cell(l)[0], field(l[lowest_cell(l)[0]], 1, "nan"))),
}
ok = True
code, out = run(data["a"], data["b"])
ok &= code == 0
print(f"{'pass' if code == 0 else 'WRONG'}: unmodified: {out}")
for name, f in cases.items():
    code, out = run(f(data["a"]), f(data["b"]))
    ok &= code != 0
    print(f"{'refused' if code else 'ACCEPTED'}: both, {name}: {out[:200]}")
# one side: the lowest bit of a retained score outside every pool (each producer still validates alone)
b = edit("u8-retrieval.tsv", lambda l: set_(l, lowest_cell(l)[0], field(l[lowest_cell(l)[0]], 1, step(lowest_cell(l)[1]))))(data["b"])
code, out = run(data["a"], b)
ok &= code != 0
print(f"{'refused' if code else 'ACCEPTED'}: one side, a retrieval score's lowest bit outside the pool: {out[:200]}")
print("all validation controls behave" if ok else "CONTROLS FAILED")
sys.exit(0 if ok else 1)
