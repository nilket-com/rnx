#!/usr/bin/env bash
# Record 0108: replay the committed evidence. Decompresses evidence/*.json.gz,
# checks every file against digests.txt, then recomputes the census and the
# batch table from the bundle and requires them byte-identical to the
# committed ones. Needs nothing outside this commit (no rustdoc, no build).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
d="$root/target/0108/replay"; rm -rf "$d"; mkdir -p "$d"
for f in "$here"/evidence/*.json.gz; do gzip -dc "$f" > "$d/$(basename "$f" .gz)"; done
( cd "$d" && sha256sum -c --quiet "$here/digests.txt" )
mv "$d/census.json" "$d/census.committed.json"; mv "$d/batches.json" "$d/batches.committed.json"
# the release file the bundled surface was generated from (0108 v2.toml)
CENSUS_RELEASE_SHA=0da7919d82fbb9865b9a08b1c1881acb22c6d90c8d87f871c0b21530ddfb6edf python3 "$here/census.py" "$d" > /dev/null
python3 "$here/batches.py" "$d" > /dev/null
cmp "$d/census.json" "$d/census.committed.json"
cmp "$d/batches.json" "$d/batches.committed.json"
echo "0108 evidence verified: $(ls "$here"/evidence | wc -l) files, census and batches reproduced byte for byte"
