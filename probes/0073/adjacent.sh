#!/usr/bin/env bash
# Record 0073 gate 4: the frozen generator against the adjacent release.
# Generates from the 0.54.4 inventory into a scratch adapter pinned to
# 0.54.4, type-checks it under a committed lock, and reports what changed.
# Any failing step fails the script. `ADJACENT_CARGO` overrides the cargo
# binary (the negative control uses it).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
CARGO="${ADJACENT_CARGO:-cargo}"
inv="$root/probes/0072/out/0.54.4-adapter/result/inventory.json"
[ -f "$inv" ] || { echo "FAIL: missing $inv (run probes/0072/run.sh report)"; exit 1; }
scratch="${ADJACENT_SCRATCH:-$root/target/0073-adjacent}"
lock="$here/adjacent.lock"
rm -rf "$scratch"; mkdir -p "$scratch/polars"
# only what the adapter needs: never its target directory
for f in Cargo.toml src tests surface.json; do cp -r "$root/adapters/polars/$f" "$scratch/polars/"; done
python3 - "$scratch/polars/Cargo.toml" "$root" <<'PY'
import sys
p, root = sys.argv[1], sys.argv[2]
t = open(p).read().replace("=0.55.2", "=0.54.4").replace('path = "../.."', f'path = "{root}"')
open(p, "w").write(t)
PY
if [ -f "$lock" ]; then cp "$lock" "$scratch/polars/Cargo.lock"; else
  ( cd "$scratch/polars" && "$CARGO" generate-lockfile -q ) && cp "$scratch/polars/Cargo.lock" "$lock" && echo "new configuration: lock saved to $lock"
fi
( cd "$root/tools/polars-gen" && CARGO_TARGET_DIR="$root/target/0073" "$CARGO" build -q --locked )
"$root/target/0073/debug/polars-gen" "$inv" "$scratch/polars" --release "$root/tools/polars-gen/releases/0.54.4.toml" --buckets mechanical,conversion,option_struct > "$scratch/gen.log"
cat "$scratch/gen.log"
if ( cd "$scratch/polars" && CARGO_TARGET_DIR="$root/target/0073-adjacent-target" "$CARGO" check -q --locked 2> "$scratch/check.log" ); then
  echo "cargo check: ok"
else
  echo "cargo check: FAILED"; grep "^error" "$scratch/check.log" | head -20; exit 1
fi
cmp -s "$lock" "$scratch/polars/Cargo.lock" || { echo "FAIL: the adjacent lock changed during the check"; exit 1; }
python3 - "$root/adapters/polars/surface.json" "$scratch/polars/surface.json" <<'PY'
import json,sys,collections
a=json.load(open(sys.argv[1])); b=json.load(open(sys.argv[2]))
ka={e['canonical_path']:e for e in a['entries']}; kb={e['canonical_path']:e for e in b['entries']}
same=set(ka)&set(kb)
print("| measure | count |\n|---|---:|")
print(f"| entries only in 0.55.2 | {len(set(ka)-set(kb))} |")
print(f"| entries only in 0.54.4 | {len(set(kb)-set(ka))} |")
print(f"| same path, signature changed | {sum(1 for p in same if ka[p].get('signature')!=kb[p].get('signature'))} |")
print(f"| same path, status changed | {sum(1 for p in same if ka[p]['status']!=kb[p]['status'])} |")
print(f"| same path, same signature and status | {sum(1 for p in same if ka[p]['status']==kb[p]['status'] and ka[p].get('signature')==kb[p].get('signature'))} |")
print("| 0.54.4 statuses | " + ", ".join(f"{k} {v}" for k,v in sorted(collections.Counter(e['status'] for e in b['entries']).items())) + " |")
PY
for f in functions.rs types.rs catalogue.rs; do
  if cmp -s "$root/adapters/polars/src/generated/$f" "$scratch/polars/src/generated/$f"; then echo "$f: identical"; else echo "$f: differs ($(diff "$root/adapters/polars/src/generated/$f" "$scratch/polars/src/generated/$f" | grep -c '^[<>]') changed lines)"; fi
done
