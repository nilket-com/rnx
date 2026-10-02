"""Record 0151: the eight development systems, from a VALIDATED U8 directory
(validate.validate), and the frozen selection rule (plans/0151 section 3).

Systems: R (retrieval, the candidate order), B (BM25, bm25.py over the same
retained passages), and P1..P6 over the one-pair-per-call pool:
  P1 c=20 k=1, P2 c=20 k=2, P3 c=20 k=3, P4 c=40 k=1, P5 c=40 k=3: a
  candidate's score is the max CE over its first k passages; re-ranked by
  that score, descending, ties to the better retrieval rank;
  P6: 1/(60 + r_R) + 1/(60 + r_P5) over the same 40 candidates, descending,
  ties to the better r_R.
Per query and system (0147's record dedup: a record's rank is its earliest
document's): r, hit@1, hit@5, recall@5, RR@20 (0 beyond 20), and candidate
recall at the system's candidate count. Selection: the highest development
mean RR@20 among R, B, P1..P6; exact ties to the simpler, in the order B, R,
P1, P2, P4, P3, P5, P6. Paired against R and against B: wins/losses/ties by
RR@20, an exact two-sided sign test, a bootstrap 95% interval of the mean
delta RR (10,000 resamples, seed 151). Costs: each system's logical pairs.
Writes selected.json (the frozen configuration for 0152).

  evaluate.py U8_DIR DEV_TSV D1 OUT_SELECTED_JSON [TIMING_LINE]"""
import json, math, os, random, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import bm25
from validate import queries, validate

ORDER = ["B", "R", "P1", "P2", "P4", "P3", "P5", "P6"]
PIPES = {"P1": (20, 1), "P2": (20, 2), "P3": (20, 3), "P4": (40, 1), "P5": (40, 3)}


def measures(docs, support):
    """docs: ranked document paths. 0147's record dedup."""
    records = [d[:4] for d in docs]
    def first(rec):
        return records.index(rec) + 1 if rec in records else None
    ranks = [first(s) for s in support]
    hits = [r for r in ranks if r is not None]
    r = min(hits) if hits else None
    within5 = len({s for s in support if first(s) is not None and first(s) <= 5})
    return {"r": r, "hit1": r == 1, "hit5": r is not None and r <= 5,
            "recall5": within5 / len(set(support)),
            "rr20": (1.0 / r) if r is not None and r <= 20 else 0.0}


def systems(paths, texts, pool, ce, qs):
    """{system: {query: ranked document paths}} and each system's logical
    pairs per query."""
    out = {s: {} for s in ORDER}
    pairs = {s: 0 for s in ORDER}
    index = bm25.Index(texts)
    by_q = {}
    for row, score in zip(pool, ce):
        q, cand, doc, prank, pid, _ = row
        by_q.setdefault(q, {}).setdefault(cand, [doc, []])[1].append((prank, score))
    for q, text, _ in qs:
        cands = by_q[q]
        r_order = [cands[c][0] for c in sorted(cands)]
        out["R"][q] = r_order
        out["B"][q] = [d for d, _, _ in bm25.rank(paths, index, text, 40)]
        rank_p5 = None
        for name, (c, k) in PIPES.items():
            scored = []
            for cand in sorted(cands)[:c]:
                doc, ps = cands[cand]
                use = [s for pr, s in ps if pr <= k]
                pairs[name] += len(use)
                scored.append((max(use), cand, doc))
            order = sorted(scored, key=lambda x: (-x[0], x[1]))
            out[name][q] = [doc for _, _, doc in order]
            if name == "P5":
                rank_p5 = {cand: i + 1 for i, (_, cand, _) in enumerate(order)}
        fused = sorted(((1 / (60 + cand) + 1 / (60 + rank_p5[cand]), cand, cands[cand][0]) for cand in sorted(cands)),
                       key=lambda x: (-x[0], x[1]))
        out["P6"][q] = [doc for _, _, doc in fused]
        pairs["P6"] += sum(len([s for pr, s in cands[cand][1] if pr <= 3]) for cand in sorted(cands)[:40])
    return out, pairs


def sign_test(w, l):
    n, k = w + l, min(w, l)
    return 1.0 if n == 0 else min(1.0, 2 * sum(math.comb(n, i) for i in range(k + 1)) / 2 ** n)


def bootstrap(deltas, seed=151, reps=10000):
    rng = random.Random(seed)
    n = len(deltas)
    means = sorted(sum(deltas[rng.randrange(n)] for _ in range(n)) / n for _ in range(reps))
    return means[int(0.025 * reps)], means[int(0.975 * reps) - 1]


def main(d, dev, d1, selected_path, timing=None):
    qs = queries(dev)
    paths, texts, scores, pool, ce = validate(d, qs, d1)
    print(f"{d}: validated against its retained evidence and D1 ({len(pool)} pool pairs)")
    ranked, pairs = systems(paths, texts, pool, ce, qs)
    res = {s: {q: measures(ranked[s][q], sup) for q, _, sup in qs} for s in ORDER}
    cands = {"R": 40, "B": 40, "P1": 20, "P2": 20, "P3": 20, "P4": 40, "P5": 40, "P6": 40}
    print("\n| system | candidates | mean RR@20 | hit@1 | hit@5 | mean recall@5 | mean candidate recall | logical CE pairs |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|")
    for s in ORDER:
        n = len(qs)
        mean = lambda f: sum(f(res[s][q]) for q, _, _ in qs) / n
        crec = sum(len({sp for sp in sup if sp in {x[:4] for x in ranked[s][q][:cands[s]]}}) / len(set(sup)) for q, _, sup in qs) / n
        print(f"| {s} | {cands[s]} | {mean(lambda m: m['rr20']):.3f} | {sum(res[s][q]['hit1'] for q, _, _ in qs)} | "
              f"{sum(res[s][q]['hit5'] for q, _, _ in qs)} | {mean(lambda m: m['recall5']):.3f} | {crec:.3f} | {pairs[s]} |")
    means = {s: sum(res[s][q]["rr20"] for q, _, _ in qs) / len(qs) for s in ORDER}
    best = max(means.values())
    winner = next(s for s in ORDER if means[s] == best)
    print(f"\nselection (highest mean RR@20, exact ties to the simpler in {ORDER}): {winner} at {best:.4f}")
    for base in ("R", "B"):
        print(f"\npaired against {base} (RR@20): system, wins / losses / ties, sign test p, mean delta, 95% bootstrap")
        for s in ORDER:
            if s == base:
                continue
            d_ = [res[s][q]["rr20"] - res[base][q]["rr20"] for q, _, _ in qs]
            w, l = sum(x > 0 for x in d_), sum(x < 0 for x in d_)
            lo, hi = bootstrap(d_)
            print(f"  {s}: {w} / {l} / {len(d_) - w - l}, p = {sign_test(w, l):.3f}, {sum(d_) / len(d_):+.3f} [{lo:+.3f}, {hi:+.3f}]")
    print("\nper query, r by system (> 20 shown as -):")
    print("| query | supporting | " + " | ".join(ORDER) + " |")
    print("|---|---|" + "---:|" * len(ORDER))
    for q, _, sup in qs:
        cells = [str(res[s][q]["r"]) if res[s][q]["r"] is not None and res[s][q]["r"] <= 20 else "-" for s in ORDER]
        print(f"| {q} | {' '.join(sup)} | " + " | ".join(cells) + " |")
    if timing:
        print(f"\ncost of the shared run: {timing}")
    configurations = {
        "B": {"system": "BM25", "contract": "probes/0151/bm25.py, frozen", "depth": 40},
        "R": {"system": "embedding retrieval", "candidates": 40},
        **{k: {"candidates": c, "passages": kk, "score": "max CE over the first k passages, one pair per call",
               "order": "descending, ties to the better retrieval rank"} for k, (c, kk) in PIPES.items()},
        "P6": {"candidates": 40, "passages": 3, "score": "1/(60 + r_R) + 1/(60 + r_P5)", "order": "descending, ties to the better r_R"},
    }
    cfg = {"selected": winner, "dev_mean_rr20": best, "dev_means": means, "tie_order": ORDER,
           "configuration": configurations[winner]}
    json.dump(cfg, open(selected_path, "w"), indent=1)
    print(f"\nwrote {selected_path}")


if __name__ == "__main__":
    main(*sys.argv[1:6])
