#!/usr/bin/env bash
# Record 0119: replay the committed evidence. The 0118 surfaces are 0118's
# evidence; the 0.55.2 catalogue is 0118's commit (e0c3878), the v2 one is
# bundled here (regenerated with the 0118 generator, surface-identical to
# 0118's evidence). Checks every digest, rebuilds both frozen lists byte for
# byte, checks every frozen binding survives in the 0119 surfaces, and
# recomputes the audits at both pins.
set -euo pipefail
export LC_ALL=C
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
d="$root/target/0119/replay"; rm -rf "$d"; mkdir -p "$d"
for p in 0.55.2 v2; do gzip -dc "$root/probes/0118/evidence/surface-$p.json.gz" > "$d/0118-surface-$p.json"; done
git -C "$root" show e0c3878:adapters/polars/src/generated/catalogue.rs > "$d/catalogue-0118-0.55.2.rs"
gzip -dc "$root/probes/0117/evidence/inventory-v2.json.gz" > "$d/inv-v2.json"
gzip -dc "$root/probes/0117/evidence/inventory-0.55.2-adapter-narrow.json.gz" > "$d/inv-0.55.2.json"
for f in "$here"/evidence/*.gz; do gzip -dc "$f" > "$d/$(basename "$f" .gz)"; done
for f in "$here"/evidence/*.json "$here"/frozen-*.json; do cp "$f" "$d/"; done
( cd "$d" && sha256sum -c --quiet "$here/digests.txt" )
for p in 0.55.2 v2; do
  python3 "$here/frozen.py" build "$d/0118-surface-$p.json" "$d/catalogue-0118-$p.rs" "$d/re-frozen-$p.json"
  cmp "$d/frozen-$p.json" "$d/re-frozen-$p.json"
  python3 "$here/frozen.py" check "$d/frozen-$p.json" "$d/surface-$p.json" > /dev/null
  python3 "$here/audit.py" "$d/inv-$p.json" "$d/0118-surface-$p.json" "$d/surface-$p.json" "$d/re-audit-$p.json" > /dev/null
  cmp "$d/audit-$p.json" "$d/re-audit-$p.json"
done
echo "0119 evidence verified: frozen lists rebuilt, every frozen binding present, audits reproduced at both pins"
