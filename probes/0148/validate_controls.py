"""Record 0148: corruption controls for validate.py (plans/0148 section 6,
review round 1, R3). Each corruption is applied to BOTH producers alike, so
agreement alone can't rescue it, except the last, which is one-sided. Every
one must be refused; the unmodified pair must pass.

  validate_controls.py SCRIPT_DIR TWIN_DIR D2_JSON RANGES LABELS"""
import pathlib, shutil, struct, subprocess, sys, tempfile
from validate import identities, softmax_entail

here = pathlib.Path(__file__).parent
a_dir, b_dir, d2, ranges, labels = sys.argv[1:6]
ids, numbers, names = identities(d2, ranges, labels)
pairs = {n: open(f"{d}/u6-pairs.tsv").read().splitlines() for n, d in (("a", a_dir), ("b", b_dir))}
tickets = {n: open(f"{d}/u6-tickets.tsv").read().splitlines() for n, d in (("a", a_dir), ("b", b_dir))}


def run(pa, pb, ta, tb):
    with tempfile.TemporaryDirectory() as t:
        dirs = []
        for name, p, tk in (("a", pa, ta), ("b", pb, tb)):
            d = pathlib.Path(t, name)
            d.mkdir()
            (d / "u6-pairs.tsv").write_text("".join(l + "\n" for l in p))
            (d / "u6-tickets.tsv").write_text("".join(l + "\n" for l in tk))
            dirs.append(d)
        r = subprocess.run([sys.executable, here / "validate.py", *dirs, d2, ranges, labels], capture_output=True, text=True)
        return r.returncode, (r.stdout + r.stderr).strip().splitlines()[-1]


def field(line, i, v):
    f = line.split("\t")
    f[i] = v
    return "\t".join(f)


k = len(names)
# a ticket with at least two passages, its first row
two = next(r for r, i in enumerate(ids) if i[1] == 1) - k + 1  # rows are 1-based in the files (row 0 is the header)
other = str(numbers[1])


def both(f):
    return lambda p: f(list(p))


def swap(l, i, j):
    l[i], l[j] = l[j], l[i]
    return l


cases = {
    "a passage swapped between two pairs (identities kept in place)": both(lambda l: [field(x, 1, "1") if r == two else field(x, 1, "0") if r == two + k else x for r, x in enumerate(l)]),
    "a ticket's identity swapped": both(lambda l: [field(x, 0, other) if r == 1 else x for r, x in enumerate(l)]),
    "a label swapped": both(lambda l: [field(x, 2, l[2].split("\t")[2]) if r == 1 else field(x, 2, l[1].split("\t")[2]) if r == 2 else x for r, x in enumerate(l)]),
    "a missing identity": both(lambda l: l[:5] + l[6:]),
    "an extra identity": both(lambda l: l + [l[-1]]),
    "a duplicate identity": both(lambda l: l[:2] + [l[1]] + l[3:]),
}
ok = True
code, out = run(pairs["a"], pairs["b"], tickets["a"], tickets["b"])
ok &= code == 0
print(f"{'pass' if code == 0 else 'WRONG'}: unmodified: {out}")
for name, f in cases.items():
    code, out = run(f(pairs["a"]), f(pairs["b"]), tickets["a"], tickets["b"])
    ok &= code != 0
    print(f"{'refused' if code else 'ACCEPTED'}: both, {name}: {out}")


# a ticket's class changed consistently with forged scores, in both tables
def forge(t):
    t = list(t)
    f = t[1].split("\t")
    f[1], f[2], f[3], f[4] = names[0], names[0], names[1], "0.99"
    f[5] = "0.99"
    t[1] = "\t".join(f)
    return t


code, out = run(pairs["a"], pairs["b"], forge(tickets["a"]), forge(tickets["b"]))
ok &= code != 0
print(f"{'refused' if code else 'ACCEPTED'}: both, a ticket's class and scores forged consistently: {out}")
# one-sided: the lowest bit of a logit in a pair that is not its ticket's
# maximum for its label (so the ticket table is unchanged)
logits = [[float(x) for x in r.split("\t")[3:]] for r in pairs["b"][1:]]
best = {}
for r, i in enumerate(ids):
    key = (i[0], i[2])
    if key not in best or softmax_entail(logits[r]) > softmax_entail(logits[best[key]]):
        best[key] = r
target = next(r for r, i in enumerate(ids) if best[(i[0], i[2])] != r)
b = list(pairs["b"])
f = b[target + 1].split("\t")
bits = bytearray(struct.pack("<f", float(f[3])))
bits[0] ^= 1
f[3] = repr(struct.unpack("<f", bytes(bits))[0])
b[target + 1] = "\t".join(f)
code, out = run(pairs["a"], b, tickets["a"], tickets["b"])
ok &= code != 0
print(f"{'refused' if code else 'ACCEPTED'}: one side, the lowest bit of a non-maximal pair's logit: {out}")
print("all validation controls behave" if ok else "CONTROLS FAILED")
sys.exit(0 if ok else 1)
