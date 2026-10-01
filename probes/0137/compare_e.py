"""Record 0137: an example's output against its twin's, exactly.

  compare_e.py e5 SCRIPT_OUT TWIN_OUT    every result's name, shape and f32 value bits
  compare_e.py e4 SCRIPT_OUT TWIN_OUT    every pair at or above 0.80 and its f32 score bits

Rows are parsed strictly: the expected arity, no duplicate keys, finite
values. Exit 1 on any difference or malformed row."""
import math, struct, sys


def f32(text):
    v = float(text)
    if not math.isfinite(v):
        raise SystemExit(f"a non-finite value {text!r}")
    return struct.unpack("<I", struct.pack("<f", v))[0]


def rows(path, header, arity):
    lines = open(path).read().splitlines()
    if header is not None:
        if not lines or lines[0].split("\t") != header:
            raise SystemExit(f"{path}: header {lines[:1]}, want {header}")
        lines = lines[1:]
    out = {}
    for l in lines:
        if not l:
            continue
        f = l.split("\t")
        if len(f) != arity:
            raise SystemExit(f"{path}: a row of {len(f)} fields: {f}")
        key = tuple(f[: arity - 1]) if header else f[0]
        if key in out:
            raise SystemExit(f"{path}: duplicate row {key}")
        out[key] = f
    return out


kind, ours, twin = sys.argv[1:4]
if kind == "e5":
    a, b = rows(ours, None, 3), rows(twin, None, 3)
    norm = lambda r: {k: (v[1], [f32(x) for x in v[2].split(",") if x]) for k, v in r.items()}
    same = norm(a) == norm(b)
else:
    hdr = ["issue_a", "issue_b", "score"]
    a, b = rows(ours, hdr, 3), rows(twin, hdr, 3)
    norm = lambda r: {k: f32(v[2]) for k, v in r.items()}
    same = norm(a) == norm(b)
print(f"{kind}: {len(a)} rows (script), {len(b)} (twin): {'EQUAL' if same else 'DIFFERENT'}")
sys.exit(0 if same else 1)
