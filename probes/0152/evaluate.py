"""Record 0152: the locked comparison (plans/0152 section 2), from a
VALIDATED retrieval-only directory (validate_retrieval.validate). Two
systems only: B, the BM25 contract 0151 selected (bm25.py, depth 40), and
R, the unchanged retrieval (the pool's candidate order, 40 documents). The
measures, sign test and bootstrap are 0151's (evaluate.py, pinned), seed
151. The outcome category comes from the bootstrap 95% interval of the mean
delta RR@20 (B - R): entirely above 0, B better; entirely below, B worse;
otherwise inconclusive. Writes RESULT_JSON.

  evaluate.py U8_DIR QUERIES_TSV D1 DEV_SELECTED_JSON RESULT_JSON"""
import importlib.util, json, os, sys, time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(1, os.path.join(HERE, "..", "0151"))
import bm25
from validate_retrieval import queries, validate

# 0151's evaluate.py (pinned), loaded under its own name: this file shares it
_spec = importlib.util.spec_from_file_location("evaluate0151", os.path.join(HERE, "..", "0151", "evaluate.py"))
_e0151 = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_e0151)
bootstrap, measures, sign_test = _e0151.bootstrap, _e0151.measures, _e0151.sign_test

d, qfile, d1, dev_selected, result_path = sys.argv[1:6]
qs = queries(qfile)
kind = {l.split("\t")[0]: l.split("\t")[4] for l in open(qfile).read().splitlines()[1:] if l}
paths, texts, scores, pool = validate(d, qs, d1)
print(f"{d}: validated against its retained evidence and D1 ({len(pool)} pool rows, retrieval only)")
ranked = {"B": {}, "R": {}}
cands = {}
for q, cand, doc, _, _, _ in pool:
    cands.setdefault(q, {})[cand] = doc
t = time.perf_counter()
index = bm25.Index(texts)
for q, text, _ in qs:
    ranked["B"][q] = [doc for doc, _, _ in bm25.rank(paths, index, text, 40)]
bm25_s = time.perf_counter() - t
for q, _, _ in qs:
    ranked["R"][q] = [cands[q][c] for c in sorted(cands[q])]
res = {s: {q: measures(ranked[s][q], sup) for q, _, sup in qs} for s in ("B", "R")}


def table(subset, title):
    n = len(subset)
    print(f"\n{title} ({n} queries)")
    print("| system | mean RR@20 | hit@1 | hit@5 | mean recall@5 | mean candidate recall@40 |")
    print("|---|---:|---:|---:|---:|---:|")
    out = {}
    for s in ("B", "R"):
        rr = sum(res[s][q]["rr20"] for q, _, _ in subset) / n
        crec = sum(len({sp for sp in sup if sp in {x[:4] for x in ranked[s][q][:40]}}) / len(set(sup))
                   for q, _, sup in subset) / n
        h1 = sum(res[s][q]["hit1"] for q, _, _ in subset)
        h5 = sum(res[s][q]["hit5"] for q, _, _ in subset)
        r5 = sum(res[s][q]["recall5"] for q, _, _ in subset) / n
        print(f"| {s} | {rr:.3f} | {h1} | {h5} | {r5:.3f} | {crec:.3f} |")
        out[s] = {"mean_rr20": rr, "hit1": h1, "hit5": h5, "mean_recall5": r5, "mean_candidate_recall40": crec}
    return out


summary = table(qs, f"all queries in {os.path.basename(qfile)}")
subsets = {k: table([x for x in qs if kind[x[0]] == k], f"{k} (descriptive only)") for k in ("direct", "paraphrase")}
deltas = [res["B"][q]["rr20"] - res["R"][q]["rr20"] for q, _, _ in qs]
w, l = sum(x > 0 for x in deltas), sum(x < 0 for x in deltas)
lo, hi = bootstrap(deltas)
mean = sum(deltas) / len(deltas)
outcome = "B better" if lo > 0 else "B worse" if hi < 0 else "inconclusive"
p = sign_test(w, l)
print(f"\nprimary, paired B - R on RR@20: mean {mean:+.3f}, 95% bootstrap [{lo:+.3f}, {hi:+.3f}] (10,000 resamples, seed 151)")
print(f"sign test (descriptive): {w} wins / {l} losses / {len(deltas) - w - l} ties, p = {p:.3f}")
print(f"OUTCOME (plans/0152 section 2): {outcome}")
dev = json.load(open(dev_selected))
print(f"\ndevelopment (0151, selection-biased) against holdout, mean RR@20: "
      f"B {dev['dev_means']['B']:.3f} -> {summary['B']['mean_rr20']:.3f}; R {dev['dev_means']['R']:.3f} -> {summary['R']['mean_rr20']:.3f}")
print("\nper query, r by system (> 20 shown as -):")
print("| query | kind | supporting | B | R |")
print("|---|---|---|---:|---:|")
for q, _, sup in qs:
    cell = lambda s: str(res[s][q]["r"]) if res[s][q]["r"] is not None and res[s][q]["r"] <= 20 else "-"
    print(f"| {q} | {kind[q]} | {' '.join(sup)} | {cell('B')} | {cell('R')} |")
print(f"\ncost: BM25 index and {len(qs)} rankings over {len(texts)} passages in {bm25_s:.2f} s (no model)")
json.dump({"outcome": outcome, "mean_delta_rr20": mean, "bootstrap95": [lo, hi], "seed": 151,
           "sign_test": {"wins": w, "losses": l, "ties": len(deltas) - w - l, "p": p},
           "systems": summary, "subsets": subsets, "queries": len(qs),
           "per_query": {q: {s: res[s][q] for s in ("B", "R")} for q, _, _ in qs},
           "dev_means": {"B": dev["dev_means"]["B"], "R": dev["dev_means"]["R"]}},
          open(result_path, "w"), indent=1)
print(f"wrote {result_path}")
