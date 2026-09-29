#!/usr/bin/env bash
# Record 0118: replay the committed evidence. The inventories and the 0117
# surfaces are 0117's (its evidence bundle; the 0.55.2 surface at 120e37e);
# the 0118 surfaces, the v2 oracle results, the stage-1 census and the
# audits are this record's. Checks every digest and recomputes the census at
# both pins (from the 0117 surfaces) and the audits (census + 0117 + 0118
# surfaces) byte for byte.
set -euo pipefail
export LC_ALL=C
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
d="$root/target/0118/replay"; rm -rf "$d"; mkdir -p "$d"
gzip -dc "$root/probes/0117/evidence/inventory-v2.json.gz" > "$d/inv-v2.json"
gzip -dc "$root/probes/0117/evidence/inventory-0.55.2-adapter-narrow.json.gz" > "$d/inv-0.55.2.json"
gzip -dc "$root/probes/0117/evidence/surface-v2.json.gz" > "$d/0117-surface-v2.json"
git -C "$root" show 120e37e:adapters/polars/surface.json > "$d/0117-surface-0.55.2.json"
gzip -dc "$root/probes/0116/evidence/final.json.gz" > "$d/0116-final.json"
for f in "$here"/evidence/*.json.gz; do gzip -dc "$f" > "$d/$(basename "$f" .gz)"; done
for f in "$here"/evidence/*.json; do cp "$f" "$d/"; done
( cd "$d" && sha256sum -c --quiet "$here/digests.txt" )
python3 "$here/census.py" "$d/inv-v2.json" "$d/0117-surface-v2.json" "$d/re-census-v2.json" --reconcile "$d/0116-final.json" > /dev/null
python3 "$here/census.py" "$d/inv-0.55.2.json" "$d/0117-surface-0.55.2.json" "$d/re-census-0.55.2.json" > /dev/null
for p in v2 0.55.2; do
  cmp "$d/census-$p.json" "$d/re-census-$p.json"
  python3 "$here/audit.py" "$d/census-$p.json" "$d/0117-surface-$p.json" "$d/surface-$p.json" "$d/re-audit-$p.json" > /dev/null
  cmp "$d/audit-$p.json" "$d/re-audit-$p.json"
done
echo "0118 evidence verified: the census and the audits at both pins reproduced byte for byte"
