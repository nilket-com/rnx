#!/bin/sh
# Run on the frozen image pair. Both providers use the image-controls feature.
# Arguments: stock runner, ordinary and extended launchers, demo data, stock installer oracle,
# optionally a prebuilt optimized observer launcher.
set -eu
stock=$(realpath -- "$1")
plain=$(realpath -- "$2")
extended=$(realpath -- "$3")
data=$(realpath -- "$4")
oracle=$(realpath -- "${5:?expected stock_file_inventory binary}")
here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
counters=$here/file-effect-counters.sh
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
if [ "$#" -ge 6 ]; then
	observer=$(realpath -- "$6")
else
	root=$(realpath "$here/../../..")
	build=$(realpath -m "${CARGO_TARGET_DIR:-$root/target}")
	cargo build --manifest-path "$root/Cargo.toml" --target-dir "$build" --locked --offline \
		--release -p rnx-engine-image --example engine_image --features image-controls
	cargo build --manifest-path "$root/demos/candle/Cargo.toml" --target-dir "$build" --locked --offline \
		--release --example file_provider --features image-controls
	"$here/../build-adapter-launcher.sh" "$build/release/examples/libengine_image.so" \
		"$build/release/examples/libfile_provider.a" "$tmp/observer" "$oracle" --observe
	observer=$tmp/observer/rnx
fi
# Record the archive, launcher and DSO identities on both measured paths.
for launcher in "$plain" "$observer"; do
	receipt="$(dirname "$launcher")/artifacts.sha256"
	test "$(wc -l < "$receipt")" -eq 5
	for entry in "4:$launcher" "5:$(dirname "$launcher")/libengine_image.so"; do
		expected=$(awk -v n="${entry%%:*}" 'NR==n { print $1 }' "$receipt")
		test "$(sha256sum <"${entry#*:}" | cut -d' ' -f1)" = "$expected"
	done
	cat "$receipt"
done
unset POLARS_PANIC_ON_ERR POLARS_BACKTRACE_IN_ERR RNX_TEST_ADAPTER_CELLS
# Compare the installed stock surface to the table submitted by the launcher.
# The mutants alter actual registration or table cardinality before enumeration.
export LC_ALL=C
"$oracle" --inventory >"$tmp/stock-inventory"
"$plain" inventory >"$tmp/provider-inventory"
sort "$tmp/stock-inventory" >"$tmp/stock-rows"
sort "$tmp/provider-inventory" >"$tmp/provider-rows"
compare_rows() { diff -u "$1" "$2"; }
expect_difference() {
	code=0
	compare_rows "$1" "$2" >"$tmp/inventory-diff" || code=$?
	test "$code" -eq 1
}
compare_rows "$tmp/stock-rows" "$tmp/provider-rows"
"$oracle" --inventory-extra-stock >"$tmp/extra"
sort "$tmp/extra" >"$tmp/extra-rows"
expect_difference "$tmp/extra-rows" "$tmp/provider-rows"
test "$(comm -23 "$tmp/extra-rows" "$tmp/provider-rows" | cut -f1)" = "polars::planted_inventory_extra"
"$plain" inventory-missing >"$tmp/missing-table"
sort "$tmp/missing-table" >"$tmp/missing-rows"
expect_difference "$tmp/stock-rows" "$tmp/missing-rows"
test "$(comm -23 "$tmp/stock-rows" "$tmp/missing-rows" | wc -l)" -eq 1
"$extended" inventory >"$tmp/extended-inventory"
sort "$tmp/extended-inventory" >"$tmp/extended-rows"
expect_difference "$tmp/stock-rows" "$tmp/extended-rows"
test "$(comm -13 "$tmp/stock-rows" "$tmp/extended-rows" | cut -f1)" = "candle::Tensor::fixture_copy"
frame='let f=polars::read_csv(`${args[0]}/features.csv`,[("id","i64"),("a","f64"),("b","f64"),("c","f64")])?;'
dense="$frame"'let d=f.to_dense(["a","b","c"],"f32")?;'
tensor="$dense"'let x=candle::Tensor::from_dense(d)?;'
model='let m=candle::Mlp::load(`${args[0]}/mlp.safetensors`)?;'
script() { printf 'pub fn main(args){%s}\n' "$1" >"$tmp/main.rn"; }
compare() {
	script "$1"
	"$stock" run --debug-engine "$tmp/main.rn" "$data" >"$tmp/want" 2>"$tmp/route"
	grep -Fxq 'rnx: engine=execution native-requested=on' "$tmp/route"
	"$plain" checked "$tmp/main.rn" "$data" >"$tmp/got" 2>"$tmp/err"
	test ! -s "$tmp/err"
	cmp "$tmp/want" "$tmp/got"
}
declines() {
	code=0
	"$oracle" "$tmp/main.rn" >"$tmp/want-refusal"
	"$plain" checked-refusal "$tmp/main.rn" "$data" >"$tmp/got" 2>"$tmp/err" || code=$?
	test "$code" -eq 2
	test ! -s "$tmp/got"
	grep -Fq "refused: $1 " "$tmp/want-refusal"
	cmp "$tmp/want-refusal" "$tmp/err"
	"$counters" checked-refusal "$observer" "$tmp/main.rn" "$data"
}
script "${frame}f.width()"
"$counters" checked "$observer" "$tmp/main.rn" "$data"
for expr in 'f.width()' 'format!("{f}")'; do compare "$frame$expr"; done
for expr in 'd.shape()' 'd.names()' 'd.dtype()' 'format!("{d}")'; do compare "$dense$expr"; done
for expr in 'x.rank()' 'x.elem_count()' 'format!("{x}")'; do compare "$tensor$expr"; done
compare "${model}m.dims()"
for op in exp log sqrt sqr abs neg recip tanh relu gelu gelu_erf silu sin cos floor ceil round sign erf; do
	compare "${tensor}x.$op()?.rank()"
done
compare "${tensor}${model}m.forward(x)?.elem_count()"
RNX_TEST_ADAPTER_CELLS="$tmp/scores" "$plain" checked "$tmp/main.rn" "$data" >"$tmp/got"
test "$(cat "$tmp/got")" = 64
# Final row contains the 64 f32 bit patterns from the model/CSV reference.
tail -n 1 "$tmp/scores" >"$tmp/row"
test "$(sha256sum <"$tmp/row" | cut -d' ' -f1)" = 5fd7e752a02335992226985b3c25d23d43a89d7ed43672f3f6fc58923ccedd86
script "${tensor}x.fixture_copy()?.rank()"
declines OpaqueReceiver
RNX_TEST_ADAPTER_CELLS="$tmp/copy" "$extended" checked "$tmp/main.rn" "$data" >"$tmp/got"
test "$(cat "$tmp/got")" = 2
test "$(wc -l <"$tmp/copy")" -eq 2
head -n 1 "$tmp/copy" >"$tmp/before"
tail -n 1 "$tmp/copy" >"$tmp/after"
cmp "$tmp/before" "$tmp/after"
script "${model}let (i,h,o)=m.dims();o"
test "$("$plain" checked "$tmp/main.rn" "$data")" = 1
test "$("$extended" checked "$tmp/main.rn" "$data")" = 2
script "${frame}f.height()?"
declines OpaqueOperation
script 'polars::DataFrame::empty().width()'
declines OpaqueEntry
script "${frame}f.width()"
(export POLARS_PANIC_ON_ERR=1; declines OpaqueEntry)
script 'polars::version()'
declines Native
script 'polars::SortMultipleOptions::default_()'
declines Native
script 'polars::not_a_registered_operation_0320()'
declines FrontendDiagnostic
cp "$data/../candle/model.rn" "$tmp/main.rn"
declines OpaqueReceiver
for body in 'candle::Mlp::load(args[0])?.dims()' 'polars::read_csv(args[0],[("id","bad")])?.width()'; do
	script "$body"
	old=0; new=0
	"$stock" run "$tmp/main.rn" "$tmp/missing" >"$tmp/want" 2>"$tmp/stock-error" || old=$?
	"$plain" checked "$tmp/main.rn" "$tmp/missing" >"$tmp/got" 2>"$tmp/err" || new=$?
	test "$old" -eq 1 && test "$new" -eq 3 && test -s "$tmp/err"
	cmp "$tmp/want" "$tmp/got"
	cmp "$tmp/stock-error" "$tmp/err"
done
