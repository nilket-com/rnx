"""Record 0149, gate 0: provenance, fatal and first. Nothing downstream may
run unless D2, 0139's frozen files, the passages' byte ranges, 0148's U6
ticket table and the instruct model are exactly what was frozen.

- D2: each issue's normalized content hash, in order, equals
  probes/0133/d2-manifest.tsv (0133's normalization, reproduced here).
- 0139's frozen labels, rules, sample, both annotations and its per-ticket
  baseline, and 0136's ranges: the SHA-256s in plans/0148 section 1.
- 0148's U6 ticket table (the NLI comparison), as at 5af908f.
- The instruct model: the five files probes/0149/fetch.sh pins (read from it).

  preflight.py D2_JSON MODEL_DIR"""
import hashlib, json, pathlib, re, sys

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
FROZEN = {
    "probes/0139/frozen/labels.tsv": "3ad1535382ab3b584de1ff466ba39a9e2aacce0cb7dba7177f6bcc2d3d910b92",
    "probes/0139/frozen/rules.md": "8aa28e2af45288406023237baa3a193e30bf816450e9181c0f124be876a4c928",
    "probes/0139/frozen/sample.tsv": "5ebc6cfa9cd3c2eec357c1c8aa6927321d0b7cfefbaf28b7d7e50e8c753c77c3",
    "probes/0139/frozen/annotations-claude.tsv": "7c42f7411899dc0a4de4d59930443c254531a8eaf01fa5d72b50891875fbb0ed",
    "probes/0139/frozen/annotations-codex.tsv": "8bca7024e91a8de1e947625f6acb09d202f60a1b722b5728f1de7d328cff3ffd",
    "probes/0139/out/u4-session.tsv": "faa81b4683920da59a6da2b8a2cac0ce5683156eb015d810b1180336184d7a5a",
    "probes/0136/out/ranges-o0.tsv": "a6940aceac5af1547262fa90a3479e4039e7b41d5379fc5643ad6a057b33403a",
    "probes/0148/out/u6-script/u6-tickets.tsv": "0111f9b1269a94ab28ee5680cd20925d372ef86c7f79c9806c1bffe60ec12c7e",
}


def sha(p):
    return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()


def norm(i):
    """0133's normalization (probes/0133/manifest.py), unchanged."""
    return json.dumps({"number": i["number"], "title": i["title"], "body": i["body"].replace("\r\n", "\n"),
                       "labels": sorted(i["labels"])}, sort_keys=True, ensure_ascii=False).encode()


d2, nli = sys.argv[1:3]
problems = []
try:
    issues = json.load(open(d2))
    got = [(str(i["number"]), hashlib.sha256(norm(i)).hexdigest()) for i in issues]
except (OSError, ValueError, KeyError, TypeError) as e:
    got = None
    problems.append(f"D2: unreadable ({e!r})")
want = [tuple(l.split("\t")) for l in (ROOT / "probes/0133/d2-manifest.tsv").read_text().splitlines()]
if got is not None and got != want:
    wn, gn = dict(want), dict(got)
    diffs = [f"#{n} {'missing' if n not in gn else 'extra' if n not in wn else 'changed'}"
             for n in sorted(set(wn) | set(gn), key=int) if wn.get(n) != gn.get(n)]
    problems.append("D2: " + (", ".join(diffs[:5]) if diffs else "the order differs from the manifest"))
for f, h in FROZEN.items():
    if not (ROOT / f).is_file():
        problems.append(f"{f} missing")
    elif sha(ROOT / f) != h:
        problems.append(f"{f} changed")
body = (HERE / "fetch.sh").read_text()
block = body[body.index("<<'SUMS'") + len("<<'SUMS'"):body.index("\nSUMS")]
sums = [tuple(l.split("  ", 1)) for l in block.strip().splitlines()]
assert sums and all(re.fullmatch(r"[0-9a-f]{64}", s) for s, _ in sums)
for h, f in sums:
    p = pathlib.Path(nli) / f
    if not p.is_file():
        problems.append(f"model: {f} missing")
    elif sha(p) != h:
        problems.append(f"model: {f} changed")
if problems:
    for p in problems:
        print(f"FAIL: {p}")
    sys.exit(1)
print(f"provenance: D2 {len(got)} issues match 0133's manifest; 0139's frozen files, its baseline, 0136's "
      f"ranges and 0148's U6 table match; the instruct model's {len(sums)} files match their pinned SHA-256s")
