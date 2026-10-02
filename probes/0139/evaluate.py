"""Record 0139: U4's triage on the frozen sample, against each annotator.
Reported, not gated. Agent-labelled evaluations, not human acceptance.

  evaluate.py U4_TSV FROZEN_DIR D2_JSON

Per annotator: coverage (assigned / 40), accuracy over assigned sample
tickets (correct / assigned), the 5 x 5 confusion table of assigned sample
tickets only (reference rows, prediction columns), abstentions by reference
label, and every error with its title and the two top labels. Then the two
annotators' agreement and Cohen's kappa."""
import collections, json, pathlib, sys

u4, frozen, d2 = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
labels = [l.split("\t")[0] for l in (frozen / "labels.tsv").read_text().splitlines()[1:] if l]
sample = [l for l in (frozen / "sample.tsv").read_text().split()[1:]]
titles = {str(i["number"]): i["title"].strip() for i in json.load(open(d2))}
rows = {}
for l in u4.read_text().splitlines()[1:]:
    f = l.split("\t")
    rows[f[0]] = f


def annotations(path):
    lines = path.read_text().splitlines()
    assert lines[0] == "number\tlabel", f"{path}: header {lines[0]!r}"
    out = {}
    for l in lines[1:]:
        if not l:
            continue
        n, lab = l.split("\t")
        assert lab in labels, f"{path}: label {lab!r}"
        assert n not in out, f"{path}: duplicate {n}"
        out[n] = lab
    assert list(out) == sample, f"{path}: not the frozen sample in its order"
    return out


annotators = sorted(p.name[len("annotations-"):-4] for p in frozen.glob("annotations-*.tsv"))
refs = {a: annotations(frozen / f"annotations-{a}.tsv") for a in annotators}
for a in annotators:
    ref = refs[a]
    assigned = [n for n in sample if rows[n][1] != "review"]
    correct = [n for n in assigned if rows[n][1] == ref[n]]
    print(f"== against {a}'s labels (agent-labelled)")
    print(f"coverage: {len(assigned)} / {len(sample)} assigned; "
          f"accuracy over assigned: {len(correct)} / {len(assigned)}"
          + (f" = {len(correct) / len(assigned):.2f}" if assigned else ""))
    w = max(len(x) for x in labels) + 1
    print("confusion (assigned only; rows: reference, columns: prediction)")
    print(" " * w + "".join(f"{x[:5]:>7}" for x in labels))
    for r in labels:
        print(f"{r:<{w}}" + "".join(f"{sum(1 for n in assigned if ref[n] == r and rows[n][1] == c):>7}" for c in labels))
    review = collections.Counter(ref[n] for n in sample if rows[n][1] == "review")
    print("abstentions by reference label:", dict(review) or "none")
    errors = [n for n in assigned if rows[n][1] != ref[n]]
    print(f"errors ({len(errors)}):")
    for n in errors:
        print(f"  #{n} reference {ref[n]}, predicted {rows[n][1]} (second {rows[n][3]}, margin {float(rows[n][4]):.3f}): {titles[n]}")
if len(annotators) >= 2:
    a, b = annotators[:2]
    agree = sum(refs[a][n] == refs[b][n] for n in sample)
    pa = collections.Counter(refs[a].values())
    pb = collections.Counter(refs[b].values())
    po = agree / len(sample)
    pe = sum(pa[x] * pb[x] for x in labels) / len(sample) ** 2
    kappa = (po - pe) / (1 - pe) if pe < 1 else 1.0
    print(f"== {a} and {b}: agreement {agree} / {len(sample)}; Cohen's kappa {kappa:.2f}")
    for n in sample:
        if refs[a][n] != refs[b][n]:
            print(f"  #{n} {a} {refs[a][n]}, {b} {refs[b][n]}: {titles[n]}")
