"""Record 0133: the workflows' complete outputs against their twins'.

  compare_outputs.py RNX_DIR TWIN_DIR            exact comparison, exit 1 on any difference
  compare_outputs.py RNX_DIR TWIN_DIR --controls also prove the comparison fails closed

U1: every (query, rank, path, passage id) and the score's f32 bits.
U2: the complete set of (band, i, j) pairs, every score's f32 bits.
U3: every ticket's component (singletons included), and every edge.
Scores are compared as f32 bit patterns: the session writes Polars' f32
text, the twin Rust's f32 text, and both parse back to the same f32."""
import pathlib, struct, sys


def f32(text):
    return struct.unpack("<I", struct.pack("<f", float(text)))[0]


def rows(path):
    return [l.split("\t") for l in pathlib.Path(path).read_text().splitlines()[1:] if l]


def u1(r):
    return sorted((q, int(k), p, int(pid), f32(s)) for q, k, p, pid, s in r)


def u2(r):
    return sorted((b, i, j, f32(s)) for b, i, j, s in r)


def u3(r):
    members = sorted((a, b) for a, b, *_ in r if a != "edge")
    edges = sorted((x[1], x[2], f32(x[3])) for x in r if x[0] == "edge")
    return members, edges


def compare(ours, twin, label):
    diffs = []
    for name, norm in [("u1", u1), ("u2", u2), ("u3", u3)]:
        a, b = norm(rows(ours / f"{name}.tsv")), norm(rows(twin / f"{name}.tsv"))
        if a != b:
            diffs.append(name)
    print(f"{label}: {'EQUAL' if not diffs else 'DIFFERENT: ' + ', '.join(diffs)}")
    return not diffs


ours, twin = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
for name in ["u1", "u2", "u3"]:
    n = len(rows(ours / f"{name}.tsv"))
    print(f"{name}: {n} rows (session), {len(rows(twin / f'{name}.tsv'))} (twin)")
ok = compare(ours, twin, "session vs twin")
if "--controls" in sys.argv:
    import shutil, tempfile
    tmp = pathlib.Path(tempfile.mkdtemp())

    def corrupted(name, fn):
        d = tmp / name
        shutil.copytree(ours, d)
        fn(d)
        return d

    def edit(file, f):
        def go(d):
            lines = (d / file).read_text().splitlines()
            (d / file).write_text("\n".join(f(lines)) + "\n")
        return go

    def last_score(lines, i):
        cells = lines[i].split("\t")
        cells[-1] = repr(float(cells[-1]) + 1e-3)
        lines[i] = "\t".join(cells)
        return lines

    def swap_path(lines):
        cells = lines[3].split("\t")
        cells[2] = "0000_not_a_record.md"
        lines[3] = "\t".join(cells)
        return lines

    def move_member(lines):
        for i, l in enumerate(lines[1:], 1):
            a, b = l.split("\t")[:2]
            if a != "edge" and a == b:
                lines[i] = f"{a}\t#999999"
                return lines
        return lines

    controls = [
        ("a non-top U2 row's score", edit("u2.tsv", lambda l: last_score(l, len(l) - 1))),
        ("a wrong U1 document", edit("u1.tsv", swap_path)),
        ("a singleton moved to another U3 component", edit("u3.tsv", move_member)),
        ("a dropped U3 singleton", edit("u3.tsv", lambda l: l[:-3] + l[-2:] if len(l) > 3 else l)),
    ]
    for label, fn in controls:
        if compare(corrupted(label.replace(" ", "_"), fn), twin, f"control, {label}"):
            print(f"CONTROL FAILED: {label} was accepted")
            ok = False
    shutil.rmtree(tmp)
sys.exit(0 if ok else 1)
