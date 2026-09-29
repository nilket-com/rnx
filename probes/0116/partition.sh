#!/usr/bin/env bash
# Record 0116 (plan evidence): replay the 793-row partition from pinned
# inputs. The generator is 6cea3a9 (0115), run with the 0072 `generic`
# bucket admitted over the v2 inventory (0112's bundle) and the v2.toml of that commit;
# the rows are 0113's `generic_methods` batch. Prints the family counts.
set -euo pipefail
export LC_ALL=C
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
d="$root/target/0116/partition"; rm -rf "$d"; mkdir -p "$d"
git -C "$root" worktree add -q --detach "$d/gen" 6cea3a9
trap 'git -C "$root" worktree remove --force "$d/gen"' EXIT
CARGO_TARGET_DIR="$d/target" cargo build -q --release --locked --manifest-path "$d/gen/tools/polars-gen/Cargo.toml" 2> /dev/null
gzip -dc "$root/probes/0112/evidence/inventory.json.gz" > "$d/inventory.json"
( cd "$d" && grep -E ' inventory\.json$' "$root/probes/0112/digests.txt" | sha256sum -c --quiet )
"$d/target/release/polars-gen" "$d/inventory.json" "$d/adapter" --release "$d/gen/probes/0108/v2.toml" \
  --buckets mechanical,conversion,option_struct,callback,generic_fn,generic > "$d/gen.log"
python3 "$here/audit.py" "$root/probes/0113/evidence/batches.json.gz" "$d/inventory.json" "$d/adapter/surface.json" "$d/audit.json"
sha256sum "$d/audit.json" | cut -c1-16
