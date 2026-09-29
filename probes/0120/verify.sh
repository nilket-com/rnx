#!/usr/bin/env bash
# Record 0120: replay the committed evidence. The 0119 surfaces are 0119's
# evidence; the 0.55.2 catalogue is 0119's commit (44a362b), the v2 one is
# bundled here (regenerated with the 0119 generator, surface-identical to
# 0119's evidence). Checks every digest, rebuilds both frozen lists byte for
# byte, checks every frozen binding survives in the 0120 surfaces, and
# recomputes the audits at both pins from the release files.
set -euo pipefail
export LC_ALL=C
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
d="$root/target/0120/replay"; rm -rf "$d"; mkdir -p "$d"
for p in 0.55.2 v2; do gzip -dc "$root/probes/0119/evidence/surface-$p.json.gz" > "$d/0119-surface-$p.json"; done
git -C "$root" show 44a362b:adapters/polars/src/generated/catalogue.rs > "$d/catalogue-0119-0.55.2.rs"
gzip -dc "$root/probes/0117/evidence/inventory-v2.json.gz" > "$d/inv-v2.json"
gzip -dc "$root/probes/0117/evidence/inventory-0.55.2-adapter-narrow.json.gz" > "$d/inv-0.55.2.json"
for f in "$here"/evidence/*.gz; do gzip -dc "$f" > "$d/$(basename "$f" .gz)"; done
for f in "$here"/evidence/*.json "$here"/frozen-*.json "$here"/reach-*.json; do cp "$f" "$d/"; done
( cd "$d" && sha256sum -c --quiet "$here/digests.txt" )
declare -A rel=( [0.55.2]="$root/tools/polars-gen/releases/0.55.2-joins.toml" [v2]="$root/probes/0108/v2.toml" )
for p in 0.55.2 v2; do
  python3 "$root/probes/0119/frozen.py" build "$d/0119-surface-$p.json" "$d/catalogue-0119-$p.rs" "$d/re-frozen-$p.json"
  cmp "$d/frozen-$p.json" "$d/re-frozen-$p.json"
  python3 "$root/probes/0119/frozen.py" check "$d/frozen-$p.json" "$d/surface-$p.json" > /dev/null
  python3 "$here/audit.py" "$d/inv-$p.json" "${rel[$p]}" "$d/0119-surface-$p.json" "$d/surface-$p.json" "$d/re-audit-$p.json" > /dev/null
  cmp "$d/audit-$p.json" "$d/re-audit-$p.json"
done
echo "0120 evidence verified: frozen lists rebuilt from 0119, every frozen binding present, audits reproduced at both pins"
