"""Record 0149: U7's gate. Each producer's retained files are validated on
their own first (plans/0149 section 4a), then the two are compared exactly.

Per producer:
- the rows: u7-triage.tsv has exactly the 252 D2 tickets in D2 order,
  u7-summaries.tsv exactly the 40 sample tickets in sample order, with exact
  widths;
- the prompts: reconstructed from verified D2 (title + "\\n" + body), 0136's
  passage-0 byte range, the frozen labels and the frozen prompt text, built
  into Qwen's template, tokenized with the pinned tokenizer.json, and equal
  in count to the producer's;
- the steps (u7-steps.tsv): exactly one per generated id, k = 0, 1, ... in
  order, position p + k, five distinct in-range ids, finite logits in
  descending order (an exact tie ordered by id), the chosen id equal to the
  top-1 id and to the row's id at k;
- stopping: ids include a terminal EOS (151645 or 151643) only when the stop
  is "eos", with no EOS earlier; a "length" stop has exactly max_new_tokens
  ids and no EOS;
- the text: the pinned tokenizer's decode of the ids without the terminal
  EOS, special tokens skipped; the class: the frozen parse of the text.
Then, between producers: every generated id, and every top-5 id and logit
(in f32 bits).

  validate.py SCRIPT_DIR TWIN_DIR D2_JSON RANGES LABELS SAMPLE TOKENIZER_JSON
  validate.py --controls ...   (see validate_controls.py)"""
import json, math, struct, sys
import tokenizers

EOS = (151645, 151643)
TRIAGE_SYSTEM = "You are a triage assistant for the issue tracker of Rune, a scripting language. Answer with exactly one word."
SUMMARY_SYSTEM = "You are an assistant that summarizes issue tracker tickets."
LIMITS = {"triage": 8, "summary": 48}


def fail(msg):
    sys.exit(f"FAIL: {msg}")


def template(system, user):
    return f"<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n"


def inputs(d2, ranges, labels, sample):
    """The ordered chats: [(task, ticket, system, user)], the labels, the
    sample. Built from the verified inputs alone."""
    issues = json.load(open(d2))
    texts = {f"d2:#{i['number']}": (i["title"] + "\n" + i["body"]).encode() for i in issues}
    first = {}
    for line in open(ranges).read().splitlines()[1:]:
        src, k, s, e = line.split("\t")
        if src.startswith("d2:") and k == "0":
            first[src] = texts[src][int(s):int(e)].decode()
    names, descs = [], []
    for line in open(labels).read().splitlines()[1:]:
        if line:
            n, d = line.split("\t")
            names.append(n)
            descs.append(d)
    sample = [l for l in open(sample).read().split()[1:]]
    chats = []
    for i in issues:
        n = str(i["number"])
        user = "Which one category fits this ticket best?\n" + "".join(f"{a}: {b}\n" for a, b in zip(names, descs))
        user += f"\nTicket:\n{first['d2:#' + n]}\n\nAnswer with one word: bug, feature, question, documentation or performance."
        chats.append(("triage", n, TRIAGE_SYSTEM, user))
    for n in sample:
        chats.append(("summary", n, SUMMARY_SYSTEM,
                      "Summarize this ticket in one sentence of at most 20 words.\n\nTicket:\n" + first["d2:#" + n]))
    return chats, names


def parse(text, names):
    t = text.strip().lower()
    while t and t[-1] in ".,:;!":
        t = t[:-1]
    return t if t in names else "review"


def f32(x):
    return struct.pack("<f", x)


def number(text, what):
    try:
        v = float(text)
    except ValueError:
        fail(f"{what}: {text!r} is not a number")
    if not math.isfinite(v):
        fail(f"{what}: {text} is not finite")
    return struct.unpack("<f", f32(v))[0]


def check_steps(where, ids, p, steps, vocab):
    """One generation's steps against its row (section 4a)."""
    if len(steps) != len(ids):
        fail(f"{where}: {len(steps)} steps for {len(ids)} ids")
    for k, st in enumerate(steps):
        if st["k"] != k or st["p"] != p or st["position"] != p + k:
            fail(f"{where} step {k}: k/p/position {st['k']}/{st['p']}/{st['position']}, want {k}/{p}/{p + k}")
        ti, tl = st["top_ids"], st["top_logits"]
        if len(ti) != 5 or len(set(ti)) != 5 or any(not 0 <= i < vocab for i in ti):
            fail(f"{where} step {k}: the top 5 ids {ti}")
        for a in range(4):
            if not (tl[a] > tl[a + 1] or (tl[a] == tl[a + 1] and ti[a] < ti[a + 1])):
                fail(f"{where} step {k}: the top 5 are not in descending order, ties by id")
        if st["id"] != ti[0] or st["id"] != ids[k]:
            fail(f"{where} step {k}: the chosen id {st['id']} is not the top-1 {ti[0]} and the row's {ids[k]}")


def validate(d, chats, names, tok, vocab):
    out = {}
    steps = {}
    lines = open(f"{d}/u7-steps.tsv").read().splitlines()
    if not lines or lines[0] != "task\tticket\tk\tp\tposition\tid\ttop_ids\ttop_logits":
        fail(f"{d}/u7-steps.tsv: header")
    for r, line in enumerate(lines[1:]):
        f = line.split("\t")
        if len(f) != 8:
            fail(f"{d}/u7-steps.tsv row {r + 1}: {len(f)} fields")
        try:
            st = {"k": int(f[2]), "p": int(f[3]), "position": int(f[4]), "id": int(f[5]),
                  "top_ids": [int(x) for x in f[6].split(",")]}
        except ValueError:
            fail(f"{d}/u7-steps.tsv row {r + 1}: not integers")
        st["top_logits"] = [number(x, f"{d}/u7-steps.tsv row {r + 1}") for x in f[7].split(",")]
        if len(st["top_logits"]) != 5:
            fail(f"{d}/u7-steps.tsv row {r + 1}: {len(st['top_logits'])} logits")
        steps.setdefault((f[0], f[1]), []).append(st)
    for task, fname, width in (("triage", "u7-triage.tsv", 6), ("summary", "u7-summaries.tsv", 5)):
        lines = open(f"{d}/{fname}").read().splitlines()
        want_head = "ticket\tprompt_tokens\tids\tstop\ttext" + ("\tclass" if task == "triage" else "")
        if not lines or lines[0] != want_head:
            fail(f"{d}/{fname}: header")
        rows = [l.split("\t") for l in lines[1:]]
        mine = [c for c in chats if c[0] == task]
        if [r[0] for r in rows] != [c[1] for c in mine] or any(len(r) != width for r in rows):
            fail(f"{d}/{fname}: not exactly the {len(mine)} tickets in order, {width} fields each")
        for r, (_, n, system, user) in zip(rows, mine):
            where = f"{d} {task} #{n}"
            prompt = tok.encode(template(system, user), add_special_tokens=False).ids
            if len(prompt) > 1024:
                fail(f"{where}: the reconstructed prompt is {len(prompt)} tokens, over 1024")
            if r[1] != str(len(prompt)):
                fail(f"{where}: prompt_tokens {r[1]}, reconstructed {len(prompt)}")
            try:
                ids = [int(x) for x in r[2].split(",")]
            except ValueError:
                fail(f"{where}: ids {r[2][:40]!r}")
            limit = LIMITS[task]
            if r[3] == "eos":
                if not (1 <= len(ids) <= limit and ids[-1] in EOS and not any(i in EOS for i in ids[:-1])):
                    fail(f"{where}: an eos stop must end in one EOS, none earlier, within {limit}")
                body = ids[:-1]
            elif r[3] == "length":
                if len(ids) != limit or any(i in EOS for i in ids):
                    fail(f"{where}: a length stop must have exactly {limit} ids and no EOS")
                body = ids
            else:
                fail(f"{where}: stop {r[3]!r}")
            if any(not 0 <= i < tok.get_vocab_size(True) for i in ids):
                fail(f"{where}: an id has no token")
            check_steps(where, ids, len(prompt), steps.pop((task, n), []), vocab)
            try:
                text = json.loads(r[4])
            except ValueError:
                fail(f"{where}: text is not a JSON string")
            if text != tok.decode(body, skip_special_tokens=True):
                fail(f"{where}: the text is not the decode of its ids")
            if task == "triage" and r[5] != parse(text, names):
                fail(f"{where}: class {r[5]!r} is not the parse {parse(text, names)!r}")
            out[(task, n)] = (len(prompt), ids, r[3], text, r[5] if task == "triage" else None)
    if steps:
        fail(f"{d}/u7-steps.tsv: steps for no generation: {sorted(steps)[:3]}")
    allsteps = {}
    for line in open(f"{d}/u7-steps.tsv").read().splitlines()[1:]:
        f = line.split("\t")
        allsteps.setdefault((f[0], f[1]), []).append(([int(x) for x in f[6].split(",")],
                                                      [f32(number(x, "logit")) for x in f[7].split(",")]))
    return out, allsteps


if __name__ == "__main__":
    a_dir, b_dir, d2, ranges, labels, sample, tokjson = sys.argv[1:8]
    chats, names = inputs(d2, ranges, labels, sample)
    tok = tokenizers.Tokenizer.from_file(tokjson)
    vocab = 151936
    a, sa = validate(a_dir, chats, names, tok, vocab)
    b, sb = validate(b_dir, chats, names, tok, vocab)
    for key in a:
        if a[key][:3] != b[key][:3]:
            fail(f"{key}: the producers' generations differ")
        if sa[key] != sb[key]:
            fail(f"{key}: the producers' top-5 ids or logits differ")
    n_ids = sum(len(v[1]) for v in a.values())
    print(f"IDENTICAL (each producer validated first against {len(chats)} reconstructed chats): "
          f"{n_ids} generated ids and their top-5 ids and logits (f32 bits), stops, texts and classes")
