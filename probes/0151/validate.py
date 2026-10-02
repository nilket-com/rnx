"""Record 0151: U8's gate. Each producer is validated on its own first,
against the frozen corpus and its own retained evidence; then the two are
compared exactly.

Per producer:
- u8-passages.json ({"paths", "texts"} by pid): the documents are exactly
  D1's .md files, and each document's passages occur in its text in order,
  without overlapping;
- u8-retrieval.tsv: `pid` and the development query ids, one finite row
  per pid;
- u8-pool.tsv: RECOMPUTED from the retained scores and passages: per query,
  pids in a stable descending sort by f32 score (ties to the lower pid);
  the first 40 distinct documents in that order (candidate ranks 1..40),
  each with its first 3 passages in that order (passage ranks 1..3; all
  when fewer); rows in query, candidate, passage order. The producer's
  rows must equal that, with the retrieval score in f32 bits, and a finite
  cross-encoder score each.
Then, between producers: the passages, every retrieval score and every
pool row, scores in f32 bits.

  validate.py SCRIPT_DIR TWIN_DIR DEV_TSV D1"""
import gzip, json, math, os, struct, sys

CANDIDATES, PASSAGES = 40, 3


def fail(msg):
    sys.exit(f"FAIL: {msg}")


def text(d, name):
    if os.path.exists(f"{d}/{name}.gz"):
        return gzip.open(f"{d}/{name}.gz", "rt").read()
    return open(f"{d}/{name}").read()


def f32(x):
    return struct.pack("<f", x)


def num(t, what):
    try:
        v = float(t)
    except ValueError:
        fail(f"{what}: {t!r} is not a number")
    if not math.isfinite(v):
        fail(f"{what}: {t} is not finite")
    return v


def queries(dev):
    out = []
    for line in open(dev).read().splitlines()[1:]:
        if line:
            f = line.split("\t")
            out.append((f[0], f[2], f[3].split(" ")))
    return out


def retained(d, ids, d1):
    try:
        ps = json.loads(text(d, "u8-passages.json"))
        paths, texts = ps["paths"], ps["texts"]
    except (OSError, ValueError, KeyError, TypeError) as e:
        fail(f"{d}/u8-passages.json: {e!r}")
    if not (isinstance(paths, list) and isinstance(texts, list) and len(paths) == len(texts) and paths):
        fail(f"{d}/u8-passages.json: paths and texts are not equal-length lists")
    if sorted(set(paths)) != sorted(n for n in os.listdir(d1) if n.endswith(".md")):
        fail(f"{d}/u8-passages.json: the documents are not exactly D1's")
    at = {}
    for pid, (p, t) in enumerate(zip(paths, texts)):
        if not isinstance(t, str) or not t:
            fail(f"{d}: passage {pid} is not text")
        if p not in at:
            at[p] = (open(os.path.join(d1, p)).read(), 0)
        body, start = at[p]
        i = body.find(t, start)
        if i < 0:
            fail(f"{d}: passage {pid} does not occur in {p} after the previous one")
        at[p] = (body, i + len(t))
    lines = text(d, "u8-retrieval.tsv").splitlines()
    if not lines or lines[0].split("\t") != ["pid"] + ids:
        fail(f"{d}/u8-retrieval.tsv: the header is not pid and the development ids")
    if len(lines) != 1 + len(paths):
        fail(f"{d}/u8-retrieval.tsv: {len(lines) - 1} rows for {len(paths)} passages")
    scores = []
    for pid, line in enumerate(lines[1:]):
        f = line.split("\t")
        if len(f) != 1 + len(ids) or f[0] != str(pid):
            fail(f"{d}/u8-retrieval.tsv row {pid + 1}: want pid {pid} and {len(ids)} scores")
        scores.append([struct.unpack("<f", f32(num(x, "retrieval")))[0] for x in f[1:]])
    return paths, texts, scores


def expected_pool(paths, scores, ids):
    """[(query, cand, path, prank, pid, retrieval f32 bits)] in order."""
    rows = []
    for k, q in enumerate(ids):
        order = sorted(range(len(paths)), key=lambda p: -scores[p][k])  # stable: ties keep pid order
        rank, slots = {}, []
        for p in order:
            doc = paths[p]
            if doc not in rank:
                if len(slots) == CANDIDATES:
                    continue
                rank[doc] = len(slots)
                slots.append((doc, []))
            if len(slots[rank[doc]][1]) < PASSAGES:
                slots[rank[doc]][1].append(p)
        for c, (doc, ps) in enumerate(slots):
            for j, p in enumerate(ps):
                rows.append((q, c + 1, doc, j + 1, p, f32(scores[p][k])))
    return rows


def validate(d, qs, d1):
    ids = [q for q, _, _ in qs]
    paths, texts, scores = retained(d, ids, d1)
    want = expected_pool(paths, scores, ids)
    lines = text(d, "u8-pool.tsv").splitlines()
    if not lines or lines[0] != "query\tcand\tpath\tprank\tpid\tretrieval\tce":
        fail(f"{d}/u8-pool.tsv: header")
    rows = lines[1:]
    if len(rows) != len(want):
        fail(f"{d}/u8-pool.tsv: {len(rows)} rows, the recomputed pool has {len(want)}")
    ce = []
    for r, (line, w) in enumerate(zip(rows, want)):
        f = line.split("\t")
        if len(f) != 7:
            fail(f"{d}/u8-pool.tsv row {r + 1}: {len(f)} fields")
        try:
            have = (f[0], int(f[1]), f[2], int(f[3]), int(f[4]), f32(num(f[5], "retrieval")))
        except ValueError:
            fail(f"{d}/u8-pool.tsv row {r + 1}: not integers")
        if have != w:
            fail(f"{d}/u8-pool.tsv row {r + 1}: {have[:5]} is not the recomputed {w[:5]} (or its retrieval score differs)")
        ce.append(struct.unpack("<f", f32(num(f[6], "ce")))[0])
    return paths, texts, scores, want, ce


if __name__ == "__main__":
    a_dir, b_dir, dev, d1 = sys.argv[1:5]
    qs = queries(dev)
    a, b = validate(a_dir, qs, d1), validate(b_dir, qs, d1)
    if a[0] != b[0] or a[1] != b[1]:
        fail("the producers' passages differ")
    if [[f32(x) for x in r] for r in a[2]] != [[f32(x) for x in r] for r in b[2]]:
        fail("the producers' retrieval scores differ")
    if a[3] != b[3]:
        fail("the producers' pools differ")
    if [f32(x) for x in a[4]] != [f32(x) for x in b[4]]:
        fail("the producers' cross-encoder scores differ")
    print(f"BIT-EQUAL (each producer validated first, its pool recomputed from its retained evidence over D1): "
          f"{len(a[0])} passages and their {len(qs)} query scores, {len(a[3])} pool pairs "
          f"({CANDIDATES} candidates x up to {PASSAGES} passages) with cross-encoder scores in f32 bits")
