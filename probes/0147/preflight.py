"""Record 0147, gate 0: provenance, fatal and first. Nothing downstream may
run unless D1 and both models are exactly what earlier records froze.

- D1: the ordered (name, sha256) list of its .md files equals
  probes/0133/d1-manifest.tsv (321 files).
- MiniLM: the six files probes/0131/fetch.sh pins.
- The cross-encoder: the three files probes/0146/fetch.sh pins.
The pinned sums are read from those scripts, not copied here.

  preflight.py D1 MINILM_DIR CE_DIR"""
import hashlib, pathlib, re, sys

HERE = pathlib.Path(__file__).resolve().parent
PROBES = HERE.parent


def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def pinned(fetch):
    body = (PROBES / fetch).read_text()
    block = body[body.index("<<'SUMS'") + len("<<'SUMS'"):body.index("\nSUMS")]
    sums = [tuple(l.split("  ", 1)) for l in block.strip().splitlines()]
    if not sums or any(not re.fullmatch(r"[0-9a-f]{64}", s) for s, _ in sums):
        sys.exit(f"FAIL: cannot read the pinned sums in {fetch}")
    return sums


def check_model(name, d, fetch):
    bad = []
    for want, f in pinned(fetch):
        p = pathlib.Path(d) / f
        if not p.is_file():
            bad.append(f"{f} missing")
        elif sha(p) != want:
            bad.append(f"{f} changed")
    return [f"{name}: {b}" for b in bad]


d1, minilm, ce = sys.argv[1:4]
problems = []
want = [tuple(l.split("\t")) for l in (PROBES / "0133" / "d1-manifest.tsv").read_text().splitlines()]
got = [(p.name, sha(p)) for p in sorted(pathlib.Path(d1).glob("*.md"))]
if got != want:
    wn, gn = dict(want), dict(got)
    for n in sorted(set(wn) | set(gn)):
        if n not in gn:
            problems.append(f"D1: {n} missing")
        elif n not in wn:
            problems.append(f"D1: {n} extra")
        elif wn[n] != gn[n]:
            problems.append(f"D1: {n} changed")
    if not problems:
        problems.append("D1: the file order differs from the manifest")
problems += check_model("MiniLM", minilm, "0131/fetch.sh")
problems += check_model("cross-encoder", ce, "0146/fetch.sh")
if problems:
    for p in problems:
        print(f"FAIL: {p}")
    sys.exit(1)
print(f"provenance: D1 {len(got)} files match 0133's manifest; MiniLM 6 and cross-encoder 3 files match their pinned SHA-256s")
