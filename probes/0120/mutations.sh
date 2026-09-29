#!/usr/bin/env bash
# Record 0120: fail-closed controls against the real releases. Each
# mutation of a copy of a release must refuse generation (exit 2) with the
# named reason; the unmutated copy must generate. Needs the generator built
# (CARGO_TARGET_DIR=target/0108/gen-test cargo build --release).
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
gen="$root/target/0108/gen-test/release/polars-gen"
d="$root/target/0120/mut"; rm -rf "$d"; mkdir -p "$d/adapter"
cp -r "$root/adapters/polars/." "$d/adapter/"
rm -rf "$d/adapter/target"
declare -A inv=( [0.55.2]="$root/probes/0072/out/0.55.2-adapter-narrow/result/inventory.json" [v2]="$root/probes/0108/out/v2py/result/inventory.json" )
declare -A rel=( [0.55.2]="$root/tools/polars-gen/releases/0.55.2-joins.toml" [v2]="$root/probes/0108/v2.toml" )
run() { # pin, mutated release, expected exit, expected text
	local out; set +e; out=$("$gen" "${inv[$1]}" "$d/adapter" --release "$2" --buckets mechanical,conversion,option_struct,callback,generic_fn,generic 2>&1); local rc=$?; set -e
	if [ "$rc" != "$3" ] || ! grep -qF -- "$4" <<<"$out"; then echo "FAIL $1 $(basename "$2"): exit $rc, wanted $3 with '$4'"; grep -E 'refusing' <<<"$out" | head -2; exit 1; fi
	echo "ok   $1 $(basename "$2" .toml): exit $rc ($4)"
}
mutate() { # pin, name, python expression over s
	local out="$d/$1-$2.toml"
	python3 - "${rel[$1]}" "$out" "$3" <<'PY'
import sys
s = open(sys.argv[1]).read()
exec(sys.argv[3])
open(sys.argv[2], "w").write(s)
PY
	echo "$out"
}
for pin in 0.55.2 v2; do
	run "$pin" "$(mutate "$pin" unmutated 'pass')" 0 "frozen bindings"
	# a required guard removed
	run "$pin" "$(mutate "$pin" guard-removed 'b = s.index("path = \"polars_arrow::array::utf8::Utf8Array::value\""); a = s.rindex("[[receiver_guards]]", 0, b); e = s.index("\n\n", b); s = s[:a] + s[e + 2:]')" 2 "polars_arrow::array::utf8::Utf8Array::value: a required receiver guard is missing"
	# the strict range swapped for the zero-length exemption
	run "$pin" "$(mutate "$pin" guard-swapped 'b = s.index("path = \"polars_arrow::array::primitive::PrimitiveArray::sliced\""); e = s.index("\n\n", b); s = s[:b] + s[b:e].replace("range_len_all", "range_len") + s[e:]')" 2 "the required guard is \`range_len_all\`"
	# an instantiation the probe never produced
	run "$pin" "$(mutate "$pin" unproven 's += "\n[[concrete_arrays]]\nowner = \"polars_arrow::array::utf8::Utf8Array\"\nnative = \"i32\"\nname = \"StringArray\"\ndowncast = \"as_string_array\"\nroute = \"none\"\ncite = \"c\"\n"')" 2 "not an identity the 0120 probe proved"
	# an allowlist row without its probe route
	run "$pin" "$(mutate "$pin" unrouted 'b = s.index("name = \"Int64Array\""); e = s.index("\n\n", b); import re; s = s[:b] + re.sub(r"route = \"[^\"]*\"", "route = \"\"", s[b:e]) + s[e:]')" 2 "no citation or probe route"
	# a listed identity twice
	run "$pin" "$(mutate "$pin" duplicate 'b = s.index("[[concrete_arrays]]"); e = s.index("\n\n", b); s += "\n" + s[b:e].replace("Int8Array", "Int8ArrayAgain").replace("as_int8_array", "as_int8_array_again") + "\n"')" 2 "listed twice"
done
echo "0120 mutations: every control refused as named"
