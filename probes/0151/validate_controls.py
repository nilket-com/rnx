"""Record 0151: corruption controls for validate.py. Each corruption is
applied to BOTH producers alike (agreement can't rescue it), except the
last, which is one-sided. Every one must be refused; the unmodified pair
must pass.

  validate_controls.py SCRIPT_DIR TWIN_DIR DEV_TSV D1"""
import json, pathlib, struct, subprocess, sys, tempfile
sys.path.insert(0, str(pathlib.Path(__file__).parent))
from validate import text

here = pathlib.Path(__file__).parent
a_dir, b_dir, dev, d1 = sys.argv[1:5]
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
        r = subprocess.run([sys.executable, here / "validate.py", *dirs, dev, d1], capture_output=True, text=True)
        return r.returncode, (r.stdout + r.stderr).strip().splitlines()[-1]


def pool_edit(fn):
    def go(files):
        files = dict(files)
        lines = files["u8-pool.tsv"].splitlines()
        fn(lines)
        files["u8-pool.tsv"] = "".join(l + "\n" for l in lines)
        return files
    return go


def field(line, i, v):
    f = line.split("\t")
    f[i] = v
    return "\t".join(f)


def step(x):
    return repr(struct.unpack("<f", struct.pack("<I", struct.unpack("<I", struct.pack("<f", float(x)))[0] + 1))[0])


def swap(l, i, j):
    l[i], l[j] = l[j], l[i]


def set_(l, i, v):
    l[i] = v


def passages_edit(files):
    files = dict(files)
    v = json.loads(files["u8-passages.json"])
    v["texts"][5] = v["texts"][5] + " invented"
    files["u8-passages.json"] = json.dumps(v)
    return files


def retrieval_edit(files):
    files = dict(files)
    lines = files["u8-retrieval.tsv"].splitlines()
    first_pid = data["a"]["u8-pool.tsv"].splitlines()[1].split("\t")[4]
    f = lines[1 + int(first_pid)].split("\t")
    f[1] = step(f[1])
    lines[1 + int(first_pid)] = "\t".join(f)
    files["u8-retrieval.tsv"] = "".join(l + "\n" for l in lines)
    return files


cases = {
    "a missing pool row": pool_edit(lambda l: l.pop(3)),
    "an extra pool row": pool_edit(lambda l: l.append(l[-1])),
    "a duplicate pool row": pool_edit(lambda l: set_(l, 3, l[2])),
    "two pool rows reordered": pool_edit(lambda l: swap(l, 1, 2)),
    "a forged candidate rank": pool_edit(lambda l: set_(l, 1, field(l[1], 1, "2"))),
    "another passage's pid": pool_edit(lambda l: set_(l, 1, field(l[1], 4, str(int(l[1].split("\t")[4]) + 1)))),
    "a wrong document path": pool_edit(lambda l: set_(l, 1, field(l[1], 2, "9999_nonexistent.md"))),
    "a retrieval score one f32 step away": pool_edit(lambda l: set_(l, 1, field(l[1], 5, step(l[1].split("\t")[5])))),
    "a non-finite cross-encoder score": pool_edit(lambda l: set_(l, 1, field(l[1], 6, "nan"))),
    "a passage text not in its document": passages_edit,
    "a retained retrieval score changed (the pool no longer recomputes)": retrieval_edit,
}
ok = True
code, out = run(data["a"], data["b"])
ok &= code == 0
print(f"{'pass' if code == 0 else 'WRONG'}: unmodified: {out}")
for name, f in cases.items():
    code, out = run(f(data["a"]), f(data["b"]))
    ok &= code != 0
    print(f"{'refused' if code else 'ACCEPTED'}: both, {name}: {out}")
b = dict(data["b"])
lines = b["u8-pool.tsv"].splitlines()
f = lines[1].split("\t")
bits = bytearray(struct.pack("<f", float(f[6])))
bits[0] ^= 1
f[6] = repr(struct.unpack("<f", bytes(bits))[0])
lines[1] = "\t".join(f)
b["u8-pool.tsv"] = "".join(l + "\n" for l in lines)
code, out = run(data["a"], b)
ok &= code != 0
print(f"{'refused' if code else 'ACCEPTED'}: one side, a cross-encoder score's lowest bit: {out}")
print("all validation controls behave" if ok else "CONTROLS FAILED")
sys.exit(0 if ok else 1)
