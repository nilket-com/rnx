"""Record 0154, rehearsal path (b)'s independent reference (review R1): the
frozen full-dev centroids, decoded from their f32 hex with numpy, scored
against the retained D3-dev ticket embeddings with numpy f64 dot products;
auto-routed at the frozen threshold; errors against Claude's dev labels.
No 0153 or 0154 scoring code is used.

  reference_b.py DEV_DIR"""
import json, sys
import numpy as np

d = sys.argv[1]
root = __file__.rsplit("/probes/", 1)[0] + "/probes/0153"
cent = {}
for line in open(f"{root}/out/dev/centroids.tsv").read().splitlines()[1:]:
    f = line.split("\t")
    cent[f[0]] = np.frombuffer(bytes.fromhex("".join(f[1:])), dtype="<f4").astype(np.float64)
order = ["bug", "feature", "question", "documentation"]
t = json.load(open(f"{root}/out/dev/selected.json"))["thresholds"]["C"]
ref = {}
for l in open(f"{root}/frozen/annotations-claude-dev.tsv").read().splitlines()[1:]:
    n, lab = l.split("\t")
    ref[int(n)] = "bug" if lab == "performance" else lab
auto = wrong = 0
for l in open(f"{d}/t3-embeddings.tsv").read().splitlines()[1:]:
    f = l.split("\t")
    e = np.array([np.float32(float(x)) for x in f[1:]], dtype=np.float32).astype(np.float64)
    s = {q: float(e @ cent[q]) for q in order if q in cent}
    ranked = sorted(s, key=lambda q: (-s[q], order.index(q)))
    conf = s[ranked[0]] - s[ranked[1]]
    if conf >= t:
        auto += 1
        wrong += ranked[0] != ref[int(f[0])]
print(f"independent reference (b): {auto} auto-routed of {len(ref)}, {wrong} errors; A {auto / len(ref):.3f}, E {wrong / auto:.3f}")
