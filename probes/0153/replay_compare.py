"""Record 0153, review R1/R2 replay: the replayed producer evidence against
the retained pre-replay evidence (out/pre-replay/, gzipped): passages and
NLI statuses exact; embeddings, cosines and NLI logits in f32 bits with exact
identities. For D3-dev, selected.json, curves.json and centroids.tsv must be
byte-identical. Any movement stops: it is reported, not absorbed.

  replay_compare.py PRE_DIR NEW_DIR [dev]"""
import gzip, json, os, struct, sys


def read(d, name):
    p = os.path.join(d, name)
    return gzip.open(p + ".gz", "rt").read() if os.path.exists(p + ".gz") else open(p).read()


def f32bits(t):
    return struct.pack("<f", float(t))


def rows(text):
    return [l.split("\t") for l in text.splitlines()]


pre, new = sys.argv[1:3]
problems = []
for side in ("s", "t"):
    a, b = os.path.join(pre, side), os.path.join(new, side)
    if json.loads(read(a, "t3-passages.json")) != json.loads(read(b, "t3-passages.json")):
        problems.append(f"{side}: passages moved")
    if read(a, "t3-status.tsv") != read(b, "t3-status.tsv"):
        problems.append(f"{side}: NLI statuses moved")
    for name, first in (("t3-embeddings.tsv", 1), ("t3-cosines.tsv", 1), ("t3-nli.tsv", 3)):
        x, y = rows(read(a, name)), rows(read(b, name))
        if len(x) != len(y) or x[0] != y[0]:
            problems.append(f"{side}/{name}: shape or header moved")
            continue
        for i, (r, q) in enumerate(zip(x[1:], y[1:])):
            if r[:first] != q[:first] or [f32bits(v) for v in r[first:]] != [f32bits(v) for v in q[first:]]:
                problems.append(f"{side}/{name} row {i + 1} moved")
                break
    est = rows(read(b, "t3-embedding-status.tsv"))
    print(f"{side}: embedding statuses {dict((s, sum(1 for r in est[1:] if r[1] == s)) for s in sorted({r[1] for r in est[1:]}))}")
if len(sys.argv) > 3 and sys.argv[3] == "dev":
    for name in ("selected.json", "curves.json", "centroids.tsv"):
        if open(os.path.join(pre, name), "rb").read() != open(os.path.join(new, name), "rb").read():
            problems.append(f"{name} moved")
if problems:
    for p in problems:
        print(f"MOVED: {p}")
    sys.exit(1)
print(f"{new}: unchanged against {pre}: passages and NLI statuses exact; embeddings, cosines and NLI logits "
      f"identical in f32 bits for both producers" + ("; selected.json, curves.json, centroids.tsv byte-identical" if len(sys.argv) > 3 else ""))
