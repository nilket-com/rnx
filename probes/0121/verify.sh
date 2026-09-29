#!/usr/bin/env bash
# Record 0121: replay the committed evidence. The pool is recomputed from
# 0120's committed v2 results and must equal baseline-v2.json; over 0121's
# results it must be empty. Both surfaces pass the surface gate against
# 0120's (v2: 0120's evidence; 0.55.2: 0120's commit), and the moved-case
# reports are recomputed byte for byte.
set -euo pipefail
export LC_ALL=C
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
d="$root/target/0121/replay"; rm -rf "$d"; mkdir -p "$d"
python3 "$here/baseline.py" "$root/probes/0120/evidence/oracle-results-v2.json.gz" "$d/baseline-v2.json" > /dev/null
cmp "$d/baseline-v2.json" "$here/baseline-v2.json"
python3 "$here/baseline.py" "$here/evidence/oracle-results-v2.json.gz" | grep -q '"total": 0,'
gzip -dc "$root/probes/0120/evidence/surface-v2.json.gz" > "$d/0120-surface-v2.json"
gzip -dc "$here/evidence/surface-v2.json.gz" > "$d/surface-v2.json"
python3 "$here/surface_gate.py" "$d/0120-surface-v2.json" "$d/surface-v2.json" > /dev/null
git -C "$root" show badc7a8:adapters/polars/surface.json > "$d/0120-surface-0.55.2.json"
python3 "$here/surface_gate.py" "$d/0120-surface-0.55.2.json" "$root/adapters/polars/surface.json" > /dev/null
python3 "$here/moved.py" "$root/probes/0120/evidence/oracle-results-v2.json.gz" "$here/evidence/oracle-results-v2.json.gz" "$d/moved-v2.json" > /dev/null
cmp "$d/moved-v2.json" "$here/evidence/moved-v2.json"
git -C "$root" show badc7a8:adapters/polars/oracle-results.json > "$d/0120-oracle-0.55.2.json"
python3 "$here/moved.py" "$d/0120-oracle-0.55.2.json" "$root/adapters/polars/oracle-results.json" "$d/moved-0.55.2.json" > /dev/null
cmp "$d/moved-0.55.2.json" "$here/evidence/moved-0.55.2.json"
# the gate refuses a moved contract field and admits only the listed annotations
python3 "$here/surface_gate.py" --self-test "$root/adapters/polars/surface.json" > /dev/null
# launch cannot move: the release build's sources equal 0120's, apart from the
# fixtures module, which is compiled only with `test-support`
git -C "$root" diff --quiet badc7a8 -- adapters/polars/src adapters/polars/Cargo.toml adapters/polars/Cargo.lock ':(exclude)adapters/polars/src/generated/fixtures.rs'
grep -B2 -F 'pub mod fixtures;' "$root/adapters/polars/src/generated/mod.rs" | grep -Fq '#[cfg(feature = "test-support")]'
echo "0121 evidence verified: pool reproduced (363 -> 0), both surfaces gated, moved cases reproduced at both pins, gate self-test ok, release build unchanged from 0120"
