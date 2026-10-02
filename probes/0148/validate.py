"""Record 0148: U6's gate. Each producer's retained files are validated on
their own first, against identities the validator reconstructs from the
verified inputs; then the two producers are compared exactly.

The identities (plans/0148 section 6, review round 1, R3): the ordered
Cartesian set of (ticket in D2 order, passage index in ranges-o0.tsv order,
label in labels.tsv order), each with its passage text cut from verified D2
(title + "\\n" + body, the committed byte range) and the frozen description.

Per producer:
- u6-pairs.tsv: exactly those identities, in that order, each with three
  finite logits (contradiction, entailment, neutral), each read as the f32
  its text rounds to.
- u6-tickets.tsv: one row per D2 ticket, in D2 order, recomputed from the
  pairs' logits by section 4's rule in Python f64: p_entail by a 3-way
  softmax, s_label the maximum over the ticket's passages; each s_label
  within 1e-12 of the producer's; class the best label (an exact tie to the
  first in labels.tsv order), "review" below 0.5 (exactly 0.5 assigned);
  classes, best and second identical, except a ticket whose best s_label is
  within 1e-12 of 0.5 or of its second label's, which is named.
Then, between producers: every logit in f32 bits, every ticket row exactly
(s_label in f64 bits).

  validate.py SCRIPT_DIR TWIN_DIR D2_JSON RANGES LABELS"""
import json, math, struct, sys

EPS = 1e-12


def fail(msg):
    sys.exit(f"FAIL: {msg}")


def identities(d2, ranges, labels):
    """[(ticket, passage index, label, passage text, description)] in order,
    and the ticket numbers and label names."""
    issues = json.load(open(d2))
    texts = {f"d2:#{i['number']}": (i["title"] + "\n" + i["body"]).encode() for i in issues}
    numbers = [i["number"] for i in issues]
    names, descs = [], []
    for line in open(labels).read().splitlines()[1:]:
        if line:
            n, d = line.split("\t")
            names.append(n)
            descs.append(d)
    passages = {}
    for line in open(ranges).read().splitlines()[1:]:
        src, k, s, e = line.split("\t")
        if src.startswith("d2:"):
            got = passages.setdefault(src, [])
            if int(k) != len(got):
                fail(f"{ranges}: {src} passage {k} out of order")
            got.append(texts[src][int(s):int(e)].decode())
    out = []
    for num in numbers:
        for p, text in enumerate(passages[f"d2:#{num}"]):
            for j in range(len(names)):
                out.append((num, p, names[j], text, descs[j]))
    return out, numbers, names


def f32(x):
    return struct.pack("<f", x)


def num(text, what):
    try:
        v = float(text)
    except ValueError:
        fail(f"{what}: {text!r} is not a number")
    if not math.isfinite(v):
        fail(f"{what}: {text} is not finite")
    return v


def softmax_entail(row):
    m = max(row)
    e = [math.exp(x - m) for x in row]
    return e[1] / sum(e)


def decide(s, names):
    best = 0
    for j in range(1, len(s)):
        if s[j] > s[best]:
            best = j
    nxt = 1 if best == 0 else 0
    for j in range(len(s)):
        if j != best and s[j] > s[nxt]:
            nxt = j
    return ("review" if s[best] < 0.5 else names[best]), names[best], names[nxt], best, nxt


def explained(s, decision, names, eps):
    """Whether `decision` (triage, best, second) is what section 4's rule
    gives for SOME scores within `eps` of `s` (review round 1, R2): the
    chosen best is within 2·eps of every other score, the chosen second
    within 2·eps of every remaining one, and the threshold side holds
    within eps. A choice no such perturbation yields is not explained."""
    triage, best, second = decision
    if best not in names or second not in names or best == second:
        return False
    b, c = names.index(best), names.index(second)
    if any(s[b] < s[j] - 2 * eps for j in range(len(s)) if j != b):
        return False
    if any(s[c] < s[j] - 2 * eps for j in range(len(s)) if j not in (b, c)):
        return False
    if triage == "review":
        return s[b] < 0.5 + eps
    return triage == best and s[b] >= 0.5 - eps


def check_ticket(where, have, want, fields, names, eps):
    """One ticket row against section 4's rule (review round 1, R2):
    1. the producer's decisions follow its OWN retained scores exactly
       (ties to labels.tsv order, below 0.5 to review, exactly 0.5 assigned);
    2. its scores are within eps of the recomputed ones;
    3. its decisions equal the recomputed decisions, or differ only as a
       perturbation of the recomputed scores within eps would make them
       (a genuine rounding ambiguity, which is returned to be named).
    A forged decision fails at 1 whatever its scores; a threshold or tie
    ambiguity can't excuse an unrelated label."""
    triage, best, second, _, _ = decide(have, names)
    if list(fields) != [triage, best, second]:
        fail(f"{where}: {list(fields)} is not the rule's {[triage, best, second]} on its own scores")
    for j, n in enumerate(names):
        if abs(have[j] - want[j]) > eps:
            fail(f"{where}: s_{n} {have[j]!r} is not the recomputed {want[j]!r}")
    w = decide(want, names)[:3]
    if list(w) != list(fields):
        if not explained(want, fields, names, eps):
            fail(f"{where}: {list(fields)} differs from the recomputed {list(w)} beyond a within-{eps} ambiguity")
        return f"{where}: {list(fields)} against the recomputed {list(w)}: a rounding ambiguity within {eps}"
    return None


def validate(d, ids, numbers, names):
    lines = open(f"{d}/u6-pairs.tsv").read().splitlines()
    if not lines or lines[0] != "ticket\tpassage\tlabel\tcontradiction\tentailment\tneutral":
        fail(f"{d}/u6-pairs.tsv: header")
    rows = lines[1:]
    if len(rows) != len(ids):
        fail(f"{d}/u6-pairs.tsv: {len(rows)} pairs, want {len(ids)}")
    logits = []
    for r, (line, (num_, p, lab, _, _)) in enumerate(zip(rows, ids)):
        f = line.split("\t")
        if len(f) != 6 or (f[0], f[1], f[2]) != (str(num_), str(p), lab):
            fail(f"{d}/u6-pairs.tsv row {r + 1}: want ticket {num_} passage {p} label {lab}, have {f[:3]}")
        # each logit is the f32 its text rounds to (a producer may print the
        # f32's exact value or its shortest round-trip form); the recompute
        # uses that exact value, as the runtime does
        v = [struct.unpack("<f", f32(num(x, f"{d}/u6-pairs.tsv row {r + 1}")))[0] for x in f[3:]]
        if not all(math.isfinite(x) for x in v):
            fail(f"{d}/u6-pairs.tsv row {r + 1}: a logit overflows f32")
        logits.append(v)
    # the ticket table, recomputed
    k = len(names)
    lines = open(f"{d}/u6-tickets.tsv").read().splitlines()
    if not lines or lines[0] != "ticket\ttriage\tbest\tsecond\tbest_score\t" + "\t".join(names):
        fail(f"{d}/u6-tickets.tsv: header")
    rows = lines[1:]
    if [r.split("\t")[0] for r in rows] != [str(n) for n in numbers]:
        fail(f"{d}/u6-tickets.tsv: not one row per D2 ticket in D2 order")
    named, table = [], []
    for num_, line in zip(numbers, rows):
        f = line.split("\t")
        if len(f) != 5 + k:
            fail(f"{d}/u6-tickets.tsv ticket {num_}: {len(f)} fields")
        have = [num(x, f"{d} ticket {num_}") for x in f[5:]]
        want = [max(softmax_entail(logits[r]) for r, i in enumerate(ids) if i[0] == num_ and i[2] == names[j])
                for j in range(k)]
        if f[1] not in names + ["review"] or f[2] not in names or f[3] not in names or f[2] == f[3]:
            fail(f"{d} ticket {num_}: classes {f[1:4]}")
        if num(f[4], "best_score") != max(have):
            fail(f"{d} ticket {num_}: best_score is not the best s_label")
        note = check_ticket(f"{d} ticket {num_}", have, want, f[1:4], names, EPS)
        if note:
            named.append(note)
        table.append(f)
    return logits, table, named


def controls():
    """Synthetic controls of the decision checks (review round 1, R2)."""
    names = ["bug", "feature", "question", "documentation", "performance"]
    third = 1 / 3
    ok = True

    def expect(name, have, want, fields, refused):
        nonlocal ok
        import io, contextlib
        try:
            with contextlib.redirect_stderr(io.StringIO()):
                note = check_ticket("ticket", have, want, fields, names, EPS)
            got = f"accepted{' (named: ' + note + ')' if note else ''}"
            good = not refused
        except SystemExit as e:
            got, good = f"refused ({e})", refused
        ok &= good
        print(f"{'pass' if good else 'WRONG'}: {name}: {got}")

    # entailment exactly 0.5 for bug (logits [0, 0, -1000]), 1/3 elsewhere
    half = softmax_entail([0, 0, -1000])
    s = [half, third, third, third, third]
    print(f"bug's p_entail from logits [0, 0, -1000] = {half!r}")
    expect("exactly 0.5 is assigned", s, s, ["bug", "bug", "feature"], False)
    expect("Codex's forged row at the boundary", s, s, ["question", "question", "documentation"], True)
    below = [0.5 - 1e-9, third, third, third, third]
    expect("just below 0.5 abstains", below, below, ["review", "bug", "feature"], False)
    expect("just below 0.5 assigned anyway", below, below, ["bug", "bug", "feature"], True)
    tie = [0.6, 0.6, 0.1, 0.1, 0.1]
    expect("an exact tie goes to the first label", tie, tie, ["bug", "bug", "feature"], False)
    expect("an exact tie given to the second label", tie, tie, ["feature", "feature", "bug"], True)
    # a genuine rounding case: the recompute lands just below 0.5, the
    # producer's own score is exactly 0.5 (within EPS) and it assigned
    want = [0.5 - 1e-13, third, third, third, third]
    expect("a genuine rounding ambiguity at 0.5", s, want, ["bug", "bug", "feature"], False)
    # the same scores, but an unrelated label chosen: refused
    expect("an unrelated label at a rounding ambiguity", s, want, ["question", "question", "bug"], True)
    print("all decision controls behave" if ok else "CONTROLS FAILED")
    return ok


if __name__ == "__main__":
    if sys.argv[1:] == ["--controls"]:
        sys.exit(0 if controls() else 1)
    a_dir, b_dir, d2, ranges, labels = sys.argv[1:6]
    ids, numbers, names = identities(d2, ranges, labels)
    la, ta, na = validate(a_dir, ids, numbers, names)
    lb, tb, nb = validate(b_dir, ids, numbers, names)
    for n in na + nb:
        print(f"named: {n}")
    for r, (x, y) in enumerate(zip(la, lb)):
        if [f32(v) for v in x] != [f32(v) for v in y]:
            fail(f"pair {r + 1} ({ids[r][:3]}): logits differ in f32 bits")
    for x, y in zip(ta, tb):
        if x[:4] != y[:4] or [struct.pack('<d', float(v)) for v in x[4:]] != [struct.pack('<d', float(v)) for v in y[4:]]:
            fail(f"ticket {x[0]}: rows differ ({x[:4]} against {y[:4]})")
    print(f"BIT-EQUAL (each producer validated first against {len(ids)} reconstructed identities): "
          f"{len(ids)} pairs x 3 logits in f32 bits, {len(numbers)} tickets' classes and s_label in f64 bits; "
          f"the ticket tables recomputed from the logits within {EPS}")
