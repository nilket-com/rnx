"""Record 0147, review round 1 (R1): corruptions of U1' against the real
saved outputs. Each must make evaluate.py refuse; the unmodified U1' must
pass. A genuine tie is controlled synthetically in `evaluate.py --controls`
(the real run has none).

  u1_controls.py U5_DIR U1_TSV RUBRIC D1"""
import json, os, struct, subprocess, sys, tempfile

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(here, "..", "0146"))
from compare_u5 import text  # noqa: E402

u5, u1, rubric, d1 = sys.argv[1:5]
lines = open(u1).read().splitlines()
paths = json.loads(text(u5, "u5-passages.json"))["paths"]
row = lambda i: lines[i].split("\t")


def setf(i, **kw):
    f = row(i)
    for k, v in kw.items():
        f[{"query": 0, "rank": 1, "path": 2, "pid": 3, "score": 4}[k]] = v
    return lines[:i] + ["\t".join(f)] + lines[i + 1:]


def ulp(x):
    return repr(struct.unpack("<f", struct.pack("<I", struct.unpack("<I", struct.pack("<f", float(x)))[0] + 1))[0])


first = row(1)
top5 = {row(i)[2] for i in range(1, 6)}
# a document outside E01's top 5, so only the pid-to-path check can refuse it
other = next(p for p in sorted(set(paths)) if p not in top5)
# another passage of the same document, with that passage's own retained
# E01 score, so only the best-passage check can refuse it
retained = [l.split("\t") for l in text(u5, "u5-retrieval.tsv").splitlines()]
k = retained[0].index("E01")
same_doc_other_pid = next(i for i, p in enumerate(paths) if p == first[2] and i != int(first[3]))
same_doc_score = retained[1 + same_doc_other_pid][k]
cases = {
    "Codex's: a nonexistent document and pid, score kept": setf(1, path="9999_nonexistent.md", pid="999999999"),
    "Codex's: the score NaN, identity kept": setf(1, score="nan"),
    "an out-of-range pid, path kept": setf(1, pid=str(len(paths))),
    "a valid pid with another document's path": setf(1, path=other),
    "a forged finite score, identity kept (one f32 step)": setf(1, score=ulp(first[4])),
    "another passage of the same document, with its own retained score": setf(1, pid=str(same_doc_other_pid), score=same_doc_score),
    "a duplicated document": setf(2, path=first[2], pid=first[3], score=first[4]),
    "an unexpected query": setf(1, query="E99"),
    "an extra row": lines + ["E01\t6\t" + "\t".join(first[2:])],
    "a missing row": lines[:-1],
    "a wrong header": ["query\trank\tpath\tpid"] + lines[1:],
}
ok = True


def run(ls):
    with tempfile.NamedTemporaryFile("w", suffix=".tsv", delete=False) as t:
        t.write("".join(l + "\n" for l in ls))
    try:
        r = subprocess.run([sys.executable, os.path.join(here, "evaluate.py"), u5, t.name, rubric, d1], capture_output=True, text=True)
    finally:
        os.unlink(t.name)
    return r.returncode, (r.stdout + r.stderr).strip().splitlines()


code, out = run(lines)
ok &= code == 0
print(f"{'pass' if code == 0 else 'WRONG'}: unmodified: {next(l for l in out if l.startswith(chr(85) + chr(49)))}")
for name, ls in cases.items():
    code, out = run(ls)
    ok &= code != 0
    print(f"{'refused' if code else 'ACCEPTED'}: {name}: {out[-1]}")
print("all U1' controls behave" if ok else "CONTROLS FAILED")
sys.exit(0 if ok else 1)
