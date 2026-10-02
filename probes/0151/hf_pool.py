"""Record 0151: the semantic check of U8's pool. The directory is validated
first (validate.validate), then sentence-transformers' CrossEncoder scores
every pool pair, rebuilt from the development query text and the validated
retained passage text; each score must be finite and within 1e-4 of U8's.
A completed comparison writes RESULT_JSON and exits 0 (PASS) or 3 (FAIL);
anything else is the checker failing.

  hf_pool.py MODEL_DIR U8_DIR DEV_TSV D1 RESULT_JSON"""
import json, math, sys, time
from validate import queries, validate

model, d, dev, d1, result_path = sys.argv[1:6]
qs = queries(dev)
paths, texts, scores, pool, ce = validate(d, qs, d1)
text_of = {q: t for q, t, _ in qs}
pairs = [(text_of[row[0]], texts[row[4]]) for row in pool]
from sentence_transformers import CrossEncoder

m = CrossEncoder(model, device="cpu")
t = time.perf_counter()
hf = m.predict(pairs, batch_size=32, show_progress_bar=False)
took = time.perf_counter() - t
hf = [float(x) for x in hf]
finite = all(math.isfinite(x) for x in hf)
worst = max(abs(a - b) for a, b in zip(ce, hf)) if finite else float("inf")
ok = finite and len(hf) == len(ce) and worst <= 1e-4
print(f"{len(pairs)} pool pairs rebuilt from the development queries and validated passages; "
      f"max |rnx - HF| = {worst:.3g} (bound 1e-4); HF predict {took:.1f} s")
print("semantic check: PASS" if ok else "semantic check: FAIL")
json.dump({"check": "hf-pool", "completed": True, "result": "PASS" if ok else "FAIL", "pairs": len(pairs),
           "max_diff": worst if finite else None}, open(result_path, "w"), indent=1)
sys.exit(0 if ok else 3)
