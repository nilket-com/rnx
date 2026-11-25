# Linux pathname sockets have 107 usable bytes. Even a 20-digit stage
# index under run/t is shorter than the stable fixed-route TMPDIR below.
check_socket_room() {
	local LC_ALL=C
	local path="$1/run/scratch/fixed-route-tmp/rnx-0275-4294967295-18446744073709551615/ready"
	if (( ${#path} > 107 )); then
		echo 'RNX_FINAL_RUNS_ROOT exceeds 32 bytes; stage TMPDIR must leave 47 bytes for a Unix socket pathname' >&2
		return 1
	fi
}

# Common ownership checks for the disposable build root.
claim_directory() {
	local path=$1 pending
	if [[ ! -e $path && ! -L $path ]]; then
		pending=$(mktemp -d -- "${path}.XXXXXXXX")
		stat -c '%d:%i' -- "$pending" >"$pending/OWNER"
		sync -- "$pending/OWNER"
		mv -nT -- "$pending" "$path"
		if [[ -d $pending ]]; then rm -- "$pending/OWNER"; rmdir -- "$pending"; fi
		sync -- "$(dirname -- "$path")"
	fi
	verify_directory "$path"
}

verify_directory() {
	local path=$1
	[[ $(realpath -e -- "$path") == "$path" && -d $path && ! -L $path/OWNER && -f $path/OWNER ]] &&
		stat -c '%d:%i' -- "$path" | cmp -s - "$path/OWNER"
}

# Cargo searches physical ancestors even with private HOME and CARGO_HOME.
no_cargo_config() {
	local parent=$1 path
	while :; do
		for path in "${parent%/}/.cargo/config" "${parent%/}/.cargo/config.toml"; do
			if [[ -e $path || -L $path ]]; then echo "build would inherit $path" >&2; return 1; fi
		done
		[[ $parent != / ]] || return 0
		parent=${parent%/*}
		[[ -n $parent ]] || parent=/
	done
}

check_build_environment() {
	local variable
	for variable in CARGO_TARGET_DIR CARGO_BUILD_TARGET_DIR CARGO_BUILD_BUILD_DIR RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER CARGO_BUILD_RUSTC_WRAPPER CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER; do
		if [[ -n ${!variable:-} ]]; then
			echo "final refuses $variable override" >&2
			return 1
		fi
	done
}

# These directories were atomically claimed by this invocation. Refuse
# any mount (including a same-device bind) before releasing scratch.
release_owned() {
	local path=$1
	verify_directory "$path" || return 1
	no_mounts "$path" || return 1
	find -P "$path" -xdev -depth -mindepth 1 ! -path "$path/OWNER" -delete &&
		rm -- "$path/OWNER" && rmdir -- "$path"
}

no_mounts() {
	findmnt --json --list -o TARGET | jq -e --arg p "$1" \
		'all(.filesystems[].target; . != $p and (startswith($p+"/")|not))' >/dev/null
}

published_head() {
	local directory=$1 remote
	declare -A seen=()
	while true; do
		[[ ! -v 'seen[$directory]' ]] || return 1
		seen[$directory]=1
		if [[ $(git -C "$directory" rev-parse --is-bare-repository) == true ]]; then
			git -C "$directory" rev-parse --verify refs/heads/main
			return
		fi
		remote=$(git -C "$directory" remote get-url origin) || return 1
		case "$remote" in
			/*) directory=$(realpath -e -- "$remote") || return 1 ;;
			file://*) directory=$(realpath -e -- "${remote#file://}") || return 1 ;;
			./*|../*) directory=$(realpath -e -- "$directory/$remote") || return 1 ;;
			*) git -C "$directory" ls-remote --exit-code origin refs/heads/main | awk '$2=="refs/heads/main"{print $1}'; return ;;
		esac
	done
}


recover_run() {
	local area=$1 run=$1/run attempt=$1/attempt.json unit boot group
	if [[ ! -e $attempt ]]; then
		[[ ! -e $run && ! -L $run ]] || { echo 'unclaimed run directory' >&2; return 1; }
		return 0
	fi
	[[ -f $attempt && ! -L $attempt ]] || return 1
	[[ $(jq -er .run "$attempt") == "$run" ]] || return 1
	unit=$(jq -er .unit "$attempt")
	boot=$(jq -er .boot "$attempt")
	group=$(jq -er .group "$attempt")
	[[ $unit == /*.service && $unit != */../* && $unit != */./* && $group =~ ^[0-9]+:[0-9]+$ ]] || return 1
	if [[ $boot == "$(cat /proc/sys/kernel/random/boot_id)" && -d /sys/fs/cgroup$unit &&
		$(stat -c '%d:%i' "/sys/fs/cgroup$unit") == "$group" ]]; then
		grep -Fxq 'populated 0' "/sys/fs/cgroup$unit/cgroup.events" || {
			echo 'prior run still active' >&2; return 1
		}
	fi
	if [[ ! -e $run && ! -L $run ]]; then
		local staged
		staged=$(jq -er .staged "$attempt")
		[[ ${staged%/*} == "$area" && ${staged##*/} == .claim-* ]] || return 1
		run=$staged
	fi
	if [[ -e $run || -L $run ]]; then
		[[ $(jq -er .identity "$attempt") == "$(stat -c '%d:%i' -- "$run")" ]] || return 1
		release_owned "$run" || return 1
	fi
	rm -- "$area/attempt.json"
}


# The durable journal names the staging inode before the atomic rename.
# Recovery can see that same inode on either side of the rename.
claim_run() {
	local area=$1 unit=$2 run=$1/run staged
	staged=$(mktemp -d -- "$area/.claim-XXXXXXXX")
	stat -c '%d:%i' -- "$staged" >"$staged/OWNER"
	jq -nc --arg run "$run" --arg staged "$staged" --arg identity "$(stat -c '%d:%i' "$staged")" --arg unit "$unit" \
		--arg boot "$(cat /proc/sys/kernel/random/boot_id)" --arg group "$(stat -c '%d:%i' "/sys/fs/cgroup$unit")" \
		'{run:$run,staged:$staged,identity:$identity,unit:$unit,boot:$boot,group:$group}' >"$area/attempt.json.new"
	sync -- "$area/attempt.json.new"
	mv -T -- "$area/attempt.json.new" "$area/attempt.json"
	mv -T -- "$staged" "$run"
	sync -- "$area"
}

read_limits() {
	local root=$1 baseline=$2 successor=$3 out=$4
	local limits=plans/impl/0338-final-limits.json
	if [[ $baseline == b8d5ad340653623cc5e1855bc4d2c85ce00ccb8f ]]; then
		[[ $(git -C "$root" rev-parse HEAD^) == d18a457ae86fd53f03986925489ef8dcb147ea5d &&
			$(git -C "$root" log -1 --format=%s) == 'impl: adr 0338 '* ]] ||
			{ echo 'unbounded final requires the signed adoption plan' >&2; return 1; }
		[[ $successor == true || ! -e $root/$limits ]] || return 1
		printf 'null\n' >"$out"
	else
		local first
		first=$(git -C "$root" log -1 --diff-filter=A --format=%H "$baseline" -- "$limits") || return 1
		[[ -n $first ]] || { echo 'published limits missing' >&2; return 1; }
		git -C "$root" show "$first:$limits" >"$out" || return 1
		git -C "$root" show "$baseline:$limits" | cmp -s - "$out" || return 1
		cmp -- "$out" "$root/$limits" || return 1
		jq -e '(.receipt_sha256|test("^[0-9a-f]{64}$")) and (.seconds|type=="number" and floor==. and .>0) and
			(.bytes|type=="object" and has("build") and has("memory") and all(.[]; type=="number" and floor==. and .>=0))' "$out" >/dev/null
	fi
}
