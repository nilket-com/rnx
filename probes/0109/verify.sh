#!/usr/bin/env bash
# Record 0109: replay the committed v2 evidence. The v2 inventory and the two
# 0.55.2 baselines are 0108's (probes/0108/evidence, pinned there); the
# surface, oracle results, census and batches are this record's. Checks the
# digests, then recomputes the census and batches byte for byte.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
d="$root/target/0109/replay"; rm -rf "$d"; mkdir -p "$d"
for f in inventory baseline-surface-0106 baseline-inventory; do gzip -dc "$root/probes/0108/evidence/$f.json.gz" > "$d/$f.json"; done
( cd "$d" && grep -E ' (inventory|baseline-surface-0106|baseline-inventory)\.json$' "$root/probes/0108/digests.txt" | sha256sum -c --quiet )
for f in "$here"/evidence/*.json.gz; do gzip -dc "$f" > "$d/$(basename "$f" .gz)"; done
( cd "$d" && sha256sum -c --quiet "$here/digests.txt" )
mv "$d/census.json" "$d/census.committed.json"; mv "$d/batches.json" "$d/batches.committed.json"
# the release file this surface was generated from (0109 v2.toml, with the collect_concurrently refusal)
CENSUS_RELEASE_SHA=c2bca1e4d30a8f78ad227a15fc5e6fb41be032a3e3b89e0197adab7744506cc7 python3 "$root/probes/0108/census.py" "$d" > /dev/null
python3 "$root/probes/0108/batches.py" "$d" > /dev/null
cmp "$d/census.json" "$d/census.committed.json"
cmp "$d/batches.json" "$d/batches.committed.json"
echo "0109 evidence verified: census and batches reproduced byte for byte"
