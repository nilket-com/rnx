"""Record 0136: hit@1 and hit@5 of a U1 output on the frozen rubric.

  score.py U1_TSV RUBRIC_TSV

A query's hit@k is whether any of its top k documents is one of its
supporting records (the path's first four characters)."""
import sys

rows = [l.split("\t") for l in open(sys.argv[1]).read().splitlines()[1:] if l]
support = {}
for l in open(sys.argv[2]).read().splitlines()[1:]:
    if l:
        f = l.split("\t")
        support[f[0]] = set(f[3].split())
h1 = h5 = 0
for q, want in support.items():
    ranked = sorted((int(r[1]), r[2][:4]) for r in rows if r[0] == q)
    recs = [rec for _, rec in ranked]
    first = next((i + 1 for i, rec in enumerate(recs) if rec in want), None)
    h1 += first == 1
    h5 += first is not None and first <= 5
    print(f"{q}: first supporting record at rank {first}, top: {' '.join(recs)}")
print(f"hit@1 {h1} of {len(support)}, hit@5 {h5} of {len(support)}")
