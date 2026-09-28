#!/usr/bin/env bash
# Record 0111: replay the committed evidence. The v2 inventory is 0110's
# (probes/0110/evidence, pinned there), the census baselines 0108's; the
# surface, oracle results, census, batches and target table are this
# record's. Checks every digest, then recomputes census, batches and the
# target table byte for byte.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
d="$root/target/0111/replay"; rm -rf "$d"; mkdir -p "$d"
for f in baseline-surface-0106 baseline-inventory; do gzip -dc "$root/probes/0108/evidence/$f.json.gz" > "$d/$f.json"; done
( cd "$d" && grep -E ' (baseline-surface-0106|baseline-inventory)\.json$' "$root/probes/0108/digests.txt" | sha256sum -c --quiet )
gzip -dc "$root/probes/0110/evidence/inventory.json.gz" > "$d/inventory.json"
( cd "$d" && grep -E ' inventory\.json$' "$root/probes/0110/digests.txt" | sha256sum -c --quiet )
for f in "$here"/evidence/*.json.gz; do gzip -dc "$f" > "$d/$(basename "$f" .gz)"; done
( cd "$d" && sha256sum -c --quiet "$here/digests.txt" )
for f in census batches targets; do mv "$d/$f.json" "$d/$f.committed.json"; done
# the release file this surface was generated from (0111 v2.toml)
CENSUS_RELEASE_SHA=94c267ca58ba214bee2359e7fb7f8d53073363fb5f6f5c27965a5f2cbb257f55 python3 "$root/probes/0108/census.py" "$d" > /dev/null
python3 "$root/probes/0108/batches.py" "$d" > /dev/null
gzip -dc "$root/probes/0110/evidence/batches.json.gz" > "$d/batches-0110.json"
python3 "$here/targets.py" "$d/batches-0110.json" "$d/inventory.json" "$d/surface-v2.json" "$d/targets.json" > /dev/null
for f in census batches targets; do cmp "$d/$f.json" "$d/$f.committed.json"; done
echo "0111 evidence verified: census, batches and targets reproduced byte for byte"
