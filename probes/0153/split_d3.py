"""Record 0153: D3's split and D3-dev's folds (plans/0153 sections 1, 5b).

  split_d3.py write    writes frozen/d3-hold.tsv and frozen/d3-dev.tsv (ascending number)
  split_d3.py check    refuses anything but the rule's exact files

D3-hold: the 150 issues with the smallest sha256("rnx-0153-hold:" + n),
lowercase hex ascending, exact ties to the smaller number; n is the canonical
decimal issue number. D3-dev: every other issue. Folds: over D3-dev sorted by
(sha256("rnx-0153-fold:" + n) hex, number), fold = zero-based rank mod 5."""
import hashlib, pathlib, sys

FROZEN = pathlib.Path(__file__).resolve().parent / "frozen"
HOLD = 150


def h(prefix, n):
    return hashlib.sha256(f"{prefix}{n}".encode("ascii")).hexdigest()


def numbers():
    lines = (FROZEN / "d3-manifest.tsv").read_text().splitlines()
    ns = [int(l.split("\t")[0]) for l in lines[1:]]
    if [str(n) for n in ns] != [l.split("\t")[0] for l in lines[1:]] or len(set(ns)) != len(ns) or ns != sorted(ns):
        sys.exit("FAIL: the manifest's numbers are not unique, canonical and ascending")
    return ns


def split():
    ns = numbers()
    hold = sorted(sorted(ns, key=lambda n: (h("rnx-0153-hold:", n), n))[:HOLD])
    dev = [n for n in ns if n not in set(hold)]
    return hold, dev


def folds(dev):
    order = sorted(dev, key=lambda n: (h("rnx-0153-fold:", n), n))
    return {n: r % 5 for r, n in enumerate(order)}


def text(ns):
    return "number\n" + "".join(f"{n}\n" for n in ns)


if __name__ == "__main__":
    hold, dev = split()
    if sys.argv[1] == "write":
        (FROZEN / "d3-hold.tsv").write_text(text(hold))
        (FROZEN / "d3-dev.tsv").write_text(text(dev))
        print(f"wrote D3-hold ({len(hold)}) and D3-dev ({len(dev)})")
    else:
        for name, ns in (("d3-hold.tsv", hold), ("d3-dev.tsv", dev)):
            if (FROZEN / name).read_text() != text(ns):
                sys.exit(f"FAIL: frozen/{name} is not the rule's split")
        f = folds(dev)
        print(f"split replays: D3-hold {len(hold)}, D3-dev {len(dev)}; folds {[sum(1 for v in f.values() if v == k) for k in range(5)]}")
