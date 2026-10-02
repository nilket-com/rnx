"""Record 0153: the semantic checks (plans/0153 section 7). The directory is
validated first (validate.validate: passages against the issues, the split
order, the statuses, the evidence's shapes); then:

- MiniLM: sentence-transformers (PyTorch CPU, the same pinned files) embeds
  the validated passages; each ticket's mean is renormalized; every unit
  embedding component and every description cosine must be finite and
  within 1e-4 of the retained value;
- NLI, model-level (0148's amended gate): transformers' DeBERTa is fed the
  pinned tokenizer.json's ids for every retained pair, one pair at a time
  (no padding); every logit must be finite and within 1e-4. Agreement of
  the native transformers tokenizer with the pinned ids is counted and
  reported separately, not gated (0148).

A completed comparison writes RESULT_JSON, recording the verified counts,
and exits 0 (PASS) or 3 (FAIL); anything else is the checker failing.

  hf_t3.py MINILM_DIR NLI_DIR DIR ISSUES_JSON SPLIT_TSV LABELS_TSV RESULT_JSON"""
import json, math, os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from validate import fail, labels, nli_tokenizer, split_numbers, validate

BOUND = 1e-4
minilm, nli_dir, d, issues_path, split_path, labels_path, result_path = sys.argv[1:8]
issues = {i["number"]: i for i in json.load(open(issues_path))}
numbers = split_numbers(split_path)
names = labels(labels_path)
descriptions = [l.split("\t")[1] for l in open(labels_path).read().splitlines()[1:] if l]
passages, status, estatus, E, C, L = validate(d, issues, numbers, names, descriptions,
                                             nli_tokenizer(f"{nli_dir}/tokenizer.json"))

import numpy as np, tokenizers, torch, transformers
from sentence_transformers import SentenceTransformer
from transformers import AutoModelForSequenceClassification, AutoTokenizer

t0 = time.perf_counter()
st = SentenceTransformer(minilm, device="cpu")
flat = [p for ps in passages for p in ps]
pe = st.encode(flat, batch_size=32, convert_to_numpy=True, normalize_embeddings=False).astype(np.float64)
de = st.encode(descriptions, batch_size=32, convert_to_numpy=True, normalize_embeddings=False).astype(np.float64)
at, emb_diff, cos_diff, finite, invalid_ok = 0, 0.0, 0.0, True, 0
for n, ps in zip(numbers, passages):
    if not ps:
        continue
    m = pe[at:at + len(ps)].mean(axis=0)
    at += len(ps)
    if estatus[n] != "ok":
        # review R1: a claimed invalid embedding must be degenerate in HF too
        norm = float(np.linalg.norm(m))
        if math.isfinite(norm) and norm > BOUND:
            finite = False
            print(f"ticket {n}: the producer's embedding is {estatus[n]}, HF's pooled norm is {norm:.6g}")
        else:
            invalid_ok += 1
        continue
    u = m / np.linalg.norm(m)
    c = u @ de.T
    finite &= bool(np.isfinite(u).all() and np.isfinite(c).all())
    emb_diff = max(emb_diff, float(np.abs(u - np.array(E[n])).max()))
    cos_diff = max(cos_diff, float(np.abs(c - np.array(C[n])).max()))
t1 = time.perf_counter()

tok = tokenizers.Tokenizer.from_file(f"{nli_dir}/tokenizer.json")
tok.no_truncation()
tok.no_padding()
native = AutoTokenizer.from_pretrained(nli_dir)
model = AutoModelForSequenceClassification.from_pretrained(nli_dir).eval()
text_of = {n: ps for n, ps in zip(numbers, passages)}
logit_diff, native_diff = 0.0, 0
with torch.no_grad():
    for n, p, label, ours in L:
        premise, hypothesis = text_of[n][p], descriptions[names.index(label)]
        enc = tok.encode(premise, hypothesis)
        if native(premise, hypothesis)["input_ids"] != enc.ids:
            native_diff += 1
        out = model(input_ids=torch.tensor([enc.ids]), attention_mask=torch.tensor([enc.attention_mask])).logits
        if tuple(out.shape) != (1, 3):
            fail(f"ticket {n} passage {p} {label}: HF output shape {tuple(out.shape)}")
        v = [float(x) for x in out[0]]
        finite &= all(math.isfinite(x) for x in v)
        logit_diff = max(logit_diff, max(abs(a - b) for a, b in zip(v, ours)))
t2 = time.perf_counter()

ok = finite and emb_diff <= BOUND and cos_diff <= BOUND and logit_diff <= BOUND
counts = {"tickets": len(numbers), "embeddings": len(E), "cosines": len(C) * len(names), "nli_pairs": len(L)}
print(f"versions: transformers {transformers.__version__}, torch {torch.__version__}, tokenizers {tokenizers.__version__}")
print(f"MiniLM: {invalid_ok} claimed-invalid embeddings confirmed degenerate in HF; {len(E)} ticket embeddings x 384 and {len(C)} x {len(names)} cosines from {len(flat)} validated passages; "
      f"max |rnx - HF| embedding {emb_diff:.3g}, cosine {cos_diff:.3g} (bound {BOUND:g}); {t1 - t0:.1f} s")
print(f"NLI model-level: {len(L)} pairs, pinned ids, one at a time; max |rnx - HF| logit {logit_diff:.3g} "
      f"(bound {BOUND:g}); {t2 - t1:.1f} s; native tokenizer differs from the pinned ids on {native_diff} pairs (reported, not gated)")
print("semantic check: PASS" if ok else "semantic check: FAIL")
json.dump({"check": "hf-t3", "completed": True, "result": "PASS" if ok else "FAIL", **counts,
           "max_diff_embedding": emb_diff, "max_diff_cosine": cos_diff, "max_diff_logit": logit_diff,
           "native_tokenizer_differences": native_diff}, open(result_path, "w"), indent=1)
sys.exit(0 if ok else 3)
