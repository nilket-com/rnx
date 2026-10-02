"""Record 0152: 0151's order step (its order_check.py) for retrieval-only
directories. `rnx run u8_rerank_dev.rn RETAINED_TSV ORDERS` applies the
script's own `order` to every query column; this checks each full order
against the numeric stable sort, then that the retained pool is exactly the
pool that order builds (validate_retrieval.validate).

  order_check.py U8_DIR ORDERS QUERIES_TSV D1"""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from validate_retrieval import expected_pool, fail, queries, retained, validate

d, orders, qfile, d1 = sys.argv[1:5]
qs = queries(qfile)
ids = [q for q, _, _ in qs]
paths, _, scores = retained(d, ids, d1)
got = {}
for line in open(orders).read().splitlines():
    q, ps = line.split("\t")
    got[q] = [int(x) for x in ps.split(",")]
if list(got) != ids:
    fail(f"orders for {list(got)}, want {ids}")
for k, q in enumerate(ids):
    if got[q] != sorted(range(len(paths)), key=lambda p: -scores[p][k]):
        fail(f"{q}: the script's order is not the numeric stable sort")
if validate(d, qs, d1)[3] != expected_pool(paths, scores, ids):
    fail("the retained pool is not the pool of the corrected order")
print(f"{d}: the script's order equals the numeric stable sort for all {len(ids)} queries over {len(paths)} passages, "
      f"and the retained pool is exactly the pool it builds")
