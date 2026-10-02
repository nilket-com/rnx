"""Record 0139: U4's embedding and scoring with sentence-transformers, for
timing context (reported, not gated). The same texts and chunks (0136's
committed D2 byte ranges at overlap 0), the same pooling (mean of passage
embeddings, renormalized), the same model and label descriptions.

  torch_u4.py MODEL D2_JSON RANGES_TSV LABELS_TSV"""
import json, sys, time
import numpy as np, sentence_transformers, torch

model, d2, ranges, labels = sys.argv[1:5]
issues = {f"d2:#{i['number']}": (i["title"] + "\n" + i["body"]).encode() for i in json.load(open(d2))}
order = [f"d2:#{i['number']}" for i in json.load(open(d2))]
passages, owner = [], []
for line in open(ranges).read().splitlines()[1:]:
    src, k, s, e = line.split("\t")
    if src.startswith("d2:"):
        passages.append(issues[src][int(s):int(e)].decode())
        owner.append(order.index(src))
names, descriptions = zip(*[l.split("\t") for l in open(labels).read().splitlines()[1:] if l])
m = sentence_transformers.SentenceTransformer(model, device="cpu")
m.encode(passages[:32], batch_size=32)  # warm-up
t = time.perf_counter()
e = m.encode(passages, batch_size=32, normalize_embeddings=True)
owner = np.array(owner)
pooled = np.stack([e[owner == i].mean(0) for i in range(len(order))])
pooled /= np.linalg.norm(pooled, axis=1, keepdims=True)
lab = m.encode(list(descriptions), batch_size=32, normalize_embeddings=True)
scores = pooled @ lab.T
elapsed = time.perf_counter() - t
best = scores.argmax(1)
print(f"{len(order)} tickets, {len(passages)} passages: sentence-transformers {sentence_transformers.__version__}, "
      f"torch {torch.__version__} CPU ({torch.get_num_threads()} threads), batch 32: embedding and scoring {elapsed:.2f} s")
print("predicted counts:", {n: int((best == j).sum()) for j, n in enumerate(names)})
