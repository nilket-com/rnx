"""Record 0154: the locked holdout evaluation (plans/0154 section 2), and its
rehearsal (section 3, review R1).

The policy is 0153's frozen C: the full-dev centroids read AS BITS from
probes/0153/out/dev/centroids.tsv, scored by 0153's score_c (imported, not
reimplemented), auto-routed when confidence >= the frozen threshold. K and
ALL-HUMAN are the comparators.

  evaluate.py holdout DIR D3_ISSUES NLI_TOKENIZER OUT_DIR
      DIR: validated t3 evidence over D3-hold; both blind annotators
  evaluate.py rehearse DEV_DIR D3_ISSUES NLI_TOKENIZER OUT_DIR
      plumbing only: D3-dev with Claude's labels as both annotators;
      path (a) 0153's cross-fit predictions, path (b) the frozen policy"""
import importlib.util, json, math, os, struct, sys

HERE = os.path.dirname(os.path.abspath(__file__))
P0153 = os.path.join(HERE, "..", "0153")
sys.path.insert(0, P0153)
from validate import descriptions, labels as read_labels, nli_tokenizer, split_numbers, validate

_spec = importlib.util.spec_from_file_location("evaluate0153", os.path.join(P0153, "evaluate.py"))
ev = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ev)

FROZEN = os.path.join(P0153, "frozen")
SELECTED = json.load(open(os.path.join(P0153, "out", "dev", "selected.json")))
THRESHOLD = SELECTED["thresholds"]["C"]
assert SELECTED["selected"] == "C" and THRESHOLD == 0.09553107383376902


def frozen_centroids(path=os.path.join(P0153, "out", "dev", "centroids.tsv")):
    out = {}
    for line in open(path).read().splitlines()[1:]:
        f = line.split("\t")
        out[f[0]] = [struct.unpack("<f", bytes.fromhex(h))[0] for h in f[1:]]
        assert len(out[f[0]]) == 384
    return out


def maintainer_queues(issues, numbers):
    m = {}
    for l in open(os.path.join(FROZEN, "maintainer_map.tsv")).read().splitlines()[1:]:
        k, q = l.split("\t")
        m[k.lower()] = q
    out = {}
    for n in numbers:
        qs = {m[l.lower()] for l in issues[n]["labels"] if l.lower() in m}
        out[n] = "none" if not qs else (qs.pop() if len(qs) == 1 else "multi")
    return out


def measures(pred, a, b, t, N_expected=None):
    """pred: {n: (queue, conf) | None}; a, b: folded queues. Section 2."""
    N = len(pred)
    if N_expected is not None:
        assert N == N_expected
    auto = [n for n in pred if pred[n] is not None and math.isfinite(pred[n][1]) and pred[n][1] >= t]
    possible = [n for n in auto if pred[n][0] != a[n] or pred[n][0] != b[n]]
    certain = [n for n in auto if pred[n][0] != a[n] and pred[n][0] != b[n]]
    consensus = [n for n in auto if a[n] == b[n]]
    cons_wrong = [n for n in consensus if pred[n][0] != a[n]]
    k = len(auto)
    by_queue = {}
    for n in possible:
        key = f"({a[n]}, {b[n]}) -> {pred[n][0]}"
        by_queue[key] = by_queue.get(key, 0) + 1
    W = (N - k) / N
    return {
        "N": N, "auto": k, "review": N - k, "A": k / N, "W": W, "A_wilson": ev.wilson(k, N),
        "possible": len(possible), "E_possible": (len(possible) / k) if k else None,
        "E_possible_wilson": ev.wilson(len(possible), k),
        "certain": len(certain), "E_certain": (len(certain) / k) if k else None,
        "E_certain_wilson": ev.wilson(len(certain), k),
        "consensus_auto": len(consensus), "consensus_wrong": len(cons_wrong),
        "E_consensus": (len(cons_wrong) / len(consensus)) if consensus else None,
        "either_correct": k - len(certain), "either_correct_rate": ((k - len(certain)) / k) if k else None,
        "misroutes_by_queue": dict(sorted(by_queue.items())),
        "cost_possible": {c: W + c * len(possible) / N for c in (2, 5)},
        "cost_certain": {c: W + c * len(certain) / N for c in (2, 5)},
        "auto_set": sorted(auto),
    }


def outcome(m):
    """The frozen 0154 outcome rule (verbatim in plans/0154 section 2)."""
    if m["auto"] == 0:
        return "not demonstrated"
    upper = m["E_possible_wilson"][1]
    lower = m["A_wilson"][0]
    return "routing target met" if (upper <= 0.20 and lower >= 0.25) else "not demonstrated"


def agreement(a5, b5):
    def kappa(x, y, cats):
        n = len(x)
        po = sum(x[k] == y[k] for k in x) / n
        pe = sum((sum(v == c for v in x.values()) / n) * (sum(v == c for v in y.values()) / n) for c in cats)
        return po, (po - pe) / (1 - pe) if pe < 1 else float("nan")
    p5, k5 = kappa(a5, b5, ev.LABELS)
    a4 = {n: ev.fold(v) for n, v in a5.items()}
    b4 = {n: ev.fold(v) for n, v in b5.items()}
    p4, k4 = kappa(a4, b4, ev.QUEUES)
    dis = {}
    for n in a4:
        if a4[n] != b4[n]:
            key = f"{a4[n]} | {b4[n]}"
            dis[key] = dis.get(key, 0) + 1
    return {"five_labels": {"agree": sum(a5[n] == b5[n] for n in a5), "p": p5, "kappa": k5},
            "four_queues": {"agree": sum(a4[n] == b4[n] for n in a4), "p": p4, "kappa": k4},
            "queue_disagreements": dict(sorted(dis.items()))}


def fmt_iv(iv):
    return "undefined" if iv[0] is None else f"[{iv[0]:.3f}, {iv[1]:.3f}]"


def report(name, m):
    e = lambda v: "undefined" if v is None else f"{v:.3f}"
    print(f"\n{name}: auto {m['auto']} of {m['N']}, A {m['A']:.3f} {fmt_iv(m['A_wilson'])}, workload {m['W']:.3f}")
    print(f"  E_possible {e(m['E_possible'])} ({m['possible']}) {fmt_iv(m['E_possible_wilson'])}; "
          f"E_certain {e(m['E_certain'])} ({m['certain']}) {fmt_iv(m['E_certain_wilson'])}")
    print(f"  consensus-only E {e(m['E_consensus'])} ({m['consensus_wrong']} of {m['consensus_auto']}); "
          f"either-label-correct {m['either_correct']} of {m['auto']} ({e(m['either_correct_rate'])})")
    print(f"  cost, possible misroutes: c=2 {m['cost_possible'][2]:.3f}, c=5 {m['cost_possible'][5]:.3f}; "
          f"certain misroutes: c=2 {m['cost_certain'][2]:.3f}, c=5 {m['cost_certain'][5]:.3f}")
    print(f"  misroutes by queue, (Claude, Codex) -> predicted: {m['misroutes_by_queue']}")


def load(d, issues_path, tok_path, split_name):
    issues = {i["number"]: i for i in json.load(open(issues_path))}
    names = read_labels(os.path.join(FROZEN, "labels.tsv"))
    numbers = split_numbers(os.path.join(FROZEN, split_name))
    passages, status, estatus, E, C, L = validate(d, issues, numbers, names, descriptions(os.path.join(FROZEN, "labels.tsv")),
                                                  nli_tokenizer(tok_path))
    return issues, numbers, E


def policy(numbers, E):
    cents = frozen_centroids()
    return {n: ev.score_c(n, E, cents) for n in numbers}


def main(mode, d, issues_path, tok_path, out_dir):
    os.makedirs(out_dir, exist_ok=True)
    if mode == "rehearse":
        issues, dev, E = load(d, issues_path, tok_path, "d3-dev.tsv")
        ref = {n: ev.fold(l) for n, l in ev.read_annotations(os.path.join(FROZEN, "annotations-claude-dev.tsv")).items()}
        cf = ev.system_c_crossfit(dev, E, ref)
        ma = measures(cf, ref, ref, THRESHOLD)
        report("REHEARSAL (a), 0153's cross-fit predictions, Claude's labels as both annotators", ma)
        mb = measures(policy(dev, E), ref, ref, THRESHOLD)
        report("REHEARSAL (b), the frozen full-dev centroid policy on D3-dev (in-sample; plumbing only)", mb)
        json.dump({"a": {k: v for k, v in ma.items() if k != "auto_set"}, "b": {k: v for k, v in mb.items() if k != "auto_set"},
                   "b_auto_set": mb["auto_set"]}, open(os.path.join(out_dir, "rehearsal.json"), "w"), indent=1)
        return
    issues, hold, E = load(d, issues_path, tok_path, "d3-hold.tsv")
    a5 = ev.read_annotations(os.path.join(FROZEN, "annotations-claude-hold.tsv"))
    b5 = ev.read_annotations(os.path.join(FROZEN, "annotations-codex-hold.tsv"))
    assert sorted(a5) == sorted(b5) == sorted(hold)
    a = {n: ev.fold(v) for n, v in a5.items()}
    b = {n: ev.fold(v) for n, v in b5.items()}
    pc = policy(hold, E)
    mc = measures(pc, a, b, THRESHOLD, 150)
    pk = ev.system_k({n: issues[n]["title"] for n in hold}, ev.keyword_rules())
    mk = measures(pk, a, b, 1.0, 150)
    mand = {"C": sum(1 for n in hold if pc[n] is None), "K": sum(1 for n in hold if pk[n] is None)}
    print(f"D3-hold: {len(hold)} tickets; mandatory REVIEW C {mand['C']}, K {mand['K']}")
    report(f"C (the selected policy, t={THRESHOLD!r})", mc)
    report("K (comparator, its only threshold 1)", mk)
    print("\nALL-HUMAN (comparator): auto 0 of 150, A 0, workload 1.000, E undefined; cost 1.000")
    res = outcome(mc)
    print(f"\nOUTCOME (plans/0154 section 2): {res}  "
          f"(E_possible upper {fmt_iv(mc['E_possible_wilson'])}, A lower {fmt_iv(mc['A_wilson'])})")
    ag = agreement(a5, b5)
    print(f"\nannotator agreement: 4 queues {ag['four_queues']['agree']}/150 (kappa {ag['four_queues']['kappa']:.3f}); "
          f"5 labels {ag['five_labels']['agree']}/150 (kappa {ag['five_labels']['kappa']:.3f}); disagreements {ag['queue_disagreements']}")
    mq = maintainer_queues(issues, hold)
    usable = [n for n in hold if mq[n] not in ("none", "multi")]
    print(f"\nmaintainer comparison (external, descriptive): {len(usable)} mapped, "
          f"{sum(1 for n in hold if mq[n] == 'none')} none, {sum(1 for n in hold if mq[n] == 'multi')} multi (excluded)")
    for who, lab in (("Claude", a), ("Codex", b)):
        print(f"  {who}'s queue agrees with the maintainer's on {sum(1 for n in usable if lab[n] == mq[n])} of {len(usable)}")
    routed = [n for n in mc["auto_set"] if n in usable]
    print(f"  C's auto-routes with a mapped maintainer queue: {len(routed)}; agreeing {sum(1 for n in routed if pc[n][0] == mq[n])}")
    print(f"\ndevelopment (0153, cross-fit, Claude's labels): A {SELECTED['dev']['C']['A']:.3f}, E {SELECTED['dev']['C']['E']:.3f}")
    clean = lambda m: {k: v for k, v in m.items() if k != "auto_set"}
    json.dump({"outcome": res, "threshold": THRESHOLD, "C": clean(mc), "K": clean(mk), "C_auto_set": mc["auto_set"],
               "mandatory_review": mand, "agreement": ag,
               "maintainer": {"mapped": len(usable), "none": sum(1 for n in hold if mq[n] == "none"),
                              "multi": sum(1 for n in hold if mq[n] == "multi")}},
              open(os.path.join(out_dir, "result.json"), "w"), indent=1)
    print(f"wrote {out_dir}/result.json")


if __name__ == "__main__":
    main(*sys.argv[1:6])
