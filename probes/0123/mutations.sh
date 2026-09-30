#!/usr/bin/env bash
# Record 0123: fail-closed controls against the real releases. Each mutation
# of a copy of a release must refuse generation (exit 2) with the named
# reason; the unmutated copy must generate. Needs the generator built
# (CARGO_TARGET_DIR=target/0108/gen-test cargo build --release).
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
gen="$root/target/0108/gen-test/release/polars-gen"
d="$root/target/0123/mut"; rm -rf "$d"; mkdir -p "$d/adapter"
cp -r "$root/adapters/polars/." "$d/adapter/"; rm -rf "$d/adapter/target"
declare -A inv=( [0.55.2]="$root/probes/0072/out/0.55.2-adapter-narrow/result/inventory.json" [v2]="$root/probes/0108/out/v2py/result/inventory.json" )
declare -A rel=( [0.55.2]="$root/tools/polars-gen/releases/0.55.2-joins.toml" [v2]="$root/probes/0108/v2.toml" )
run() { # pin, release, exit, text
	local out; set +e; out=$("$gen" "${inv[$1]}" "$d/adapter" --release "$2" --buckets mechanical,conversion,option_struct,callback,generic_fn,generic 2>&1); local rc=$?; set -e
	if [ "$rc" != "$3" ] || ! grep -qF -- "$4" <<<"$out"; then echo "FAIL $1 $(basename "$2"): exit $rc, wanted $3 with '$4'"; grep refusing <<<"$out" | head -2; exit 1; fi
	echo "ok   $1 $(basename "$2" .toml): exit $rc ($4)"
}
mutate() { # pin, name, python over s
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
	run "$pin" "$(mutate "$pin" unmutated 'pass')" 0 "listed widenings"
	# an unlisted From source: Column has no recorded From for DataTypeExpr
	run "$pin" "$(mutate "$pin" unlisted-source 's = s.replace("sources = [\"polars_core::datatypes::dtype::DataType\"]", "sources = [\"polars_core::datatypes::dtype::DataType\", \"polars_core::frame::column::Column\"]")')" 2 "no recorded From<polars_core::frame::column::Column>"
	# an unlisted Into target: Expr has no recorded From<DataType>
	run "$pin" "$(mutate "$pin" unlisted-target 's += "\n[[into_arguments]]\ntarget = \"polars_plan::dsl::expr::Expr\"\nsources = [\"polars_core::datatypes::dtype::DataType\"]\ncite = \"c\"\n"')" 2 "no recorded From<polars_core::datatypes::dtype::DataType>"
	# the rule removed: every listed widening goes unused
	run "$pin" "$(mutate "$pin" rule-removed 'b = s.index("[[into_arguments]]"); e = s.index("\n\n", b); s = s[:b] + s[e + 2:]')" 2 "was not used"
	# a widening's new contract altered: the real change is unlisted
	run "$pin" "$(mutate "$pin" widening-altered 'b = s.index("rune = \"polars::Expr::cast\""); e = s.index("\n\n", b); s = s[:b] + s[b:e].replace("DataTypeExpr or DataType", "DataType or DataTypeExpr", 1) + s[e:]')" 2 "changed its contract"
	# a carrier that does not instantiate the impl's owner (review of 0123)
	run "$pin" "$(mutate "$pin" wrong-carrier 's = s.replace("wrapper = \"polars_core::schema::SchemaRef\"", "wrapper = \"polars_core::frame::column::Column\"")')" 2 "does not instantiate"
	# a protocol the rule does not allow
	run "$pin" "$(mutate "$pin" protocol-not-allowed 's = s.replace("impl_path = \"polars_schema::schema::Schema as core::fmt::Debug\"", "impl_path = \"polars_schema::schema::Schema as core::clone::Clone\"")')" 2 "not an allowed protocol"
done
echo "0123 mutations: every control refused as named"
