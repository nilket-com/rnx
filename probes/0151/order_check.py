"""Record 0151, R1: the corrected U8 ordering over the retained matrix.

`rnx run u8_rerank_dev.rn RETAINED_TSV ORDERS` applies the script's own
`order` (numeric stable descending sort, then cast) to every query column
of a retained u8-retrieval.tsv. This checks those orders against
validate.py's numeric stable sort for every query, then rebuilds the pool
from them and compares it with the producer's retained pool. It also
reports where the old text-first order would have differed.

  order_check.py U8_DIR ORDERS DEV_TSV D1"""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from validate import expected_pool, queries, retained, text, validate

d, orders, dev, d1 = sys.argv[1:5]
qs = queries(dev)
ids = [q for q, _, _ in qs]
paths, _, scores = retained(d, ids, d1)
got = {}
for line in open(orders).read().splitlines():
    q, ps = line.split("\t")
    got[q] = [int(x) for x in ps.split(",")]
if list(got) != ids:
    sys.exit(f"FAIL: orders for {list(got)}, want {ids}")
rows = text(d, "u8-retrieval.tsv").splitlines()[1:]
text_differs = 0
for k, q in enumerate(ids):
    numeric = sorted(range(len(paths)), key=lambda p: -scores[p][k])
    if got[q] != numeric:
        sys.exit(f"FAIL: {q}: the script's order is not the numeric stable sort")
    as_text = [r.split("\t")[1 + k] for r in rows]
    text_differs += sorted(range(len(paths)), key=lambda p: as_text[p], reverse=True) != numeric
want = expected_pool(paths, scores, ids)
_, _, _, pool, _ = validate(d, qs, d1)
if pool != want:
    sys.exit("FAIL: the retained pool is not the pool of the corrected order")
print(f"{d}: the corrected order equals the numeric stable sort for all {len(ids)} queries over {len(paths)} passages, "
      f"and the retained pool ({len(pool)} pairs) is exactly the pool it builds; the old text-first order differed in "
      f"{text_differs} of {len(ids)} full query orders (below the pools, which the text-first run produced as above)")
