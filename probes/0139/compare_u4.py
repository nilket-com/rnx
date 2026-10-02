"""Record 0139: U4's per-ticket table against the twin's, exactly.

  compare_u4.py SCRIPT_TSV TWIN_TSV D2_MANIFEST LABELS_TSV
  compare_u4.py --controls SCRIPT_TSV TWIN_TSV D2_MANIFEST LABELS_TSV

Each table is first validated on its own (review round 1):
- the header is exactly `ticket, triage, best, second, margin`, then the
  frozen labels' five score columns, in `labels.tsv`'s order;
- the tickets are exactly the committed D2 manifest's 252 identities, each
  once;
- triage is a frozen label or "review"; best and second are frozen labels,
  and differ; every number is finite.
Only then are the two compared: the labels exactly, the margin as an f64,
every score as f32 bits. With --controls, each common corruption of the
script's table must be refused. Exit 1 on any failure."""
import math, pathlib, shutil, struct, sys, tempfile


class Refused(Exception):
    pass


def num(t):
    v = float(t)
    if not math.isfinite(v):
        raise Refused(f"a non-finite value {t!r}")
    return v


def f32(t):
    return struct.unpack("<I", struct.pack("<f", num(t)))[0]


def manifest_tickets(path):
    # 0133's D2 manifest: one line per ticket, its number first
    out = [l.split("\t")[0] for l in pathlib.Path(path).read_text().splitlines() if l]
    if len(set(out)) != len(out):
        raise Refused(f"{path}: duplicate tickets in the manifest")
    return set(out)


def frozen_labels(path):
    return [l.split("\t")[0] for l in pathlib.Path(path).read_text().splitlines()[1:] if l]


def table(path, tickets, labels):
    lines = pathlib.Path(path).read_text().splitlines()
    want = ["ticket", "triage", "best", "second", "margin"] + labels
    if not lines or lines[0].split("\t") != want:
        raise Refused(f"{path}: header {lines[:1]}, want {want}")
    out = {}
    for l in lines[1:]:
        if not l:
            continue
        f = l.split("\t")
        if len(f) != len(want):
            raise Refused(f"{path}: a row of {len(f)} fields, want {len(want)}")
        t, triage, best, second = f[0], f[1], f[2], f[3]
        if t in out:
            raise Refused(f"{path}: duplicate ticket {t}")
        if t not in tickets:
            raise Refused(f"{path}: ticket {t} isn't in the D2 manifest")
        if triage not in labels and triage != "review":
            raise Refused(f"{path}: ticket {t}: triage {triage!r}")
        if best not in labels or second not in labels or best == second:
            raise Refused(f"{path}: ticket {t}: best {best!r}, second {second!r}")
        out[t] = (triage, best, second, num(f[4]), tuple(f32(x) for x in f[5:]))
    if set(out) != tickets:
        raise Refused(f"{path}: {len(out)} tickets, missing {sorted(tickets - set(out), key=int)[:5]}")
    return out


def compare(script, twin, manifest, labels_path):
    tickets, labels = manifest_tickets(manifest), frozen_labels(labels_path)
    if len(labels) != 5:
        raise Refused(f"{labels_path}: {len(labels)} labels, want 5")
    a, b = table(script, tickets, labels), table(twin, tickets, labels)
    if a != b:
        diff = [k for k in sorted(a, key=int) if a[k] != b[k]][:5]
        raise Refused(f"the tables differ, first at tickets {diff}")
    return len(a)


def checked(*args):
    try:
        n = compare(*args)
        print(f"u4: {n} tickets, each table complete and valid: BIT-EQUAL")
        return True
    except Refused as e:
        print(f"u4: refused: {e}")
        return False


def controls(script, twin, manifest, labels_path):
    tmp = pathlib.Path(tempfile.mkdtemp())
    lines = pathlib.Path(script).read_text().splitlines()

    def variant(name, new):
        p = tmp / name
        p.write_text("\n".join(new) + "\n")
        return p

    def drop_column(ls, k):
        return ["\t".join(c for i, c in enumerate(l.split("\t")) if i != k) for l in ls]

    def change_score(ls):
        f = ls[1].split("\t")
        bits = f32(f[5]) ^ 1
        f[5] = repr(struct.unpack("<f", struct.pack("<I", bits))[0])
        return [ls[0], "\t".join(f)] + ls[2:]

    def set_field(ls, k, value):
        f = ls[1].split("\t")
        f[k] = value
        return [ls[0], "\t".join(f)] + ls[2:]

    cases = [
        ("header only, both sides", lines[:1], lines[:1]),
        ("one ticket missing, both sides", lines[:-1], None),
        ("a score column missing, both sides", drop_column(lines, 9), None),
        ("a duplicate row", lines + [lines[1]], "keep"),
        ("a non-finite score", set_field(lines, 5, "nan"), "keep"),
        ("one score bit changed", change_score(lines), "keep"),
        ("an unknown triage label", set_field(lines, 1, "other"), "keep"),
        ("best equal to second", set_field(lines, 3, lines[1].split("\t")[2]), "keep"),
    ]
    twin_lines = pathlib.Path(twin).read_text().splitlines()
    ok = True
    for name, ours, theirs in cases:
        a = variant("script.tsv", ours)
        if theirs is None:
            # the same omission on both sides: agreement alone must not pass
            b = variant("twin.tsv", (drop_column(twin_lines, 9) if "column" in name else twin_lines[:-1]))
        elif theirs == "keep":
            b = pathlib.Path(twin)
        else:
            b = variant("twin.tsv", theirs)
        passed = checked(a, b, manifest, labels_path)
        print(f"{'ok' if not passed else 'WRONG'}: {name} is {'refused' if not passed else 'ACCEPTED'}")
        ok &= not passed
    shutil.rmtree(tmp)
    return ok


if __name__ == "__main__":
    if sys.argv[1] == "--controls":
        good = checked(*sys.argv[2:6])
        ok = good and controls(*sys.argv[2:6])
        print("controls:", "all refused as required" if ok else "FAILED")
    else:
        ok = checked(*sys.argv[1:5])
    sys.exit(0 if ok else 1)
