#!/bin/bash
# All code compiled by preparation comes from the published revision.
set -euo pipefail
baseline=$1
proof=$2
mechanism=$3
scratch=$4
source "$mechanism/state.sh"
store=$(dirname -- "$(dirname -- "$scratch")")
preparation_space before-bootstrap "$store" "$proof/disk.tsv" 4294967296
[[ $(git rev-parse HEAD) == "$baseline" && -z $(git status --porcelain) ]]
target=$PWD/target/final-controller
controller=$target/release/rnx-final
CARGO_INCREMENTAL=0 CARGO_BUILD_BUILD_DIR="$target" CARGO_BUILD_JOBS=1 cargo +1.98.1 build \
	--locked --release -p rnx-final --target-dir "$target" --message-format=json-render-diagnostics >"$target/bootstrap-build.jsonl"
if [[ $baseline == ee52b405e9cea581c514ec87463faaa34d803b13 ]]; then
	cp -- "$mechanism/ee52b405.jsonl" "$proof/recipes.jsonl"
	cp -- "$mechanism/ee52b405-fetch.json" "$proof/fetch.json"
else
	"$controller" checkpoint-recipes >"$scratch/catalogue.json"
	jq -ceS '.recipes|to_entries[]' "$scratch/catalogue.json" >"$proof/recipes.jsonl"
	jq -ce '.fetch' "$scratch/catalogue.json" >"$proof/fetch.json"
fi
jq -ec '.[]' "$proof/fetch.json" >"$scratch/fetch.jsonl"
while IFS= read -r row; do
	jq -ej '.[]|., "\u0000"' <<<"$row" >"$scratch/fetch-argv"
	mapfile -d '' -t command <"$scratch/fetch-argv"
	"${command[@]}"
done < "$scratch/fetch.jsonl"
bash "$mechanism/prepare.sh" "$baseline" "$scratch" "$proof/recipes.jsonl" "$proof/disk.tsv"
preparation_space before-anchor "$store" "$proof/disk.tsv"
"$controller" stage fixed-route --anchor-only --out "$scratch/reference"
preparation_space after-anchor "$store" "$proof/disk.tsv"
jq -e '.status=="PASS" and .anchor_cache != null' "$scratch/reference/RESULT.json" > /dev/null
cp -- "$scratch/reference/RESULT.json" "$proof/anchor-result.json"

# Compare admissible Cargo units and immutable inputs across independent
# preparations. Device/inode seals are useful only within one generation.
while IFS= read -r row; do
	key=$(jq -er .key <<<"$row")
	mark="$scratch/cache-recipes/$key.marks"
	if [[ -f $mark ]]; then
		jq -cS --arg recipe "$key" '{recipe:$recipe,units:([.[]|.unit]|sort|unique)}' "$mark"
	else
		jq -nc --arg recipe "$key" '{recipe:$recipe,units:[]}'
	fi
done <"$proof/recipes.jsonl" >"$proof/units.jsonl"
(
	cd "$CARGO_HOME"
	find registry/cache -type f -name '*.crate' -print0 | LC_ALL=C sort -z | xargs -0r sha256sum
	if [[ -d git/checkouts ]]; then
		find git/checkouts -name .git -prune -print0 | LC_ALL=C sort -z |
			while IFS= read -r -d '' repo; do
				printf '%s ' "$repo"
				git -C "${repo%/.git}" rev-parse HEAD 'HEAD^{tree}'
			done
	fi
) >"$proof/locked-sources.txt"
hash() { sha256sum <"$1" | cut -d' ' -f1; }
jq -ncS --arg baseline "$baseline" --arg controller "$(hash "$controller")" \
	--arg units "$(hash "$proof/units.jsonl")" --arg deps "$(hash "$proof/locked-sources.txt")" \
	--arg anchor "$(jq -er .anchor_cache.key "$proof/anchor-result.json")" \
	'{baseline:$baseline,bootstrap:$controller,lane_units:$units,dependency_home:$deps,anchor:$anchor}' >"$proof/classes.json"
