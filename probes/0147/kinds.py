"""Record 0147: recompute each rubric query's `kind` and check it matches.

A query is `direct` when it shares a distinctive content word with the
first-line title of any of its supporting records' D1 files; otherwise
`paraphrase`. Words are lowercase runs of letters and digits of 4 or more
characters, minus a fixed stoplist, compared by their first 5 characters.
A word is distinctive when at most 5% of D1's file titles contain it. The
labels are descriptive strata only; nothing is tuned on them.

  kinds.py RUBRIC D1"""
import os, re, sys

STOP = set("what which when where does with that this from have into they their there about without "
           "how why can could each only been after still more than then them make made just over long time".split())


def words(t):
    return {w[:5] for w in re.findall(r"[a-z0-9]+", t.lower()) if len(w) >= 4 and w not in STOP}


rubric, d1 = sys.argv[1], sys.argv[2]
names = sorted(n for n in os.listdir(d1) if n.endswith(".md"))
heads = {n: open(os.path.join(d1, n)).readline() for n in names}
df = {}
for n in names:
    for w in words(heads[n]):
        df[w] = df.get(w, 0) + 1
common = {w for w, c in df.items() if c > 0.05 * len(names)}
bad = 0
for line in open(rubric).read().splitlines()[1:]:
    q, _, text, support, kind = line.split("\t")
    records = support.split(" ")
    if not all(any(n[:4] == r for n in names) for r in records):
        sys.exit(f"FAIL: {q} names a record outside D1")
    title_words = set().union(*(words(heads[n]) for n in names if n[:4] in records)) - common
    want = "direct" if words(text) & title_words else "paraphrase"
    if kind != want:
        bad += 1
        print(f"{q}: labelled {kind}, rule says {want}")
print("kinds: all match the rule" if not bad else f"kinds: {bad} differ")
sys.exit(1 if bad else 0)
