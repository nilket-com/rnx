"""Record 0148, gate 3 as amended (plans/0148 section 6a): Candle's DeBERTa
semantics, isolated from tokenization. transformers' PyTorch DeBERTa
(AutoModelForSequenceClassification, the same pinned files) is fed the exact
ids, all-ones masks and pair order the pinned tokenizer.json produces for
the 2,290 inputs the validator reconstructs from verified D2, the ranges and
the labels, one pair at a time (no padding). Every output must have shape 3,
be finite and align with its identity; every logit must be within 1e-4 of
U6's; the decisions, recomputed by section 4's rule, must equal U6's, except
a ticket where a perturbation of HF's scores within 1e-4 would give U6's
decision (validate.explained; review round 1, R2), each listed.

  hf_model.py MODEL_DIR U6_DIR D2_JSON RANGES LABELS"""
import math, struct, sys
from validate import decide, explained, identities, num, softmax_entail

model_dir, d, d2, ranges, labels = sys.argv[1:6]
ids, numbers, names = identities(d2, ranges, labels)
import tokenizers, torch, transformers
from transformers import AutoModelForSequenceClassification

tok = tokenizers.Tokenizer.from_file(f"{model_dir}/tokenizer.json")
tok.no_truncation()
tok.no_padding()
model = AutoModelForSequenceClassification.from_pretrained(model_dir).eval()
rows = open(f"{d}/u6-pairs.tsv").read().splitlines()[1:]
if len(rows) != len(ids):
    sys.exit(f"FAIL: {len(rows)} U6 pairs, {len(ids)} identities")
ours, hf = [], []
with torch.no_grad():
    for r, (i, row) in enumerate(zip(ids, rows)):
        f = row.split("\t")
        if (f[0], f[1], f[2]) != (str(i[0]), str(i[1]), i[2]):
            sys.exit(f"FAIL: U6 pair {r + 1} is {f[:3]}, identity {i[:3]}")
        enc = tok.encode(i[3], i[4])
        out = model(input_ids=torch.tensor([enc.ids]), attention_mask=torch.tensor([enc.attention_mask])).logits
        if tuple(out.shape) != (1, 3):
            sys.exit(f"FAIL: pair {r + 1} output shape {tuple(out.shape)}")
        v = [float(x) for x in out[0]]
        if not all(math.isfinite(x) for x in v):
            sys.exit(f"FAIL: pair {r + 1} a non-finite HF logit")
        hf.append(v)
        ours.append([struct.unpack("<f", struct.pack("<f", num(x, "logit")))[0] for x in f[3:]])
worst = max(abs(a - b) for x, y in zip(ours, hf) for a, b in zip(x, y))
ok = worst <= 1e-4
table = {r.split("\t")[0]: r.split("\t") for r in open(f"{d}/u6-tickets.tsv").read().splitlines()[1:]}
listed = []
for n in numbers:
    s = [max(softmax_entail(hf[r]) for r, i in enumerate(ids) if i[0] == n and i[2] == names[j]) for j in range(len(names))]
    triage, best, second, bi, ni = decide(s, names)
    near = abs(s[bi] - 0.5) <= 1e-4 or abs(s[bi] - s[ni]) <= 1e-4
    if [triage, best, second] != table[str(n)][1:4]:
        fine = explained(s, table[str(n)][1:4], names, 1e-4)
        listed.append(f"ticket {n}: HF model {[triage, best, second]}, U6 {table[str(n)][1:4]}"
                      f" ({'explained' if fine else 'NOT explained'} within 1e-4)")
        ok &= fine
    elif near:
        listed.append(f"ticket {n}: near a boundary (best {s[bi]:.6f}, second {s[ni]:.6f}), decisions agree")
print(f"transformers {transformers.__version__}, torch {torch.__version__}, tokenizers {tokenizers.__version__} "
      f"(the pinned tokenizer.json, loaded directly)")
print(f"{len(ids)} reconstructed inputs, the pinned tokenizer's ids, one pair at a time: shapes (1, 3), finite, aligned; "
      f"max |rnx - HF model| logit = {worst:.3g} (bound 1e-4)")
for l in listed:
    print(f"  {l}")
print(f"decisions: {'identical' if not listed else str(len(listed)) + ' listed above'}")
print("model-level check: PASS" if ok else "model-level check: FAIL")
sys.exit(0 if ok else 1)
