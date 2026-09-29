#!/usr/bin/env bash
# Record 0115: run the 0114 generator (32d1b40) and the 0115 generator on
# every synthetic overlap case; their full outputs (the seven files, stdout,
# stderr, exit code) must be identical, and the 0115 run's trace must reach
# every line the case expects.
#   overlap.sh <old polars-gen> <new polars-gen> <out dir>
set -u
export LC_ALL=C
old=$(realpath "$1"); new=$(realpath "$2"); out=$3
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../.." && pwd)
rm -rf "$out"; mkdir -p "$out"
python3 "$here/overlap.py" "$out/cases" > /dev/null
fail=0
while IFS=$'\t' read -r name why; do
  c="$out/cases/$name"
  for side in old new; do
    d="$out/$side/$name"; mkdir -p "$d/adapter"
    bin=$old; [ $side = new ] && bin=$new
    trace=(); [ $side = new ] && trace=(env POLARS_GEN_TRACE="$out/trace-$name.tsv")
    ( cd "$root" && "${trace[@]}" "$bin" "$c/inventory.json" "$d/adapter" --release "$c/release.toml" \
        --buckets mechanical,conversion,option_struct,callback,generic_fn > "$d/stdout" 2> "$d/stderr"; echo $? > "$d/exit" )
    sed -i "s#$out/$side#<out>#g; s#$c#<case>#g" "$d/stdout" "$d/stderr"
  done
  if ! diff -r "$out/old/$name" "$out/new/$name" > /dev/null; then
    echo "DIFFERS: $name"; fail=1
  fi
  echo "$name"; touch "$out/trace-$name.tsv"
  python3 "$here/overlap.py" --check "$c" "$out/trace-$name.tsv" || fail=1
done < "$out/cases/cases.tsv"
[ $fail = 0 ] && echo "overlap: every case identical and reached" || { echo "overlap: FAILED"; exit 1; }
