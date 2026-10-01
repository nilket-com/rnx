#!/usr/bin/env python3
"""Record 0129: launch cost, the 0127/0128 method (60 interleaved launches of
`pub fn main(_) { }` per round, medians compared).

  launch.py build          # rnx-polars at 65d32e6 (0128) and HEAD, rnx-candle at HEAD
  launch.py run ROUND      # writes launch-results-ROUND.json
"""
import json, os, pathlib, random, statistics, subprocess, sys, tempfile, time

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = ROOT / "target/0129"
BASE = "65d32e6"
BIN = {
    "old": OUT / "launch-old/release/rnx-polars",
    "new": OUT / "launch-new/release/rnx-polars",
    "candle": OUT / "launch-candle/release/rnx-candle",
}
SCRIPT = "pub fn main(_) { }"


def build():
    tree = OUT / "base-tree"
    if not tree.exists():
        subprocess.run(["git", "worktree", "add", "--detach", str(tree), BASE], cwd=ROOT, check=True)
    for name, src in [("old", tree / "adapters/polars"), ("new", ROOT / "adapters/polars"),
                      ("candle", ROOT / "adapters/candle")]:
        env = dict(os.environ, CARGO_TARGET_DIR=str(OUT / f"launch-{name}"))
        subprocess.run(["cargo", "build", "--release"], cwd=src, env=env, check=True)


def once(binary, script):
    t = time.perf_counter()
    subprocess.run([str(binary), "run", script], stdin=subprocess.DEVNULL,
                   stdout=subprocess.DEVNULL, check=True)
    return (time.perf_counter() - t) * 1e3


def run(round_):
    script = pathlib.Path(tempfile.mkdtemp()) / "empty.rn"
    script.write_text(SCRIPT)
    samples = {k: [] for k in BIN}
    for k in BIN:
        once(BIN[k], script)
    for _ in range(60):
        order = list(BIN)
        random.shuffle(order)
        for k in order:
            samples[k].append(round(once(BIN[k], script), 3))
    res = {k: {"runs": len(v), "median_ms": round(statistics.median(v), 2), "min_ms": min(v),
               "p90_ms": round(sorted(v)[int(len(v) * 0.9)], 2), "samples_ms": v}
           for k, v in samples.items()}
    out = pathlib.Path(__file__).parent / f"launch-results-{round_}.json"
    out.write_text(json.dumps({"script": SCRIPT, "base": BASE,
                               "binaries": {k: str(v.relative_to(ROOT)) for k, v in BIN.items()},
                               "results": res}, indent=1) + "\n")
    print(round_, {k: res[k]["median_ms"] for k in res},
          "delta new-old", round(res["new"]["median_ms"] - res["old"]["median_ms"], 2))


if __name__ == "__main__":
    build() if sys.argv[1] == "build" else run(sys.argv[2])
