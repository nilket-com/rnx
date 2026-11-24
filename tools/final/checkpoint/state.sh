# Shared by the entry and its preparation subprocess. Both run in Bash
# with errexit and pipefail. Paths are carried as arguments, including NUL
# delimited paths in the metadata census.
uptime_seconds() {
	local seconds rest
	read -r seconds rest </proc/uptime
	printf '%s\n' "$seconds"
}

publish_file() {
	local dest=$1 tmp
	tmp=$(mktemp -- "${dest}.XXXXXXXX")
	cat >"$tmp"
	sync -- "$tmp"
	mv -T -- "$tmp" "$dest"
	sync -- "$(dirname -- "$dest")"
}

# A build or controller startup can fail after the handoff and before
# normal finalization. Only the entry that holds the lease owns this out.
timed_refusal() {
	local out=$1 store=${RNX_FINAL_CHECKPOINT_STORE:?} ended
	[[ -d /proc/self/fd/7 && /proc/self/fd/7 -ef $store ]] || return 1
	[[ ! -f $out/RESULT.json && ! -f $out/PREPARATION-FAIL.json ]] || return 0
	[[ $(jq -er .out "$store/attempt.json") == "$out" ]] || return 1
	ended=$(uptime_seconds)
	jq --argjson ended "$ended" '{
		status:"FAIL",phase:"candidate",candidate,baseline,requested,acquired,admitted,finished:$ended,
		lease_wait_seconds:(.acquired-.requested),preparation_seconds:(.admitted-.acquired),
		timed_seconds:($ended-.admitted),total_seconds:($ended-.requested)
	}' "$out/preparation/admission.json" | publish_file "$out/PREPARATION-FAIL.json"
	printf 'FAIL %s\n' "$out/PREPARATION-FAIL.json" >&2
}

# Both the catalogue build rows and its locked source fetches are inputs.
recipe_digest() {
	(cd -- "$1"; sha256sum -- recipes.jsonl fetch.json | sha256sum | cut -d' ' -f1)
}

# Sample the same filesystem and owned store at build boundaries. This
# measures retained allocation, including the temporary anchor builds.
preparation_space() {
	local label=$1 store=$2 record=$3 need=$((21474836480 + ${4:-0})) free allocated
	free=$(df -B1 --output=avail -- "$store" | tail -1 | tr -d ' ')
	allocated=$(du -sxB1 -- "$store" | cut -f1)
	printf '%s\t%s\t%s\n' "$label" "$free" "$allocated" >>"$record"
	(( free >= need )) || { echo "checkpoint $label: $free bytes free, $need needed" >&2; return 1; }
}

# stat the links themselves. Access times change on a read and are not
# evidence of a write. ctime detects replacement with restored timestamps.
tree_identity() {
	local tree
	{
		for tree in "$@"; do
			printf 'root\0%s\0' "$tree"
			if [[ -e $tree || -L $tree ]]; then
				find -P "$tree" -printf '%D:%i:%y:%s:%m:%T@:%C@:%P\0' | LC_ALL=C sort -z || exit 1
			else
				printf 'absent\0'
			fi
		done
	} | sha256sum | cut -d' ' -f1
}

checkpoint_identity() {
	local target="$1/source/target"
	tree_identity "$target/final-caches" "$target/final-controller" "$target/fixed-route-anchor-cache-v2" "$1/cargo"
}

# Product file images live below XDG_CACHE_HOME/rnx-file-images. The
# project tool defaults to XDG_CACHE_HOME/rnx/assemblies. Include the
# product parent and the exact Cargo homes; other XDG consumers are
# independent of this final. Keep link and destination names.
developer_cache_paths() {
	local account=$1 login=$2 base path resolved
	local -a roots=( "$account/.cargo" "$login/.cargo" )
	if [[ -n ${CARGO_HOME:-} ]]; then roots+=("$CARGO_HOME"); fi
	if [[ -n ${RNX_PROJECT_CACHE:-} ]]; then roots+=("$RNX_PROJECT_CACHE"); fi
	for base in "$account/.cache" "$login/.cache" "${XDG_CACHE_HOME:-$account/.cache}"; do
		roots+=("$base/rnx" "$base/rnx-file-images")
	done
	for path in "${roots[@]}"; do
		if [[ $path != /* ]]; then path="$PWD/$path"; fi
		resolved=$(realpath -m -- "$path") || return 1
		printf '%s\0' "$path" "$resolved"
	done | LC_ALL=C sort -zu
}

developer_cache_manifest() {
	local paths=$1 path
	while IFS= read -r -d '' path; do
		if [[ -e $path || -L $path ]]; then
			find -P "$path" -printf '%D:%i:%y:%s:%m:%T@:%C@\t%p\0' || return 1
		else
			printf 'absent\t%s\0' "$path"
		fi
	done <"$paths" | LC_ALL=C sort -zu
}

developer_cache_changes() {
	local before=$1 after=$2 path
	{
		LC_ALL=C comm -z -23 -- "$before" "$after" || return 1
		LC_ALL=C comm -z -13 -- "$before" "$after" || return 1
	} | cut -z -f2- | LC_ALL=C sort -zu | while IFS= read -r -d '' path; do
		printf '%q\n' "$path"
	done
}

# This is a final-created clone. Keep the recovery ledger and the named
# payload parents it can still release at their original absolute paths.
# Cargo targets elsewhere in the checkout are invocation state.
reset_build_outputs() {
	local gen=$1 reset=$2 source="$1/source" target="$1/source/target" child
	verify_directory "$gen" || return 1
	[[ $(realpath -e "$source/.git") == "$source/.git" &&
		$(git -C "$source" rev-parse --absolute-git-dir) == "$source/.git" &&
		$(git -C "$source" rev-parse --show-toplevel) == "$source" ]] || return 1
	no_mounts "$gen" || return 1
	git -C "$source" clean -qffdx -- . ':(top,exclude)target' || return 1
	mkdir -p "$target" || return 1
	[[ $(realpath -e "$target") == "$target" ]] || return 1
	local -a entries
	shopt -s dotglob nullglob
	entries=("$target"/*)
	shopt -u dotglob nullglob
	for child in "${entries[@]}"; do
		case ${child##*/} in
			final-owners|routing-cli|rnx-verify) continue ;;
			final-controller) [[ $reset != invocation ]] || continue ;;
			final-caches|fixed-route-anchor-cache-v2|fixed-route-inputs) [[ $reset == all ]] || continue ;;
		esac
		find -P "$child" -xdev -depth -delete || return 1
	done
}

# Old lockfile downloads and obsolete bootstrap units have no per-unit
# promotion evidence. Rebuild those from the published inputs even when
# the terminal receipt authorizes promotion of compatible lane units.
prepare_generation() {
	local gen=$1 mode=$2
	case "$mode" in
		restored) reset_build_outputs "$gen" all || return 1 ;;
		promoted|cold) reset_build_outputs "$gen" "$mode" || return 1 ;;
		*) return 1 ;;
	esac
	if [[ -e $gen/cargo || -L $gen/cargo ]]; then release_owned "$gen/cargo"; fi
}

# Only an atomically claimed directory can later be replaced. A preexisting
# directory without our original device/inode is never treated as a cache.
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
			if [[ -e $path || -L $path ]]; then echo "checkpoint would inherit $path" >&2; return 1; fi
		done
		[[ $parent != / ]] || return 0
		parent=${parent%/*}
		[[ -n $parent ]] || parent=/
	done
}

validate_store_root() {
	local path=$1 uid=$2 owner mode
	if [[ -z $path || ! -d $path || $(realpath -e -- "$path") != "$path" ]]; then
		echo 'RNX_FINAL_CHECKPOINT_ROOT must name an existing canonical directory' >&2; return 1
	fi
	no_cargo_config "$path" || return 1
	[[ $(stat -c %u -- "$path") == "$uid" ]] || { echo "checkpoint root has the wrong owner: $path" >&2; return 1; }
	while :; do
		read -r owner mode < <(stat -c '%u %a' -- "$path") || return 1
		if [[ $owner != 0 && $owner != "$uid" ]] || (( (8#$mode & 022) != 0 )); then
			echo "unsafe checkpoint ancestor: $path (owner $owner, mode $mode)" >&2; return 1
		fi
		[[ $path != / ]] || return 0
		path=${path%/*}
		[[ -n $path ]] || path=/
	done
}

# A compiler wrapper or redirected Cargo path can supply previously built
# objects outside the final's lease. Reject the override before any build.
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
	find -P "$path" -xdev -depth -delete
}

no_mounts() {
	findmnt --json --list -o TARGET | jq -e --arg p "$1" \
		'all(.filesystems[].target; . != $p and (startswith($p+"/")|not))' >/dev/null
}

retire_invocation() {
	local old unit boot group_identity
	[[ -f $state/attempt.json ]] || return 0
	old=$(jq -er .invocation "$state/attempt.json") || return 1
	[[ $old == "$state/invocation" ]] || return 1
	unit=$(jq -er .unit "$state/attempt.json") || return 1
	boot=$(jq -er .boot_id "$state/attempt.json") || return 1
	group_identity=$(jq -er .cgroup_identity "$state/attempt.json") || return 1
	[[ $unit == /*.service && $unit != */../* && $unit != */./* && $group_identity =~ ^[0-9]+:[0-9]+$ ]] || return 1
	if [[ $boot == "$(cat /proc/sys/kernel/random/boot_id)" && -d /sys/fs/cgroup$unit &&
		$(stat -c '%d:%i' -- "/sys/fs/cgroup$unit") == "$group_identity" ]]; then
		grep -Fxq 'populated 0' "/sys/fs/cgroup$unit/cgroup.events" || {
			echo "prior invocation still has a populated service: $unit" >&2; return 1
		}
	fi
	if [[ -e $old || -L $old ]]; then release_owned "$old"; fi
}

# The final clone's origin may be a local mirror. Only the remote main
# beyond the mirror is the authority for publication.
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

# Detached checkout changes HEAD and rematerializes changed sources after
# the preceding Cargo build. Mounting an older separate worktree here can
# give Cargo a changed source whose timestamp predates the baseline binary.
checkout_revision() {
	local checkout=$1 revision=$2
	git -C "$checkout" -c advice.detachedHead=false checkout --quiet --detach "$revision"
	[[ $(git -C "$checkout" rev-parse HEAD) == "$revision" &&
		-z $(git -C "$checkout" status --porcelain) ]]
}

# Pick under both leases. Full prior receipt revalidation still precedes any build.
choose_generation() {
	mode=cold
	if [[ -e $generation || -L $generation ]]; then
		verify_directory "$generation"
		mode=restored
		if [[ ! -e $state/in-use && -f $state/previous.json ]]; then
			last=$(jq -er .out "$state/previous.json")
			last_candidate=$(jq -er .candidate "$state/previous.json")
			if [[ $last_candidate =~ ^[0-9a-f]{40}$ && -f $last/RESULT.json &&
				$(jq -er .checkout "$state/previous.json") == "$root" &&
				$(sha256sum <"$last/RESULT.json" | cut -d' ' -f1) == "$(jq -er .result "$state/previous.json")" &&
				$(checkpoint_identity "$generation") == "$(jq -er .seal "$state/previous.json")" ]] &&
				jq -e --arg sha "$last_candidate" '.candidate==$sha and .status=="PASS" and .complete_final and .publication_eligible' "$last/RESULT.json" >/dev/null; then
				if [[ $last_candidate == "$baseline" ]] || {
					[[ $(git rev-parse "$last_candidate^") == "$(git rev-parse "$baseline^")" ]] &&
					git diff --quiet --no-renames "$last_candidate" "$baseline" -- . ':(exclude)plans/**'
				}; then
					# The previous binary at the originally measured checkout
					# path must be the one whose complete final can vouch for it.
					if [[ $(sha256sum <"$root/target/final-controller/release/rnx-final" | cut -d' ' -f1) == "$(jq -er .tools.controller.binary_sha256 "$last/MANIFEST.json")" &&
						$(jq -er .terminal_state_sha256 "$last/CHECKPOINTRELEASE.json") == "$(jq -er .seal "$state/previous.json")" ]]; then
						mode=promoted
					fi
				fi
			fi
		fi
	fi
}

# A warm repeat cannot overwrite the disposition for a time-only failure.
guard_overrun() {
	mkdir -p "$state/overruns"
	# A kill immediately after the timed verdict must retain the decision too.
	if [[ -f $state/attempt.json ]]; then
		prior=$(jq -er '.out' "$state/attempt.json")
		prior_candidate=$(jq -er '.candidate' "$state/attempt.json")
		[[ $prior_candidate =~ ^[0-9a-f]{40}$ ]]
		if [[ -f $prior/RESULT.json ]] && jq -e --arg sha "$prior_candidate" '.candidate==$sha and .status=="FAIL" and .wall_budget_error!=null and .error==.wall_budget_error and .release_error==null and .manifest_error==null and all(.checks[];.exit==0)' "$prior/RESULT.json" >/dev/null; then
			printf '%s\n' "$prior" | publish_file "$state/overruns/$prior_candidate"
		fi
	fi
	if [[ -e $state/overruns/$candidate ]]; then
		echo "the terminal time-only failure for $candidate is at $(cat "$state/overruns/$candidate")" >&2; exit 1
	fi
}

refusal() {
	local code=$? ended disk=null
	trap - EXIT
	if ((code)) && [[ ! -f $out/RESULT.json ]]; then
		ended=$(uptime_seconds)
		if [[ -f $out/preparation/$outcome.json && $outcome == before-* ]]; then
			disk=$(jq -c '[.sites[] | select(.pass == false)][0]' "$out/preparation/$outcome.json")
		fi
		jq -n --argjson disk "$disk" --arg outcome "$outcome" --arg candidate "$candidate" --arg baseline "$baseline" \
			--argjson requested "$requested" --argjson acquired "$acquired" --argjson admitted "$admitted" --argjson ended "$ended" \
			'{status:"FAIL",phase:$outcome,candidate:$candidate,baseline:$baseline,requested:$requested,acquired:$acquired,
			admitted:$admitted,finished:$ended,total_seconds:($ended-$requested),
			lease_wait_seconds:(($acquired//$ended)-$requested),
			preparation_seconds:(if $acquired then (($admitted//$ended)-$acquired) else 0 end),
			timed_seconds:(if $admitted then ($ended-$admitted) else 0 end)} + (if $disk then {disk:$disk} else {} end)' | publish_file "$out/PREPARATION-FAIL.json"
		printf 'FAIL %s\n' "$out/PREPARATION-FAIL.json" >&2
	fi
	if [[ -n $run && $code == 0 ]]; then
		release_owned "$run" || exit 1
	fi
	exit "$code"
}
