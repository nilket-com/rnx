#!/bin/bash
# Execute the controller in the same service process that claimed the run.
set -euo pipefail
umask 077
started=$1
shift
caller=$(pwd -P)
source "$caller/tools/final/clean/ownership.sh"
args=()
out=
successor=false
while (($#)); do
	case "$1" in
		--out) shift; out=$(realpath -m -- "${1:?output directory required}") ;;
		--complete|--required) args+=("$1") ;;
		--docs-successor) successor=true; args+=("$1"); shift; args+=("$(realpath -e -- "${1:?missing previous final}")") ;;
		--stages) args+=("$1"); shift; args+=("${1:?missing option value}") ;;
		*) echo "unknown final option: $1" >&2; exit 2 ;;
	esac
	shift
done
[[ -n $out ]]
check_build_environment
# The persistent siblings hold the lock, recovery record, and verified gate inputs.
area=${RNX_FINAL_RUNS_ROOT:-$HOME/.rnx-finals}
check_socket_room "$area"
[[ $area == /* && $(realpath -m -- "$area") == "$area" ]]
for protected in "${CARGO_HOME:-$HOME/.cargo}" "${XDG_CACHE_HOME:-$HOME/.cache}" "$caller"; do
	protected=$(realpath -m -- "$protected")
	case "$area/" in "$protected/"*) echo "run area overlaps $protected" >&2; exit 1 ;; esac
	case "$protected/" in "$area/"*) echo "run area contains $protected" >&2; exit 1 ;; esac
done
no_cargo_config "$area"
claim_directory "$area"
[[ $(stat -c '%u:%a' -- "$area") == "$(id -u):700" ]]
case "$out/" in "$area/"*) echo 'receipt directory is inside the final state' >&2; exit 1 ;; esac
exec 7<"$area"
flock -nx 7 || { echo 'another final owns the build area' >&2; exit 1; }
recover_run "$area"
run=$area/run
unit=$(sed -n 's/^0:://p' /proc/self/cgroup)
[[ $unit == /*.service && ${unit##*/} != user@* ]] || { echo 'start the build final in a dedicated service' >&2; exit 1; }
candidate=$(git rev-parse HEAD)
[[ -z $(git status --porcelain) ]]
mkdir -p -- "$(dirname -- "$out")"
mkdir -- "$out"
baseline=$(published_head "$caller")
[[ $baseline =~ ^[0-9a-f]{40}$ ]]
case "$(git log -1 --format=%s)" in
	'fix: '*|'plan: adr '*) parent=$(git rev-parse HEAD^) ;;
	'impl: adr '*) parent=$(git rev-parse HEAD^^) ;;
	*) echo 'candidate has no published record baseline' >&2; exit 1 ;;
esac
[[ $baseline == "$parent" ]]
claim_run "$area" "$unit"
abort() {
	local code=$?
	trap - EXIT INT TERM
	cd /
	if [[ $(jq -er .identity "$area/attempt.json") == "$(stat -c '%d:%i' "$run")" ]] && verify_directory "$run" && release_owned "$run"; then
		rm -- "$area/attempt.json"
		jq -nc --arg candidate "$candidate" --argjson started "$started" --argjson ended "$(cut -d' ' -f1 /proc/uptime)" \
			'{status:"FAIL",phase:"bootstrap",candidate:$candidate,removed:true,started:$started,finished:$ended}' >"$out/BOOTSTRAP-FAIL.json"
	fi
	exit "$code"
}
trap abort EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
read_limits "$caller" "$baseline" "$successor" "$run/limits"
mkdir "$run/home" "$run/tmp" "$run/tools" "$run/cargo" "$run/scratch"
export RNX_FINAL_AREA="$area" RNX_FINAL_RUN="$run" RNX_FINAL_BASELINE="$baseline"
export RNX_FINAL_CANDIDATE="$candidate" RNX_FINAL_STARTED="$started" RNX_FINAL_RECEIPTS="$out"
# Capture availability before the first download, clone or compilation.
"$caller/tools/final/clean/space.sh" start "$run" "$out"
if [[ -z ${RNX_JUPYTER_PYTHON:-} && -f $caller/target/jupyter-client/bin/python ]]; then
	export RNX_JUPYTER_PYTHON="$caller/target/jupyter-client/bin/python"
fi
export RUSTUP_HOME=${RUSTUP_HOME:-$HOME/.rustup} GIT_CONFIG_GLOBAL=${GIT_CONFIG_GLOBAL:-$HOME/.gitconfig}
proxy=$(command -v rustup)
install -m 755 -- "$proxy" "$run/tools/rustup"
(cd "$run/tools"; sha256sum --status -c <<<'5be191e8c70a5007074254cb74d78dbfdd25cd5a61c27de7b8e5410122ec24f4  rustup')
for name in cargo rustc rustdoc rustfmt cargo-fmt cargo-clippy clippy-driver rust-gdb rust-lldb; do ln -s rustup "$run/tools/$name"; done
export HOME="$run/home" TMPDIR="$run/tmp" CARGO_HOME="$run/cargo"
export XDG_CACHE_HOME="$HOME/.cache"
IFS=: read -ra directories <<<"$PATH"
PATH="$run/tools"
for directory in "${directories[@]}"; do
	[[ $directory == /* && $(realpath -m -- "$directory") != */.cargo/bin ]] || continue
	PATH+=":$directory"
done
export PATH PYTHONDONTWRITEBYTECODE=1
unset RUSTUP_AUTO_INSTALL
unset RNX_PROJECT_CACHE
mkdir "$XDG_CACHE_HOME"
ln -s "$CARGO_HOME" "$HOME/.cargo"
ln -s "$run/tools" "$CARGO_HOME/bin"
ln -s "$RUSTUP_HOME" "$HOME/.rustup"
git clone --local --quiet --no-checkout "$caller" "$run/source"
git -C "$run/source" fetch -q "$caller" "$candidate"
git -C "$run/source" -c advice.detachedHead=false checkout -q --detach "$candidate"
cd "$run/source"
mkdir -p target/fixed-route-inputs
pin=$(jq -er .installed_setup.outer_reference.host_inputs.loader_cache_sha256 tools/column/fixed_route_policy.json)
[[ $pin =~ ^[0-9a-f]{64}$ ]]
seed="$caller/target/fixed-route-inputs/$pin"
[[ -f $seed ]] || seed=/etc/ld.so.cache
cp -- "$seed" "target/fixed-route-inputs/$pin"
(cd target/fixed-route-inputs; sha256sum --status -c <<<"$pin  $pin")
fetch_started=$(cut -d' ' -f1 /proc/uptime)
source tools/final/clean/downloads.sh
downloads=$area/downloads
went_online=false
plan_fetches "$baseline" "$successor"
locks=()
for manifest in "${manifests[@]}"; do locks+=("${manifest%Cargo.toml}Cargo.lock"); done
for config in "${inventory[@]}"; do locks+=("probes/0072/inventory/locks/0.55.2-$config.lock"); done
download_plan "${locks[@]}" >"$run/download-plan"
restore_downloads "$downloads" "$run/download-plan"
# Limit the historical Rune URL mapping to the fetch phase.
(
	export CARGO_NET_GIT_FETCH_WITH_CLI=true GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0="url.file://$PWD/.insteadOf"
	export GIT_CONFIG_VALUE_0=https://github.com/nilket-com/rune
	for manifest in "${manifests[@]}"; do
		fetch_locked cargo +1.98.1 fetch --locked --manifest-path "$manifest"
		bash tools/final/clean/space.sh sample "$run" "$out"
	done
	export RNX_FINAL_POLARS_SCRATCH="$run/scratch/inventory"
	for config in "${inventory[@]}"; do
		fetch_locked bash probes/0072/inventory/doc.sh 0.55.2 "$config" --fetch-only
	done
	echo "$went_online" >"$run/went-online"
)
fetch_finished=$(cut -d' ' -f1 /proc/uptime)
read -r cargo_bytes _ < <(du -sB1 -- "$CARGO_HOME")
jq -n --argjson start "$fetch_started" --argjson end "$fetch_finished" --argjson bytes "$cargo_bytes" --slurpfile online "$run/went-online" \
	'{went_online:$online[0],seconds:($end-$start),cargo_home_allocated_bytes:$bytes}' > "$run/fetch.json"
bash tools/final/clean/space.sh sample "$run" "$out"
export CARGO_NET_OFFLINE=true
exec sh tools/final/final.sh --out "$out" "${args[@]}"
