#!/usr/bin/env bash
# Record 0114: replay the split's proofs from a clean checkout.
#  1. golden: the generator at eee390c and at HEAD over every inventory ×
#     release file × bucket set (probes/0072/out, probes/0108/out), plus the
#     --check pass and drift cases and --self-test: diff -r must be empty,
#     and the base outputs must match the committed digests.
#  2. moved.py: the base main.rs against the new tree, item by item.
#  3. cargo test: the 34 self-test wrappers.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
d="$root/target/0114/replay"; rm -rf "$d"; mkdir -p "$d"
base=eee390c
# the inputs golden reads: the five 0072 inventories (bundled here) and the
# v2 inventory (0112's bundle), restored to their build paths when absent and
# checked against their hashes either way
for n in 0.54.4-adapter 0.54.4-full 0.55.2-adapter-narrow 0.55.2-adapter 0.55.2-full; do
  f="$root/probes/0072/out/$n/result/inventory.json"
  [ -f "$f" ] || { mkdir -p "$(dirname "$f")"; gzip -dc "$here/evidence/inventory-$n.json.gz" > "$f"; }
done
f="$root/probes/0108/out/v2py/result/inventory.json"
[ -f "$f" ] || { mkdir -p "$(dirname "$f")"; gzip -dc "$root/probes/0112/evidence/inventory.json.gz" > "$f"; }
( cd "$root/probes" && sha256sum -c --quiet "$here/inventories.sha256" )
git -C "$root" worktree add -q --detach "$d/base" "$base"
trap 'git -C "$root" worktree remove --force "$d/base"' EXIT
CARGO_TARGET_DIR="$d/target-base" cargo build -q --release --locked --manifest-path "$d/base/tools/polars-gen/Cargo.toml" 2> /dev/null
CARGO_TARGET_DIR="$d/target-new" cargo build -q --release --locked --manifest-path "$root/tools/polars-gen/Cargo.toml" 2> /dev/null
"$here/golden.sh" "$d/target-base/release/polars-gen" "$d/golden-base"
"$here/golden.sh" "$d/target-new/release/polars-gen" "$d/golden-new"
diff -r "$d/golden-base" "$d/golden-new"
diff "$here/digests.txt" "$d/golden-base/digests.txt"
git -C "$root" show "$base:tools/polars-gen/src/main.rs" > "$d/main.rs.base"
python3 "$here/moved.py" --self-test
python3 "$here/moved.py" "$d/main.rs.base" "$root/tools/polars-gen/src"
CARGO_TARGET_DIR="$d/target-new" cargo test -q --release --locked --manifest-path "$root/tools/polars-gen/Cargo.toml" 2>&1 | grep -E '^test result'
echo "0114 replay: ok"
