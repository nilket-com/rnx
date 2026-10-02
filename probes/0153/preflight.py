"""Record 0153, gate 0: provenance and the freeze, fatal and first (plans/0153
section 3). Nothing downstream runs unless all of it holds.

- the freeze: frozen_check.py (the accepted design SHA-256, 0139's rules
  pin, the design tables, every annotation file, SHA256SUMS);
- D3: manifest.py check (content and label hashes, the snapshot hash);
- the split and folds: split_d3.py check;
- D2 (rehearsal and diagnostic only): 0133's manifest, 0148's normalization;
  0139's frozen sample and both annotation files at 0148's pins;
- MiniLM: probes/0131/fetch.sh's pinned SHA-256s; NLI: probes/0148/fetch.sh's.

  preflight.py D3_DIR D2_JSON MINILM_DIR NLI_DIR"""
import hashlib, json, pathlib, re, subprocess, sys

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
D2_PINNED = {
    "probes/0139/frozen/sample.tsv": "5ebc6cfa9cd3c2eec357c1c8aa6927321d0b7cfefbaf28b7d7e50e8c753c77c3",
    "probes/0139/frozen/annotations-claude.tsv": "7c42f7411899dc0a4de4d59930443c254531a8eaf01fa5d72b50891875fbb0ed",
    "probes/0139/frozen/annotations-codex.tsv": "8bca7024e91a8de1e947625f6acb09d202f60a1b722b5728f1de7d328cff3ffd",
}


def sha(p):
    return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()


def pinned(fetch):
    body = (ROOT / "probes" / fetch).read_text()
    block = body[body.index("<<'SUMS'") + len("<<'SUMS'"):body.index("\nSUMS")]
    sums = [tuple(l.split("  ", 1)) for l in block.strip().splitlines()]
    assert sums and all(re.fullmatch(r"[0-9a-f]{64}", s) for s, _ in sums)
    return sums


def norm(i):
    """0133's D2 normalization, as 0148's preflight reproduces it."""
    return json.dumps({"number": i["number"], "title": i["title"], "body": i["body"].replace("\r\n", "\n"),
                       "labels": sorted(i["labels"])}, sort_keys=True, ensure_ascii=False).encode()


d3, d2, minilm, nli = sys.argv[1:5]
problems = []
for script, args in (("frozen_check.py", []), ("manifest.py", ["check", d3]), ("split_d3.py", ["check"])):
    r = subprocess.run([sys.executable, str(HERE / script), *args], capture_output=True, text=True)
    if r.returncode != 0:
        problems += [l for l in (r.stdout + r.stderr).splitlines() if l.startswith("FAIL")] or [f"{script} failed"]
try:
    got = [(str(i["number"]), hashlib.sha256(norm(i)).hexdigest()) for i in json.load(open(d2))]
except (OSError, ValueError, KeyError, TypeError) as e:
    got = None
    problems.append(f"FAIL: D2: unreadable ({e!r})")
want = [tuple(l.split("\t")) for l in (ROOT / "probes/0133/d2-manifest.tsv").read_text().splitlines()]
if got is not None and got != want:
    problems.append("FAIL: D2 changed")
for f, h in D2_PINNED.items():
    if not (ROOT / f).is_file() or sha(ROOT / f) != h:
        problems.append(f"FAIL: {f} changed")
for name, d, fetch in (("MiniLM", minilm, "0131/fetch.sh"), ("NLI", nli, "0148/fetch.sh")):
    for h, f in pinned(fetch):
        p = pathlib.Path(d) / f
        if not p.is_file():
            problems.append(f"FAIL: {name}: {f} missing")
        elif sha(p) != h:
            problems.append(f"FAIL: {name}: {f} changed")
if problems:
    for p in problems:
        print(p)
    sys.exit(1)
print("provenance: the freeze, D3 (content, labels, snapshot), the split and folds, D2 and 0139's sample and labels, "
      "MiniLM and NLI pins all hold")
