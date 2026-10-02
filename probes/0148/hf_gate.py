"""Record 0148: the early semantic stop gate, before any D2 score.
sentence-transformers' CrossEncoder scores the frozen non-D2 gate pairs;
every Candle logit, batched (padded) and alone, must agree within 1e-4,
and every HF output must be finite. A failure stops the record: it is
reported as a Candle-against-HF architecture failure, never worked around
and never met by loosening the tolerance.

  hf_gate.py MODEL_DIR CANDLE_JSON"""
import json, math, sys
from sentence_transformers import CrossEncoder

model, path = sys.argv[1], sys.argv[2]
c = json.load(open(path))
ps, hs = c["premises"], c["hypotheses"]
ce = CrossEncoder(model, device="cpu")
lengths = [len(ce.tokenizer(p, h)["input_ids"]) for p, h in zip(ps, hs)]
hf = ce.predict(list(zip(ps, hs)), batch_size=len(ps), show_progress_bar=False, activation_fn=None, convert_to_numpy=True)
hf = [float(x) for row in hf for x in row]
if not all(math.isfinite(x) for x in hf):
    sys.exit("FAIL: a non-finite HF output")
ok = True
for name in ("batched", "alone"):
    ours = c[name]
    if len(ours) != len(hf):
        sys.exit(f"FAIL: {name} has {len(ours)} logits, HF {len(hf)}")
    worst = max(abs(a - b) for a, b in zip(ours, hf))
    ok &= worst <= 1e-4
    print(f"{name}: {len(ps)} pairs x 3 logits, max |Candle - HF| = {worst:.3g} (bound 1e-4)")
print(f"pair lengths (tokens): {lengths}; Candle batches {c['batches']}")
for k, (p, h) in enumerate(zip(ps, hs)):
    print(f"  {k}: HF {[round(x, 4) for x in hf[3 * k:3 * k + 3]]}  ({h})")
print("early gate: PASS" if ok else "early gate: FAIL (stop: report the architecture failure)")
sys.exit(0 if ok else 1)
