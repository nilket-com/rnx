#!/usr/bin/env bash
# Record 0125 (review): replay the rekey from the retained inputs (0124's
# surface and catalogue at 40f0334, the adapter-narrow and adapter-json
# inventories) and compare with the committed frozen list; then run the
# synthetic controls. Needs both inventories (probes/0072/inventory/doc.sh).
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd); here="$root/probes/0125"
d=$(mktemp -d); trap 'rm -rf "$d"' EXIT
python3 "$root/probes/0119/frozen.py" build <(git -C "$root" show 40f0334:adapters/polars/surface.json) \
	<(git -C "$root" show 40f0334:adapters/polars/src/generated/catalogue.rs) "$d/frozen.json" > /dev/null
git -C "$root" show 40f0334:tools/polars-gen/releases/0.55.2-joins.toml > "$d/release.toml"
python3 "$here/rekey.py" "$root/probes/0072/out/0.55.2-adapter-narrow/result/inventory.json" \
	"$root/probes/0072/out/0.55.2-adapter-json/result/inventory.json" "$d/frozen.json" "$d/release.toml"
cmp "$d/frozen.json" "$here/frozen-0.55.2.json"
python3 "$here/rekey_controls.py"
echo "0125 rekey replayed: the committed frozen list, and the controls"
