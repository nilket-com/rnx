"""Record 0147: the frozen rubric's measures for retrieval alone and for
re-ranking, from a U5 directory validated against its retained evidence
and D1 (compare_u5.validate), with U1' as a consistency check.

Per query and order: r (the rank of the first supporting record's earliest
document among the 20, None beyond), hit@1, hit@5, recall@5, RR (1/r, 0
beyond 20: MRR truncated at 20), and candidate recall@20 (the same for both
orders). Paired: delta r (censored when either rank is beyond 20), delta RR,
delta recall@5, and a win, loss or tie by RR. Aggregates overall and by
stratum, an exact two-sided sign test and a paired bootstrap interval on
mean delta RR (10,000 resamples, seed 147), all descriptive.

The measures here are written independently of compare_u5's, and must
reproduce U5's own in-trace metrics exactly for every query and order.

  evaluate.py U5_DIR U1_TSV RUBRIC D1
  evaluate.py --controls"""
import math, os, random, struct, sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "0146"))


def first_rank(records, support):
    """1-based rank of the earliest candidate whose record supports, or None."""
    if not support:
        raise ValueError("a query with no supporting records")
    for i, rec in enumerate(records):
        if rec in support:
            return i + 1
    return None


def measure(records, support):
    r = first_rank(records, support)
    found5 = {rec for rec in records[:5] if rec in support}
    found20 = {rec for rec in records[:20] if rec in support}
    return {
        "r": r,
        "hit1": r == 1,
        "hit5": r is not None and r <= 5,
        "recall5": len(found5) / len(set(support)),
        "rr": 0.0 if r is None else 1.0 / r,
        "cand20": len(found20) / len(set(support)),
    }


def delta_r(a, b):
    if a is None or b is None:
        return f"{'> 20' if a is None else a} -> {'> 20' if b is None else b} (censored)"
    return b - a


def sign_test(wins, losses):
    """Exact two-sided binomial p-value at 1/2 over non-tied pairs."""
    n, k = wins + losses, min(wins, losses)
    if n == 0:
        return 1.0
    p = sum(math.comb(n, i) for i in range(k + 1)) / 2 ** n
    return min(1.0, 2 * p)


def bootstrap(deltas, seed=147, reps=10000):
    rng = random.Random(seed)
    n = len(deltas)
    means = sorted(sum(deltas[rng.randrange(n)] for _ in range(n)) / n for _ in range(reps))
    return means[int(0.025 * reps)], means[int(0.975 * reps) - 1]


def f32b(x):
    return struct.pack("<f", x)


def check_u1(text, ids, cands, paths, scores):
    """Validate U1' against U5's retained evidence (paths by pid; scores as
    f32 bytes by pid and query index), then against U5's validated
    candidates. Returns the equal-score choices it allowed; exits on any
    other difference. Review round 1, R1."""
    lines = text.splitlines()
    if not lines or lines[0] != "query\trank\tpath\tpid\tscore":
        fail("U1': the header is not query, rank, path, pid, score")
    rows = [l.split("\t") for l in lines[1:]]
    if any(len(r) != 5 for r in rows):
        fail("U1': a row is not five fields")
    if [r[0] for r in rows] != [q for q in ids for _ in range(5)] or [r[1] for r in rows] != ["1", "2", "3", "4", "5"] * len(ids):
        fail("U1': the rows are not each rubric query's ranks 1 to 5, in order")
    # each document's best retained f32 score, per query
    best = [{} for _ in ids]
    for pid, p in enumerate(paths):
        for k in range(len(ids)):
            v = struct.unpack("<f", scores[pid][k])[0]
            if p not in best[k] or v > best[k][p]:
                best[k][p] = v
    ties = []
    for k, q in enumerate(ids):
        mine = rows[5 * k:5 * k + 5]
        if len({r[2] for r in mine}) != 5:
            fail(f"U1' {q}: a document repeats")
        for j, r in enumerate(mine):
            where = f"U1' {q} rank {j + 1}"
            try:
                v = float(r[4])
            except ValueError:
                fail(f"{where}: score {r[4]!r} is not a number")
            if not math.isfinite(v):
                fail(f"{where}: score {r[4]} is not finite")
            if not r[3].isdigit() or int(r[3]) >= len(paths):
                fail(f"{where}: pid {r[3]!r} is not a retained passage")
            pid = int(r[3])
            if paths[pid] != r[2]:
                fail(f"{where}: pid {pid} belongs to {paths[pid]}, not {r[2]}")
            if f32b(v) != scores[pid][k]:
                fail(f"{where}: score {r[4]} is not pid {pid}'s retained score")
            if f32b(v) != f32b(best[k][r[2]]):
                fail(f"{where}: pid {pid} is not {r[2]}'s best passage")
            # the same rank's score as U5's; a different document there is
            # then an equal-score choice (each document at its best, distinct)
            c = cands[q][j]
            if f32b(v) != f32b(c[2]):
                fail(f"{where}: score {r[4]} differs from U5's {c[2]} at that rank")
            if (r[2], pid) != (c[0], c[1]):
                ties.append(f"{where}: {r[2]} (pid {pid}) against U5's {c[0]} (pid {c[1]}) at an equal f32 score")
    return ties


def fail(msg):
    sys.exit(f"FAIL: {msg}")


def controls():
    ok = True

    def expect(name, got, want):
        nonlocal ok
        good = got == want
        ok &= good
        print(f"{'pass' if good else 'WRONG'}: {name}: {got}")

    rest = [f"{9000 + i}" for i in range(20)]
    expect("supporting at rank 1", measure(["0001"] + rest[:19], {"0001"})["r"], 1)
    expect("supporting at rank 5", measure(rest[:4] + ["0001"] + rest[4:19], {"0001"})["hit5"], True)
    m = measure(rest[:5] + ["0001"] + rest[5:19], {"0001"})
    expect("supporting at rank 6", (m["r"], m["hit5"], m["recall5"], m["rr"]), (6, False, 0.0, 1 / 6))
    m = measure(rest, {"0001"})
    expect("absent", (m["r"], m["hit1"], m["rr"], m["cand20"]), (None, False, 0.0, 0.0))
    m = measure(["0002"] + rest[:19], {"0002", "0003"})
    expect("multi-record, one found one absent", (m["r"], m["recall5"], m["cand20"]), (1, 0.5, 0.5))
    m = measure(["0004", "0001", "0004"] + rest[:17], {"0004"})
    expect("a record's second document also ranked", (m["r"], m["recall5"]), (1, 1.0))
    expect("censored delta r", delta_r(None, 3), "> 20 -> 3 (censored)")
    expect("sign test, 0 wins 2 losses", sign_test(0, 2), 0.5)
    try:
        measure(rest, set())
        expect("empty supporting set refused", "accepted", "refused")
    except ValueError:
        expect("empty supporting set refused", "refused", "refused")
    # U1' against synthetic evidence: two documents tie at rank 5
    paths = ["a.md", "b.md", "c.md", "d.md", "e.md", "f.md", "a.md", "g.md", "g.md"]
    vals = [0.9, 0.8, 0.7, 0.6, 0.5, 0.5, 0.1, 0.5, 0.3]
    sc = [[f32b(v)] for v in vals]
    cands = {"Q": [(p, i, vals[i]) for i, p in enumerate(paths[:5])]}
    u1 = lambda last: "query\trank\tpath\tpid\tscore\n" + "".join(
        f"Q\t{j + 1}\t{p}\t{i}\t{vals[i]}\n" for j, (p, i) in enumerate([("a.md", 0), ("b.md", 1), ("c.md", 2), ("d.md", 3), last]))
    expect("U1' genuine tie passes, named", len(check_u1(u1(("f.md", 5)), ["Q"], cands, paths, sc)), 1)
    expect("U1' identical passes", check_u1(u1(("e.md", 4)), ["Q"], cands, paths, sc), [])
    try:
        check_u1(u1(("g.md", 8)), ["Q"], cands, paths, sc)
        expect("U1' a document's non-best passage refused", "accepted", "refused")
    except SystemExit:
        expect("U1' a document's non-best passage refused", "refused", "refused")
    print("all evaluator controls behave" if ok else "CONTROLS FAILED")
    return ok


def main(u5, u1, rubric_path, d1):
    from compare_u5 import rubric_of, validate

    rubric = rubric_of(rubric_path)
    kinds = {l.split("\t")[0]: l.split("\t")[4] for l in open(rubric_path).read().splitlines()[1:] if l}
    out, (paths, _, scores, _, _) = validate(u5, rubric, d1)
    print(f"{u5}: validated against its retained evidence and D1")

    # U1' consistency (review round 1, R1): U1' is validated on its own
    # against the same retained evidence, then compared with U5's top 5
    ties = check_u1(open(u1).read(), [q for q, _, _ in rubric], {q: out[q][0] for q, _, _ in rubric}, paths, scores)
    for t in ties:
        print(f"tie: {t}")
    print(f"U1' validated against U5's retained evidence; its top 5 equals U5's retrieval top 5 for all "
          f"{len(rubric)} queries ({len(ties)} equal-score choices named above)")

    rows = []
    for q, _, support in rubric:
        cands, order = out[q][0], out[q][1]
        records = [c[0][:4] for c in cands]
        ret = measure(records, set(support))
        rer = measure([records[i - 1] for i in order], set(support))
        # independent agreement with U5's in-trace metrics (as validated)
        for mine, theirs, what in ((ret, out[q][2], "retrieval"), (rer, out[q][3], "re-ranked")):
            if (mine["hit1"], mine["hit5"], mine["recall5"], mine["rr"]) != tuple(theirs):
                sys.exit(f"FAIL: {q} {what}: evaluator {mine} against the trace's {theirs}")
        rows.append((q, kinds[q], support, ret, rer))
    print(f"the evaluator reproduces U5's in-trace hit@1, hit@5, recall@5 and MRR for all {len(rows)} queries, both orders")

    show = lambda r: "> 20" if r is None else str(r)
    print("\n| query | kind | supporting | retrieval r | re-ranked r | delta r | delta RR | recall@5 | candidate recall@20 |")
    print("|---|---|---|---:|---:|---:|---:|---|---:|")
    for q, kind, support, a, b in rows:
        print(f"| {q} | {kind} | {' '.join(support)} | {show(a['r'])} | {show(b['r'])} | {delta_r(a['r'], b['r'])} | "
              f"{b['rr'] - a['rr']:+.3f} | {a['recall5']:.2f} -> {b['recall5']:.2f} | {a['cand20']:.2f} |")

    def summary(name, sel):
        n = len(sel)
        if not n:
            return
        mean = lambda f: sum(f(x) for x in sel) / n
        d = [b["rr"] - a["rr"] for _, _, _, a, b in sel]
        wins, losses = sum(x > 0 for x in d), sum(x < 0 for x in d)
        lo, hi = bootstrap(d)
        print(f"| {name} | {n} | {sum(a['hit1'] for *_, a, _ in sel)} -> {sum(b['hit1'] for *_, b in sel)} | "
              f"{sum(a['hit5'] for *_, a, _ in sel)} -> {sum(b['hit5'] for *_, b in sel)} | "
              f"{mean(lambda x: x[3]['recall5']):.3f} -> {mean(lambda x: x[4]['recall5']):.3f} | "
              f"{mean(lambda x: x[3]['rr']):.3f} -> {mean(lambda x: x[4]['rr']):.3f} | "
              f"{mean(lambda x: x[3]['cand20']):.3f} | {wins} / {losses} / {n - wins - losses} | "
              f"{sign_test(wins, losses):.3f} | [{lo:+.3f}, {hi:+.3f}] |")

    print("\n| stratum | queries | hit@1 | hit@5 | mean recall@5 | MRR (to 20) | mean candidate recall@20 | RR wins / losses / ties | sign test p | mean delta RR, 95% bootstrap |")
    print("|---|---:|---|---|---|---|---:|---|---:|---|")
    summary("all", rows)
    for k in ("direct", "paraphrase"):
        summary(k, [x for x in rows if x[1] == k])
    summary("single-record", [x for x in rows if len(x[2]) == 1])
    summary("multi-record", [x for x in rows if len(x[2]) > 1])


if __name__ == "__main__":
    if sys.argv[1:] == ["--controls"]:
        sys.exit(0 if controls() else 1)
    main(*sys.argv[1:5])
