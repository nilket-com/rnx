"""Record 0143, review round 1: the E6 comparer's corruption controls. Each
corruption is applied to BOTH real traces (so agreement alone would pass),
or to one where noted; every one must be refused. The unmodified pair must
pass.

  compare_controls.py SCRIPT_TRACE TWIN_TRACE"""
import pathlib, subprocess, sys, tempfile

here = pathlib.Path(__file__).parent
a0 = open(sys.argv[1]).read().splitlines()
b0 = open(sys.argv[2]).read().splitlines()
N, K, D = "252", "6", "384"


def run(a, b):
    with tempfile.TemporaryDirectory() as t:
        pa, pb = pathlib.Path(t, "a"), pathlib.Path(t, "b")
        pa.write_text("".join(l + "\n" for l in a))
        pb.write_text("".join(l + "\n" for l in b))
        r = subprocess.run([sys.executable, here / "compare_e6.py", pa, pb, N, K, D],
                           capture_output=True, text=True)
        return r.returncode, (r.stdout + r.stderr).strip()


def without(lines, pred):
    return [l for l in lines if not pred(l)]


def edit(lines, kind, f):
    return [f(l) if l.startswith(kind) else l for l in lines]


def first(kind, f):
    def g(lines):
        out, done = [], False
        for l in lines:
            if not done and l.startswith(kind):
                out.append(f(l))
                done = True
            else:
                out.append(l)
        return out
    return g


both = {
    "both empty": lambda l: [],
    "assign 0 and centroid 0 deleted": lambda l: without(l, lambda x: x.startswith("assign\t0\t") or x.startswith("centroid\t0\t")),
    "a centroid value dropped": first("centroid\t", lambda x: x.rsplit(",", 1)[0]),
    "a centroid value NaN": first("centroid\t", lambda x: "\t".join(x.split("\t")[:2] + ["nan," + x.split("\t")[2].split(",", 1)[1]])),
    "the iteration count changed": lambda l: edit(l, "iterations\t", lambda x: x.replace("\t23\t", "\t24\t")),
    "the last iter line removed": lambda l: [x for i, x in enumerate(l) if not (x.startswith("iter\t") and not l[i + 1].startswith("iter\t"))],
    "every iter line removed": lambda l: without(l, lambda x: x.startswith("iter\t")),
    "a final assignment changed": lambda l: edit(l, "assign\t5\t", lambda x: x[:-1] + str((int(x[-1]) + 1) % 6)),
    "an assign line duplicated": lambda l: l[:] + [next(x for x in l if x.startswith("assign\t"))],
    "an unknown line": lambda l: l + ["note\tx"],
    "init with five picks": lambda l: [l[0].rsplit(",", 1)[0]] + l[1:],
    "converged with one iteration": lambda l: edit(l, "iterations\t", lambda x: "iterations\t1\ttrue"),
}
ok = True
code, out = run(a0, b0)
print(f"{'pass' if code == 0 else 'WRONG'}: unmodified: {out.splitlines()[-1]}")
ok &= code == 0
for name, f in both.items():
    code, out = run(f(a0), f(b0))
    good = code != 0
    ok &= good
    print(f"{'refused' if good else 'ACCEPTED'}: both, {name}: {out.splitlines()[-1] if out else ''}")
# one side only: a single centroid bit
def flip(lines):
    out, done = [], False
    import struct
    for l in lines:
        if not done and l.startswith("centroid\t"):
            p = l.split("\t")
            v = p[2].split(",")
            b = bytearray(struct.pack("<f", float(v[0])))
            b[0] ^= 1
            v[0] = repr(float(struct.unpack("<f", bytes(b))[0]))
            out.append("\t".join(p[:2] + [",".join(v)]))
            done = True
        else:
            out.append(l)
    return out
code, out = run(flip(a0), b0)
ok &= code != 0
print(f"{'refused' if code else 'ACCEPTED'}: one side, a centroid's lowest bit: {out.splitlines()[-1]}")
print("all controls behave" if ok else "CONTROLS FAILED")
sys.exit(0 if ok else 1)
