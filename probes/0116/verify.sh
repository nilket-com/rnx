#!/usr/bin/env bash
# Record 0116: replay the committed evidence. The v2 inventory is 0112's
# (pinned there), the census baselines 0108's; the surface, oracle results,
# census, batches, the 793-row partition and its final dispositions are this
# record's. Checks every digest, recomputes census, batches and the final
# audit byte for byte, and replays the partition from its pinned generator.
set -euo pipefail
export LC_ALL=C
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
d="$root/target/0116/replay"; rm -rf "$d"; mkdir -p "$d/stage1"
for f in baseline-surface-0106 baseline-inventory; do gzip -dc "$root/probes/0108/evidence/$f.json.gz" > "$d/$f.json"; done
( cd "$d" && grep -E ' (baseline-surface-0106|baseline-inventory)\.json$' "$root/probes/0108/digests.txt" | sha256sum -c --quiet )
gzip -dc "$root/probes/0112/evidence/inventory.json.gz" > "$d/inventory.json"
( cd "$d" && grep -E ' inventory\.json$' "$root/probes/0112/digests.txt" | sha256sum -c --quiet )
for f in "$here"/evidence/*.json.gz; do gzip -dc "$f" > "$d/$(basename "$f" .gz)"; done
for f in "$here"/evidence/stage1/*.json.gz; do gzip -dc "$f" > "$d/stage1/$(basename "$f" .gz)"; done
( cd "$d" && sha256sum -c --quiet "$here/digests.txt" )
for f in census batches final; do mv "$d/$f.json" "$d/$f.committed.json"; done
# the release file this surface was generated from (0116 v2.toml)
CENSUS_RELEASE_SHA="$(cat "$here/v2-release.sha256")" python3 "$root/probes/0108/census.py" "$d" > /dev/null
python3 "$root/probes/0108/batches.py" "$d" > /dev/null
python3 "$here/final.py" "$d/audit.json" "$d/surface-v2.json" "$d/final.json" > /dev/null
for f in census batches final; do cmp "$d/$f.json" "$d/$f.committed.json"; done
# the partition, from its pinned inputs (the 0115 generator)
bash "$here/partition.sh" > "$d/partition.log"
cmp "$root/target/0116/partition/audit.json" "$d/audit.json"
echo "0116 evidence verified: census, batches and the 793-row final audit reproduced byte for byte; partition replayed"
