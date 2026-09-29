#!/usr/bin/env bash
# Record 0117 stage A: replay the approved reason-text drift from pinned
# inputs. The generator is 812238f (0116), run with that commit's release
# files over the old inventories (0112's bundle) and the new ones (this
# record's) at both pins; drift.py asserts nothing but the 9 named reason
# texts per pin changes and writes the triples to <out.json>.
set -euo pipefail
export LC_ALL=C
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
out="$1"
d="$root/target/0117/drift"; rm -rf "$d"; mkdir -p "$d"/{old,new}/{prod,v2}
git -C "$root" worktree add -q --detach "$d/gen" 812238f
trap 'git -C "$root" worktree remove --force "$d/gen"' EXIT
CARGO_TARGET_DIR="$d/target" cargo build -q --release --locked --manifest-path "$d/gen/tools/polars-gen/Cargo.toml" 2> /dev/null
gzip -dc "$root/probes/0112/evidence/inventory-0.55.2-adapter-narrow.json.gz" > "$d/old/prod/inventory.json"
gzip -dc "$root/probes/0112/evidence/inventory.json.gz" > "$d/old/v2/inventory.json"
gzip -dc "$here/evidence/inventory-0.55.2-adapter-narrow.json.gz" > "$d/new/prod/inventory.json"
gzip -dc "$here/evidence/inventory-v2.json.gz" > "$d/new/v2/inventory.json"
buckets=mechanical,conversion,option_struct,callback,generic_fn,generic
for side in old new; do
  "$d/target/release/polars-gen" "$d/$side/prod/inventory.json" "$d/$side/prod/adapter" \
    --release "$d/gen/tools/polars-gen/releases/0.55.2-joins.toml" --buckets "$buckets" > "$d/$side/prod/gen.log"
  "$d/target/release/polars-gen" "$d/$side/v2/inventory.json" "$d/$side/v2/adapter" \
    --release "$d/gen/probes/0108/v2.toml" --buckets "$buckets" > "$d/$side/v2/gen.log"
done
python3 "$here/drift.py" "$out" \
  "0.55.2=$d/old/prod/adapter,$d/new/prod/adapter" "v2=$d/old/v2/adapter,$d/new/v2/adapter"
