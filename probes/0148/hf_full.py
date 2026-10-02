"""Record 0148: the semantic check over U6, independent of Candle's DeBERTa.
sentence-transformers' CrossEncoder scores the 2,290 pairs the validator
reconstructs from verified D2, the ranges and the labels (not the
producer's texts). Every HF logit must be finite and within 1e-4 of the
producer's; the decisions, recomputed from HF's logits by plans/0148
section 4's rule, must equal the producer's, except a ticket whose HF best
score is within 1e-4 of 0.5 or of its second label's (each listed). The
time is reported for context (median of three).

Review round 1 (R1, R2): a completed comparison writes RESULT_JSON and
exits 0 (PASS) or 3 (a completed mismatch: FAIL); anything else, an
exception included, is the checker failing, with no artifact. A decision
may differ from HF's only as a perturbation of HF's scores within 1e-4
would make it (validate.explained), never as an unrelated label.

  hf_full.py MODEL_DIR U6_DIR D2_JSON RANGES LABELS RESULT_JSON"""
import json, math, statistics, struct, sys, time
from validate import decide, explained, identities, num, softmax_entail

model, d, d2, ranges, labels, result_path = sys.argv[1:7]


def finish(ok, **details):
    json.dump({"check": "hf-frozen", "completed": True, "result": "PASS" if ok else "FAIL", **details},
              open(result_path, "w"), indent=1)
    sys.exit(0 if ok else 3)


ids, numbers, names = identities(d2, ranges, labels)
from sentence_transformers import CrossEncoder

ce = CrossEncoder(model, device="cpu")
pairs = [(i[3], i[4]) for i in ids]
times = []
for _ in range(3):
    t = time.perf_counter()
    hf = ce.predict(pairs, batch_size=32, show_progress_bar=False, activation_fn=None, convert_to_numpy=True)
    times.append(time.perf_counter() - t)
hf = [[float(x) for x in row] for row in hf]
if any(not math.isfinite(x) for row in hf for x in row):
    print("semantic check: FAIL (a non-finite HF output)")
    finish(False, pairs=len(pairs), reason="a non-finite HF output")
rows = open(f"{d}/u6-pairs.tsv").read().splitlines()[1:]
if len(rows) != len(ids):
    sys.exit(f"FAIL: {len(rows)} producer pairs, {len(ids)} identities")
ours = [[struct.unpack("<f", struct.pack("<f", num(x, "logit")))[0] for x in r.split("\t")[3:]] for r in rows]
worst = max(abs(a - b) for x, y in zip(ours, hf) for a, b in zip(x, y))
ok = worst <= 1e-4
table = {r.split("\t")[0]: r.split("\t") for r in open(f"{d}/u6-tickets.tsv").read().splitlines()[1:]}
k = len(names)
listed = []
for n in numbers:
    s = [max(softmax_entail(hf[r]) for r, i in enumerate(ids) if i[0] == n and i[2] == names[j]) for j in range(k)]
    triage, best, second, bi, ni = decide(s, names)
    mine = table[str(n)][1:4]
    near = abs(s[bi] - 0.5) <= 1e-4 or abs(s[bi] - s[ni]) <= 1e-4
    if [triage, best, second] != mine:
        fine = explained(s, mine, names, 1e-4)
        listed.append(f"ticket {n}: HF {[triage, best, second]}, producer {mine}, HF best {s[bi]:.6f} second {s[ni]:.6f}"
                      f" ({'explained' if fine else 'NOT explained'} within 1e-4)")
        ok &= fine
    elif near:
        listed.append(f"ticket {n}: near a boundary (HF best {s[bi]:.6f}, second {s[ni]:.6f}), decisions agree")
print(f"{len(pairs)} pairs reconstructed from verified D2, the ranges and the labels; max |rnx - HF| logit = {worst:.3g} (bound 1e-4)")
for l in listed:
    print(f"  {l}")
print(f"decisions: {'identical' if not listed else str(len(listed)) + ' listed above'}")
print(f"HF CrossEncoder.predict: {statistics.median(times):.1f} s median of {[round(t, 1) for t in times]} s")
print("semantic check: PASS" if ok else "semantic check: FAIL")
finish(ok, pairs=len(pairs), max_logit_diff=worst, listed=listed, predict_s=times)
