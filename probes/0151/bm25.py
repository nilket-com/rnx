"""Record 0151: B, the untuned BM25 baseline, by the contract frozen in
plans/0151 section 2 (and carried unchanged to 0152).

- Units: passages (pid order); a document scores the maximum over its
  passages.
- Tokens: lowercase runs of [a-z0-9]; no stemming, no stopwords.
- Okapi BM25, k1 = 1.2, b = 0.75; N, each term's n and the average length
  are counted over passages; idf = ln(1 + (N - n + 0.5) / (n + 0.5)).
- A passage's score: the sum over the query's DISTINCT terms of
  idf * tf * (k1 + 1) / (tf + k1 * (1 - b + b * dl / avgdl)), in IEEE f64,
  accumulated in ascending lexicographic order of those terms.
- An all-token-free corpus (avgdl 0) and an empty query give every passage
  0; rankings then follow the tie rule.
- Ties: a document's passage ties go to the lower pid; documents tie to the
  lower pid of their best passage.

  bm25.py --controls
  (as a module: rank(paths, texts, query, depth) -> [(path, best_pid, score)])"""
import math, re, sys

K1, B = 1.2, 0.75


def tokens(text):
    return re.findall(r"[a-z0-9]+", text.lower())


class Index:
    def __init__(self, texts):
        self.tf = []
        self.dl = []
        df = {}
        for t in texts:
            counts = {}
            for w in tokens(t):
                counts[w] = counts.get(w, 0) + 1
            self.tf.append(counts)
            self.dl.append(sum(counts.values()))
            for w in counts:
                df[w] = df.get(w, 0) + 1
        self.n = len(texts)
        self.df = df
        self.avgdl = sum(self.dl) / self.n if self.n else 0.0

    def scores(self, query):
        terms = sorted(set(tokens(query)))
        out = [0.0] * self.n
        if self.avgdl == 0.0 or not terms:
            return out
        for p in range(self.n):
            s = 0.0
            for w in terms:
                tf = self.tf[p].get(w, 0)
                if tf == 0:
                    continue
                n = self.df[w]
                idf = math.log(1 + (self.n - n + 0.5) / (n + 0.5))
                s += idf * tf * (K1 + 1) / (tf + K1 * (1 - B + B * self.dl[p] / self.avgdl))
            out[p] = s
        return out


def rank(paths, index, query, depth):
    """Documents by their best passage's score, descending; a document's
    best passage is its lowest-pid passage among equals, and documents tie
    to the lower pid of that passage. Returns the first `depth`."""
    s = index.scores(query)
    best = {}
    for pid, (p, v) in enumerate(zip(paths, s)):
        if p not in best or v > best[p][1]:
            best[p] = (pid, v)
    docs = sorted(best.items(), key=lambda kv: (-kv[1][1], kv[1][0]))
    return [(p, pid, v) for p, (pid, v) in docs[:depth]]


def controls():
    ok = True

    def expect(name, got, want, tol=1e-12):
        nonlocal ok
        good = (abs(got - want) <= tol) if isinstance(want, float) else got == want
        ok &= good
        print(f"{'pass' if good else 'WRONG'}: {name}: {got!r}")

    texts = ["the cat sat", "the dog sat down", "a cat and a cat", "nothing here"]
    paths = ["a.md", "a.md", "b.md", "c.md"]
    ix = Index(texts)
    # hand-computed: N 4, avgdl (3 + 4 + 5 + 2) / 4 = 3.5; "cat" n = 2
    idf = math.log(1 + (4 - 2 + 0.5) / (2 + 0.5))
    p0 = idf * 1 * 2.2 / (1 + 1.2 * (0.25 + 0.75 * 3 / 3.5))
    p2 = idf * 2 * 2.2 / (2 + 1.2 * (0.25 + 0.75 * 5 / 3.5))
    s = ix.scores("cat")
    expect("passage 0, 'cat', hand-computed", s[0], p0)
    expect("passage 2, 'cat' twice, hand-computed", s[2], p2)
    expect("a passage without the term", s[1], 0.0)
    expect("a repeated query term counts once", ix.scores("cat cat CAT"), s)
    expect("case and punctuation are ignored", ix.scores("Cat!"), s)
    expect("an empty query scores 0 everywhere", ix.scores(""), [0.0] * 4)
    expect("a token-free passage scores 0", Index(["cat", "!!!"]).scores("cat")[1], 0.0)
    expect("an all-token-free corpus scores 0", Index(["!!", "--"]).scores("cat"), [0.0, 0.0])
    # documents: a.md's best passage is pid 0 or 1; b.md wins on "cat"
    expect("ranking by the best passage", [d for d, _, _ in rank(paths, ix, "cat", 3)], ["b.md", "a.md", "c.md"])
    # ties: two documents with equal scores go to the lower pid
    tie = Index(["x y", "x y", "z"])
    expect("document ties to the lower pid", rank(["q.md", "p.md", "r.md"], tie, "x", 3)[:2],
           [("q.md", 0, tie.scores("x")[0]), ("p.md", 1, tie.scores("x")[1])])
    expect("an empty query ranks by pid", [d for d, _, _ in rank(["q.md", "p.md", "r.md"], tie, "", 3)], ["q.md", "p.md", "r.md"])
    print("all BM25 controls behave" if ok else "CONTROLS FAILED")
    return ok


if __name__ == "__main__":
    sys.exit(0 if controls() else 1)
