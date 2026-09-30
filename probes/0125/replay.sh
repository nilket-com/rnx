#!/usr/bin/env bash
# Record 0125: replay the loading probe. The Rust twins are run at v2; the
# before state is 0124's tree (40f0334: its generator, releases and adapter
# source) and the after state is the committed tree. Both are probed at v2
# and at 0.55.2 with this record's probe, which runs the same W1 recipe
# (read_json) before and after, and every table is byte-compared with the
# committed ones. Needs the v2 scratch adapter (probes/0108/build.sh).
# PROBE_INIT=1 writes the tables and twins on the first run.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd); here="$root/probes/0125"
d="$root/target/0125/replay"; rm -rf "$d"; mkdir -p "$d"
inv="$root/probes/0108/out/v2py/result/inventory.json"
buckets=mechanical,conversion,option_struct,callback,generic_fn,generic
scratch="$root/target/0108/adapter/polars"
fixed=(A.collect_dtypes E.schema_display F.dtype_expr_arg P.preview_dtypes Q.preview_columns)
nightly() { ( cd "$1" && CARGO_TARGET_DIR="$2" cargo +nightly-2026-09-20 build -q --locked --release --features generated,test-support --bin rnx-polars --bin rnx-polars-fixture ); }
stable() { cargo build -q --release --locked --manifest-path "$1/Cargo.toml" --target-dir "$2" --features generated,test-support --bin rnx-polars --bin rnx-polars-fixture; }
probe() { # binary dir, twins, out, pin, extra fixed families
	local b="$1" t="$2" o="$3" pin="$4"; shift 4
	python3 "$here/probe.py" "$b/release/rnx-polars" "$t" "$o" "$pin" --fixed "${fixed[@]}" "$@" --display "$b/release/rnx-polars-fixture" > /dev/null
}
fin() { # PROBE_INIT=1: the first run writes the committed file
	if [ "${PROBE_INIT:-}" = 1 ]; then cp "$1" "$2"; fi
	cmp "$1" "$2"
}

# the Rust twins, at v2
trap 'rm -f "$scratch/tests/probe0125_twins.rs"' EXIT
cp "$here/twins.rs" "$scratch/tests/probe0125_twins.rs"
( cd "$scratch" && PROBE_DATA="$here/data" PROBE_OUT="$d" CARGO_TARGET_DIR="$root/target/0108/adapter-target" \
	cargo +nightly-2026-09-20 test -q --locked --release --features generated,test-support --test probe0125_twins ) > "$d/twins.log" 2>&1
# the twins, through the probe's own normalization (a join's row order and
# the std's last ulp vary from run to run in Polars itself, as in 0122)
if [ "${PROBE_INIT:-}" = 1 ]; then cp "$d/twins.json" "$here/twins-v2.json"; fi
python3 - "$d/twins.json" "$here/twins-v2.json" "$here" <<'PY'
import json, sys
sys.path.insert(0, sys.argv[3])
import probe
a, b = (json.load(open(p)) for p in sys.argv[1:3])
norm = lambda k, v: probe.approx(probe.unordered(v)) if k in probe.UNORDERED | probe.APPROX else v
assert a.keys() == b.keys(), a.keys() ^ b.keys()
bad = [k for k in a if norm(k, a[k]) != norm(k, b[k])]
assert not bad, f"twins moved: {bad}"
PY

# before: 0124's tree
wt="$d/wt124"; git -C "$root" worktree add -q "$wt" 40f0334
trap 'rm -f "$scratch/tests/probe0125_twins.rs"; git -C "$root" worktree remove --force "$wt" 2>/dev/null || true' EXIT
( cd "$wt" && CARGO_TARGET_DIR="$root/target/0125/gen124" cargo build -q --release --locked --manifest-path tools/polars-gen/Cargo.toml )
b="$d/before-v2/polars"; mkdir -p "$(dirname "$b")"; cp -r "$scratch" "$b"; rm -rf "$b/target" "$b/src" "$b/tests"
cp -r "$wt/adapters/polars/src" "$wt/adapters/polars/tests" "$b/"
( cd "$wt" && "$root/target/0125/gen124/release/polars-gen" "$inv" "$b" --release probes/0108/v2.toml --buckets $buckets ) > "$d/gen-before.log"
nightly "$b" "$root/target/0125/before-v2-target"
probe "$root/target/0125/before-v2-target" "$here/twins-v2.json" "$d/before" v2
stable "$wt/adapters/polars" "$root/target/0125/before-0.55.2-target"
probe "$root/target/0125/before-0.55.2-target" - "$d/before" 0.55.2

# after: the committed tree
nightly "$scratch" "$root/target/0108/adapter-target"
probe "$root/target/0108/adapter-target" "$here/twins-v2.json" "$d" v2 B.read_csv_schema C1.json_file D.path_type
stable "$root/adapters/polars" "$root/target/0125/after-0.55.2-target"
probe "$root/target/0125/after-0.55.2-target" - "$d" 0.55.2 B.read_csv_schema C1.json_file D.path_type

for f in outcomes-v2 outcomes-0.55.2; do
	fin "$d/before/$f.json" "$here/$f-before.json"
	fin "$d/$f.json" "$here/$f.json"
done
bash "$here/rekey_check.sh" > /dev/null
echo "0125 replayed: the twins, before and after tables at v2 and 0.55.2, and the rekey"
