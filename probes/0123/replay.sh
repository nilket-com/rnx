#!/usr/bin/env bash
# Record 0123: replay the probe with its display check. Rebuilds the before
# state (0122: e16b445's generator and release, with this record's lib.rs,
# whose only difference is none) and the after state, probes both at v2 with
# the display check, probes 0.55.2, and byte-compares every table with the
# committed ones. Needs the v2 scratch adapter (probes/0108/build.sh).
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd); here="$root/probes/0123"
d="$root/target/0123/replay"; rm -rf "$d"; mkdir -p "$d"
inv="$root/probes/0108/out/v2py/result/inventory.json"
buckets=mechanical,conversion,option_struct,callback,generic_fn,generic
nightly() { ( cd "$1" && CARGO_TARGET_DIR="$2" cargo +nightly-2026-09-20 build -q --locked --release --features generated,test-support --bin rnx-polars --bin rnx-polars-fixture ); }
# before: 0122's generation of v2
wt="$d/wt122"; git -C "$root" worktree add -q "$wt" e16b445
trap 'git -C "$root" worktree remove --force "$wt" 2>/dev/null || true' EXIT
( cd "$wt" && CARGO_TARGET_DIR="$root/target/0123/gen122" cargo build -q --release --locked --manifest-path tools/polars-gen/Cargo.toml )
mkdir -p "$d/before-adapter"; cp -r "$root/target/0108/adapter/polars" "$d/before-adapter/"; rm -rf "$d/before-adapter/polars/target"
( cd "$wt" && "$root/target/0123/gen122/release/polars-gen" "$inv" "$d/before-adapter/polars" --release probes/0108/v2.toml --buckets $buckets ) > "$d/gen-before.log"
cp "$root/adapters/polars/src/lib.rs" "$d/before-adapter/polars/src/lib.rs"
cp "$root/adapters/polars/src/generated/support.rs" "$d/before-adapter/polars/src/generated/support.rs"
nightly "$d/before-adapter/polars" "$root/target/0123/before-target"
python3 "$here/probe.py" "$root/target/0123/before-target/release/rnx-polars" "$here/twins-v2.json" "$d/before" v2 \
	--fixed A.collect_dtypes --display "$root/target/0123/before-target/release/rnx-polars-fixture" > /dev/null
cmp "$d/before/outcomes-v2.json" "$here/outcomes-v2-before.json"
# after: the committed source
sc="$root/target/0108/adapter/polars"
cp "$root/adapters/polars/src/lib.rs" "$sc/src/lib.rs"
nightly "$sc" "$root/target/0108/adapter-target"
python3 "$here/probe.py" "$root/target/0108/adapter-target/release/rnx-polars" "$here/twins-v2.json" "$d" v2 \
	--fixed A.collect_dtypes E.schema_display F.dtype_expr_arg --display "$root/target/0108/adapter-target/release/rnx-polars-fixture" > /dev/null
cmp "$d/outcomes-v2.json" "$here/outcomes-v2.json"
# 0.55.2
cargo build -q --release --locked --manifest-path "$root/adapters/polars/Cargo.toml" --features generated,test-support --bin rnx-polars --bin rnx-polars-fixture
python3 "$here/probe.py" "$root/adapters/polars/target/release/rnx-polars" - "$d" 0.55.2 --display "$root/adapters/polars/target/release/rnx-polars-fixture" > /dev/null
cmp "$d/outcomes-0.55.2.json" "$here/outcomes-0.55.2.json"
bash "$here/mutations.sh" > /dev/null
echo "0123 replayed: before and after v2 tables (with display), the 0.55.2 table, and the mutation controls"
