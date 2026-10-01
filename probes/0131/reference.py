"""Record 0131: the independent reference. sentence-transformers on PyTorch
CPU, loading the same local pinned files, embeds every text the probe uses.

  reference.py MODEL_DIR DATA_DIR OUT.json
"""
import json, resource, sys, time
import numpy as np
import sentence_transformers, torch, transformers

model_dir, data_dir, out = sys.argv[1:4]
texts = json.load(open(f"{data_dir}/texts.json"))
t0 = time.perf_counter()
model = sentence_transformers.SentenceTransformer(model_dir, device="cpu")
load_s = time.perf_counter() - t0
assert model.max_seq_length == 256, model.max_seq_length


def emb(xs, batch_size=32):
    return model.encode(xs, batch_size=batch_size, convert_to_numpy=True, normalize_embeddings=False).astype(np.float64)


res = {"versions": {"torch": torch.__version__, "sentence_transformers": sentence_transformers.__version__,
                    "transformers": transformers.__version__, "threads": torch.get_num_threads()},
       "load_s": load_s}
docs, queries = emb(texts["docs"]), emb(texts["queries"])
res["docs"], res["queries"] = docs.tolist(), queries.tolist()
res["scores"] = (docs @ queries.T).tolist()
res["controls"] = {k: emb(v).tolist() for k, v in texts["controls"].items()}
# throughput, for context
for name, xs in [("one", texts["queries"][:1]), ("batch32", texts["docs"][:32]), ("docs60", texts["docs"]), ("corpus10k", texts["corpus"])]:
    emb(xs)  # warm
    t = time.perf_counter(); emb(xs); res[f"embed_s_{name}"] = time.perf_counter() - t
res["peak_rss_kib"] = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
json.dump(res, open(out, "w"))
print(json.dumps({k: v for k, v in res.items() if k not in ("docs", "queries", "scores", "controls")}, indent=1))
