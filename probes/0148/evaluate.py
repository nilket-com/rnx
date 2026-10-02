"""Record 0148: classification quality, reported and not gated (plans/0148
section 7). On 0139's frozen 40-ticket sample, against each annotator's
frozen labels, for U6 (NLI) and 0139's saved U4 table alike:

- coverage (assigned / 40) and accuracy over assigned;
- full coverage, no abstention: NLI's best label against 0139's `best`;
- recall by label at full coverage, `question` first;
- the confusion tables (assigned, and full coverage);
- the paired full-coverage comparison with an exact two-sided McNemar
  p-value on the discordant tickets (descriptive: 40 tickets);
- every NLI error under the frozen rule, with the title and the two best
  s_label values; abstentions by reference label.
Agent-labelled evaluations, not human or user acceptance.

  evaluate.py U6_TICKETS U4_TSV FROZEN_DIR D2_JSON"""
import json, math, pathlib, sys

u6, u4, frozen, d2 = sys.argv[1], sys.argv[2], pathlib.Path(sys.argv[3]), sys.argv[4]
names = [l.split("\t")[0] for l in (frozen / "labels.tsv").read_text().splitlines()[1:] if l]
order = ["question"] + [n for n in names if n != "question"]
sample = (frozen / "sample.tsv").read_text().split()[1:]
titles = {str(i["number"]): i["title"].strip() for i in json.load(open(d2))}


def table(path):
    lines = open(path).read().splitlines()
    head = lines[0].split("\t")
    return {l.split("\t")[0]: dict(zip(head, l.split("\t"))) for l in lines[1:]}


nli, base = table(u6), table(u4)
annotators = {}
for who in ("claude", "codex"):
    lines = (frozen / f"annotations-{who}.tsv").read_text().splitlines()
    assert lines[0] == "number\tlabel"
    annotators[who] = dict(l.split("\t") for l in lines[1:])
    assert list(annotators[who]) == sample


def mcnemar(b, c):
    n, k = b + c, min(b, c)
    return 1.0 if n == 0 else min(1.0, 2 * sum(math.comb(n, i) for i in range(k + 1)) / 2 ** n)


def confusion(pairs):
    print("      " + " ".join(f"{p[:5]:>6}" for p in names))
    for r in names:
        print(f"{r[:5]:>5} " + " ".join(f"{sum(1 for a, b in pairs if a == r and b == p):>6}" for p in names))


print("Agent-labelled evaluation, not human or user acceptance. 40 tickets: descriptive only.\n")
for who, ref in annotators.items():
    print(f"=== against {who}'s frozen labels")
    for name, t in (("NLI (0148)", nli), ("0139 baseline", base)):
        assigned = [n for n in sample if t[n]["triage"] != "review"]
        right = sum(t[n]["triage"] == ref[n] for n in assigned)
        full = sum(t[n]["best"] == ref[n] for n in sample)
        print(f"{name}: coverage {len(assigned)} / 40; accuracy over assigned {right} / {len(assigned)}"
              f"{f' = {right / len(assigned):.2f}' if assigned else ''}; full coverage {full} / 40 = {full / 40:.2f}")
        recall = []
        for lab in order:
            refs = [n for n in sample if ref[n] == lab]
            recall.append(f"{lab} {sum(t[n]['best'] == lab for n in refs)}/{len(refs)}")
        print(f"  recall at full coverage: {', '.join(recall)}")
        abst = {}
        for n in sample:
            if t[n]["triage"] == "review":
                abst[ref[n]] = abst.get(ref[n], 0) + 1
        print(f"  abstentions by reference label: {abst or 'none'}")
        print("  confusion, assigned (rows reference, columns prediction):")
        confusion([(ref[n], t[n]["triage"]) for n in assigned])
        print("  confusion, full coverage:")
        confusion([(ref[n], t[n]["best"]) for n in sample])
    both = sum(nli[n]["best"] == ref[n] and base[n]["best"] == ref[n] for n in sample)
    only_nli = sum(nli[n]["best"] == ref[n] and base[n]["best"] != ref[n] for n in sample)
    only_base = sum(nli[n]["best"] != ref[n] and base[n]["best"] == ref[n] for n in sample)
    neither = 40 - both - only_nli - only_base
    print(f"paired, full coverage: both right {both}, NLI only {only_nli}, 0139 only {only_base}, both wrong {neither}; "
          f"exact McNemar p = {mcnemar(only_nli, only_base):.3f}")
    print("NLI errors under the frozen rule (assigned tickets):")
    for n in sample:
        if nli[n]["triage"] not in ("review", ref[n]):
            s = sorted(((float(nli[n][l]), l) for l in names), reverse=True)
            print(f"  #{n} {titles[n][:60]!r}: {ref[n]} -> {nli[n]['triage']} ({s[0][1]} {s[0][0]:.3f}, {s[1][1]} {s[1][0]:.3f})")
    print()
counts = {}
for r in nli.values():
    counts[r["triage"]] = counts.get(r["triage"], 0) + 1
print(f"NLI classes over all {len(nli)} tickets: {dict(sorted(counts.items(), key=lambda x: -x[1]))}")
