"""Record 0143: E6's exact gate (strict after review round 1).

Each trace is first validated on its own, against the expected case
dimensions (N tickets, K clusters, D values per centroid) and the frozen
algorithm's invariants; only then are the two compared, line for line.

A trace, in this order and nothing else:
- `init\t<K indices in [0, N)>`;
- `move\t<iteration>\t<cluster>\t<ticket>`, iterations non-decreasing and at
  most the count, the moved ticket in that cluster in that iteration;
- `iter\t<i>\t<N clusters>` for i = 1 .. iterations: every iteration's
  assignments after its moves, every cluster non-empty;
- `iterations\t<1..50>\t<true|false>`: converged means at least 2 iterations
  and the last two equal; not converged means exactly 50;
- `assign\t<i>\t<cluster>` for i = 0 .. N-1, equal to the last iteration;
- `centroid\t<c>\t<D values>` for c = 0 .. K-1, each finite and exactly an
  f32 (the notation may differ between the two: Rune writes small negatives
  without an exponent, Rust's {:?} with one, so values are compared as f32
  bits).

  compare_e6.py SCRIPT_TRACE TWIN_TRACE N K D"""
import math, struct, sys


def fail(msg):
    sys.exit(f"FAIL: {msg}")


def ints(text, what, n=None, lo=0, hi=None):
    try:
        v = [int(x) for x in text.split(",")]
    except ValueError:
        fail(f"{what}: not integers")
    if n is not None and len(v) != n:
        fail(f"{what}: {len(v)} values, want {n}")
    if any(x < lo or (hi is not None and x >= hi) for x in v):
        fail(f"{what}: a value outside [{lo}, {hi})")
    return v


def f32bits(text, what):
    try:
        v = float(text)
    except ValueError:
        fail(f"{what}: {text!r} is not a number")
    if not math.isfinite(v):
        fail(f"{what}: {text} is not finite")
    b = struct.pack("<f", v)
    if struct.unpack("<f", b)[0] != v:
        fail(f"{what}: {text} is not an f32 value")
    return b


def validate(path, n, k, d):
    lines = open(path).read().splitlines()
    if not lines:
        fail(f"{path}: empty")
    fields = [l.split("\t") for l in lines]
    pos = 0
    def take(kind, arity):
        nonlocal pos
        if pos >= len(fields) or fields[pos][0] != kind:
            return None
        f = fields[pos]
        if len(f) != arity:
            fail(f"{path} line {pos + 1}: {kind} has {len(f)} fields, want {arity}")
        pos += 1
        return f
    f = take("init", 2) or fail(f"{path}: no init line first")
    init = ints(f[1], f"{path} init", k, 0, n)
    moves = []
    while (f := take("move", 4)):
        moves.append(tuple(ints(",".join(f[1:]), f"{path} move", 3)))
    history = []
    while (f := take("iter", 3)):
        i = ints(f[1], f"{path} iter number", 1)[0]
        if i != len(history) + 1:
            fail(f"{path}: iteration {i} out of order")
        a = ints(f[2], f"{path} iter {i}", n, 0, k)
        if len(set(a)) != k:
            fail(f"{path}: iteration {i} leaves a cluster empty")
        history.append(a)
    f = take("iterations", 3) or fail(f"{path}: no iterations line after the iter lines")
    it = ints(f[1], f"{path} iterations", 1, 1, 51)[0]
    if f[2] not in ("true", "false"):
        fail(f"{path}: converged is {f[2]!r}")
    converged = f[2] == "true"
    if len(history) != it:
        fail(f"{path}: {len(history)} iter lines for {it} iterations")
    if converged and (it < 2 or history[-1] != history[-2]):
        fail(f"{path}: converged without two equal last iterations")
    if not converged and it != 50:
        fail(f"{path}: not converged after {it} iterations")
    last_it = 0
    for (mi, c, t) in moves:
        if not (1 <= mi <= it) or mi < last_it or c >= k or t >= n:
            fail(f"{path}: move {(mi, c, t)} is out of range or order")
        if history[mi - 1][t] != c:
            fail(f"{path}: move {(mi, c, t)} is not in iteration {mi}'s assignments")
        last_it = mi
    assign = []
    while (f := take("assign", 3)):
        i = ints(f[1], f"{path} assign index", 1)[0]
        if i != len(assign):
            fail(f"{path}: assign {i} out of order")
        assign.append(ints(f[2], f"{path} assign {i}", 1, 0, k)[0])
    if len(assign) != n:
        fail(f"{path}: {len(assign)} assign lines, want {n}")
    if assign != history[-1]:
        fail(f"{path}: the final assignments differ from the last iteration's")
    cents = []
    while (f := take("centroid", 3)):
        c = ints(f[1], f"{path} centroid index", 1)[0]
        if c != len(cents):
            fail(f"{path}: centroid {c} out of order")
        vals = f[2].split(",")
        if len(vals) != d:
            fail(f"{path}: centroid {c} has {len(vals)} values, want {d}")
        cents.append([f32bits(x, f"{path} centroid {c}") for x in vals])
    if len(cents) != k:
        fail(f"{path}: {len(cents)} centroids, want {k}")
    if pos != len(fields):
        fail(f"{path} line {pos + 1}: unexpected {fields[pos][0]!r}")
    return dict(init=init, moves=moves, history=history, it=it, converged=converged,
                assign=assign, cents=cents, texts=[l.split("\t")[2].split(",") for l in lines if l.startswith("centroid\t")])


if __name__ == "__main__":
    a_path, b_path = sys.argv[1], sys.argv[2]
    n, k, d = (int(x) for x in sys.argv[3:6])
    a, b = validate(a_path, n, k, d), validate(b_path, n, k, d)
    for key in ("init", "moves", "history", "it", "converged", "assign"):
        if a[key] != b[key]:
            fail(f"{key} differs")
    if a["cents"] != b["cents"]:
        fail("centroid f32 bits differ")
    notation = sum(p != q for x, y in zip(a["texts"], b["texts"]) for p, q in zip(x, y))
    print(f"BIT-EQUAL (each trace validated first, N={n} K={k} D={d}): init, {len(a['moves'])} moves, "
          f"{a['it']} iterations of {n} assignments (converged {a['converged']}), {n} final assignments, "
          f"{k * d} centroid values in f32 bits ({notation} in a different notation)")
