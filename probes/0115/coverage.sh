#!/usr/bin/env bash
# Record 0115: which (hook, family, outcome) triples the repository's
# generating runs reach. Runs the generator with POLARS_GEN_TRACE over every
# inventory x release file x bucket set that generates, then counts.
#   coverage.sh <polars-gen binary> <out dir>
set -u
export LC_ALL=C
gen=$(realpath "$1"); out=$2
root=$(cd "$(dirname "$0")/../.." && pwd); cd "$root"
rm -rf "$out"; mkdir -p "$out"
for inv in probes/0072/out/*/result/inventory.json probes/0108/out/v2py/result/inventory.json; do
  for rel in tools/polars-gen/releases/*.toml probes/0108/v2.toml; do
    for b in "mechanical,conversion,option_struct,callback,generic_fn" default; do
      args=("$inv" "$out/adapter" --release "$rel"); [ "$b" = default ] || args+=(--buckets "$b")
      rm -rf "$out/adapter"
      POLARS_GEN_TRACE="$out/trace.tsv" "$gen" "${args[@]}" > /dev/null 2>&1 || true
    done
  done
done
rm -rf "$out/adapter"
cut -f1-3 "$out/trace.tsv" | sort | uniq -c | awk '{printf "%s\t%s\t%s\t%s\n", $2, $3, $4, $1}' > "$out/coverage.tsv"
wc -l < "$out/coverage.tsv"
