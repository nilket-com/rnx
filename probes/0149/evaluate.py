"""Record 0149: classification quality, reported and not gated (plans/0149
section 5). On 0139's frozen 40-ticket sample, against each annotator's
frozen labels: U7 (generation), with 0139 (embeddings) and 0148 (NLI) from
their saved tables. Coverage (parsed / 40) and accuracy over parsed; full
coverage (U7: "review", i.e. unparsed, counts as wrong; 0139 and 0148: their
`best`, as in 0148); recall by label, question first; confusion tables;
exact two-sided McNemar of U7 against 0139 and against 0148 at full
coverage; every U7 error with its raw text. Agent-labelled, not human or
user acceptance; 40 tickets, descriptive only.

  evaluate.py U7_TRIAGE U4_TSV U6_TICKETS FROZEN_DIR D2_JSON"""
import json, math, pathlib, sys

u7, u4, u6, frozen, d2 = sys.argv[1], sys.argv[2], sys.argv[3], pathlib.Path(sys.argv[4]), sys.argv[5]
names = [l.split("\t")[0] for l in (frozen / "labels.tsv").read_text().splitlines()[1:] if l]
order = ["question"] + [n for n in names if n != "question"]
sample = (frozen / "sample.tsv").read_text().split()[1:]
titles = {str(i["number"]): i["title"].strip() for i in json.load(open(d2))}


def table(path):
    lines = open(path).read().splitlines()
    head = lines[0].split("\t")
    return {l.split("\t")[0]: dict(zip(head, l.split("\t"))) for l in lines[1:]}


gen, base, nli = table(u7), table(u4), table(u6)
# full-coverage predictions: U7's class (review = wrong), the others' best
full = {"U7 (0149)": {n: gen[n]["class"] for n in sample}, "0139": {n: base[n]["best"] for n in sample},
        "0148": {n: nli[n]["best"] for n in sample}}
assigned = {"U7 (0149)": {n: gen[n]["class"] for n in sample}, "0139": {n: base[n]["triage"] for n in sample},
            "0148": {n: nli[n]["triage"] for n in sample}}


def mcnemar(b, c):
    n, k = b + c, min(b, c)
    return 1.0 if n == 0 else min(1.0, 2 * sum(math.comb(n, i) for i in range(k + 1)) / 2 ** n)


def confusion(pairs):
    print("      " + " ".join(f"{p[:5]:>6}" for p in names))
    for r in names:
        print(f"{r[:5]:>5} " + " ".join(f"{sum(1 for a, b in pairs if a == r and b == p):>6}" for p in names))


print("Agent-labelled evaluation, not human or user acceptance. 40 tickets: descriptive only.\n")
for who in ("claude", "codex"):
    lines = (frozen / f"annotations-{who}.tsv").read_text().splitlines()
    ref = dict(l.split("\t") for l in lines[1:])
    assert list(ref) == sample
    print(f"=== against {who}'s frozen labels")
    for name in ("U7 (0149)", "0139", "0148"):
        a = [n for n in sample if assigned[name][n] != "review"]
        right = sum(assigned[name][n] == ref[n] for n in a)
        fc = sum(full[name][n] == ref[n] for n in sample)
        print(f"{name}: coverage {len(a)} / 40; accuracy over assigned {right} / {len(a)}"
              f"{f' = {right / len(a):.2f}' if a else ''}; full coverage {fc} / 40 = {fc / 40:.2f}")
        rec = [f"{lab} {sum(full[name][n] == lab for n in sample if ref[n] == lab)}/{sum(ref[n] == lab for n in sample)}" for lab in order]
        print(f"  recall at full coverage: {', '.join(rec)}")
    print("  U7 confusion, full coverage (rows reference, columns prediction; review not shown):")
    confusion([(ref[n], full["U7 (0149)"][n]) for n in sample])
    for other in ("0139", "0148"):
        u = full["U7 (0149)"]
        o = full[other]
        both = sum(u[n] == ref[n] and o[n] == ref[n] for n in sample)
        only_u = sum(u[n] == ref[n] and o[n] != ref[n] for n in sample)
        only_o = sum(u[n] != ref[n] and o[n] == ref[n] for n in sample)
        print(f"paired with {other}, full coverage: both right {both}, U7 only {only_u}, {other} only {only_o}, "
              f"both wrong {40 - both - only_u - only_o}; exact McNemar p = {mcnemar(only_u, only_o):.3f}")
    print("U7 errors (including review):")
    for n in sample:
        if gen[n]["class"] != ref[n]:
            print(f"  #{n} {titles[n][:60]!r}: {ref[n]} -> {gen[n]['class']} (generated {json.loads(gen[n]['text'])!r})")
    print()
counts = {}
for r in gen.values():
    counts[r["class"]] = counts.get(r["class"], 0) + 1
print(f"U7 classes over all {len(gen)} tickets: {dict(sorted(counts.items(), key=lambda x: -x[1]))}")
