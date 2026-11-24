#!/bin/bash
# Run inside the leased final checkout, on the verified
# publication. Only this subprocess has the retired-source Git URL mapping.
set -euo pipefail
root=$(pwd -P)
baseline=$1
work=$2
recipes=$3
record=$4
source "$(dirname -- "$0")/state.sh"
store=$(dirname -- "$(dirname -- "$work")")
controller=$root/target/final-controller/release/rnx-final
lanes=$root/target/final-caches
[[ $(git rev-parse HEAD) == "$baseline" ]]
[[ -z $(git status --porcelain) ]]
mkdir "$work/cache-recipes"
export CARGO_NET_GIT_FETCH_WITH_CLI=true
export GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0="url.file://$root/.insteadOf"
export GIT_CONFIG_VALUE_0=https://github.com/nilket-com/rune
unset CARGO_INCREMENTAL CARGO_TARGET_DIR CARGO_BUILD_BUILD_DIR
preparation_space before-recipes "$store" "$record"
for manifest in probes/0072/extract/Cargo.toml tools/polars-gen/Cargo.toml; do
	cargo +1.98.1 fetch --locked --manifest-path "$manifest"
done
export RNX_FINAL_POLARS_SCRATCH="$work/inventory"
for configuration in adapter-json adapter adapter-narrow; do
	bash probes/0072/inventory/doc.sh 0.55.2 "$configuration" --fetch-only
done

# These are fresh preparation lanes or previously proven, exclusively
# owned lanes. Never attach an ownership marker to an existing directory.
claim_lane() {
	local path=$1 stamp
	if mkdir -- "$path" 2>/dev/null; then
		stamp=$(stat -c '%d %i' -- "$path")
		read -r dev ino <<<"$stamp"
		jq -nc --arg path "$path" --argjson dev "$dev" --argjson ino "$ino" \
			'[$path,{device:$dev,inode:$ino}]' >"$path/.rnx-cache-owner.json"
	fi
	[[ -d $path && $(realpath -e "$path") == "$path" ]]
	stamp=$(stat -c '%d %i' -- "$path")
	[[ $(jq -r --arg path "$path" 'select(.[0]==$path)|.[1]|[.device,.inode]|@tsv' \
		"$path/.rnx-cache-owner.json" | tr '\t' ' ') == "$stamp" ]]
}
claim_lane "$lanes"
subject=$(git log -1 --format=%s "$baseline")
case "$subject" in
	"fix: "*) predecessor=$(git rev-parse "$baseline^") ;;
	"impl: adr "*) predecessor=$(git rev-parse "$baseline^^") ;;
	*) echo "Unrecognized published revision: $baseline" >&2; exit 1 ;;
esac
while IFS= read -r row; do
	key=$(jq -er '.key' <<<"$row")
	lane=$(jq -er '.value.lane' <<<"$row")
	[[ $key =~ ^[[:alnum:]._/-]+-[a-f0-9]{64}$ && $key != */* && $lane =~ ^[a-f0-9]{64}$ ]]
	source=$(jq -er '.value.source' <<<"$row")
	if [[ $source == checkout ]]; then
		directory=$root
	else
		[[ $source != record-baseline ]] || source=$predecessor
		[[ $source =~ ^[a-f0-9]{40}$ ]]
		# The original lint archives were under the final checkout. The
		# xtask build script queries its enclosing Git HEAD and status.
		directory="$root/target/checkpoint-archives/$source"
		if [[ ! -d $directory ]]; then
			mkdir -p "$directory"
			sh tools/export-source.sh "$source" | tar -xf - -C "$directory"
		fi
	fi
	claim_lane "$lanes/$lane"
	cold=$(jq -er '.value.cold_bytes | select(type=="number" and .>=0 and .==floor)' <<<"$row")
	allocated=$(du -sxB1 -- "$lanes/$lane" | cut -f1)
	growth=$((cold - allocated))
	(( growth >= 1073741824 )) || growth=1073741824
	preparation_space "before-$key" "$store" "$record" "$growth"
	route=$(jq -c --arg target "$lanes/$lane" --arg marks "$work/cache-recipes/$key.marks" \
		'{target:$target,group:.value.group,recipe:.key,disable_incremental:(.value.incremental|not),mark:$marks}' <<<"$row")
	jq -ej '.value.command[]|select(contains("\u0000")|not)|., "\u0000"' <<<"$row" >"$work/argv"
	mapfile -d '' -t command <"$work/argv"
	[[ ${command[0]} == cargo ]]
	(
		cd "$directory"
		if [[ " ${command[*]} " == *" rune/tools/xtask/column/probe/Cargo.toml "* ]]; then
			valgrind_root=${RNX_VALGRIND_ROOT:-}
			if [[ -z $valgrind_root ]]; then
				executable=$(realpath -e -- "$(command -v valgrind)")
				valgrind_root=${executable%/bin/*}
			fi
			export VALGRIND_INCLUDE="$valgrind_root/include"
		fi
		"$controller" cached-cargo "$route" "${command[@]}"
	) >"$work/$key.log" 2>&1 || { tail -20 "$work/$key.log" >&2; exit 1; }
	preparation_space "$key" "$store" "$record"
done <"$recipes"
# Cargo's build-finished output decides the positive marks. Some
# invocations legitimately have no mark, for example a check without a
# retained executable; never manufacture marks for them.
jq -sr '[.[] | .value.lane] | unique[]' "$recipes" >"$work/lane-names"
while IFS= read -r lane; do
	ledger="$lanes/$lane/.rnx-marks.json"
	# A promoted ledger retains every previous mark and pending unlink.
	# cached-cargo journals these preparations in .rnx-observed. The
	# candidate's regular complete transaction reconciles both sets.
	if [[ -e $ledger ]]; then
		jq -e '(.recipes|type)=="object" and (.pending|type)=="object"' "$ledger" >/dev/null
		continue
	fi
	jq -sr --arg lane "$lane" '.[]|select(.value.lane==$lane)|.key' "$recipes" >"$work/keys"
	{
		while IFS= read -r key; do
			mark="$work/cache-recipes/$key.marks"
			if [[ -f $mark ]]; then jq -c --arg key "$key" '{($key):.}' "$mark"; fi
		done <"$work/keys"
	} | jq -s '{recipes:(add // {}), pending:{}}' >"$ledger.pending"
	mv -T "$ledger.pending" "$ledger"
done <"$work/lane-names"
jq -Rs --arg base "$lanes/" 'split("\n")|map(select(length>0)|$base+.)' <"$work/lane-names" >"$work/registry"
if [[ -f $lanes/lanes.json ]]; then
	jq -es '(.[0]|sort) == (.[1]|sort)' "$lanes/lanes.json" "$work/registry" >/dev/null
else
	cp -- "$work/registry" "$lanes/lanes.json"
fi
