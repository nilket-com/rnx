#!/usr/bin/env python3
"""Record 0073: process-launch cost of adapter binaries, raw samples kept.

  launch.py <label=binary> [<label=binary> ...] [--runs N] [--out file.json]

Runs an empty script through each binary, interleaving binaries run by
run so host drift affects all alike, and writes every sample.
"""
import json, os, statistics, subprocess, sys, tempfile, time
args = []
skip = False
for a in sys.argv[1:]:
    if skip: skip = False; continue
    if a.startswith("--"): skip = a in ("--runs", "--out"); continue
    args.append(a)
runs = int(sys.argv[sys.argv.index("--runs") + 1]) if "--runs" in sys.argv else 40
out = sys.argv[sys.argv.index("--out") + 1] if "--out" in sys.argv else None
bins = [a.split("=", 1) for a in args]
script = os.path.join(tempfile.mkdtemp(), "empty.rn")
open(script, "w").write("pub fn main(_) { }\n")
samples = {label: [] for label, _ in bins}

def launch(label, path):
    """One launch; a nonzero exit or any output fails the measurement."""
    t0 = time.perf_counter()
    r = subprocess.run([path, "run", script], stdin=subprocess.DEVNULL, capture_output=True, text=True)
    dt = (time.perf_counter() - t0) * 1000
    if r.returncode != 0 or r.stdout or r.stderr:
        sys.stderr.write(f"launch of {label} ({path}) failed: exit {r.returncode}\nstdout: {r.stdout[:500]!r}\nstderr: {r.stderr[:500]!r}\n")
        sys.exit(1)
    return dt

for label, path in bins:  # one warm-up each, not recorded
    launch(label, path)
for _ in range(runs):
    for label, path in bins:
        samples[label].append(launch(label, path))
report = {}
for label, _ in bins:
    s = sorted(samples[label])
    report[label] = {"runs": runs, "median_ms": round(statistics.median(s), 2), "min_ms": round(s[0], 2), "p90_ms": round(s[int(len(s) * 0.9)], 2), "samples_ms": [round(x, 3) for x in samples[label]]}
    print(f"{label}: median {report[label]['median_ms']} ms, min {report[label]['min_ms']} ms, p90 {report[label]['p90_ms']} ms ({runs} interleaved launches)")
if out:
    json.dump({"script": "pub fn main(_) { }", "binaries": dict(bins), "results": report}, open(out, "w"), indent=1)
