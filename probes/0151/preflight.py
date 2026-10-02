"""Record 0151, gate 0: provenance, fatal and first. Nothing downstream runs
unless D1, both models and both frozen query files are exactly what was
frozen, and the split replays.

- D1: the ordered (name, sha256) list of its .md files equals
  probes/0133/d1-manifest.tsv;
- MiniLM: probes/0131/fetch.sh's pinned SHA-256s; the cross-encoder:
  probes/0146/fetch.sh's;
- dev.tsv and holdout.tsv: the SHA-256s in plans/0151 section 1;
- split.py: both files are exactly the frozen rule's.

  preflight.py D1 MINILM_DIR CE_DIR"""
import hashlib, pathlib, re, subprocess, sys

HERE = pathlib.Path(__file__).resolve().parent
PROBES = HERE.parent
QUERIES = {"dev.tsv": "2442696a1acaac0ce8643e03a6cc5347a42f5586eea754cde88edfba149cbfee",
           "holdout.tsv": "f0e6f3455b4258f3ef350c179f9893ef0aba31ceef37ace8deed8b572bb3ff7a"}


def sha(p):
    return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()


def pinned(fetch):
    body = (PROBES / fetch).read_text()
    block = body[body.index("<<'SUMS'") + len("<<'SUMS'"):body.index("\nSUMS")]
    sums = [tuple(l.split("  ", 1)) for l in block.strip().splitlines()]
    assert sums and all(re.fullmatch(r"[0-9a-f]{64}", s) for s, _ in sums)
    return sums


d1, minilm, ce = sys.argv[1:4]
problems = []
want = [tuple(l.split("\t")) for l in (PROBES / "0133" / "d1-manifest.tsv").read_text().splitlines()]
got = [(p.name, sha(p)) for p in sorted(pathlib.Path(d1).glob("*.md"))]
if got != want:
    wn, gn = dict(want), dict(got)
    problems += [f"D1: {n} {'missing' if n not in gn else 'extra' if n not in wn else 'changed'}"
                 for n in sorted(set(wn) | set(gn)) if wn.get(n) != gn.get(n)] or ["D1: the order differs"]
for name, d, fetch in (("MiniLM", minilm, "0131/fetch.sh"), ("cross-encoder", ce, "0146/fetch.sh")):
    for h, f in pinned(fetch):
        p = pathlib.Path(d) / f
        if not p.is_file():
            problems.append(f"{name}: {f} missing")
        elif sha(p) != h:
            problems.append(f"{name}: {f} changed")
for f, h in QUERIES.items():
    if not (HERE / f).is_file():
        problems.append(f"{f} missing")
    elif sha(HERE / f) != h:
        problems.append(f"{f} changed")
if not problems:
    r = subprocess.run([sys.executable, str(HERE / "split.py"), d1], capture_output=True, text=True)
    if r.returncode != 0:
        problems.append(f"split: {(r.stdout + r.stderr).strip().splitlines()[-1]}")
if problems:
    for p in problems:
        print(f"FAIL: {p}")
    sys.exit(1)
print(f"provenance: D1 {len(got)} files match 0133's manifest; MiniLM and the cross-encoder match their pinned SHA-256s; "
      f"dev.tsv and holdout.tsv match plans/0151; the split replays")
