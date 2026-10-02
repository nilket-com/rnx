"""Record 0152: the retrieval-only gate. 0151's validate.py checks, in its
retrieval mode: each producer validated on its own (passages against D1,
retained retrieval scores, the pool recomputed numerically from them), then
the two compared exactly. The pool has six columns and no cross-encoder
score. The helpers are imported from probes/0151/validate.py, pinned by
gate 0.

  validate_retrieval.py SCRIPT_DIR TWIN_DIR QUERIES_TSV D1"""
import os, sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "0151"))
from validate import CANDIDATES, PASSAGES, expected_pool, f32, fail, num, queries, retained, text

HEADER = "query\tcand\tpath\tprank\tpid\tretrieval"


def validate(d, qs, d1):
    ids = [q for q, _, _ in qs]
    paths, texts, scores = retained(d, ids, d1)
    want = expected_pool(paths, scores, ids)
    lines = text(d, "u8-pool.tsv").splitlines()
    if not lines or lines[0] != HEADER:
        fail(f"{d}/u8-pool.tsv: header is not the retrieval-only header")
    rows = lines[1:]
    if len(rows) != len(want):
        fail(f"{d}/u8-pool.tsv: {len(rows)} rows, the recomputed pool has {len(want)}")
    for r, (line, w) in enumerate(zip(rows, want)):
        f = line.split("\t")
        if len(f) != 6:
            fail(f"{d}/u8-pool.tsv row {r + 1}: {len(f)} fields")
        try:
            have = (f[0], int(f[1]), f[2], int(f[3]), int(f[4]), f32(num(f[5], "retrieval")))
        except ValueError:
            fail(f"{d}/u8-pool.tsv row {r + 1}: not integers")
        if have != w:
            fail(f"{d}/u8-pool.tsv row {r + 1}: {have[:5]} is not the recomputed {w[:5]} (or its retrieval score differs)")
    return paths, texts, scores, want


if __name__ == "__main__":
    a_dir, b_dir, qfile, d1 = sys.argv[1:5]
    qs = queries(qfile)
    a, b = validate(a_dir, qs, d1), validate(b_dir, qs, d1)
    if a[0] != b[0] or a[1] != b[1]:
        fail("the producers' passages differ")
    if [[f32(x) for x in r] for r in a[2]] != [[f32(x) for x in r] for r in b[2]]:
        fail("the producers' retrieval scores differ")
    if a[3] != b[3]:
        fail("the producers' pools differ")
    print(f"BIT-EQUAL (each producer validated first, its pool recomputed from its retained evidence over D1): "
          f"{len(a[0])} passages and their {len(qs)} query scores, {len(a[3])} pool rows "
          f"({CANDIDATES} candidates x up to {PASSAGES} passages), retrieval only")
