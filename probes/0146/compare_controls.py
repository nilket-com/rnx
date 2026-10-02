"""Record 0146: U5's comparer corruption controls. Each corruption is
applied to BOTH traces (agreement alone would pass) or, where noted, to one;
every one must be refused, and the unmodified pair must pass. Review round
1 adds corruptions of the identities (applied to both producers, so
agreement cannot rescue them) and of the retained evidence.

  compare_controls.py SCRIPT_DIR TWIN_DIR RUBRIC D1"""
import json, pathlib, struct, subprocess, sys, tempfile
from compare_u5 import text

here = pathlib.Path(__file__).parent
da, db, rubric, d1 = sys.argv[1:5]
a0 = text(da, "u5-trace.tsv").splitlines()
b0 = text(db, "u5-trace.tsv").splitlines()
passages_a = json.loads(text(da, "u5-passages.json"))
FILES = ["u5-trace.tsv", "u5-passages.json", "u5-retrieval.tsv", "u5-pairs.json"]


def run(a, b, edit=None, sides=()):
    """Both traces as given; `edit(dir)` changes retained files in `sides`."""
    with tempfile.TemporaryDirectory() as t:
        out = []
        for name, src, lines in (("a", da, a), ("b", db, b)):
            d = pathlib.Path(t, name)
            d.mkdir()
            for f in FILES[1:]:
                (d / f).write_text(text(src, f))
            (d / "u5-trace.tsv").write_text("".join(l + "\n" for l in lines))
            if edit and name in sides:
                edit(d)
            out.append(d)
        r = subprocess.run([sys.executable, here / "compare_u5.py", *out, rubric, d1], capture_output=True, text=True)
        return r.returncode, (r.stdout + r.stderr).strip().splitlines()[-1]


def at(i, f):
    return lambda l: [f(x) if k == i else x for k, x in enumerate(l)]


def field(x, i, v):
    p = x.split("\t"); p[i] = v; return "\t".join(p)


first_rerank = next(i for i, l in enumerate(a0) if l.startswith("rerank\t"))
first_metrics = next(i for i, l in enumerate(a0) if l.startswith("metrics\t"))
both = {
    "both empty": lambda l: [],
    "a candidate deleted": lambda l: l[:5] + l[6:],
    "a whole query deleted": lambda l: l[21:first_metrics] + l[first_metrics + 1:],
    "a candidate's ce score NaN": at(3, lambda x: field(x, 6, "nan")),
    "retrieval order broken": lambda l: [l[1], l[0]] + l[2:],
    "rerank not a permutation": at(first_rerank, lambda x: field(x, 2, ",".join(["1"] * 20))),
    "rerank not by score": at(first_rerank, lambda x: field(x, 2, ",".join(str(i) for i in range(1, 21)))),
    # the order check itself: two retrieval scores swapped, numbering kept
    "retrieval scores out of order": lambda l: [field(l[0], 5, l[1].split("\t")[5]), field(l[1], 5, l[0].split("\t")[5])] + l[2:],
    "a metric changed": at(first_metrics, lambda x: field(x, 4, "0.5")),
    "a hit flag flipped": at(first_metrics, lambda x: field(x, 6, "false" if x.split("\t")[6] == "true" else "true")),
    "a duplicated document": at(1, lambda x: field(x, 3, a0[0].split("\t")[3])),
    "an unknown line": lambda l: l + ["note\tx"],
    # review round 1: identities, which must match the retained evidence
    "a nonexistent document": at(19, lambda x: field(x, 3, "9999_nonexistent.md")),
    "an out-of-range pid": at(19, lambda x: field(x, 4, "999999999")),
    # a pid of a document outside Q1's candidates, so only the binding can
    # refuse it (not the distinct-passages check)
    "an existing document with another document's pid": at(19, lambda x: field(x, 4, str(next(
        pid for pid, p in enumerate(passages_a["paths"])
        if p not in {l.split("\t")[3] for l in a0[:20]})))),
    "a document swapped for another existing one": at(19, lambda x: field(x, 3, next(
        p for p in sorted(passages_a["paths"])
        if p not in {l.split("\t")[3] for l in a0[:20]}))),
    # candidate 1's score raised by one f32 step: the order stays monotone
    "a retrieval score changed, still monotone": at(0, lambda x: field(x, 5, repr(struct.unpack(
        "<f", struct.pack("<I", struct.unpack("<I", struct.pack("<f", float(x.split("\t")[5])))[0] + 1))[0]))),
}
ok = True
code, out = run(a0, b0)
print(f"{'pass' if code == 0 else 'WRONG'}: unmodified: {out}")
ok &= code == 0
for name, f in both.items():
    code, out = run(f(a0), f(b0))
    ok &= code != 0
    print(f"{'refused' if code else 'ACCEPTED'}: both, {name}: {out}")
def flip(lines):
    p = lines[0].split("\t")
    b = bytearray(struct.pack("<f", float(p[6]))); b[0] ^= 1
    p[6] = repr(struct.unpack("<f", bytes(b))[0])
    return ["\t".join(p)] + lines[1:]
code, out = run(flip(a0), b0)
ok &= code != 0
print(f"{'refused' if code else 'ACCEPTED'}: one side, a ce score's lowest bit: {out}")


def edit_json(name, f):
    def go(d):
        v = json.load(open(d / name))
        f(v)
        (d / name).write_text(json.dumps(v))
    return go


def edit_lines(name, f):
    def go(d):
        lines = (d / name).read_text().splitlines()
        (d / name).write_text("".join(l + "\n" for l in f(lines)))
    return go


cand1 = int(a0[0].split("\t")[4])
# retained evidence, edited in both producers
retained = {
    "a passage text not in its document": edit_json("u5-passages.json", lambda v: v["texts"].__setitem__(cand1, v["texts"][cand1] + " invented")),
    # to the first document that is not its own (0147: a neighbouring pid can
    # belong to the same document, which made this a no-op on 0147's data)
    "a passage moved to another document": edit_json("u5-passages.json", lambda v: v["paths"].__setitem__(cand1, next(
        p for p in v["paths"] if p != v["paths"][cand1]))),
    "a D1 document with no passages": edit_json("u5-passages.json", lambda v: [v[k].__setitem__(slice(None), [x for x, p in zip(v[k], list(v["paths"])) if p != v["paths"][0]]) for k in ("texts", "paths")]),
    "a retained score raised past candidate 1": edit_lines("u5-retrieval.tsv", lambda l: [l[0]] + [
        "\t".join([f[0], "0.99"] + f[2:]) if f[0] == a0[20 * 0 + 5].split("\t")[4] else r
        for r in l[1:] for f in [r.split("\t")]]),
    "a retained row deleted": edit_lines("u5-retrieval.tsv", lambda l: l[:-1]),
    "a pair's passage changed": edit_json("u5-pairs.json", lambda v: v["passages"].__setitem__(3, v["passages"][4])),
    "a pair's query changed": edit_json("u5-pairs.json", lambda v: v["queries"].__setitem__(0, v["queries"][0] + "?")),
}
for name, f in retained.items():
    code, out = run(a0, b0, f, ("a", "b"))
    ok &= code != 0
    print(f"{'refused' if code else 'ACCEPTED'}: both, {name}: {out}")


# one producer's retained score, one f32 step away (its candidates unchanged)
def step(lines):
    f = lines[-1].split("\t")
    f[1] = repr(struct.unpack("<f", struct.pack("<I", struct.unpack("<I", struct.pack("<f", float(f[1])))[0] ^ 1))[0])
    return lines[:-1] + ["\t".join(f)]
code, out = run(a0, b0, edit_lines("u5-retrieval.tsv", step), ("b",))
ok &= code != 0
print(f"{'refused' if code else 'ACCEPTED'}: one side, a non-candidate retained score's lowest bit: {out}")
print("all controls behave" if ok else "CONTROLS FAILED")
sys.exit(0 if ok else 1)
