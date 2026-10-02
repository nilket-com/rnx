"""Record 0146: U5's exact gate. Each trace is validated on its own first,
against the rubric; then the two are compared, scores as f32 bits and
measures as f64 bits.

A trace, per rubric query in order: exactly 20 `cand\tQ\tj\tpath\tpid\tret\tce`
lines, j = 1 .. 20 (distinct paths, each `NNNN_*.md`; pids distinct
non-negative integers; finite scores; retrieval scores non-increasing), then
`rerank\tQ\t<a permutation of 1..20>` ordered by ce score descending, ties
to the smaller retrieval rank. After every query, one `metrics\tQ\t...` line
per query: hit@1, hit@5, recall@5 and MRR for retrieval and re-ranking,
equal to those recomputed from the ranks and the rubric's supporting
records (a record's rank is its earliest document's; recall@5 counts it
once).

Review round 1: each producer's directory also retains its retrieval
evidence, and the trace is bound to it and to the frozen corpus:
- u5-passages.json: {"paths": [...], "texts": [...]}, indexed by pid. The
  paths are exactly D1's .md files; each document's passages occur in its
  text, in order and without overlapping.
- u5-retrieval.tsv: `pid` then the rubric's ids, one row per pid, finite.
- Each query's 20 candidates are RECOMPUTED from the retained scores (a
  stable descending sort by f32 score, ties to pid order; each document's
  first passage; the first 20 documents) and must equal the trace's
  (path, pid, f32 retrieval score) exactly.
- u5-pairs.json is exactly (the rubric's query text, the candidate passage's
  retained text), in query then retrieval order.
Then every file is compared between the producers (scores as f32 bits).

  compare_u5.py SCRIPT_DIR TWIN_DIR RUBRIC D1"""
import gzip, json, math, os, struct, sys


def text(d, name):
    """A producer's file, as saved plain or gzipped (out/ keeps the large
    retained files gzipped)."""
    if os.path.exists(f"{d}/{name}.gz"):
        return gzip.open(f"{d}/{name}.gz", "rt").read()
    return open(f"{d}/{name}").read()


def fail(msg):
    sys.exit(f"FAIL: {msg}")


def num(text, what):
    try:
        v = float(text)
    except ValueError:
        fail(f"{what}: {text!r} is not a number")
    if not math.isfinite(v):
        fail(f"{what}: {text} is not finite")
    return v


def f32(v):
    return struct.pack("<f", v)


def measures(records, support):
    ranks = [records.index(r) + 1 if r in records else 0 for r in support]
    hits = [k for k in ranks if k > 0]
    first = min(hits) if hits else 0
    within = sum(1 for k in ranks if 0 < k <= 5)
    return (first == 1, 0 < first <= 5, within / len(support), 0.0 if first == 0 else 1.0 / first)


def rubric_of(path):
    out = []
    for line in open(path).read().splitlines()[1:]:
        if line:
            f = line.split("\t")
            out.append((f[0], f[2], f[3].split(" ")))
    return out


def retained(d, rubric, d1):
    """The retained passages and scores, checked against the corpus."""
    try:
        ps = json.loads(text(d, "u5-passages.json"))
        paths, texts = ps["paths"], ps["texts"]
    except (OSError, ValueError, KeyError, TypeError) as e:
        fail(f"{d}/u5-passages.json: {e!r}")
    if not (isinstance(paths, list) and isinstance(texts, list) and len(paths) == len(texts) and paths):
        fail(f"{d}/u5-passages.json: paths and texts are not equal-length lists")
    corpus = sorted(n for n in os.listdir(d1) if n.endswith(".md"))
    if sorted(set(paths)) != corpus:
        fail(f"{d}/u5-passages.json: the documents are not exactly D1's")
    at = {}
    for pid, (p, t) in enumerate(zip(paths, texts)):
        if not isinstance(t, str) or not t:
            fail(f"{d}/u5-passages.json: passage {pid} is not text")
        if p not in at:
            at[p] = (open(os.path.join(d1, p)).read(), 0)
        body, start = at[p]
        i = body.find(t, start)
        if i < 0:
            fail(f"{d}/u5-passages.json: passage {pid} does not occur in {p} after the previous one")
        at[p] = (body, i + len(t))
    lines = text(d, "u5-retrieval.tsv").splitlines()
    if not lines or lines[0].split("\t") != ["pid"] + [q for q, _, _ in rubric]:
        fail(f"{d}/u5-retrieval.tsv: the header is not pid and the rubric's ids")
    if len(lines) != 1 + len(paths):
        fail(f"{d}/u5-retrieval.tsv: {len(lines) - 1} rows for {len(paths)} passages")
    scores = []
    for pid, line in enumerate(lines[1:]):
        f = line.split("\t")
        if len(f) != 1 + len(rubric) or f[0] != str(pid):
            fail(f"{d}/u5-retrieval.tsv row {pid + 1}: want pid {pid} and {len(rubric)} scores")
        scores.append([num(x, f"{d}/u5-retrieval.tsv") for x in f[1:]])
    return paths, texts, scores


def top20(paths, scores, k):
    """Each document's first passage in a stable descending sort by f32
    score (ties to pid order), the first 20 documents."""
    order = sorted(range(len(paths)), key=lambda p: -struct.unpack("<f", f32(scores[p][k]))[0])
    seen, out = set(), []
    for p in order:
        if paths[p] not in seen:
            seen.add(paths[p])
            out.append((paths[p], p, f32(scores[p][k])))
            if len(out) == 20:
                break
    return out


def validate(d, rubric, d1):
    paths, texts, scores = retained(d, rubric, d1)
    out = validate_trace(d, [(q, s) for q, _, s in rubric])
    for k, (q, _, _) in enumerate(rubric):
        got = [(c[0], c[1], f32(c[2])) for c in out[q][0]]
        if got != top20(paths, scores, k):
            fail(f"{d}: {q}'s candidates are not the top 20 recomputed from the retained scores and passages")
    try:
        pairs = json.loads(text(d, "u5-pairs.json"))
        pq, pp = pairs["queries"], pairs["passages"]
    except (OSError, ValueError, KeyError, TypeError) as e:
        fail(f"{d}/u5-pairs.json: {e!r}")
    wq = [text for q, text, _ in rubric for _ in range(20)]
    wp = [texts[c[1]] for q, _, _ in rubric for c in out[q][0]]
    if pq != wq or pp != wp:
        fail(f"{d}/u5-pairs.json: not the rubric's queries and the candidates' retained passages, in order")
    return out, (paths, texts, [[f32(x) for x in r] for r in scores], pq, pp)


def validate_trace(d, rubric):
    path = f"{d}/u5-trace.tsv"
    lines = text(d, "u5-trace.tsv").splitlines()
    if not lines:
        fail(f"{path}: empty")
    pos = 0
    out = {}
    for q, support in rubric:
        cands = []
        for j in range(1, 21):
            if pos >= len(lines):
                fail(f"{path}: {q} has {j - 1} candidates, want 20")
            f = lines[pos].split("\t")
            if f[0] != "cand" or len(f) != 7 or f[1] != q or f[2] != str(j):
                fail(f"{path} line {pos + 1}: want candidate {j} of {q}")
            p, pid = f[3], f[4]
            if not (len(p) > 5 and p[:4].isdigit() and p[4] == "_" and p.endswith(".md")):
                fail(f"{path} line {pos + 1}: {p!r} is not a record document")
            if not pid.isdigit():
                fail(f"{path} line {pos + 1}: pid {pid!r}")
            cands.append((p, int(pid), num(f[5], f"{path} retrieval score"), num(f[6], f"{path} ce score")))
            pos += 1
        if len({c[0] for c in cands}) != 20 or len({c[1] for c in cands}) != 20:
            fail(f"{path}: {q}'s candidates repeat a document or passage")
        if any(cands[i][2] < cands[i + 1][2] for i in range(19)):
            fail(f"{path}: {q}'s retrieval order is not by score")
        if pos >= len(lines):
            fail(f"{path}: {q} has no rerank line")
        f = lines[pos].split("\t")
        if f[0] != "rerank" or len(f) != 3 or f[1] != q:
            fail(f"{path} line {pos + 1}: want {q}'s rerank line")
        try:
            order = [int(x) for x in f[2].split(",")]
        except ValueError:
            fail(f"{path}: {q}'s rerank is not integers")
        if sorted(order) != list(range(1, 21)):
            fail(f"{path}: {q}'s rerank is not a permutation of 1..20")
        want = sorted(range(1, 21), key=lambda r: (-cands[r - 1][3], r))
        if order != want:
            fail(f"{path}: {q}'s rerank is not by ce score with ties to the retrieval rank")
        pos += 1
        records = [c[0][:4] for c in cands]
        rrecords = [records[r - 1] for r in order]
        out[q] = (cands, order, measures(records, support), measures(rrecords, support))
    for q, _ in rubric:
        if pos >= len(lines):
            fail(f"{path}: no metrics line for {q}")
        f = lines[pos].split("\t")
        if f[0] != "metrics" or len(f) != 10 or f[1] != q:
            fail(f"{path} line {pos + 1}: want {q}'s metrics line")
        def parse(xs):
            if xs[0] not in ("true", "false") or xs[1] not in ("true", "false"):
                fail(f"{path}: {q}'s hit values are not booleans")
            return (xs[0] == "true", xs[1] == "true", num(xs[2], "recall@5"), num(xs[3], "MRR"))
        if (parse(f[2:6]), parse(f[6:10])) != (out[q][2], out[q][3]):
            fail(f"{path}: {q}'s metrics are not those of its ranks")
        pos += 1
    if pos != len(lines):
        fail(f"{path} line {pos + 1}: unexpected {lines[pos].split(chr(9))[0]!r}")
    return out


if __name__ == "__main__":
    rubric, d1 = rubric_of(sys.argv[3]), sys.argv[4]
    (a, ra), (b, rb) = validate(sys.argv[1], rubric, d1), validate(sys.argv[2], rubric, d1)
    for i, what in enumerate(["passage documents", "passage texts", "retained scores", "pair queries", "pair passages"]):
        if ra[i] != rb[i]:
            fail(f"the producers' {what} differ")
    for q, _, _ in rubric:
        ca, cb = a[q][0], b[q][0]
        if [(c[0], c[1], f32(c[2]), f32(c[3])) for c in ca] != [(c[0], c[1], f32(c[2]), f32(c[3])) for c in cb]:
            fail(f"{q}: candidates or scores differ")
        if a[q][1:] != b[q][1:]:
            fail(f"{q}: the re-ranking or metrics differ")
    print(f"BIT-EQUAL (each producer validated first, its candidates recomputed from its retained "
          f"retrieval evidence over D1): {len(ra[0])} passages and their {len(rubric)} query scores, "
          f"{len(rubric)} queries x 20 candidates (paths, pids, retrieval and cross-encoder scores in f32 bits), "
          f"the {len(ra[3])} pairs' texts, re-ranked orders and metrics")
