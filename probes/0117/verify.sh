#!/usr/bin/env bash
# Record 0117: replay the committed evidence. The old inventories are 0112's
# (pinned there); the new inventories, the v2 surface and oracle results,
# stage A's comparisons and reason drift, and the five-pool audit are this
# record's. Checks every digest, recomputes stage A's inventory comparison and
# its reason drift (0116 generator, drift.sh) at both pins and the audit byte for byte.
set -euo pipefail
export LC_ALL=C
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
d="$root/target/0117/replay"; rm -rf "$d"; mkdir -p "$d/old"
gzip -dc "$root/probes/0112/evidence/inventory.json.gz" > "$d/old/inventory.json"
gzip -dc "$root/probes/0112/evidence/inventory-0.55.2-adapter-narrow.json.gz" > "$d/old/inventory-0.55.2-adapter-narrow.json"
( cd "$d/old" && grep -E ' (inventory|inventory-0\.55\.2-adapter-narrow)\.json$' "$root/probes/0112/digests.txt" | sha256sum -c --quiet )
for f in surface-v2 final; do gzip -dc "$root/probes/0116/evidence/$f.json.gz" > "$d/0116-$f.json"; done
for f in "$here"/evidence/*.json.gz; do gzip -dc "$f" > "$d/$(basename "$f" .gz)"; done
for f in "$here"/evidence/*.json; do cp "$f" "$d/"; done
( cd "$d" && sha256sum -c --quiet "$here/digests.txt" )
# stage A: only the new fields and the listed recovered impl rows differ
python3 "$here/compare.py" "$d/old/inventory-0.55.2-adapter-narrow.json" "$d/inventory-0.55.2-adapter-narrow.json" --json "$d/re-stageA-compare-0.55.2.json" > /dev/null
python3 "$here/compare.py" "$d/old/inventory.json" "$d/inventory-v2.json" --json "$d/re-stageA-compare-v2.json" > /dev/null
for p in 0.55.2 v2; do cmp "$d/stageA-compare-$p.json" "$d/re-stageA-compare-$p.json"; done
# stage A's approved drift, regenerated with the 0116 generator from the
# pinned old and new inventories: only the 9 named reason texts per pin change
bash "$here/drift.sh" "$d/re-stageA-reason-drift.json" > /dev/null
cmp "$d/stageA-reason-drift.json" "$d/re-stageA-reason-drift.json"
# the five pools, every row with one disposition, from the committed surfaces
python3 "$here/audit.py" "$d/0116-surface-v2.json" "$d/0116-final.json" "$d/surface-v2.json" "$d/re-audit.json" > /dev/null
cmp "$d/audit.json" "$d/re-audit.json"
echo "0117 evidence verified: stage A comparisons and reason drift at both pins, and the five-pool audit, reproduced byte for byte"
