#!/usr/bin/env bash
# Record 0115: replay the registry's proofs from a clean checkout.
#  1. golden (0114's harness): the 0114 generator (32d1b40) and HEAD over every
#     inventory x release x bucket set, plus the --check pass/drift and
#     --self-test controls: identical, and equal to 0114's committed digests
#  2. overlap: the synthetic overlap cases, old and new byte for byte, each
#     reaching the combination it names
#  3. coverage: the hook triples the generating runs reach, as committed
#  4. orders: each registry list derived from the 32d1b40 source
#  5. cargo test (the pinned orders, scopes, collect, the unreachable
#     refusals, the unknown-key control and the 34 self-tests)
#  6. the end-to-end unknown-key control on the real binary
#  7. the mutations, each caught by its proof
set -euo pipefail
export LC_ALL=C
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
d="$root/target/0115/replay"; rm -rf "$d"; mkdir -p "$d"
base=32d1b40
for n in 0.54.4-adapter 0.54.4-full 0.55.2-adapter-narrow 0.55.2-adapter 0.55.2-full; do
  f="$root/probes/0072/out/$n/result/inventory.json"
  [ -f "$f" ] || { mkdir -p "$(dirname "$f")"; gzip -dc "$root/probes/0114/evidence/inventory-$n.json.gz" > "$f"; }
done
f="$root/probes/0108/out/v2py/result/inventory.json"
[ -f "$f" ] || { mkdir -p "$(dirname "$f")"; gzip -dc "$root/probes/0112/evidence/inventory.json.gz" > "$f"; }
( cd "$root/probes" && sha256sum -c --quiet "$root/probes/0114/inventories.sha256" )
git -C "$root" worktree add -q --detach "$d/base" "$base"
trap 'git -C "$root" worktree remove --force "$d/base"' EXIT
CARGO_TARGET_DIR="$d/target-base" cargo build -q --release --locked --manifest-path "$d/base/tools/polars-gen/Cargo.toml" 2> /dev/null
CARGO_TARGET_DIR="$d/target-new" cargo build -q --release --locked --manifest-path "$root/tools/polars-gen/Cargo.toml" 2> /dev/null
old="$d/target-base/release/polars-gen"; new="$d/target-new/release/polars-gen"
"$root/probes/0114/golden.sh" "$old" "$d/golden-base"
"$root/probes/0114/golden.sh" "$new" "$d/golden-new"
diff -r "$d/golden-base" "$d/golden-new"
diff "$root/probes/0114/digests.txt" "$d/golden-new/digests.txt"
"$here/overlap.sh" "$old" "$new" "$d/overlap"
"$here/coverage.sh" "$new" "$d/coverage" > /dev/null
diff "$here/coverage.tsv" "$d/coverage/coverage.tsv"
python3 "$here/orders.py" "$root" "$base"
python3 "$here/overlap.py" --self-test
CARGO_TARGET_DIR="$d/target-new" cargo test -q --release --locked --manifest-path "$root/tools/polars-gen/Cargo.toml" 2>&1 | grep -E '^test result'
# 6. a misspelled table is refused by name, before generation; the declared name loads
inv="$root/probes/0072/out/0.55.2-adapter-narrow/result/inventory.json"
rel="$root/tools/polars-gen/releases/0.55.2-joins.toml"
entry='key = "polars_core:3510"\npath = "polars_core::chunked_array::ChunkedArray::downcast_as_array"\npairs = []\ncite = "c"\n'
{ cat "$rel"; printf "\n[[array_snapshot]]\n$entry"; } > "$d/typo.toml"
set +e; "$new" "$inv" "$d/typo-out" --release "$d/typo.toml" > "$d/typo.stdout" 2> "$d/typo.stderr"; code=$?; set -e
[ $code = 2 ] && grep -q "unknown top-level key \`array_snapshot\`" "$d/typo.stderr" && [ ! -e "$d/typo-out/surface.json" ] \
  && echo "typo control: refused with the key named, nothing written" || { echo "typo control FAILED (exit $code)"; exit 1; }
"$here/mutations.sh" "$old" "$d/mutations"
echo "0115 replay: ok"
