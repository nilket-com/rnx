"""Record 0131: rnx against the independent reference (sentence-transformers).

  compare.py DATA_DIR    (reads rnx.json and ref.json there)

For every text: the cosine between rnx's embedding and the reference's is at
least 0.9999; every score is within 1e-4; each query's top 5 is identical.
The controls compare too, the halved batches against the unhalved reference.
"""
import json, sys
import numpy as np

d = sys.argv[1]
ours, ref = json.load(open(f"{d}/rnx.json")), json.load(open(f"{d}/ref.json"))


def cos_rows(a, b):
    a, b = np.asarray(a, np.float64), np.asarray(b, np.float64)
    return (a * b).sum(1) / np.linalg.norm(a, axis=1) / np.linalg.norm(b, axis=1)


report, ok = {}, True
for key in ["docs", "queries"]:
    c = cos_rows(ours[key], ref[key]); report[key] = float(c.min()); ok &= c.min() >= 0.9999
s, r = np.asarray(ours["scores"]), np.asarray(ref["scores"])
report["scores_max_abs_diff"] = float(np.abs(s - r).max()); ok &= report["scores_max_abs_diff"] <= 1e-4
tops = []
for q in range(s.shape[1]):
    a, b = list(np.argsort(-s[:, q], kind="stable")[:5]), list(np.argsort(-r[:, q], kind="stable")[:5])
    tops.append(a == b); ok &= a == b
    # the margin between 5th and 6th, so an identical top 5 isn't luck
    report.setdefault("top5_margin_min", 1.0)
    srt = np.sort(s[:, q])[::-1]; report["top5_margin_min"] = min(report["top5_margin_min"], float(srt[4] - srt[5]))
report["top5_identical"] = f"{sum(tops)}/{len(tops)}"
for k, v in ours["controls"].items():
    base = k.removesuffix("_halved")
    c = cos_rows(v, ref["controls"][base]); report[f"control:{k}"] = float(c.min()); ok &= c.min() >= 0.9999
print(json.dumps(report, indent=1))
print("PASS" if ok else "FAIL")
sys.exit(0 if ok else 1)
