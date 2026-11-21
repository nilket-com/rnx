#!/bin/sh
# Runs against the exact shared object supplied by Cargo or the release build.
set -eu
src=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
image=$(realpath -- "$1")
out=$(mktemp -d)
trap 'rm -rf -- "$out"' EXIT
"$src/build-launchers.sh" "$image" "$out"
printf '%s\n' 'pub fn main(args){host::apply(40)}' > "$out/main.rn"
printf '%s\n' 'pub fn main(args){host::apply(40); b"bytes"}' > "$out/refused.rn"
printf '%s\n' 'pub fn main(args){host::apply(40); panic!("terminal")}' > "$out/failed.rn"
success() {
	"$out/$1/rnx" run --debug-engine --native=on "$out/main.rn" > "$out/stdout" 2> "$out/stderr"
	test "$(cat "$out/stdout")" = "$2"
	grep -Fxq 'rnx: engine=execution native-requested=on' "$out/stderr"
	grep -Fxq 'rnx: callback-count=1' "$out/stderr"
}
success plain 42
success tagged 43
mv "$out/plain" "$out/relocated"
success relocated 42
readelf -d "$out/relocated/rnx" > "$out/dynamic"
grep -Fq '[libengine_image.so]' "$out/dynamic"
grep -Fq '[$ORIGIN]' "$out/dynamic"
nm -D --defined-only "$image" | awk '{print $3}' > "$out/exports"
test "$(cat "$out/exports")" = "$(printf 'rnx_engine_handles_v2\nrnx_engine_run_file_v1')"
cc -Wall -Wextra -Werror -O2 -pthread "$src/tests/contracts.c" "-L$out/tagged" "-Wl,-rpath,$out/tagged" -lengine_image -o "$out/contracts"
"$out/contracts" "$out/main.rn" "$out/refused.rn" "$out/failed.rn" > "$out/calls"
sort -n "$out/calls" > "$out/sorted"
test "$(cat "$out/sorted")" = "$(printf '42\n43\n47\n49')"
refuse() {
	if "$@" run --debug-engine "$out/main.rn" > "$out/stdout" 2> "$out/stderr"; then exit 1; fi
	test ! -s "$out/stdout"
	! grep -q 'callback-count\|engine=execution' "$out/stderr"
}
refuse env LD_LIBRARY_PATH="$out/tagged" "$out/relocated/rnx"
grep -Fq 'identity mismatch' "$out/stderr"
printf x >> "$out/relocated/libengine_image.so"
refuse "$out/relocated/rnx"
grep -Fq 'identity mismatch' "$out/stderr"
rm "$out/relocated/libengine_image.so"
refuse "$out/relocated/rnx"
cc -std=c11 -O2 -shared -fPIC "$src/tests/provider.c" -o "$out/provider.so"
readelf -Ws "$image" > "$out/symbols"
slot=$(awk '$3 == 8 && $4 == "TLS" && $5 == "LOCAL" && $6 == "HIDDEN" && $8 == "rnx_allocator_memory" {print $2}' "$out/symbols")
test -n "$slot"
readelf -Wr "$image" > "$out/relocs"
awk -v offset="$slot" '$3=="R_X86_64_TPOFF64" && ("0x"$4)+0==("0x"offset)+0 {found++} END {exit found!=1}' "$out/relocs"
nm -aC "$image" > "$out/names"
take=$(awk '$3=="rune_alloc::limit::take"{print $1}' "$out/names")
release=$(awk '$3=="rune_alloc::limit::release"{print $1}' "$out/names")
test -n "$take" && test -n "$release"
rustc --edition=2024 --crate-name image_tls_control -O "$src/tests/support/tls.rs" \
	-C link-arg=-Wl,--no-as-needed -C "link-arg=-L$out/tagged" -C link-arg=-lengine_image \
	-C "link-arg=-Wl,-rpath,$out/tagged" -o "$out/tls"
"$out/tls" "$take" "$release" "$out/provider.so"
printf '%s\n' 'pub fn main(args){let item=probe::make("item", [])?; let alias=item; item.read()+alias.read()}' > "$out/checked.rn"
printf '%s\n' 'pub fn main(args){let item=probe::make("item", [])?; item.read(); panic!("after reading");}' > "$out/effect.rn"
printf '%s\n' 'pub fn main(args){42}' > "$out/ordinary.rn"
printf '%s\n' 'pub fn main(args){let a=probe::make("a", [])?; let alias=a; let b=probe::make("b", [])?; a.read()+alias.read()+b.read()}' > "$out/pair.rn"
for role in plain tagged; do
	set -- cc -std=c11 -Wall -Wextra -Werror -O2 -pthread
	if [ "$role" = tagged ]; then set -- "$@" -DTAGGED; fi
	"$@" "$src/tests/handles.c" "-L$out/tagged" "-Wl,-rpath,$out/tagged" -lengine_image -o "$out/handles-$role"
	"$out/handles-$role" "$out/checked.rn" "$out/effect.rn" "$out/ordinary.rn" "$out/pair.rn" > "$out/$role-answer"
done
grep -Fxq 'checked scalar=42' "$out/plain-answer"
grep -Fxq 'checked scalar=44' "$out/tagged-answer"
cc -std=c11 -Wall -Wextra -Werror -O2 -pthread -Wl,--export-dynamic-symbol=getenv \
	"$src/tests/eligibility.c" "-L$out/tagged" "-Wl,-rpath,$out/tagged" -lengine_image -ldl -o "$out/eligibility"
env -u PERF_BUILDID_DIR "$out/eligibility" "$out/ordinary.rn" 0
PERF_BUILDID_DIR=/nonexistent "$out/eligibility" "$out/ordinary.rn" 1
