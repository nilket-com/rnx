"""Record 0149: corruption controls for validate.py (plans/0149 section 4a)
and, for the one forgery a validator can't see, for hf_model.py (3b).
Each corruption is applied to BOTH producers alike (agreement can't rescue
it), except the last, which is one-sided. Every one must be refused; the
unmodified pair must pass.

  validate_controls.py SCRIPT_DIR TWIN_DIR D2_JSON RANGES LABELS SAMPLE MODEL_DIR HF_PYTHON"""
import json, os, pathlib, shutil, subprocess, sys, tempfile
import tokenizers

here = pathlib.Path(__file__).parent
a_dir, b_dir, d2, ranges, labels, sample, model, hfpy = sys.argv[1:9]
tok = tokenizers.Tokenizer.from_file(f"{model}/tokenizer.json")
FILES = ["u7-triage.tsv", "u7-summaries.tsv", "u7-steps.tsv"]
files = {n: {f: open(f"{d}/{f}").read().splitlines() for f in FILES} for n, d in (("a", a_dir), ("b", b_dir))}


def write(t, name, data):
    d = pathlib.Path(t, name)
    d.mkdir()
    for f in FILES:
        (d / f).write_text("".join(l + "\n" for l in data[f]))
    return d


def run(da, db):
    with tempfile.TemporaryDirectory() as t:
        pa, pb = write(t, "a", da), write(t, "b", db)
        r = subprocess.run([hfpy, here / "validate.py", pa, pb, d2, ranges, labels, sample, f"{model}/tokenizer.json"],
                           capture_output=True, text=True)
        return r.returncode, (r.stdout + r.stderr).strip().splitlines()[-1]


def edit(fn):
    def go(data):
        data = {f: list(v) for f, v in data.items()}
        fn(data)
        return data
    return go


def field(line, i, v):
    f = line.split("\t")
    f[i] = v
    return "\t".join(f)


steps_first = 1  # the first step row (after the header)
# a triage row with at least two steps, by its first step row
two = next(r for r, l in enumerate(files["a"]["u7-steps.tsv"][1:], 1) if l.split("\t")[2] == "1") - 1
cases = {
    "a missing step": edit(lambda d: d["u7-steps.tsv"].pop(two + 1)),
    "an extra step": edit(lambda d: d["u7-steps.tsv"].append(d["u7-steps.tsv"][two])),
    "a duplicate step": edit(lambda d: d["u7-steps.tsv"].__setitem__(two + 1, d["u7-steps.tsv"][two])),
    "two steps reordered": edit(lambda d: d["u7-steps.tsv"].__setitem__(slice(two, two + 2), [d["u7-steps.tsv"][two + 1], d["u7-steps.tsv"][two]])),
    "a forged stop": edit(lambda d: d["u7-triage.tsv"].__setitem__(1, field(d["u7-triage.tsv"][1], 3, "length" if d["u7-triage.tsv"][1].split("\t")[3] == "eos" else "eos"))),
    "a forged count (an id dropped)": edit(lambda d: d["u7-triage.tsv"].__setitem__(1, field(d["u7-triage.tsv"][1], 2, ",".join(d["u7-triage.tsv"][1].split("\t")[2].split(",")[:-1]) or "0"))),
    "a non-finite logit": edit(lambda d: d["u7-steps.tsv"].__setitem__(steps_first, field(d["u7-steps.tsv"][steps_first], 7, "nan," + ",".join(d["u7-steps.tsv"][steps_first].split("\t")[7].split(",")[1:])))),
}


def forge_row(d):
    """A row-level consistent forgery: the first triage row's first id
    replaced, its text re-decoded and its class re-parsed; its steps left."""
    from validate import parse
    names = [l.split("\t")[0] for l in open(labels).read().splitlines()[1:] if l]
    f = d["u7-triage.tsv"][1].split("\t")
    ids = [int(x) for x in f[2].split(",")]
    ids[0] = tok.token_to_id("feature")
    body = ids[:-1] if f[3] == "eos" else ids
    text = tok.decode(body, skip_special_tokens=True)
    f[2], f[4], f[5] = ",".join(map(str, ids)), json.dumps(text), parse(text, names)
    d["u7-triage.tsv"][1] = "\t".join(f)


cases["a consistent forged token, text and parse (row level)"] = edit(forge_row)
ok = True
code, out = run(files["a"], files["b"])
ok &= code == 0
print(f"{'pass' if code == 0 else 'WRONG'}: unmodified: {out}")
for name, f in cases.items():
    code, out = run(f(files["a"]), f(files["b"]))
    ok &= code != 0
    print(f"{'refused' if code else 'ACCEPTED'}: both, {name}: {out}")
# one-sided: the lowest bit of one recorded logit
import struct
b = {f: list(v) for f, v in files["b"].items()}
f = b["u7-steps.tsv"][steps_first].split("\t")
logits = f[7].split(",")
bits = bytearray(struct.pack("<f", float(logits[2])))
bits[0] ^= 1
logits[2] = repr(struct.unpack("<f", bytes(bits))[0])
f[7] = ",".join(logits)
b["u7-steps.tsv"][steps_first] = "\t".join(f)
code, out = run(files["a"], b)
ok &= code != 0
print(f"{'refused' if code else 'ACCEPTED'}: one side, the lowest bit of a recorded logit: {out}")


# the fully consistent forgery, steps included: the validator can't see it
# (by design), the 3b model-level check must
def forge_all(d):
    forge_row(d)
    f = d["u7-triage.tsv"][1].split("\t")
    ticket, new = f[0], int(f[2].split(",")[0])
    for r, l in enumerate(d["u7-steps.tsv"]):
        g = l.split("\t")
        if g[0] == "triage" and g[1] == ticket and g[2] == "0":
            top = g[6].split(",")
            if str(new) in top:
                top.remove(str(new))
            top = [str(new)] + top[:4]
            g[5], g[6] = str(new), ",".join(top)
            d["u7-steps.tsv"][r] = "\t".join(g)


fa = edit(forge_all)(files["a"])
fb = edit(forge_all)(files["b"])
code, out = run(fa, fb)
print(f"{'accepted' if code == 0 else 'refused'} by the validator (expected: it can't see this): both, a fully consistent forgery, steps included: {out}")
ticket = fa["u7-triage.tsv"][1].split("\t")[0]
with tempfile.TemporaryDirectory() as t:
    pa = write(t, "a", fa)
    r = subprocess.run([hfpy, "hf_model.py", model, pa, d2, ranges, labels, sample, f"{t}/3b.json"], cwd=here,
                       capture_output=True, text=True, env=dict(os.environ, RNX_0149_ONLY=f"triage:{ticket}"))
    last = [l for l in (r.stdout + r.stderr).splitlines() if l.startswith(("3b:", "  "))]
    refused = r.returncode == 3
    ok &= refused
    print(f"{'refused' if refused else 'ACCEPTED'} by 3b (exit {r.returncode}): the same forgery, triage #{ticket}: {' / '.join(last[:2])}")
print("all validation controls behave" if ok else "CONTROLS FAILED")
sys.exit(0 if ok else 1)
