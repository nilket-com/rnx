#!/usr/bin/env bash
# Record 0112: replay the committed evidence. The inventories are this
# record's (trait lifetimes recorded); the 0.55.2 baselines
# for the census are 0108's, pinned there. Checks the digests, then
# recomputes the census, batches and target table byte for byte.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
d="$root/target/0112/replay"; rm -rf "$d"; mkdir -p "$d"
for f in baseline-surface-0106 baseline-inventory; do gzip -dc "$root/probes/0108/evidence/$f.json.gz" > "$d/$f.json"; done
( cd "$d" && grep -E ' (baseline-surface-0106|baseline-inventory)\.json$' "$root/probes/0108/digests.txt" | sha256sum -c --quiet )
for f in "$here"/evidence/*.json.gz; do gzip -dc "$f" > "$d/$(basename "$f" .gz)"; done
( cd "$d" && sha256sum -c --quiet "$here/digests.txt" )
for f in census batches targets; do mv "$d/$f.json" "$d/$f.committed.json"; done
# the release file this surface was generated from (0112 v2.toml)
CENSUS_RELEASE_SHA=94c267ca58ba214bee2359e7fb7f8d53073363fb5f6f5c27965a5f2cbb257f55 python3 "$root/probes/0108/census.py" "$d" > /dev/null
python3 "$root/probes/0108/batches.py" "$d" > /dev/null
gzip -dc "$root/probes/0111/evidence/batches.json.gz" > "$d/batches-0111.json"
python3 "$here/targets.py" "$d/batches-0111.json" "$d/inventory.json" "$d/surface-v2.json" "$d/targets.json" > /dev/null
for f in census batches targets; do cmp "$d/$f.json" "$d/$f.committed.json"; done
# review of 0112 (round 3): TableStatistics(pub Arc<DataFrame>) is refused in both JSON directions
python3 - "$d/surface-v2.json" <<'PY'
import json, sys
s = json.load(open(sys.argv[1]))
rows = [e for e in s["entries"] if e["canonical_path"].startswith("polars_plan::dsl::file_scan::TableStatistics as serde_core::")]
assert len(rows) == 2, [e["canonical_path"] for e in rows]
for e in rows:
    assert e["status"] == "unsupported" and "TableStatistics" in e["reason"] and "tuple fields are not extracted" in e["reason"], (e["canonical_path"], e["status"], e.get("reason"))
PY
echo "0112 evidence verified: census, batches and targets reproduced byte for byte"
