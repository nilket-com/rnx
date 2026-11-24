#!/bin/bash
# The system tools here are needed before the first candidate compilation.
# The leased generation is disposable. Receipts, the publication journal
# and the lifetime overrun decision live outside it.
set -euo pipefail
umask 077
root=$(pwd -P)
mechanism="$root/tools/final/checkpoint"
source "$mechanism/state.sh"
source "$mechanism/disk.sh"
requested=$1
shift
out=
complete=false
while (($#)); do
	case "$1" in
		--out) shift; out=${1:?missing output} ;;
		--complete) [[ $complete == false ]] || { echo 'duplicate --complete' >&2; exit 2; }; complete=true ;;
		*) echo "unexpected complete-final argument: $1" >&2; exit 2 ;;
	esac
	shift
done
[[ -n $out && $complete == true ]]
out=$(realpath -m -- "$out")
configured=${RNX_FINAL_CHECKPOINT_ROOT:-}
state="$configured/store"
account=$HOME
login_home=$(getent passwd "$(id -u)" | cut -d: -f6)
[[ $login_home == /* ]]
protected=( "${CARGO_HOME:-$account/.cargo}" "${XDG_CACHE_HOME:-$account/.cache}" "$account/.cargo" "$account/.cache" "$login_home/.cargo" "$login_home/.cache" )
if [[ -n ${RNX_PROJECT_CACHE:-} ]]; then protected+=("$RNX_PROJECT_CACHE"); fi
for path in "${protected[@]}"; do protected+=("$(realpath -m -- "$path")"); done
for path in "$state" "${protected[@]}"; do
	case "$out/" in "$path/"*) echo "output overlaps cache state: $path" >&2; exit 1 ;; esac
done
mkdir -p -- "$(dirname -- "$out")"
mkdir -- "$out"
acquired=null
admitted=null
outcome=refused
candidate=$(git rev-parse --verify HEAD)
baseline=
run=
generation="$state/generation"
trap refusal EXIT
[[ -z $(git status --porcelain) ]]
check_build_environment
validate_store_root "$configured" "$(id -u)"
for home_cache in "${protected[@]}"; do
	case "$state/" in "$home_cache/"*) echo "checkpoint overlaps developer cache: $home_cache" >&2; exit 1 ;; esac
	case "$home_cache/" in "$state/"*) echo "developer cache overlaps checkpoint: $home_cache" >&2; exit 1 ;; esac
done
claim_directory "$state"
[[ $(stat -c '%u:%a' "$state") == "$(id -u):700" ]] || { echo 'checkpoint store must be private to its owner' >&2; exit 1; }
no_cargo_config "$state"
# Descriptor 7 is inherited across exec. The same open file description
# covers preparation, the bootstrap, the controller and its terminal write.
exec 7<"$state"
flock -x 7
acquired=$(uptime_seconds)
sh tools/final/final.sh --checkpoint-resources "$state" >"$out/preparation-resource.json"
baseline=$(published_head "$root")
[[ $baseline =~ ^[0-9a-f]{40}$ ]]
case "$(git log -1 --format=%s)" in
	"fix: "*|"plan: adr "*) expected=$(git rev-parse HEAD^) ;;
	"impl: adr "*) expected=$(git rev-parse HEAD^^) ;;
	*) echo 'candidate has no record baseline' >&2; exit 1 ;;
esac
[[ $baseline == "$expected" ]] || { echo "published main $baseline differs from record baseline $expected" >&2; exit 1; }
guard_overrun
retire_invocation

# Rustup is a pinned external executable. Provision its published bytes
# separately so that the Cargo home and its bin directory can be masked
# even for grandchildren that discard CARGO_HOME and PATH.
proxies="$state/tool-bin"
rustup_sha=5be191e8c70a5007074254cb74d78dbfdd25cd5a61c27de7b8e5410122ec24f4
if [[ ! -d $proxies ]]; then
	tools=$(mktemp -d "$state/.tools-XXXXXXXX")
	curl --fail --silent --show-error --location --proto '=https' --output "$tools/rustup" \
		https://static.rust-lang.org/rustup/archive/1.29.2/x86_64-unknown-linux-gnu/rustup-init
	(cd "$tools"; sha256sum --status -c <<<"$rustup_sha  rustup"; chmod 755 rustup
		for name in cargo rustc rustdoc rustfmt cargo-fmt cargo-clippy clippy-driver rust-gdb rust-lldb; do ln -s rustup "$name"; done)
	mv -T "$tools" "$proxies"
fi
(cd "$proxies"; sha256sum --status -c <<<"$rustup_sha  rustup")
rustup_home=${RUSTUP_HOME:-$HOME/.rustup}
tools_sha=$(
	set -o pipefail
	{
		printf '%s\n' "$rustup_sha"
		for pin in 1.98.1 nightly-2026-09-01 nightly-2026-09-20; do
			tree="$rustup_home/toolchains/$pin-x86_64-unknown-linux-gnu"
			[[ -d $tree ]]
			find -P "$tree/bin" "$tree/lib" -type f -print0 | LC_ALL=C sort -z | xargs -0r sha256sum
		done
	} | sha256sum | cut -d' ' -f1
)
if [[ -f $state/toolchains && $(cat "$state/toolchains") != "$tools_sha" ]]; then
	echo 'checkpoint pinned toolchain contents changed' >&2; exit 1
fi
printf '%s\n' "$tools_sha" | publish_file "$state/toolchains"
proof="$out/preparation"
mkdir "$proof"
mv "$out/preparation-resource.json" "$proof/resources.json"
# Make the private checkout and the receipt locations stable across the
# baseline and candidate builds. The controller itself execs in the unit's
# original PID; every stage keeps its existing process containment.
caller=$root
if [[ ! -v RNX_JUPYTER_PYTHON && -f $caller/target/jupyter-client/bin/python ]]; then
	export RNX_JUPYTER_PYTHON="$caller/target/jupyter-client/bin/python"
fi
root=$generation/source
target=$root/target
choose_generation
if [[ $mode == promoted ]]; then
	(cd "$root"; "$target/final-controller/release/rnx-final" --out "$last" --revalidate)
fi
printf '%s\n' "$candidate" | publish_file "$state/in-use"
boot=$(cat /proc/sys/kernel/random/boot_id)
unit=$(sed -n 's/^0:://p' /proc/self/cgroup)
[[ $unit == /*.service && ${unit##*/} != user@* ]] || { echo 'a complete final needs its own service' >&2; exit 1; }
group_identity=$(stat -c '%d:%i' -- "/sys/fs/cgroup$unit")
run="$state/invocation"
[[ ! -e $run && ! -L $run ]] || { echo 'unexpected invocation root' >&2; exit 1; }
jq -nc --arg out "$out" --arg candidate "$candidate" --arg run "$run" --arg unit "$unit" --arg boot "$boot" --arg group "$group_identity" \
	'{out:$out,candidate:$candidate,invocation:$run,unit:$unit,boot_id:$boot,cgroup_identity:$group}' | publish_file "$state/attempt.json"
claim_directory "$run"
claim_directory "$generation"
no_cargo_config "$generation"
if [[ ! -e $root ]]; then
	git clone --quiet --local --no-checkout "$caller" "$run/checkout"
	printf '%s\n' "$state" | publish_file "$run/checkout/.git/rnx-final-store"
	mv -T -- "$run/checkout" "$root"
else
	[[ $(realpath -e "$root/.git") == "$root/.git" ]]
	[[ $(cat "$root/.git/rnx-final-store") == "$state" ]] || { echo 'unclaimed checkpoint checkout' >&2; exit 1; }
	git -C "$root" fetch --quiet "$caller" "$candidate"
fi
prepare_generation "$generation" "$mode"
checkout_revision "$root" "$baseline"
no_cargo_config "$target"
inputs="$target/fixed-route-inputs"
pin=$(jq -er .installed_setup.outer_reference.host_inputs.loader_cache_sha256 "$root/tools/column/fixed_route_policy.json")
[[ $pin =~ ^[a-f0-9]{64}$ ]]
mkdir -p "$inputs"
[[ $(realpath -e "$inputs") == "$inputs" ]]
if [[ ! -f $inputs/$pin ]]; then
	seed=/etc/ld.so.cache
	[[ ! -f $caller/target/fixed-route-inputs/$pin ]] || seed="$caller/target/fixed-route-inputs/$pin"
	cp -- "$seed" "$inputs/$pin"
fi
(cd "$inputs"; sha256sum -c --status <<<"$pin  $pin")
claim_directory "$generation/cargo"
lanes=$target/final-caches
bootstrap=$target/final-controller
if mkdir -- "$lanes" 2>/dev/null; then
	read -r dev ino < <(stat -c '%d %i' "$lanes")
	jq -nc --arg path "$lanes" --argjson dev "$dev" --argjson ino "$ino" \
		'[$path,{device:$dev,inode:$ino}]' >"$lanes/.rnx-cache-owner.json"
fi
if mkdir -- "$bootstrap" 2>/dev/null; then stat -c '%d:%i' "$root" "$bootstrap" >"$bootstrap/OWNER"; fi
stat -c '%d:%i' "$root" "$bootstrap" | cmp -s - "$bootstrap/OWNER"
claim_directory "$run/scratch"
mkdir "$run/home"
claim_directory "$run/tmp"
developer_cache_paths "$account" "$login_home" >"$run/home-paths"
developer_cache_manifest "$run/home-paths" >"$run/home-before"
developer_before=$(sha256sum <"$run/home-before" | cut -d' ' -f1)
export GIT_CONFIG_GLOBAL=${GIT_CONFIG_GLOBAL:-$account/.gitconfig}
export HOME="$run/home" CARGO_HOME="$generation/cargo" RUSTUP_HOME="$rustup_home"
export XDG_CACHE_HOME="$HOME/.cache" TMPDIR="$run/tmp"
mkdir "$XDG_CACHE_HOME"
ln -s -- "$CARGO_HOME" "$HOME/.cargo"
ln -s -- "$RUSTUP_HOME" "$HOME/.rustup"
# Put the independently authenticated proxies first, including for child
# programs which derive cargo's path from HOME rather than from PATH.
[[ -e $CARGO_HOME/bin ]] || ln -s -- "$proxies" "$CARGO_HOME/bin"
[[ $(realpath -e "$CARGO_HOME/bin") == "$proxies" ]]
IFS=: read -ra old_path <<<"$PATH"
private_path=$proxies
for directory in "${old_path[@]}"; do
	[[ $directory == /* ]] || continue
	physical=$(realpath -m -- "$directory")
	for path in "${protected[@]}"; do
		case "$physical/" in "$path/"*) continue 2 ;; esac
	done
	private_path+=":$directory"
done
export PATH="$private_path"
unset RNX_PROJECT_CACHE CARGO_TARGET_DIR CARGO_BUILD_BUILD_DIR RUSTUP_AUTO_INSTALL
cd "$root"
prepare_checkpoint() {
	no_cargo_config "$generation"
	usable=$(jq -er '.memory_usable_bytes' "$proof/resources.json")
	jobs=$(( (usable - 2147483648) / 3221225472 ))
	cores=$(nproc)
	(( jobs <= cores )) || jobs=$cores
	(( jobs >= 1 ))
	CARGO_BUILD_JOBS=$jobs bash "$mechanism/baseline.sh" "$baseline" "$proof" "$mechanism" "$run/scratch" >"$proof/preparation.log" 2>&1
	release_owned "$run/scratch"
	reset_build_outputs "$generation" invocation
	# Discard every product of the preparation home, including implicit
	# tool caches outside XDG_CACHE_HOME. Only the four checkpoint classes
	# can supply the timed candidate.
	[[ $(realpath -e "$HOME") == "$run/home" ]]
	no_mounts "$HOME"
	find -P "$HOME" -xdev -mindepth 1 -depth -delete
	mkdir "$XDG_CACHE_HOME"
	ln -s -- "$CARGO_HOME" "$HOME/.cargo"
	ln -s -- "$RUSTUP_HOME" "$HOME/.rustup"
	release_owned "$TMPDIR"
	claim_directory "$TMPDIR"
	prepared=$(sha256sum <"$proof/classes.json" | cut -d' ' -f1)
	recipes_sha=$(recipe_digest "$proof")
	no_cargo_config "$generation"
	[[ $(published_head "$caller") == "$baseline" && $(git -C "$caller" rev-parse HEAD) == "$candidate" && -z $(git -C "$caller" status --porcelain) ]]
}
prepare_and_admit
jq -nc --arg baseline "$baseline" --arg candidate "$candidate" --arg boot "$boot" --arg prep "$mode" \
	--arg tools "$tools_sha" --arg recipes "$recipes_sha" --arg prepared "$prepared" --arg dev "$developer_before" \
	--argjson requested "$requested" --argjson acquired "$acquired" --argjson admitted "$admitted" \
	'{baseline:$baseline,candidate:$candidate,boot_id:$boot,preparation:$prep,recipe_sha256:$recipes,toolchains_sha256:$tools,
	prepared_state_sha256:$prepared,developer_state_sha256:$dev,requested:$requested,acquired:$acquired,admitted:$admitted}' | publish_file "$proof/admission.json"
outcome=candidate
checkout_revision "$root" "$candidate"
no_cargo_config "$target"
export RNX_FINAL_CHECKPOINT_STORE="$state" RNX_FINAL_CHECKPOINT_INVOCATION="$run"
exec sh tools/final/final.sh --complete --out "$out"
