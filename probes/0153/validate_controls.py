"""Record 0153: corruption controls for validate.py. Each corruption is
applied to BOTH producers alike (agreement can't rescue it), except the
last, which is one-sided. Every one must be refused; the unmodified pair
must pass.

  validate_controls.py SCRIPT_DIR TWIN_DIR ISSUES_JSON SPLIT_TSV LABELS_TSV NLI_TOKENIZER_JSON"""
import json, pathlib, struct, subprocess, sys, tempfile

here = pathlib.Path(__file__).parent
a_dir, b_dir, issues, split, labels, tok = sys.argv[1:7]
FILES = ["t3-passages.json", "t3-status.tsv", "t3-embedding-status.tsv", "t3-embeddings.tsv", "t3-cosines.tsv", "t3-nli.tsv"]
data = {n: {f: pathlib.Path(d, f).read_text() for f in FILES} for n, d in (("a", a_dir), ("b", b_dir))}


def run(da, db):
    with tempfile.TemporaryDirectory() as t:
        dirs = []
        for name, files in (("a", da), ("b", db)):
            p = pathlib.Path(t, name)
            p.mkdir()
            for f, body in files.items():
                (p / f).write_text(body)
            dirs.append(p)
        r = subprocess.run([sys.executable, here / "validate.py", *dirs, issues, split, labels, tok], capture_output=True, text=True)
        return r.returncode, (r.stdout + r.stderr).strip().splitlines()[-1]


def lines_edit(name, fn):
    def go(files):
        files = dict(files)
        lines = files[name].splitlines()
        fn(lines)
        files[name] = "".join(l + "\n" for l in lines)
        return files
    return go


def json_edit(fn):
    def go(files):
        files = dict(files)
        v = json.loads(files["t3-passages.json"])
        fn(v)
        files["t3-passages.json"] = json.dumps(v)
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


def forged_refusal(files):
    files = dict(files)
    status = files["t3-status.tsv"].splitlines()
    ticket = status[1].split("\t")[0]
    status[1] = f"{ticket}\tnli_refused"
    files["t3-status.tsv"] = "".join(l + "\n" for l in status)
    files["t3-nli.tsv"] = "".join(l + "\n" for l in files["t3-nli.tsv"].splitlines() if l.split("\t")[0] != ticket)
    return files


cases = {
    "a ticket dropped from the passages": json_edit(lambda v: (v["tickets"].pop(), v["passages"].pop())),
    "two tickets reordered": json_edit(lambda v: (swap(v["tickets"], 0, 1), swap(v["passages"], 0, 1))),
    "a passage not in its ticket's text": json_edit(lambda v: v["passages"][0].__setitem__(0, v["passages"][0][0] + " invented")),
    "two passages of a ticket reordered": json_edit(lambda v: next(swap(ps, 0, 1) for ps in v["passages"] if len(ps) > 1)),
    "a status changed to nli_refused": lines_edit("t3-status.tsv", lambda l: set_(l, 1, field(l[1], 1, "nli_refused"))),
    "an unknown status": lines_edit("t3-status.tsv", lambda l: set_(l, 1, field(l[1], 1, "skipped"))),
    "an embedding row missing": lines_edit("t3-embeddings.tsv", lambda l: l.pop(2)),
    "an embedding scaled (norm off 1)": lines_edit("t3-embeddings.tsv", lambda l: set_(l, 1, "\t".join([l[1].split("\t")[0]] + [repr(float(x) * 1.01) for x in l[1].split("\t")[1:]]))),
    "a non-finite embedding value": lines_edit("t3-embeddings.tsv", lambda l: set_(l, 1, field(l[1], 5, "nan"))),
    "a cosine column dropped": lines_edit("t3-cosines.tsv", lambda l: [set_(l, i, "\t".join(l[i].split("\t")[:-1])) for i in range(len(l))]),
    "an NLI row missing": lines_edit("t3-nli.tsv", lambda l: l.pop(3)),
    "two NLI rows reordered": lines_edit("t3-nli.tsv", lambda l: swap(l, 1, 2)),
    "an NLI label renamed": lines_edit("t3-nli.tsv", lambda l: set_(l, 1, field(l[1], 2, "bugs"))),
    "a non-finite logit": lines_edit("t3-nli.tsv", lambda l: set_(l, 1, field(l[1], 4, "inf"))),
    "a forged nli_refused with that ticket's NLI rows deleted (review R2)": forged_refusal,
    "an unknown embedding status": lines_edit("t3-embedding-status.tsv", lambda l: set_(l, 1, field(l[1], 1, "skipped"))),
    "an embedding marked no_passages for a ticket with passages": lines_edit("t3-embedding-status.tsv", lambda l: set_(l, 1, field(l[1], 1, "no_passages"))),
    "an embedding marked zero_norm with its rows left in": lines_edit("t3-embedding-status.tsv", lambda l: set_(l, 1, field(l[1], 1, "zero_norm"))),
}
ok = True
code, out = run(data["a"], data["b"])
ok &= code == 0
print(f"{'pass' if code == 0 else 'WRONG'}: unmodified: {out}")
for name, f in cases.items():
    code, out = run(f(data["a"]), f(data["b"]))
    ok &= code != 0
    print(f"{'refused' if code else 'ACCEPTED'}: both, {name}: {out[:180]}")
one = {
    "an embedding value's lowest bit": lines_edit("t3-embeddings.tsv", lambda l: set_(l, 1, field(l[1], 7, step(l[1].split("\t")[7])))),
    "a cosine's lowest bit": lines_edit("t3-cosines.tsv", lambda l: set_(l, 1, field(l[1], 2, step(l[1].split("\t")[2])))),
    "a logit's lowest bit": lines_edit("t3-nli.tsv", lambda l: set_(l, 1, field(l[1], 3, step(l[1].split("\t")[3])))),
}
for name, f in one.items():
    code, out = run(data["a"], f(data["b"]))
    ok &= code != 0
    print(f"{'refused' if code else 'ACCEPTED'}: one side, {name}: {out[:180]}")
print("all validation controls behave" if ok else "CONTROLS FAILED")
sys.exit(0 if ok else 1)
