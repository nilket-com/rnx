#!/bin/sh
# Record 0133: fetch every rune-rs/rune issue (pull requests excluded) into
# $RNX_UAT_DATA/d2/issues.json. The text stays local; only the manifest
# (numbers and normalized-content hashes) is committed.
set -eu
out=${RNX_UAT_DATA:?set RNX_UAT_DATA}/d2
rm -rf "$out/pages"
mkdir -p "$out/pages"
# cursor pagination: follow the Link header's rel="next" until there is none
# (page numbers are refused past 1,000 results)
url="https://api.github.com/repos/rune-rs/rune/issues?state=all&per_page=100"
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
python3 - "$out" <<'PY'
import json, pathlib, sys
out = pathlib.Path(sys.argv[1])
items = []
for p in sorted((out / "pages").glob("*.json"), key=lambda p: int(p.stem)):
    for i in json.load(open(p)):
        if "pull_request" in i:
            continue
        items.append({"number": i["number"], "title": i["title"], "body": (i["body"] or "").replace("\r\n", "\n"),
                      "labels": sorted(l["name"] for l in i["labels"]), "state": i["state"], "updated_at": i["updated_at"]})
items.sort(key=lambda i: i["number"])
json.dump(items, open(out / "issues.json", "w"))
print(len(items), "issues")
PY
