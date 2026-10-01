"""Record 0134: an example's machine-readable output against its twin's.

  compare_e.py e2 SCRIPT_OUT TWIN_OUT    the correlation matrix (f64 bits), the pairs, the unusual documents
  compare_e.py e1 SCRIPT_OUT TWIN_OUT    per query, the five documents and their f32 score bits"""
import struct, sys


def f64bits(t):
    return struct.unpack("<Q", struct.pack("<d", float(t)))[0]


def f32bits(t):
    return struct.unpack("<I", struct.pack("<f", float(t)))[0]


kind, ours, twin = sys.argv[1:4]
a = [l.split("\t") for l in open(ours).read().splitlines() if l]
b = [l.split("\t") for l in open(twin).read().splitlines() if l]
if kind == "e2":
    def norm(rows, hexed):
        out = []
        for r in rows:
            if r[0] == "corr":
                out.append(("corr", tuple(int(x, 16) if hexed else f64bits(x) for x in r[1:])))
            elif r[0] == "pair":
                out.append(("pair", int(r[1]), int(r[2])))
            else:
                out.append(("unusual", int(r[1], 16) if hexed else f64bits(r[1]), r[2]))
        return out
    same = norm(a, False) == norm(b, True)
else:
    same = [(r[0], r[1], f32bits(r[2])) for r in a] == [(r[0], r[1], int(r[2], 16)) for r in b]
print(f"{kind}: {len(a)} rows, {'EQUAL' if same else 'DIFFERENT'}")
sys.exit(0 if same else 1)
