"""Record 0152: the semantic check of retrieval. The directory is validated
first (validate_retrieval.validate: passages against D1, the retained
matrix's header equal to the queries file's exact ordered ids, one row per
pid in order, every score finite), then its shape is bound explicitly:
exactly PASSAGES rows and the queries file's count of columns. Only then
does sentence-transformers (PyTorch CPU, the same pinned MiniLM files)
embed the validated passage texts and the queries file's texts, in that
order; each cosine must be finite and within 1e-4 of the retained score.
A completed comparison writes RESULT_JSON, recording the verified
dimensions and value count, and exits 0 (PASS) or 3 (FAIL); anything else
is the checker failing.

  hf_retrieval.py MODEL_DIR U8_DIR QUERIES_TSV D1 PASSAGES RESULT_JSON"""
import json, math, os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from validate_retrieval import fail, queries, validate

BOUND = 1e-4
model, d, qfile, d1, n_passages, result_path = sys.argv[1:7]
n_passages = int(n_passages)
qs = queries(qfile)
paths, texts, scores, _ = validate(d, qs, d1)
if len(texts) != n_passages or len(scores) != n_passages:
    fail(f"shape: {len(texts)} passages, {len(scores)} matrix rows; want exactly {n_passages}")
if any(len(row) != len(qs) for row in scores):
    fail(f"shape: a matrix row does not have {len(qs)} columns")
if not all(math.isfinite(x) for row in scores for x in row):
    fail("a retained score is not finite")
import numpy as np
from sentence_transformers import SentenceTransformer

m = SentenceTransformer(model, device="cpu")
t = time.perf_counter()
pe = m.encode(texts, batch_size=32, convert_to_numpy=True, normalize_embeddings=True).astype(np.float64)
qe = m.encode([q for _, q, _ in qs], batch_size=32, convert_to_numpy=True, normalize_embeddings=True).astype(np.float64)
took = time.perf_counter() - t
hf = pe @ qe.T
if hf.shape != (n_passages, len(qs)):
    fail(f"HF shape {hf.shape}, want ({n_passages}, {len(qs)})")
rnx = np.array(scores, dtype=np.float64)
finite = bool(np.isfinite(hf).all())
diff = np.abs(rnx - hf)
worst = float(diff.max()) if finite else float("inf")
over = int((diff > BOUND).sum()) if finite else None
ok = finite and worst <= BOUND
values = n_passages * len(qs)
print(f"{n_passages} passages x {len(qs)} queries = {values} retrieval scores, bound to the validated passages and the "
      f"queries file's ordered ids and texts; max |rnx - HF| = {worst:.3g} (bound {BOUND:g}), {over} over; HF encode {took:.1f} s")
print("semantic check: PASS" if ok else "semantic check: FAIL")
json.dump({"check": "hf-retrieval", "completed": True, "result": "PASS" if ok else "FAIL", "passages": n_passages,
           "queries": len(qs), "values": values, "query_ids": [q for q, _, _ in qs], "over_bound": over,
           "max_diff": worst if finite else None}, open(result_path, "w"), indent=1)
sys.exit(0 if ok else 3)
