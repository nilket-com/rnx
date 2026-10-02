"""Record 0151: the frozen development/holdout split, replayable.

From the union of dev.tsv and holdout.tsv (70 tasks, ids T01..T70), this
recomputes each task's kind (0147's rule, probes/0147/kinds.py's code) and
the split, then requires both files to be exactly what the rule produces,
byte for byte (rows in id order within each file).

The rule:
- stratum = (records, kind), records "multi" (two or more supporting
  records) or "single"; strata ordered by the Python string order of
  (records, kind): multi/direct, multi/paraphrase, single/direct,
  single/paraphrase;
- within a stratum, tasks ordered by the SHA-256 hex digest of the exact id
  string ("T01", ...), ascending;
- over that whole ordered sequence, position i (0-based) is development iff
  floor((i + 1) * 30 / 70) > floor(i * 30 / 70) (an integer accumulator
  starting at 0, selecting each position where it crosses an integer); the
  other 40 are the holdout.

Refuses duplicates, omissions, a count other than 70 (30 + 40), any id
outside T01..T70, and any kind that differs from the rule's.

  split.py D1"""
import hashlib, os, re, sys

here = os.path.dirname(os.path.abspath(__file__))
d1 = sys.argv[1]
STOP = set("what which when where does with that this from have into they their there about without "
           "how why can could each only been after still more than then them make made just over long time".split())


def words(t):
    return {w[:5] for w in re.findall(r"[a-z0-9]+", t.lower()) if len(w) >= 4 and w not in STOP}


def fail(msg):
    sys.exit(f"FAIL: {msg}")


HEAD = "query\tid\ttext\tsupporting_records\tkind"
rows = []
for name in ("dev", "holdout"):
    lines = open(f"{here}/{name}.tsv").read().splitlines()
    if not lines or lines[0] != HEAD:
        fail(f"{name}.tsv: header")
    rows += [l.split("\t") for l in lines[1:]]
ids = [r[0] for r in rows]
if len(rows) != 70 or sorted(ids) != [f"T{i:02d}" for i in range(1, 71)]:
    fail(f"want exactly T01..T70 once each; have {len(rows)} rows, duplicates "
         f"{sorted({i for i in ids if ids.count(i) > 1})}")
names = sorted(n for n in os.listdir(d1) if n.endswith(".md"))
heads = {n: open(os.path.join(d1, n)).readline() for n in names}
df = {}
for n in names:
    for w in words(heads[n]):
        df[w] = df.get(w, 0) + 1
common = {w for w, c in df.items() if c > 0.05 * len(names)}
by_id = {}
for q, slug, text, support, kind in rows:
    recs = support.split(" ")
    if not all(any(n[:4] == r for n in names) for r in recs):
        fail(f"{q}: a supporting record outside D1")
    tw = set().union(*(words(heads[n]) for n in names if n[:4] in recs)) - common
    want = "direct" if words(text) & tw else "paraphrase"
    if kind != want:
        fail(f"{q}: kind {kind}, the rule gives {want}")
    by_id[q] = ("multi" if len(recs) > 1 else "single", kind, [q, slug, text, support, kind])
order = sorted(by_id, key=lambda q: (by_id[q][0], by_id[q][1], hashlib.sha256(q.encode()).hexdigest()))
dev = {q for i, q in enumerate(order) if (i + 1) * 30 // 70 > i * 30 // 70}
for name, chosen in (("dev", sorted(dev)), ("holdout", sorted(set(by_id) - dev))):
    want = HEAD + "\n" + "".join("\t".join(by_id[q][2]) + "\n" for q in chosen)
    if open(f"{here}/{name}.tsv").read() != want:
        fail(f"{name}.tsv is not the rule's {name} set, in id order")
counts = {}
for q in by_id:
    key = ("dev" if q in dev else "holdout", by_id[q][0], by_id[q][1])
    counts[key] = counts.get(key, 0) + 1
print(f"split: both files are exactly the rule's (dev {len(dev)}, holdout {70 - len(dev)}); strata {dict(sorted(counts.items()))}")
