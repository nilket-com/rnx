"""Record 0146: the semantic gate, independent of rnx's reading of the
architecture. sentence-transformers' CrossEncoder scores the exact pairs
U5 scored (u5-pairs.json); each score must agree with the trace's within
1e-4 absolute, and each query's re-ranked order must be the same, except
between candidates whose scores differ by less than 1e-4 (listed). The
time is reported for context (median of three runs).

Review round 1: the directory is validated first (compare_u5.validate), so
the pairs scored are bound to the candidates' identities and the retained
retrieval evidence over D1.

  hf_check.py MODEL_DIR U5_DIR RUBRIC D1"""
import json, statistics, sys, time
from compare_u5 import rubric_of, text, validate
from sentence_transformers import CrossEncoder

model, d = sys.argv[1], sys.argv[2]
validate(d, rubric_of(sys.argv[3]), sys.argv[4])
print(f"{d}: validated against its retained evidence and D1")
pairs = json.loads(text(d, "u5-pairs.json"))
qs, ps = pairs["queries"], pairs["passages"]
ce = CrossEncoder(model, device="cpu")
times = []
for _ in range(3):
    t = time.perf_counter()
    hf = ce.predict(list(zip(qs, ps)), batch_size=32, show_progress_bar=False)
    times.append(time.perf_counter() - t)
cands = [l.split("\t") for l in text(d, "u5-trace.tsv").splitlines() if l.startswith("cand\t")]
reranks = {l.split("\t")[1]: [int(x) for x in l.split("\t")[2].split(",")] for l in text(d, "u5-trace.tsv").splitlines() if l.startswith("rerank\t")}
if len(cands) != len(hf):
    sys.exit(f"FAIL: {len(cands)} trace pairs, {len(hf)} scored")
ours = [float(c[6]) for c in cands]
worst = max(abs(a - b) for a, b in zip(ours, hf))
ok = worst <= 1e-4
by = {}
for c, h in zip(cands, hf):
    by.setdefault(c[1], []).append((int(c[2]), float(h)))
near = []
for q, rows in by.items():
    hf_order = [r for r, _ in sorted(rows, key=lambda x: (-x[1], x[0]))]
    if hf_order != reranks[q]:
        s = {r: h for r, h in rows}
        diffs = [(a, b) for a, b in zip(hf_order, reranks[q]) if a != b]
        close = all(abs(s[a] - s[b]) < 1e-4 for a, b in diffs)
        near.append((q, diffs, close))
        ok &= close
print(f"{len(hf)} pairs; max |rnx - HF| = {worst:.3g} (bound 1e-4)")
print(f"re-ranked orders: {'identical' if not near else near}")
print(f"HF CrossEncoder.predict: {statistics.median(times) * 1e3:.0f} ms median of {[round(t * 1e3) for t in times]} ms")
print("semantic gate: PASS" if ok else "semantic gate: FAIL")
sys.exit(0 if ok else 1)
