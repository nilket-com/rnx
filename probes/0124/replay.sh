#!/usr/bin/env bash
# Record 0124: replay the display check. The only source this record changes
# is the hand-written preview (adapters/polars/src/preview.rs), so the before
# state is the committed tree with 0123's preview.rs (7629cd3) and the after
# state is the committed tree. Both are probed at v2 and at 0.55.2 with this
# record's probe, and every table is byte-compared with the committed ones.
# Needs the v2 scratch adapter (probes/0108/build.sh). PROBE_INIT=1 writes
# the tables on the first run.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd); here="$root/probes/0124"
d="$root/target/0124/replay"; rm -rf "$d"; mkdir -p "$d"
fixed=(A.collect_dtypes E.schema_display F.dtype_expr_arg)
nightly() { ( cd "$1" && CARGO_TARGET_DIR="$2" cargo +nightly-2026-09-20 build -q --locked --release --features generated,test-support --bin rnx-polars --bin rnx-polars-fixture ); }
stable() { cargo build -q --release --locked --manifest-path "$1/Cargo.toml" --target-dir "$2" --features generated,test-support --bin rnx-polars --bin rnx-polars-fixture; }
probe() { # binary dir, twins, out, pin, extra --fixed
	local b="$1"; shift; local t="$1" o="$2" pin="$3"; shift 3
	python3 "$here/probe.py" "$b/release/rnx-polars" "$t" "$o" "$pin" --fixed "${fixed[@]}" "$@" --display "$b/release/rnx-polars-fixture" > /dev/null
}
sc="$root/target/0108/adapter/polars"; cp "$root/adapters/polars/src/lib.rs" "$sc/src/lib.rs"
# one target directory per pin: only the adapter crate differs between the
# states, so each state is probed before the next build replaces its binaries
for state in before after; do
	for pin in v2 0.55.2; do
		src=$([ "$pin" = v2 ] && echo "$sc" || echo "$root/adapters/polars")
		a="$d/$state-$pin/polars"; mkdir -p "$(dirname "$a")"; cp -r "$src" "$a"; rm -rf "$a/target"
		cp "$root/adapters/polars/src/preview.rs" "$a/src/preview.rs"
		# the copy sits elsewhere: point its rnx dependency at the repository
		sed -i "s|^rnx = { path = \"../..\"|rnx = { path = \"$root\"|" "$a/Cargo.toml"
		[ "$state" = before ] && git -C "$root" show 7629cd3:adapters/polars/src/preview.rs > "$a/src/preview.rs"
		t="$root/target/0124/$pin-target"
		if [ "$pin" = v2 ]; then nightly "$a" "$t"; tw="$here/twins-v2.json"; else stable "$a" "$t"; tw=-; fi
		o=$([ "$state" = before ] && echo "$d/before" || echo "$d")
		extra=(); [ "$state" = after ] && extra=(P.preview_dtypes Q.preview_columns)
		probe "$t" "$tw" "$o" "$pin" "${extra[@]}"
	done
done
for f in outcomes-v2 outcomes-0.55.2; do
	# PROBE_INIT=1: the first run only, which writes the committed tables
	if [ "${PROBE_INIT:-}" = 1 ]; then cp "$d/before/$f.json" "$here/$f-before.json"; cp "$d/$f.json" "$here/$f.json"; fi
	cmp "$d/before/$f.json" "$here/$f-before.json"
	cmp "$d/$f.json" "$here/$f.json"
done
echo "0124 replayed: before and after display tables at v2 and 0.55.2"
