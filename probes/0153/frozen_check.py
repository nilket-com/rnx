"""Record 0153, gate 0's frozen-file check (plans/0153 sections 2, 3; R3).

- the design file plans/0153_triage_routing.md has the accepted SHA-256;
- 0139's rules.md has its pinned SHA-256;
- frozen/labels.tsv, frozen/keywords.tsv and frozen/maintainer_map.tsv equal
  the design's tables (keywords with Markdown's \\| read as |);
- frozen/rules.md carries 0139's five classification rules verbatim;
- every annotation file is exactly its split: the header `number\\tlabel`,
  one row per ticket in the split file's order, each label one of the five,
  no blank, missing, extra or duplicate rows;
- every frozen file matches frozen/SHA256SUMS, and every file in frozen/ is
  listed there.

  frozen_check.py"""
import hashlib, pathlib, re, sys

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
FROZEN = HERE / "frozen"
DESIGN = ROOT / "plans" / "0153_triage_routing.md"
DESIGN_SHA = "b1b7557344e960ddd047b7f2a218f79d0f4d0c6138a7003d00027062566f8549"
RULES0139 = ROOT / "probes" / "0139" / "frozen" / "rules.md"
RULES0139_SHA = "8aa28e2af45288406023237baa3a193e30bf816450e9181c0f124be876a4c928"
LABELS = ["bug", "feature", "question", "documentation", "performance"]
ANNOTATIONS = {"annotations-claude-hold.tsv": "d3-hold.tsv", "annotations-codex-hold.tsv": "d3-hold.tsv",
               "annotations-claude-dev.tsv": "d3-dev.tsv"}


def sha(p):
    return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()


problems = []


def need(ok, msg):
    if not ok:
        problems.append(msg)


need(DESIGN.is_file() and sha(DESIGN) == DESIGN_SHA, "the design file is not the accepted design (SHA-256)")
need(RULES0139.is_file() and sha(RULES0139) == RULES0139_SHA, "0139's rules.md changed")
plan = DESIGN.read_text() if DESIGN.is_file() else ""

# tables from the design
labels = re.findall(r"^\| (bug|feature|question|documentation|performance) \| (A .+?\.) \|$", plan, re.M)
want = "label\tdescription\n" + "".join(f"{l}\t{d}\n" for l, d in labels)
need(len(labels) == 5 and (FROZEN / "labels.tsv").read_text() == want, "frozen/labels.tsv is not the design's label table")
kw = re.findall(r"^\| (bug|feature|question|documentation) \| `(.+)` \|$", plan, re.M)
want = "queue\tregex\n" + "".join(f"{q}\t{r.replace(chr(92) + '|', '|')}\n" for q, r in kw)
need(len(kw) == 4 and (FROZEN / "keywords.tsv").read_text() == want, "frozen/keywords.tsv is not the design's keyword table")
m = re.search(r"\| maintainer labels \| mapped queue \|\n\|---\|---\|\n((?:\|.+\|\n)+)", plan)
pairs = []
for row in (m.group(1).splitlines() if m else []):
    names, queue = [c.strip() for c in row.strip("|").split("|")]
    pairs += [(n.strip(), queue) for n in names.split(",")]
want = "maintainer_label\tqueue\n" + "".join(f"{n}\t{q}\n" for n, q in pairs)
need(bool(pairs) and (FROZEN / "maintainer_map.tsv").read_text() == want, "frozen/maintainer_map.tsv is not the design's map")
rules = [l for l in RULES0139.read_text().splitlines() if re.match(r"^[1-5]\. ", l)] if RULES0139.is_file() else []
need(len(rules) == 5 and all(r in (FROZEN / "rules.md").read_text().splitlines() for r in rules),
     "frozen/rules.md does not carry 0139's five rules verbatim")

# annotation files
for name, split in ANNOTATIONS.items():
    p = FROZEN / name
    if not p.is_file():
        problems.append(f"{name} missing")
        continue
    order = [l for l in (FROZEN / split).read_text().splitlines()[1:]]
    lines = p.read_text().split("\n")
    if lines and lines[-1] == "":
        lines = lines[:-1]
    else:
        problems.append(f"{name}: no final newline")
    if not lines or lines[0] != "number\tlabel":
        problems.append(f"{name}: header")
        continue
    rows = [l.split("\t") for l in lines[1:]]
    for i, r in enumerate(rows):
        if len(r) != 2 or not r[0] or not r[1]:
            problems.append(f"{name} row {i + 1}: blank or malformed")
        elif r[1] not in LABELS:
            problems.append(f"{name} row {i + 1}: label {r[1]!r} is not one of the five")
    nums = [r[0] for r in rows]
    if len(set(nums)) != len(nums):
        problems.append(f"{name}: duplicate rows")
    if nums != order:
        missing = sorted(set(order) - set(nums), key=int)[:5]
        extra = sorted(set(nums) - set(order), key=lambda x: (len(x), x))[:5]
        problems.append(f"{name}: rows are not exactly {split} in order (missing {missing}, extra {extra})")

# SHA256SUMS over frozen/
sums = FROZEN / "SHA256SUMS"
if not sums.is_file():
    problems.append("frozen/SHA256SUMS missing")
else:
    listed = {}
    for l in sums.read_text().splitlines():
        h, f = l.split("  ", 1)
        listed[f] = h
    present = {p.name for p in FROZEN.iterdir() if p.is_file() and p.name != "SHA256SUMS"}
    for f in sorted(present | set(listed)):
        if f not in listed:
            problems.append(f"frozen/{f} is not in SHA256SUMS")
        elif f not in present:
            problems.append(f"frozen/{f} missing")
        elif sha(FROZEN / f) != listed[f]:
            problems.append(f"frozen/{f} changed")

if problems:
    for p in problems:
        print(f"FAIL: {p}")
    sys.exit(1)
print(f"frozen: the design (accepted SHA-256), 0139's rules pin, labels, rules, keywords, maintainer map, "
      f"{len(ANNOTATIONS)} annotation files and SHA256SUMS all check")
