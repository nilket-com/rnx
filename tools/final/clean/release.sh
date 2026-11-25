#!/bin/bash
# Called only after the controller's guardian drain and failure evidence copy-out.
set -euo pipefail
run=$1 out=$2 phase=$3
area=${RNX_FINAL_AREA:?}
[[ $run == "$area/run" && -d /proc/self/fd/7 && /proc/self/fd/7 -ef $area ]]
source "$(dirname -- "$0")/ownership.sh"
verify_directory "$run"
[[ $(jq -er .identity "$area/attempt.json") == "$(stat -c '%d:%i' "$run")" ]]
no_mounts "$run"
count=0
clean_status=0
if [[ -d $run/source ]]; then
	cd "$run/source"
	# Cargo creates tagged targets on first use. The bootstrap claims
	# its own target before invoking Cargo and supplies the same signature.
	while IFS= read -r -d '' stamp; do
		target=${stamp%/CACHEDIR.TAG}
		[[ -d $target ]] || continue
		[[ $(realpath -e -- "$target") == "$target" && $target == "$run/source/"* ]]
		grep -Fxq 'Signature: 8a477f597d28d172789f06886806bc55' "$stamp" || continue
		if CARGO_BUILD_BUILD_DIR="$target" timeout --kill-after=10s 900s cargo +1.98.1 clean \
			--target-dir "$target" --offline --manifest-path "$run/source/Cargo.toml" >/dev/null; then
			count=$((count+1))
		else
			clean_status=1
		fi
	done < <(find -P "$run/source" -depth -name CACHEDIR.TAG -type f -print0)
fi
# Even an exhausted device must release the build output. Failure of the
# final measurement still fails the verdict after release.
sample_status=0
bash "$(dirname -- "$(realpath -- "$0")")/space.sh" release "$run" "$out" || sample_status=$?
peaks=$(jq 'map_values(.peak)' "$run/space.json")
if [[ -f $run/fetch.json ]]; then
	cp -- "$run/fetch.json" "$out/FETCH.json.pending"
	mv -- "$out/FETCH.json.pending" "$out/FETCH.json"
fi
cd /
release_owned "$run"
rm -- "$area/attempt.json"
ended=$(cut -d' ' -f1 /proc/uptime)
jq -nc --arg dir "$run" --arg base "$RNX_FINAL_BASELINE" --arg candidate "$RNX_FINAL_CANDIDATE" --argjson start "$RNX_FINAL_STARTED" \
	--argjson end "$ended" --argjson peaks "$peaks" --argjson count "$count" \
	'{directory:$dir,candidate:$candidate,baseline:$base,removed:true,started:$start,finished:$end,peak_bytes:$peaks,targets_cleaned:$count}' >"$out/CLEANUP.json.pending"
mv "$out/CLEANUP.json.pending" "$out/CLEANUP.json"
if [[ $phase == bootstrap ]]; then echo 'FAIL bootstrap' >&2; fi

(( clean_status == 0 && sample_status == 0 ))
