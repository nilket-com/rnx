#!/usr/bin/env bash
# Record 0122: replay the workflow probe. Recomputes the v2 twins and the
# v2 outcome table (after Stage 2's fix, `--fixed A.collect_dtypes`) and the
# 0.55.2 outcome table (exit status only), and byte-compares each with the
# committed tables. Needs the v2 scratch adapter (probes/0108/build.sh).
# The before table is rebuilt from the pre-fix source (before_lib.py) and
# compared too.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd); here="$root/probes/0122"
d="$root/target/0122/replay"; rm -rf "$d"; mkdir -p "$d"
scratch="$root/target/0108/adapter/polars"
trap 'rm -f "$scratch/tests/probe0122_twins.rs"' EXIT
cp "$root/adapters/polars/src/lib.rs" "$scratch/src/lib.rs"
cp "$here/twins.rs" "$scratch/tests/probe0122_twins.rs"
( cd "$scratch" && PROBE_DATA="$here/data" PROBE_OUT="$d" CARGO_TARGET_DIR="$root/target/0108/adapter-target" \
	cargo +nightly-2026-09-20 test -q --locked --release --features generated,test-support --test probe0122_twins ) > "$d/twins.log" 2>&1 \
	|| { tail -20 "$d/twins.log"; exit 1; }
# the twins, through the probe's own normalization (a left join's row order
# and the std's last ulp vary from run to run in Polars itself)
python3 - "$d/twins.json" "$here/twins-v2.json" <<'PY'
import json, sys
sys.path.insert(0, sys.argv[0] and __import__("os").path.dirname(sys.argv[2]))
import probe
a, b = (json.load(open(p)) for p in sys.argv[1:3])
norm = lambda k, v: probe.approx(probe.unordered(v)) if k in probe.UNORDERED | probe.APPROX else v
assert a.keys() == b.keys(), a.keys() ^ b.keys()
bad = [k for k in a if norm(k, a[k]) != norm(k, b[k])]
assert not bad, f"twins moved: {bad}"
PY
build() { ( cd "$scratch" && CARGO_TARGET_DIR="$root/target/0108/adapter-target" \
	cargo +nightly-2026-09-20 build -q --locked --release --features generated,test-support --bin rnx-polars ) > "$d/v2-build.log" 2>&1; }
# the before table: the pre-fix source (plan commit b7d2c17's lib.rs plus the
# test-support oracle_repr), probed without --fixed
git -C "$root" show b7d2c17:adapters/polars/src/lib.rs > "$d/lib-plan.rs"
python3 "$here/before_lib.py" "$d/lib-plan.rs" "$root/adapters/polars/src/lib.rs" "$scratch/src/lib.rs"
build
mkdir -p "$d/before"
python3 "$here/probe.py" "$root/target/0108/adapter-target/release/rnx-polars" "$d/twins.json" "$d/before" v2 > /dev/null
cmp "$d/before/outcomes-v2.json" "$here/outcomes-v2-before.json"
# the after table: the committed source, with Stage 2's family fixed
cp "$root/adapters/polars/src/lib.rs" "$scratch/src/lib.rs"
build
python3 "$here/probe.py" "$root/target/0108/adapter-target/release/rnx-polars" "$d/twins.json" "$d" v2 --fixed A.collect_dtypes > /dev/null
cmp "$d/outcomes-v2.json" "$here/outcomes-v2.json"
cargo build -q --release --locked --manifest-path "$root/adapters/polars/Cargo.toml" --features generated,test-support --bin rnx-polars
python3 "$here/probe.py" "$root/adapters/polars/target/release/rnx-polars" - "$d" 0.55.2 > /dev/null
cmp "$d/outcomes-0.55.2.json" "$here/outcomes-0.55.2.json"
echo "0122 probe replayed: twins, the before and after v2 tables, and the 0.55.2 table all match"
