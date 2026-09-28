#!/usr/bin/env bash
# Record 0110: replay the committed evidence. The inventories are this
# record's (implementors recovered by trait identity); the 0.55.2 baselines
# for the census are 0108's, pinned there. Checks the digests, then
# recomputes the census, batches and target table byte for byte.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
d="$root/target/0110/replay"; rm -rf "$d"; mkdir -p "$d"
for f in baseline-surface-0106 baseline-inventory; do gzip -dc "$root/probes/0108/evidence/$f.json.gz" > "$d/$f.json"; done
( cd "$d" && grep -E ' (baseline-surface-0106|baseline-inventory)\.json$' "$root/probes/0108/digests.txt" | sha256sum -c --quiet )
for f in "$here"/evidence/*.json.gz; do gzip -dc "$f" > "$d/$(basename "$f" .gz)"; done
( cd "$d" && sha256sum -c --quiet "$here/digests.txt" )
for f in census batches targets; do mv "$d/$f.json" "$d/$f.committed.json"; done
# the release file this surface was generated from (0110 v2.toml, with the upsample unordered policy)
CENSUS_RELEASE_SHA=94c267ca58ba214bee2359e7fb7f8d53073363fb5f6f5c27965a5f2cbb257f55 python3 "$root/probes/0108/census.py" "$d" > /dev/null
python3 "$root/probes/0108/batches.py" "$d" > /dev/null
gzip -dc "$root/probes/0109/evidence/batches.json.gz" > "$d/batches-0109.json"
python3 "$here/targets.py" "$d/batches-0109.json" "$d/inventory.json" "$d/surface-v2.json" "$d/targets.json" > /dev/null
for f in census batches targets; do cmp "$d/$f.json" "$d/$f.committed.json"; done
echo "0110 evidence verified: census, batches and targets reproduced byte for byte"
