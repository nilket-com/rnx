#!/bin/bash
# Release private products and verify the original homes before stopping the
# clock. Evidence and the recovery ledger live outside the invocation home.
set -euo pipefail
run=$1
source tools/final/checkpoint/state.sh
store=${RNX_FINAL_CHECKPOINT_STORE:?}
[[ $run == "$store/invocation" ]]
verify_directory "$run"
gen="$store/generation"
no_cargo_config "$gen"
if [[ $2 == PASS ]]; then reset_build_outputs "$gen" invocation; fi
home_sha=$(jq -er .developer_state_sha256 "$3")
[[ $(sha256sum <"$run/home-before" | cut -d' ' -f1) == "$home_sha" ]] || {
	echo 'developer cache manifest differs from admission' >&2; exit 1
}
developer_cache_manifest "$run/home-paths" >"$run/home-after"
if ! cmp -s -- "$run/home-before" "$run/home-after"; then
	changes="$(dirname -- "$3")/developer-cache-changes.txt"
	developer_cache_changes "$run/home-before" "$run/home-after" | publish_file "$changes"
	echo "developer cache metadata changed; full list: $changes" >&2
	head -n 20 -- "$changes" >&2
	exit 1
fi
state_sha=$(checkpoint_identity "$gen")
release_owned "$run"
jq -nc --arg home "$home_sha" --arg state "$state_sha" '{developer_state_sha256:$home,terminal_state_sha256:$state}'
