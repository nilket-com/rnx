"""Record 0153: the producers' gate (plans/0153 section 7). Each producer is
validated on its own, against the issues file, the split and its own
evidence; then the two are compared exactly (f32 bits).

Per producer (directory with t3-passages.json, t3-status.tsv,
t3-embeddings.tsv, t3-cosines.tsv, t3-nli.tsv):
- passages: tickets exactly the split, in order; each ticket's passages occur
  in title + "\\n" + body, in order, without overlapping, and are non-empty;
- status (NLI): one row per ticket in order, `ok`, `no_passages` or
  `nli_refused` (5a); `no_passages` exactly when a ticket has no passages;
  `nli_refused` exactly when one of the ticket's (passage, description)
  pairs is over 512 tokens by the PINNED NLI tokenizer.json (no truncation,
  no padding), recomputed here (review R2);
- embedding status: one row per ticket in order, `ok`, `no_passages`,
  `zero_norm` or `nonfinite_embedding` (5a, review R1); `no_passages`
  exactly when a ticket has no passages;
- embeddings and cosines: one row per ticket whose embedding status is `ok`,
  in order, 384 and 5 finite values; each embedding's norm within 1e-5 of 1
  (a non-finite value where an embedding is valid is a numerical fault and
  fails here, 5a);
- NLI: for each `ok` ticket, exactly passages x labels rows in passage-major,
  label order, three finite logits each; none for other tickets.

  validate.py SCRIPT_DIR TWIN_DIR ISSUES_JSON SPLIT_TSV LABELS_TSV NLI_TOKENIZER_JSON
(it needs the `tokenizers` package: the HF Python)"""
import json, math, struct, sys

STATUSES = {"ok", "no_passages", "nli_refused"}
EMBEDDING = {"ok", "no_passages", "zero_norm", "nonfinite_embedding"}
LIMIT = 512


def fail(msg):
    sys.exit(f"FAIL: {msg}")


def f32(t, what):
    try:
        v = float(t)
    except ValueError:
        fail(f"{what}: {t!r} is not a number")
    if not math.isfinite(v):
        fail(f"{what}: {t} is not finite")
    return struct.unpack("<f", struct.pack("<f", v))[0]


def bits(x):
    return struct.pack("<f", x)


def split_numbers(path):
    lines = open(path).read().splitlines()
    if not lines or lines[0] != "number":
        fail(f"{path}: header")
    return [int(l) for l in lines[1:] if l]


def labels(path):
    return [l.split("\t")[0] for l in open(path).read().splitlines()[1:] if l]


def descriptions(path):
    return [l.split("\t")[1] for l in open(path).read().splitlines()[1:] if l]


def nli_tokenizer(path):
    import tokenizers
    tok = tokenizers.Tokenizer.from_file(path)
    tok.no_truncation()
    tok.no_padding()
    return tok


def table(d, name, header):
    try:
        lines = open(f"{d}/{name}").read().splitlines()
    except OSError as e:
        fail(f"{d}/{name}: {e!r}")
    if not lines or lines[0] != header:
        fail(f"{d}/{name}: header")
    return [l.split("\t") for l in lines[1:]]


def validate(d, issues, numbers, names, descs, tok):
    """Returns (passages, NLI status, embedding status, E, C, L)."""
    k = len(names)
    try:
        v = json.load(open(f"{d}/t3-passages.json"))
        tickets, passages = v["tickets"], v["passages"]
    except (OSError, ValueError, KeyError, TypeError) as e:
        fail(f"{d}/t3-passages.json: {e!r}")
    if tickets != numbers or not isinstance(passages, list) or len(passages) != len(numbers):
        fail(f"{d}/t3-passages.json: the tickets are not exactly the split, in order")
    for n, ps in zip(numbers, passages):
        text = issues[n]["title"] + "\n" + issues[n]["body"]
        at = 0
        if not isinstance(ps, list):
            fail(f"{d}: ticket {n}'s passages are not a list")
        for j, p in enumerate(ps):
            if not isinstance(p, str) or not p:
                fail(f"{d}: ticket {n} passage {j} is not text")
            i = text.find(p, at)
            if i < 0:
                fail(f"{d}: ticket {n} passage {j} does not occur in its text after the previous one")
            at = i + len(p)
    status = table(d, "t3-status.tsv", "ticket\tstatus")
    if [r[0] for r in status] != [str(n) for n in numbers] or any(len(r) != 2 or r[1] not in STATUSES for r in status):
        fail(f"{d}/t3-status.tsv: not one valid status per ticket in order")
    status = {n: r[1] for n, r in zip(numbers, status)}
    for n, ps in zip(numbers, passages):
        if (status[n] == "no_passages") != (len(ps) == 0):
            fail(f"{d}: ticket {n} is {status[n]} with {len(ps)} passages")
        if ps:
            over = max(len(e.ids) for e in tok.encode_batch([(p, h) for p in ps for h in descs])) > LIMIT
            if (status[n] == "nli_refused") != over:
                fail(f"{d}: ticket {n} is {status[n]}, but its longest pair is "
                     f"{'over' if over else 'within'} {LIMIT} tokens by the pinned tokenizer")
    est = table(d, "t3-embedding-status.tsv", "ticket\tembedding")
    if [r[0] for r in est] != [str(n) for n in numbers] or any(len(r) != 2 or r[1] not in EMBEDDING for r in est):
        fail(f"{d}/t3-embedding-status.tsv: not one valid embedding status per ticket in order")
    estatus = {n: r[1] for n, r in zip(numbers, est)}
    for n, ps in zip(numbers, passages):
        if (estatus[n] == "no_passages") != (len(ps) == 0):
            fail(f"{d}: ticket {n}'s embedding is {estatus[n]} with {len(ps)} passages")
    with_p = [n for n in numbers if estatus[n] == "ok"]
    emb = table(d, "t3-embeddings.tsv", "ticket\t" + "\t".join(f"e{c}" for c in range(384)))
    cos = table(d, "t3-cosines.tsv", "ticket\t" + "\t".join(names))
    if [r[0] for r in emb] != [str(n) for n in with_p] or [r[0] for r in cos] != [str(n) for n in with_p]:
        fail(f"{d}: embedding or cosine rows are not exactly the tickets whose embedding is ok, in order")
    E, C = {}, {}
    for r in emb:
        if len(r) != 385:
            fail(f"{d}/t3-embeddings.tsv ticket {r[0]}: {len(r) - 1} values")
        E[int(r[0])] = [f32(x, f"embedding {r[0]}") for x in r[1:]]
        norm = math.sqrt(sum(x * x for x in E[int(r[0])]))
        if abs(norm - 1) > 1e-5:
            fail(f"{d}: ticket {r[0]}'s embedding has norm {norm}")
    for r in cos:
        if len(r) != 1 + k:
            fail(f"{d}/t3-cosines.tsv ticket {r[0]}: {len(r) - 1} values")
        C[int(r[0])] = [f32(x, f"cosine {r[0]}") for x in r[1:]]
    nli = table(d, "t3-nli.tsv", "ticket\tpassage\tlabel\tcontradiction\tentailment\tneutral")
    want = [(str(n), str(p), names[j]) for n, ps in zip(numbers, passages) if status[n] == "ok"
            for p in range(len(ps)) for j in range(k)]
    if [tuple(r[:3]) for r in nli] != want or any(len(r) != 6 for r in nli):
        fail(f"{d}/t3-nli.tsv: rows are not exactly each ok ticket's passages x labels, in order")
    L = [(int(r[0]), int(r[1]), r[2], [f32(x, f"logit {r[0]}") for x in r[3:]]) for r in nli]
    return passages, status, estatus, E, C, L


if __name__ == "__main__":
    a_dir, b_dir, issues_path, split_path, labels_path, tok_path = sys.argv[1:7]
    issues = {i["number"]: i for i in json.load(open(issues_path))}
    numbers = split_numbers(split_path)
    names = labels(labels_path)
    descs = descriptions(labels_path)
    tok = nli_tokenizer(tok_path)
    a = validate(a_dir, issues, numbers, names, descs, tok)
    b = validate(b_dir, issues, numbers, names, descs, tok)
    if a[0] != b[0]:
        fail("the producers' passages differ")
    if a[1] != b[1] or a[2] != b[2]:
        fail("the producers' statuses differ")
    for what, x, y in (("embeddings", a[3], b[3]), ("cosines", a[4], b[4])):
        if x.keys() != y.keys() or any([bits(v) for v in x[n]] != [bits(v) for v in y[n]] for n in x):
            fail(f"the producers' {what} differ")
    if [r[:3] for r in a[5]] != [r[:3] for r in b[5]] or any([bits(v) for v in r[3]] != [bits(v) for v in s[3]] for r, s in zip(a[5], b[5])):
        fail("the producers' NLI logits differ")
    counts = {s: sum(1 for v in a[1].values() if v == s) for s in sorted(STATUSES)}
    ecounts = {s: sum(1 for v in a[2].values() if v == s) for s in sorted(EMBEDDING)}
    print(f"BIT-EQUAL (each producer validated first; refusals bound to the pinned tokenizer): {len(numbers)} tickets, "
          f"{sum(len(p) for p in a[0])} passages, {len(a[3])} embeddings x 384 and cosines x {len(names)}, "
          f"{len(a[5])} NLI pairs x 3 logits, in f32 bits; NLI statuses {counts}; embedding statuses {ecounts}")
