#!/usr/bin/env bash
# Record 0120: replay the reachability probe at both pins and compare with
# the committed route tables. Needs the v2 scratch adapter (probes/0108/build.sh).
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
here="$root/probes/0120"
tmp=$(mktemp -d)
trap 'rm -f "$root/adapters/polars/tests/reach0120.rs" "$root/target/0108/adapter/polars/tests/reach0120.rs"; rm -rf "$tmp"' EXIT
cp "$here/reach.rs" "$root/adapters/polars/tests/reach0120.rs"
REACH_OUT="$tmp/reach-0.55.2.json" cargo test -q --release --locked \
	--manifest-path "$root/adapters/polars/Cargo.toml" --features generated,test-support \
	--test reach0120 > "$tmp/0.55.2.log" 2>&1 || { tail -20 "$tmp/0.55.2.log"; exit 1; }
cp "$here/reach.rs" "$root/target/0108/adapter/polars/tests/reach0120.rs"
( cd "$root/target/0108/adapter/polars" && REACH_OUT="$tmp/reach-v2.json" \
	CARGO_TARGET_DIR="$root/target/0108/adapter-target" cargo +nightly-2026-09-20 test -q \
	--locked --release --features generated,test-support --test reach0120 ) > "$tmp/v2.log" 2>&1 \
	|| { tail -20 "$tmp/v2.log"; exit 1; }
for pin in 0.55.2 v2; do
	cmp -s "$tmp/reach-$pin.json" "$here/reach-$pin.json" || { echo "reach-$pin.json differs"; diff "$here/reach-$pin.json" "$tmp/reach-$pin.json" | head; exit 1; }
done
echo "0120 reachability replayed: both pins match"
