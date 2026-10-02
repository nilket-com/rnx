"""Record 0145, D3: daily English Wikipedia pageviews (user agents, all
access) for four programming-language articles, 2023-07-01 to 2026-09-30,
from the Wikimedia REST API (public domain data). Frozen before any analysis:
the TSV and its SHA-256 are committed.

  fetch_pageviews.py OUT_TSV"""
import json, sys, urllib.request

ARTICLES = ["Rust_(programming_language)", "Python_(programming_language)",
            "Go_(programming_language)", "Zig_(programming_language)"]
START, END = "20230701", "20260930"
UA = "rnx-record-0145 (https://github.com/nilket-com/rnx)"
rows = {}
for a in ARTICLES:
    url = ("https://wikimedia.org/api/rest_v1/metrics/pageviews/per-article/"
           f"en.wikipedia/all-access/user/{a}/daily/{START}/{END}")
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    items = json.load(urllib.request.urlopen(req, timeout=60))["items"]
    for it in items:
        rows.setdefault(it["timestamp"][:8], {})[a] = it["views"]
days = sorted(rows)
with open(sys.argv[1], "w") as f:
    f.write("date\t" + "\t".join(ARTICLES) + "\n")
    for d in days:
        f.write(d + "\t" + "\t".join(str(rows[d].get(a, "")) for a in ARTICLES) + "\n")
print(len(days), "days;", "missing cells:", sum(1 for d in days for a in ARTICLES if a not in rows[d]))
