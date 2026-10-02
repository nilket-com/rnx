#!/bin/sh
# Record 0153: fetch every rhaiscript/rhai issue (pull requests excluded) into
# $RNX_UAT_DATA/d3/issues.json, 0133's method: unauthenticated GitHub REST,
# state=all, full cursor pagination following rel="next". The text stays
# local; manifest.py commits only numbers and hashes.
set -eu
out=${RNX_UAT_DATA:?set RNX_UAT_DATA}/d3
[ -e "$out/issues.json" ] && { echo "refusing: $out/issues.json exists (the snapshot is the first fetch)" >&2; exit 1; }
rm -rf "$out/pages"
mkdir -p "$out/pages"
url="https://api.github.com/repos/rhaiscript/rhai/issues?state=all&per_page=100"
page=1
while [ -n "$url" ]; do
	curl -sSf -H 'Accept: application/vnd.github+json' -D "$out/pages/$page.headers" "$url" -o "$out/pages/$page.json"
	url=$(python3 -c "
import re, sys
links = [l for l in open(sys.argv[1]) if l.lower().startswith('link:')]
m = re.search(r'<([^>]+)>; rel=\"next\"', links[0]) if links else None
print(m.group(1) if m else '')" "$out/pages/$page.headers")
	page=$((page + 1))
done
date -u +%Y-%m-%dT%H:%M:%SZ > "$out/fetched_utc"
python3 - "$out" <<'PY'
import json, pathlib, sys
out = pathlib.Path(sys.argv[1])
items = []
for p in sorted((out / "pages").glob("*.json"), key=lambda p: int(p.stem)):
    for i in json.load(open(p)):
        if "pull_request" in i:
            continue
        items.append({"number": i["number"], "title": i["title"], "body": (i["body"] or "").replace("\r\n", "\n"),
                      "labels": sorted(l["name"] for l in i["labels"]), "state": i["state"],
                      "created_at": i["created_at"], "updated_at": i["updated_at"]})
items.sort(key=lambda i: i["number"])
assert len({i["number"] for i in items}) == len(items), "duplicate issue numbers"
json.dump(items, open(out / "issues.json", "w"))
print(len(items), "issues")
PY
