#!/bin/sh
# Behavioral controls for two finished, relocatable packages. No build inputs
# are mounted in the namespace where either package runs.
set -eu
stock=$(realpath "$1")
pair=$(realpath "$2")
data=$(realpath "$3")
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
cp -R "$stock" "$scratch/stock"
cp -R "$pair" "$scratch/pair"
mkdir "$scratch/data"
cp "$data/features.csv" "$data/mlp.safetensors" "$scratch/data/"
cat >"$scratch/check.rn" <<'RUNE'
pub fn main(args) {
	let frame = polars::read_csv(`${args[0]}/features.csv`, [("id","i64"),("a","f64"),("b","f64"),("c","f64")])?;
	let input = candle::Tensor::from_dense(frame.to_dense(["a","b","c"], "f32")?)?;
	let model = candle::Mlp::load(`${args[0]}/mlp.safetensors`)?;
	model.forward(input)?.elem_count()
}
RUNE
cat >"$scratch/module.rn" <<'RUNE'
pub fn run(args) { [path::join("a","b"),args,env::args()] }
RUNE
printf '%s\n' 'mod module; pub fn main(args) { module::run(args) }' >"$scratch/scalar.rn"
printf '%s\n' 'pub fn main(args) { fs::append(args[0], "x")?; b"refuse" }' >"$scratch/refusal.rn"
inside() {
	bwrap --die-with-parent --unshare-all --ro-bind /usr /usr --symlink usr/lib /lib --symlink usr/lib64 /lib64 \
		--symlink usr/bin /bin --ro-bind /etc /etc --dev /dev --proc /proc --tmpfs /tmp \
		--bind "$scratch" /fixture --chdir /fixture --clearenv --setenv PATH /usr/bin --setenv HOME /tmp "$@"
}
for kind in stock pair; do
	inside "/fixture/$kind/rnx" run --debug-engine scalar.rn 'two words' '' >"$scratch/out" 2>"$scratch/err"
	test "$(cat "$scratch/out")" = '["a/b", ["two words", ""], ["two words", ""]]'
	grep -Fxq 'rnx: engine=execution native-requested=on' "$scratch/err"
	if inside "/fixture/$kind/rnx" run refusal.rn /fixture/effect >"$scratch/out" 2>"$scratch/err"; then exit 1; fi
	grep -Fq 'execution refused:' "$scratch/err"
	test ! -e "$scratch/effect"
	if inside "/fixture/$kind/rnx" inventory >"$scratch/out" 2>"$scratch/err"; then exit 1; fi
	test ! -s "$scratch/out"
done
inside /fixture/pair/rnx run --debug-engine check.rn data >"$scratch/out" 2>"$scratch/err"
test "$(cat "$scratch/out")" = 64
grep -Fxq 'rnx: engine=execution native-requested=on' "$scratch/err"
test "$(sha256sum <"$scratch/stock/libengine_image.so")" = "$(sha256sum <"$scratch/pair/libengine_image.so")"
readelf -d "$scratch/pair/rnx" | grep -Fq '[$ORIGIN]'
test "$(readelf -d "$scratch/pair/rnx" | grep -Fc '[libengine_image.so]')" = 1
for mode in shadow substituted missing; do
	case $mode in
		shadow) set -- /usr/bin/env LD_LIBRARY_PATH=/fixture/stock ;;
		substituted) printf 'x' >>"$scratch/pair/libengine_image.so"; set -- ;;
		missing) rm "$scratch/pair/libengine_image.so"; set -- ;;
	esac
	if inside "$@" /fixture/pair/rnx run scalar.rn >"$scratch/out" 2>"$scratch/err"; then exit 1; fi
	test ! -s "$scratch/out"
	if [ "$mode" != missing ]; then grep -Fq 'engine image identity mismatch' "$scratch/err"; fi
done
