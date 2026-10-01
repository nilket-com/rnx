"""Record 0133: the datasets' manifests, and their replay check.

  manifest.py write-d1 D1_DIR           -> d1-manifest.tsv (path, sha256)
  manifest.py write-d2 ISSUES_JSON      -> d2-manifest.tsv (number, sha256 of normalized content) + d2-meta.json
  manifest.py check DATA_DIR            -> verifies both; reports unavailable or drift, exits non-zero
"""
import datetime, hashlib, json, pathlib, sys

HERE = pathlib.Path(__file__).parent


def norm(i):
    return json.dumps({"number": i["number"], "title": i["title"], "body": i["body"].replace("\r\n", "\n"),
                       "labels": sorted(i["labels"])}, sort_keys=True, ensure_ascii=False).encode()


def d1_rows(d):
    return [(p.name, hashlib.sha256(p.read_bytes()).hexdigest()) for p in sorted(pathlib.Path(d).glob("*.md"))]


def d2_rows(issues):
    return [(str(i["number"]), hashlib.sha256(norm(i)).hexdigest()) for i in issues]


def write(name, rows):
    (HERE / name).write_text("".join(f"{a}\t{b}\n" for a, b in rows))


def read(name):
    return [tuple(l.split("\t")) for l in (HERE / name).read_text().splitlines()]


cmd = sys.argv[1]
if cmd == "write-d1":
    rows = d1_rows(sys.argv[2]); write("d1-manifest.tsv", rows); print(len(rows), "D1 files")
elif cmd == "write-d2":
    issues = json.load(open(sys.argv[2])); rows = d2_rows(issues); write("d2-manifest.tsv", rows)
    (HERE / "d2-meta.json").write_text(json.dumps({
        "fetched_utc": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "issues": len(issues), "max_updated_at": max(i["updated_at"] for i in issues),
        "source": "GET /repos/rune-rs/rune/issues?state=all, pull requests excluded"}, indent=1) + "\n")
    print(len(rows), "D2 issues")
elif cmd == "check":
    data = pathlib.Path(sys.argv[2]); ok = True
    d1 = data / "d1" / "plans"
    if not d1.is_dir():
        print("D1 unavailable"); ok = False
    elif d1_rows(d1) != read("d1-manifest.tsv"):
        print("D1 drift"); ok = False
    p = data / "d2" / "issues.json"
    if not p.exists():
        print("D2 unavailable: no retained snapshot"); ok = False
    else:
        # compared as ordered lists, not dicts: a duplicated, missing, reordered
        # or changed issue is drift, and the count must be the manifest's
        want, got = read("d2-manifest.tsv"), d2_rows(json.load(open(p)))
        numbers = [n for n, _ in got]
        dupes = sorted({n for n in numbers if numbers.count(n) > 1}, key=int)
        if dupes:
            print("D2 drift: duplicate issues", " ".join(dupes)); ok = False
        if len(got) != len(want):
            print(f"D2 drift: {len(got)} issues, the manifest has {len(want)}"); ok = False
        if numbers != sorted(numbers, key=int):
            print("D2 drift: the snapshot isn't in issue-number order"); ok = False
        w, g = dict(want), dict(got)
        changed = sorted(set(w) ^ set(g) | {k for k in w if k in g and w[k] != g[k]}, key=int)
        if changed:
            print("D2 drift, issues:", " ".join(changed)); ok = False
        if ok and got != want:
            print("D2 drift: the rows differ from the manifest"); ok = False
    print("datasets verified" if ok else "NOT verified"); sys.exit(0 if ok else 1)
