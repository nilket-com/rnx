"""Record 0153: D3's committed provenance (plans/0153 section 1, R3).

  manifest.py write D3_DIR     writes frozen/d3-manifest.tsv and frozen/d3-meta.json
  manifest.py check D3_DIR     refuses any drift of text, labels, state or snapshot

Per issue: the canonical decimal number; content = sha256(title + "\\n" + body);
labels = sha256(sorted label names joined by "\\n"). The snapshot hash is
sha256 of json.dumps(issues, sort_keys=True, separators=(",", ":"),
ensure_ascii=False) in UTF-8. Label names are never committed."""
import hashlib, json, pathlib, sys

HERE = pathlib.Path(__file__).resolve().parent
FROZEN = HERE / "frozen"


def sha(s):
    return hashlib.sha256(s.encode("utf-8")).hexdigest()


def build(d3):
    issues = json.load(open(pathlib.Path(d3) / "issues.json"))
    rows = [f"{i['number']}\t{sha(i['title'] + chr(10) + i['body'])}\t{sha(chr(10).join(sorted(i['labels'])))}"
            for i in issues]
    snap = sha(json.dumps(issues, sort_keys=True, separators=(",", ":"), ensure_ascii=False))
    meta = {"source": "GET /repos/rhaiscript/rhai/issues?state=all, pull requests excluded",
            "fetched_utc": (pathlib.Path(d3) / "fetched_utc").read_text().strip(),
            "issues": len(issues), "max_updated_at": max(i["updated_at"] for i in issues), "snapshot_sha256": snap}
    return "number\tcontent\tlabels\n" + "".join(r + "\n" for r in rows), meta


mode, d3 = sys.argv[1:3]
manifest, meta = build(d3)
if mode == "write":
    (FROZEN / "d3-manifest.tsv").write_text(manifest)
    (FROZEN / "d3-meta.json").write_text(json.dumps(meta, indent=1) + "\n")
    print(f"wrote the manifest: {meta['issues']} issues, snapshot {meta['snapshot_sha256'][:16]}…")
else:
    if (FROZEN / "d3-manifest.tsv").read_text() != manifest:
        sys.exit("FAIL: D3 drifted from frozen/d3-manifest.tsv (text, labels or the issue set)")
    if json.loads((FROZEN / "d3-meta.json").read_text()) != meta:
        sys.exit("FAIL: D3's snapshot drifted from frozen/d3-meta.json")
    print(f"D3: {meta['issues']} issues match the manifest, label hashes and snapshot")
