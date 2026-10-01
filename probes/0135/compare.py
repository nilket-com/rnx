"""Record 0135: the two comparison gates, kept apart.

  compare.py exact NEW_DIR TWIN_DIR           gate 1: every complete workflow output
                                              bit for bit against the twin on the
                                              identical partition (0133's normalizers)
  compare.py tolerance OLD_DIR NEW_DIR        gate 2: the new outputs against retained
                                              old-partition outputs; identities, ranks,
                                              bands, membership and edges exact, scores
                                              within 1e-5
  compare.py embeddings OLD.bin NEW.bin DIM   gate 2 for raw embeddings: every row's
                                              cosine at least 0.999999
  compare.py controls OLD_DIR NEW_DIR TWIN_DIR
                                              each mutation fails the gate it targets,
                                              and only that one where it must

NaN and infinite values are refused in either input, never compared.
Exit 1 on any failure. 0133's own exact replay is left unchanged."""
import math, pathlib, shutil, struct, sys, tempfile

SCORE_TOL = 1e-5
COSINE_MIN = 0.999999


class Refused(Exception):
    pass


def num(text):
    v = float(text)
    if not math.isfinite(v):
        raise Refused(f"a non-finite value {text!r}")
    return v


def f32(text):
    return struct.unpack("<I", struct.pack("<f", num(text)))[0]


HEADERS = {
    "u1": ["query", "rank", "path", "pid", "score"],
    "u2": ["band", "i", "j", "score"],
    "u3": ["issue", "component"],
}


def rows(path):
    lines = pathlib.Path(path).read_text().splitlines()
    name = pathlib.Path(path).stem
    if not lines or lines[0].split("\t") != HEADERS[name]:
        raise Refused(f"{name}: header {lines[:1]}, want {HEADERS[name]}")
    return [l.split("\t") for l in lines[1:] if l]


def keyed(name, r):
    """Rows keyed by identity, so identities compare exactly and scores
    separately. Refuses a malformed row, a duplicate identity, and (U3) a
    ticket in two components; the row count is the key count."""
    out = {}

    def put(key, value):
        if key in out:
            raise Refused(f"{name}: duplicate row {key}")
        out[key] = value

    for x in r:
        if name == "u1":
            if len(x) != 5:
                raise Refused(f"u1: a row of {len(x)} fields: {x}")
            num(x[4])
            put((x[0], int(x[1]), x[2], int(x[3])), x[4])
        elif name == "u2":
            if len(x) != 4:
                raise Refused(f"u2: a row of {len(x)} fields: {x}")
            num(x[3])
            put((x[0], x[1], x[2]), x[3])
        elif x[0] == "edge":
            if len(x) != 4:
                raise Refused(f"u3: an edge of {len(x)} fields: {x}")
            num(x[3])
            put(("edge", x[1], x[2]), x[3])
        else:
            if len(x) != 2:
                raise Refused(f"u3: a member row of {len(x)} fields: {x}")
            if ("ticket", x[0]) in out:
                raise Refused(f"u3: ticket {x[0]} in two components")
            put(("ticket", x[0]), x[1])
    return out


def score(name, key):
    return not (name == "u3" and key[0] == "ticket")


def exact(new, twin):
    ok = True
    for name in ["u1", "u2", "u3"]:
        a, b = keyed(name, rows(new / f"{name}.tsv")), keyed(name, rows(twin / f"{name}.tsv"))
        same = a.keys() == b.keys() and all(
            (a[k] == b[k]) if not score(name, k) else f32(a[k]) == f32(b[k]) for k in a
        )
        print(f"  {name}: {len(a)} rows, {'BIT-EQUAL' if same else 'DIFFERENT'}")
        ok &= same
    return ok


def tolerance(old, new):
    ok = True
    for name in ["u1", "u2", "u3"]:
        a, b = keyed(name, rows(old / f"{name}.tsv")), keyed(name, rows(new / f"{name}.tsv"))
        moved = [k for k in a.keys() & b.keys() if not score(name, k) and a[k] != b[k]]
        if a.keys() != b.keys() or moved:
            only = sorted(a.keys() ^ b.keys())[:3] or moved[:3]
            print(f"  {name}: identities differ (ranks, bands, membership or edges), e.g. {only}")
            ok = False
            continue
        worst = max(
            (abs(num(a[k]) - num(b[k])) for k in a if score(name, k)), default=0.0
        )
        within = worst <= SCORE_TOL
        print(f"  {name}: {len(a)} rows, identities exact, largest score change {worst:.2e} "
              f"({'within' if within else 'OUTSIDE'} {SCORE_TOL:g})")
        ok &= within
    return ok


def embeddings(old, new, dim):
    a = struct.unpack(f"<{len(old) // 4}f", old)
    b = struct.unpack(f"<{len(new) // 4}f", new)
    if len(a) != len(b) or len(a) % dim:
        print(f"  shapes differ: {len(a)} and {len(b)} values")
        return False
    for v in a + b:
        if not math.isfinite(v):
            raise Refused("a non-finite embedding value")
    worst, equal = 1.0, 0
    for r in range(len(a) // dim):
        x, y = a[r * dim:(r + 1) * dim], b[r * dim:(r + 1) * dim]
        equal += x == y
        dot = sum(p * q for p, q in zip(x, y))
        cos = dot / (math.sqrt(sum(p * p for p in x)) * math.sqrt(sum(q * q for q in y)))
        worst = min(worst, cos)
    rows_ = len(a) // dim
    ok = worst >= COSINE_MIN
    print(f"  {rows_} rows: lowest cosine {worst:.9f} ({'at least' if ok else 'BELOW'} {COSINE_MIN}); "
          f"{equal} of {rows_} rows bit-identical")
    return ok


def guarded(fn, *args):
    try:
        return fn(*args)
    except Refused as e:
        print(f"  refused: {e}")
        return False


def controls(old, new, twin):
    tmp = pathlib.Path(tempfile.mkdtemp())

    def mutated(name, file, f):
        d = tmp / name
        shutil.copytree(new, d)
        lines = (d / file).read_text().splitlines()
        (d / file).write_text("\n".join(f(lines)) + "\n")
        return d

    def score(lines, delta, i=1):
        c = lines[i].split("\t")
        c[-1] = repr(num(c[-1]) + delta)
        lines[i] = "\t".join(c)
        return lines

    def one_bit(lines, i=1):
        c = lines[i].split("\t")
        bits = f32(c[-1]) ^ 1
        c[-1] = repr(struct.unpack("<f", struct.pack("<I", bits))[0])
        lines[i] = "\t".join(c)
        return lines

    def identity(lines, i=1):
        c = lines[i].split("\t")
        c[2] = "0000_not_a_record.md"
        lines[i] = "\t".join(c)
        return lines

    def nan(lines, i=1):
        c = lines[i].split("\t")
        c[-1] = "nan"
        lines[i] = "\t".join(c)
        return lines

    def duplicate(lines, i=1):
        return lines + [lines[i]]

    def extra_field(lines):
        for i, l in enumerate(lines[1:], 1):
            if not l.startswith("edge"):
                lines[i] = l + "\tgarbage"
                return lines

    def move_member(lines):
        # a ticket moved to another existing component
        comps = sorted({l.split("\t")[1] for l in lines[1:] if not l.startswith("edge")})
        for i, l in enumerate(lines[1:], 1):
            a, b = l.split("\t")[:2]
            if a != "edge":
                other = next(c for c in comps if c != b)
                lines[i] = f"{a}\t{other}"
                return lines

    def second_component(lines):
        a = next(l for l in lines[1:] if not l.startswith("edge")).split("\t")[0]
        return lines + [f"{a}\t#999999"]

    def drop_edge(lines):
        i = next(i for i, l in enumerate(lines) if l.startswith("edge"))
        return lines[:i] + lines[i + 1:]

    def bad_header(lines):
        return ["query\trank\tpath\tpid"] + lines[1:]

    # (label, mutated dir, gate 1 must pass?, gate 2 must pass?)
    cases = [
        ("a duplicated U1 row", mutated("dup1", "u1.tsv", duplicate), False, False),
        ("a duplicated U2 row", mutated("dup2", "u2.tsv", duplicate), False, False),
        ("a duplicated U3 member row", mutated("dup3", "u3.tsv", duplicate), False, False),
        ("a U3 member row with an extra field", mutated("arity3", "u3.tsv", extra_field), False, False),
        ("a U3 ticket moved to another component", mutated("move3", "u3.tsv", move_member), False, False),
        ("a U3 ticket in two components", mutated("two3", "u3.tsv", second_component), False, False),
        ("a U3 edge removed", mutated("edge3", "u3.tsv", drop_edge), False, False),
        ("a U1 header without its score column", mutated("head1", "u1.tsv", bad_header), False, False),
        ("one f32 bit of a U1 score", mutated("bit", "u1.tsv", one_bit), False, True),
        ("a U1 score moved by 2e-5", mutated("tol", "u1.tsv", lambda l: score(l, 2e-5)), False, False),
        ("a U1 document changed, numbers identical", mutated("id", "u1.tsv", identity), False, False),
        ("a NaN U1 score", mutated("nan", "u1.tsv", nan), False, False),
    ]
    ok = True
    for label, d, g1, g2 in cases:
        r1 = guarded(exact, d, twin)
        r2 = guarded(tolerance, old, d)
        right = r1 == g1 and r2 == g2
        print(f"{'ok' if right else 'WRONG'}: {label}: gate 1 {'passes' if r1 else 'fails'}, "
              f"gate 2 {'passes' if r2 else 'fails'}")
        ok &= right
    return ok


if __name__ == "__main__":
    mode = sys.argv[1]
    if mode == "exact":
        ok = guarded(exact, pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3]))
    elif mode == "tolerance":
        ok = guarded(tolerance, pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3]))
    elif mode == "embeddings":
        ok = guarded(embeddings, pathlib.Path(sys.argv[2]).read_bytes(),
                     pathlib.Path(sys.argv[3]).read_bytes(), int(sys.argv[4]))
    elif mode == "controls":
        ok = controls(*(pathlib.Path(a) for a in sys.argv[2:5]))
    else:
        raise SystemExit(f"unknown mode {mode}")
    print("PASS" if ok else "FAIL")
    sys.exit(0 if ok else 1)
