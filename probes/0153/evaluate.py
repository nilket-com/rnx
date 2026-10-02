"""Record 0153: the systems, the routing policy and the selection, exactly as
frozen in plans/0153 sections 4, 5, 5a and 5b, from VALIDATED producer
evidence (validate.validate).

A system gives each ticket either (queue, confidence) or None, mandatory
REVIEW (5a). Auto-routing needs a queue AND a finite confidence >= t; every
ticket counts in every denominator. Queues: bug (performance folded in),
feature, question, documentation; ties go to that order.

  evaluate.py DEV_DIR D3_ISSUES D2_DIR D2_ISSUES OUT_DIR NLI_TOKENIZER_JSON
(DEV_DIR: validated t3 evidence over D3-dev; D2_DIR: over all of D2, used
only for the frozen 40-ticket diagnostic.)"""
import json, math, os, re, struct, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from validate import descriptions, labels as read_labels, nli_tokenizer, split_numbers, validate

QUEUES = ["bug", "feature", "question", "documentation"]
LABELS = ["bug", "feature", "question", "documentation", "performance"]
FROZEN = os.path.join(HERE, "frozen")
TARGET_DEV = 0.10


def fold(label):
    return "bug" if label == "performance" else label


def f32(x):
    return struct.unpack("<f", struct.pack("<f", x))[0]


def argmax_queue(score):
    """score: {queue: value}; the top queue and the second value, ties to
    the queue order (exact equality)."""
    present = [q for q in QUEUES if q in score]
    best = present[0]
    for q in present[1:]:
        if score[q] > score[best]:
            best = q
    rest = [score[q] for q in present if q != best]
    return best, (max(rest) if rest else None)


# ---- K -------------------------------------------------------------------
def keyword_rules():
    rows = [l.split("\t", 1) for l in open(os.path.join(FROZEN, "keywords.tsv")).read().splitlines()[1:] if l]
    return [(q, re.compile(r)) for q, r in rows]


def system_k(titles, rules):
    out = {}
    for n, title in titles.items():
        hit = [q for q, rx in rules if rx.search(title.lower())]
        out[n] = (hit[0], 1.0) if len(hit) == 1 else None
    return out


# ---- Z -------------------------------------------------------------------
def system_z(numbers, C, names):
    out = {}
    for n in numbers:
        if n not in C:
            out[n] = None  # no passages: no embedding (5a)
            continue
        s = {}
        for name, v in zip(names, C[n]):
            q = fold(name)
            s[q] = v if q not in s else max(s[q], v)
        best, second = argmax_queue(s)
        conf = float(s[best]) - float(second)
        assert math.isfinite(conf), f"Z: non-finite confidence for {n}"
        out[n] = (best, conf)
    return out


# ---- N -------------------------------------------------------------------
def p_entail(logits):
    """Stable 3-way softmax in f64, summed contradiction, entailment,
    neutral (5b)."""
    l = [float(x) for x in logits]
    m = max(l)
    e = [math.exp(x - m) for x in l]
    total = (e[0] + e[1]) + e[2]
    return e[1] / total


def system_n(numbers, status, L, names):
    best_label = {}
    for n, p, label, logits in L:
        v = p_entail(logits)
        assert math.isfinite(v), f"N: non-finite probability for {n}"
        key = (n, label)
        best_label[key] = v if key not in best_label else max(best_label[key], v)
    out = {}
    for n in numbers:
        if status[n] != "ok":
            out[n] = None  # no passages, or the named pair-length refusal (5a)
            continue
        s = {}
        for name in names:
            q = fold(name)
            v = best_label[(n, name)]
            s[q] = v if q not in s else max(s[q], v)
        best, _ = argmax_queue(s)
        out[n] = (best, s[best])
    return out


# ---- C -------------------------------------------------------------------
def centroids(train, E):
    """train: [(number, queue)]; f64 sums in ascending number order, norm in
    component order, sum / norm rounded to f32 (5b). Absent: no ticket, or a
    norm that is 0 or not finite."""
    out = {}
    for q in QUEUES:
        members = sorted(n for n, lab in train if lab == q)
        if not members:
            continue
        sums = [0.0] * 384
        for n in members:
            for c, x in enumerate(E[n]):
                sums[c] += float(x)
        sq = 0.0
        for v in sums:
            sq += v * v
        norm = math.sqrt(sq)
        if norm == 0.0 or not math.isfinite(norm):
            continue
        out[q] = [f32(v / norm) for v in sums]
    return out


def score_c(n, E, cents):
    if n not in E or len(cents) < 2:
        return None  # no embedding, or fewer than two present queues (5a)
    s = {}
    for q, cen in cents.items():
        acc = 0.0
        for a, b in zip(E[n], cen):
            acc += float(a) * float(b)
        s[q] = acc
    best, second = argmax_queue(s)
    conf = s[best] - second
    assert math.isfinite(conf), f"C: non-finite confidence for {n}"
    return (best, conf)


def folds(numbers):
    import hashlib
    order = sorted(numbers, key=lambda n: (hashlib.sha256(f"rnx-0153-fold:{n}".encode("ascii")).hexdigest(), n))
    return {n: r % 5 for r, n in enumerate(order)}


def system_c_crossfit(numbers, E, ref):
    f = folds(numbers)
    out = {}
    for k in range(5):
        train = [(n, ref[n]) for n in numbers if f[n] != k and n in E]
        cents = centroids(train, E)
        for n in numbers:
            if f[n] == k:
                out[n] = score_c(n, E, cents)
    return out


# ---- the policy -------------------------------------------------------------
def at_threshold(pred, ref, t):
    N = len(ref)
    auto = [n for n in ref if pred[n] is not None and math.isfinite(pred[n][1]) and pred[n][1] >= t]
    wrong = [n for n in auto if pred[n][0] != ref[n]]
    return {"N": N, "auto": len(auto), "A": len(auto) / N, "W": 1 - len(auto) / N,
            "misroutes": len(wrong), "E": (len(wrong) / len(auto)) if auto else None,
            "wrong": wrong, "auto_set": auto}


def candidates(pred, only_one=False):
    if only_one:
        return [1.0]
    return sorted({p[1] for p in pred.values() if p is not None and math.isfinite(p[1])})


def choose(pred, ref, cands):
    """The lowest candidate with a defined E <= the dev target (5)."""
    for t in cands:
        r = at_threshold(pred, ref, t)
        if r["E"] is not None and r["E"] <= TARGET_DEV:
            return t, r
    return None, None


def mandatory_counts(pred, numbers, causes):
    out = {}
    for n in numbers:
        if pred[n] is None:
            out[causes.get(n, "rule")] = out.get(causes.get(n, "rule"), 0) + 1
    return out


def wilson(k, n, z=1.96):
    if n == 0:
        return (None, None)
    p = k / n
    d = 1 + z * z / n
    c = p + z * z / (2 * n)
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n))
    return (max(0.0, (c - h) / d), min(1.0, (c + h) / d))


def read_annotations(path):
    return {int(l.split("\t")[0]): l.split("\t")[1] for l in open(path).read().splitlines()[1:] if l}


def main(dev_dir, d3_issues, d2_dir, d2_issues, out_dir, tok_path):
    names = read_labels(os.path.join(FROZEN, "labels.tsv"))
    assert names == LABELS
    issues3 = {i["number"]: i for i in json.load(open(d3_issues))}
    dev = split_numbers(os.path.join(FROZEN, "d3-dev.tsv"))
    tok = nli_tokenizer(tok_path)
    descs = descriptions(os.path.join(FROZEN, "labels.tsv"))
    passages, status, estatus, E, C, L = validate(dev_dir, issues3, dev, names, descs, tok)
    print(f"{dev_dir}: validated ({len(dev)} D3-dev tickets)")
    ref = {n: fold(l) for n, l in read_annotations(os.path.join(FROZEN, "annotations-claude-dev.tsv")).items()}
    assert sorted(ref) == sorted(dev)
    ncauses = {n: s for n, s in status.items() if s != "ok"}
    ecauses = {n: s for n, s in estatus.items() if s != "ok"}
    preds = {
        "K": system_k({n: issues3[n]["title"] for n in dev}, keyword_rules()),
        "Z": system_z(dev, C, names),
        "N": system_n(dev, status, L, names),
        "C": system_c_crossfit(dev, E, ref),
    }
    counts = {q: sum(1 for v in ref.values() if v == q) for q in QUEUES}
    maj = max(QUEUES, key=lambda q: (counts[q], -QUEUES.index(q)))
    preds_diag = {"MAJ": {n: (maj, 1.0) for n in dev}}

    print(f"\nD3-dev reference (Claude's labels, folded to queues): {counts}; MAJ = {maj}")
    print("\n| system | mandatory REVIEW | threshold | auto | A | E | misroutes | cost c=2 | cost c=5 |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|---:|")
    chosen = {}
    for s in ["K", "Z", "C", "N"]:
        t, r = choose(preds[s], ref, candidates(preds[s], only_one=(s == "K")))
        mand = mandatory_counts(preds[s], dev, {"Z": ecauses, "C": ecauses, "N": ncauses}.get(s, {}))
        if t is None:
            print(f"| {s} | {mand} | ineligible | | | | | | |")
            continue
        chosen[s] = (t, r)
        c2 = r["W"] + 2 * r["misroutes"] / r["N"]
        c5 = r["W"] + 5 * r["misroutes"] / r["N"]
        print(f"| {s} | {mand} | {t!r} | {r['auto']} | {r['A']:.3f} | {r['E']:.3f} | {r['misroutes']} | {c2:.3f} | {c5:.3f} |")
    r = at_threshold(preds_diag["MAJ"], ref, 1.0)
    print(f"| MAJ (diagnostic only) | {{}} | 1.0 | {r['auto']} | {r['A']:.3f} | {r['E']:.3f} | {r['misroutes']} | "
          f"{r['W'] + 2 * r['misroutes'] / r['N']:.3f} | {r['W'] + 5 * r['misroutes'] / r['N']:.3f} |")
    print(f"| ALL-HUMAN (fallback) | all | - | 0 | 0.000 | undefined | 0 | 1.000 | 1.000 |")

    order = ["K", "Z", "C", "N"]
    if chosen:
        best = max(chosen.values(), key=lambda v: v[1]["A"])[1]["A"]
        selected = next(s for s in order if s in chosen and chosen[s][1]["A"] == best)
    else:
        selected = "ALL-HUMAN"
    print(f"\nSELECTION (highest dev A at its threshold, ties in the order {order}): {selected}"
          + (f", threshold {chosen[selected][0]!r}, dev A {chosen[selected][1]['A']:.4f}, dev E {chosen[selected][1]['E']:.4f}" if selected in chosen else ""))

    # curves and per-queue misroutes
    curves = {}
    for s in ["K", "Z", "C", "N"]:
        curves[s] = [{"t": t, **{k: v for k, v in at_threshold(preds[s], ref, t).items() if k not in ("wrong", "auto_set")}}
                     for t in candidates(preds[s], only_one=(s == "K"))]
        full = [n for n in dev if preds[s][n] is not None]
        conf = {(a, b): sum(1 for n in full if ref[n] == a and preds[s][n][0] == b) for a in QUEUES for b in QUEUES}
        print(f"\n{s} at full coverage ({len(full)} predicted, reference rows x predicted columns):")
        print("| reference | " + " | ".join(QUEUES) + " |")
        print("|---|" + "---:|" * 4)
        for a in QUEUES:
            print(f"| {a} | " + " | ".join(str(conf[(a, b)]) for b in QUEUES) + " |")
        if s in chosen:
            wrong = chosen[s][1]["wrong"]
            mis = {}
            for n in wrong:
                key = f"{ref[n]} -> {preds[s][n][0]}"
                mis[key] = mis.get(key, 0) + 1
            print(f"{s} misroutes at its threshold, reference -> predicted: {dict(sorted(mis.items()))}")
        sample = curves[s][:: max(1, len(curves[s]) // 12)] + curves[s][-1:]
        print(f"{s} risk-coverage (every ~{max(1, len(curves[s]) // 12)}th candidate of {len(curves[s])}):")
        for row in sample:
            e = "undefined" if row["E"] is None else f"{row['E']:.3f}"
            print(f"  t={row['t']:.6g}  A={row['A']:.3f}  E={e}")

    os.makedirs(out_dir, exist_ok=True)
    artifact = {"selected": selected, "dev_target": TARGET_DEV,
                "thresholds": {s: chosen[s][0] for s in chosen},
                "dev": {s: {k: v for k, v in chosen[s][1].items() if k not in ("wrong", "auto_set")} for s in chosen}}
    if selected == "C":
        cents = centroids([(n, ref[n]) for n in dev if n in E], E)
        cpath = os.path.join(out_dir, "centroids.tsv")
        with open(cpath, "w") as fh:
            fh.write("queue\t" + "\t".join(f"c{i}" for i in range(384)) + "\n")
            for q in QUEUES:
                if q in cents:
                    fh.write(q + "\t" + "\t".join(struct.pack("<f", v).hex() for v in cents[q]) + "\n")
        artifact["centroids"] = "centroids.tsv (f32 bits, little-endian hex)"
    json.dump(artifact, open(os.path.join(out_dir, "selected.json"), "w"), indent=1)
    json.dump(curves, open(os.path.join(out_dir, "curves.json"), "w"))
    print(f"\nwrote {out_dir}/selected.json and curves.json")

    # D2 diagnostic (descriptive only): the selected policy on 0139's 40
    if selected != "ALL-HUMAN":
        issues2 = {i["number"]: i for i in json.load(open(d2_issues))}
        d2_all = sorted(issues2)
        p2, s2, es2, E2, C2, L2 = validate(d2_dir, issues2, d2_all, names, descs, tok)
        sample = [int(l.split("\t")[0]) for l in open(os.path.join(HERE, "..", "0139", "frozen", "sample.tsv")).read().splitlines()[1:] if l]
        if selected == "K":
            pr = system_k({n: issues2[n]["title"] for n in sample}, keyword_rules())
        elif selected == "Z":
            pr = system_z(sample, C2, names)
        elif selected == "N":
            pr = system_n(sample, s2, L2, names)
        else:
            cents = centroids([(n, ref[n]) for n in dev if n in E], E)
            pr = {n: score_c(n, E2, cents) for n in sample}
        t = chosen[selected][0]
        auto = [n for n in sample if pr[n] is not None and pr[n][1] >= t]
        print(f"\nD2 diagnostic (seen before; descriptive only): {selected} at t={t!r} auto-routes {len(auto)} of {len(sample)}")
        for who in ("claude", "codex"):
            a = {n: fold(l) for n, l in read_annotations(os.path.join(HERE, "..", "0139", "frozen", f"annotations-{who}.tsv")).items()}
            wrong = sum(1 for n in auto if pr[n][0] != a[n])
            print(f"  against 0139's {who} labels: misroutes {wrong} of {len(auto)}")


if __name__ == "__main__":
    main(*sys.argv[1:7])
