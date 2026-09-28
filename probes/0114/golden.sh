#!/usr/bin/env bash
# Record 0114: run one polars-gen binary over every (inventory, release file,
# bucket set) the repository holds and keep everything it produces: the
# seven output files, stdout, stderr and the exit code. Two runs of this
# script (base generator, refactored generator) must be identical: diff -r.
#
#   golden.sh <polars-gen binary> <out dir>
set -u
# fixed collation: glob and digest order must not depend on the locale
export LC_ALL=C
gen=$(realpath "$1"); out=$2
root=$(cd "$(dirname "$0")/../.." && pwd)
rm -rf "$out"; mkdir -p "$out"
inventories=(probes/0072/out/*/result/inventory.json probes/0108/out/v2py/result/inventory.json)
releases=(tools/polars-gen/releases/*.toml probes/0108/v2.toml)
buckets=("mechanical,conversion,option_struct,callback,generic_fn" "default")
cd "$root"
for inv in "${inventories[@]}"; do
  for rel in "${releases[@]}"; do
    for b in "${buckets[@]}"; do
      name="$(echo "$inv" | cut -d/ -f2,4 | tr / _)__$(basename "$rel" .toml)__${b//,/-}"
      d="$out/$name"; mkdir -p "$d/adapter"
      args=("$inv" "$d/adapter" --release "$rel")
      [ "$b" = default ] || args+=(--buckets "$b")
      "$gen" "${args[@]}" > "$d/stdout" 2> "$d/stderr"; echo $? > "$d/exit"
      # stdout names no path that differs between runs; stderr may carry panics
      sed -i "s#$out#<out>#g" "$d/stdout" "$d/stderr"
    done
  done
done
# production --check against the committed adapter
"$gen" probes/0072/out/0.55.2-adapter-narrow/result/inventory.json adapters/polars --release tools/polars-gen/releases/0.55.2-joins.toml --buckets mechanical,conversion,option_struct,callback,generic_fn --check > "$out/check.stdout" 2>&1; echo $? > "$out/check.exit"
# --check must also report drift: one byte changed in a copy of the adapter
drift="$out/drift-adapter"; mkdir -p "$drift"; cp -r adapters/polars/src adapters/polars/tests adapters/polars/surface.json "$drift/"
sed -i '0,/fn /s//fn  /' "$drift/src/generated/functions.rs"
"$gen" probes/0072/out/0.55.2-adapter-narrow/result/inventory.json "$drift" --release tools/polars-gen/releases/0.55.2-joins.toml --buckets mechanical,conversion,option_struct,callback,generic_fn --check > "$out/drift.stdout" 2>&1; echo $? > "$out/drift.exit"
sed -i "s#$out#<out>#g" "$out/drift.stdout"; rm -rf "$drift"
# the self-test entry point and its output
"$gen" --self-test > "$out/self-test.stdout" 2>&1; echo $? > "$out/self-test.exit"
(cd "$out" && find . -type f ! -name digests.txt | sort | xargs sha256sum) > "$out/digests.txt"
echo "runs: $(ls -d "$out"/*/ | wc -l); generated: $(grep -lx 0 "$out"/*/exit | wc -l); check exit: $(cat "$out/check.exit"); drift exit: $(cat "$out/drift.exit"); self-test exit: $(cat "$out/self-test.exit")"
