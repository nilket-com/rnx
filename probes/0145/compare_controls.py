"""Record 0145: E7's comparer corruption controls. Each corruption is
applied to BOTH real traces (agreement alone would pass) or, where noted, to
one; every one must be refused, and the unmodified pair must pass.

  compare_controls.py SCRIPT_TRACE TWIN_TRACE"""
import pathlib, struct, subprocess, sys, tempfile

here = pathlib.Path(__file__).parent
a0 = open(sys.argv[1]).read().splitlines()
b0 = open(sys.argv[2]).read().splitlines()


def run(a, b):
    with tempfile.TemporaryDirectory() as t:
        pa, pb = pathlib.Path(t, "a"), pathlib.Path(t, "b")
        pa.write_text("".join(l + "\n" for l in a))
        pb.write_text("".join(l + "\n" for l in b))
        r = subprocess.run([sys.executable, here / "compare_e7.py", pa, pb, "4", "1188"],
                           capture_output=True, text=True)
        return r.returncode, (r.stdout + r.stderr).strip().splitlines()[-1]


def first(kind, f):
    def g(lines):
        out, done = [], False
        for l in lines:
            if not done and l.startswith(kind):
                out.append(f(l)); done = True
            else:
                out.append(l)
        return out
    return g


def field(l, i, v):
    p = l.split("\t"); p[i] = v; return "\t".join(p)


both = {
    "both empty": lambda l: [],
    "channel 0's trend and z deleted": lambda l: [x for x in l if not (x.startswith("trend\t0\t") or x.startswith("z\t0\t"))],
    "a trend value dropped": first("trend\t", lambda x: x.rsplit(",", 1)[0]),
    "a z value NaN": first("z\t", lambda x: field(x, 2, "nan," + x.split("\t")[2].split(",", 1)[1])),
    "an event deleted": first("event\t", lambda x: None),
    "an event's length changed": first("event\t", lambda x: field(x, 3, str(int(x.split("\t")[3]) + 1))),
    "an event's peak moved": first("event\t", lambda x: field(x, 2, str(int(x.split("\t")[2]) + 1))),
    "a weekday value dropped": first("weekday\t", lambda x: x.rsplit(",", 1)[0]),
    "the busiest week out of range": first("busiest\t", lambda x: field(x, 2, "169")),
    "a channel's lines reordered": lambda l: l[1:2] + l[:1] + l[2:],
    "an unknown line": lambda l: l + ["note\t0\tx"],
}
ok = True
code, out = run(a0, b0)
print(f"{'pass' if code == 0 else 'WRONG'}: unmodified: {out}")
ok &= code == 0
for name, f in both.items():
    fa, fb = [x for x in f(a0) if x is not None], [x for x in f(b0) if x is not None]
    code, out = run(fa, fb)
    ok &= code != 0
    print(f"{'refused' if code else 'ACCEPTED'}: both, {name}: {out}")
def flip(lines):
    out, done = [], False
    for l in lines:
        if not done and l.startswith("z\t"):
            p = l.split("\t"); v = p[2].split(",")
            b = bytearray(struct.pack("<d", float(v[0]))); b[0] ^= 1
            v[0] = repr(struct.unpack("<d", bytes(b))[0]); p[2] = ",".join(v)
            out.append("\t".join(p)); done = True
        else:
            out.append(l)
    return out
code, out = run(flip(a0), b0)
ok &= code != 0
print(f"{'refused' if code else 'ACCEPTED'}: one side, a z value's lowest bit: {out}")
print("all controls behave" if ok else "CONTROLS FAILED")
sys.exit(0 if ok else 1)
