#!/bin/sh
# Entry of rnx's final: start the clock, admit and build the controller from
# the candidate with one job, then hand it a single-use record of all of it.
# Usage: tools/final/final.sh --out DIR [--complete | --required | --stages A,B]
set -eu

# Seconds since boot (CLOCK_BOOTTIME), before anything else.
started=$(cut -d' ' -f1 /proc/uptime) || exit 1

# Constants shared with the controller; tests/bootstrap.rs checks they agree.
FLOOR_BYTES=21474836480         # controller::DISK_FLOOR_BYTES, 20 GiB
COLD_BYTES=4294967296           # scheduler::COLD_BYTES, 4 GiB
RESERVE_BYTES=2147483648        # memory::RESERVE_BYTES, 2 GiB
JOB_MEMORY_BYTES=3221225472     # memory::JOB_MEMORY_BYTES, 3 GiB

root=$(cd "$(dirname "$0")/../.." && pwd -P)
cd "$root"
export GIT_OPTIONAL_LOCKS=0
fail() {
	echo "final.sh: $*" >&2
	exit 1
}

# Each command's own status is checked before its output is believed: a
# pipeline or a substitution inside a test would hide a failure.
number() {
	case $2 in
		'' | *[!0-9]*) fail "$1 is not a number: '$2'" ;;
	esac
}
digest() {
	case $2 in
		[0-9a-f]*) [ ${#2} -eq 64 ] || fail "$1 is not a SHA-256: '$2'" ;;
		*) fail "$1 is not a SHA-256: '$2'" ;;
	esac
}

candidate=$(git rev-parse --verify HEAD) || fail "no candidate commit"
status=$(git status --porcelain) || fail "git status failed"
[ -z "$status" ] || fail "the checkout is not clean"
lock_sha=$(sha256sum Cargo.lock) || fail "cannot hash Cargo.lock"
lock_sha=${lock_sha%% *}
digest "the lock file digest" "$lock_sha"
toolchain=$(rustc +1.98.1 -vV) || fail "the pinned toolchain is missing"
toolchain_sha=$(printf '%s\n' "$toolchain" | sha256sum) || fail "cannot hash the toolchain"
toolchain_sha=${toolchain_sha%% *}
digest "the toolchain digest" "$toolchain_sha"

# Disk: the final's floor and one cold Cargo target directory.
fixture=false
target=$root/target/final-controller
probe=$target
while [ ! -e "$probe" ]; do probe=${probe%/*}; done
filesystem=$(stat -f -c '%T:%i' -- "$probe") || fail "cannot identify filesystem for $probe"
if [ "${RNX_BOOTSTRAP_DISK_BYTES+x}" ]; then
	[ -n "${RNX_BOOTSTRAP_FIXTURE:-}" ] || fail "fixture disk bytes are refused outside the bootstrap controls"
	fixture=true
	free=$RNX_BOOTSTRAP_DISK_BYTES
else
	free=$(df -B1 --output=avail -- "$probe") || fail "df failed for $probe"
	free=$(printf '%s\n' "$free" | tail -n 1 | tr -d ' ')
fi
number "free disk" "$free"
[ "$free" -ge $((FLOOR_BYTES + COLD_BYTES)) ] || fail "disk: $target (filesystem $filesystem at $probe): $free bytes free, $((FLOOR_BYTES + COLD_BYTES)) needed"

# Memory: the least of the host's available memory and every enclosing
# control group's finite high or max limit minus its usage, with no credit
# for page cache; one build job and the reserve must fit.
cgroup_root=${RNX_BOOTSTRAP_CGROUP_ROOT:-/sys/fs/cgroup}
meminfo=${RNX_BOOTSTRAP_MEMINFO:-/proc/meminfo}
if [ -n "${RNX_BOOTSTRAP_CGROUP_ROOT:-}${RNX_BOOTSTRAP_MEMINFO:-}${RNX_BOOTSTRAP_MEMBERSHIP:-}" ]; then
	# Only the bootstrap controls use fixture sources; the controller refuses
	# a handoff that says so.
	[ -n "${RNX_BOOTSTRAP_FIXTURE:-}" ] || fail "fixture memory sources are refused outside the bootstrap controls"
	fixture=true
fi
available=$(awk '/^MemAvailable:/ {print $2}' "$meminfo") || fail "cannot read $meminfo"
number "MemAvailable" "$available"
usable=$((available * 1024))
group=$(sed -n 's/^0:://p' "${RNX_BOOTSTRAP_MEMBERSHIP:-/proc/self/cgroup}") || fail "cannot read the control group membership"
[ -n "$group" ] || fail "no unified control group"
while [ -n "$group" ] && [ "$group" != "/" ]; do
	directory="$cgroup_root$group"
	current=$(cat "$directory/memory.current") || fail "cannot read $directory/memory.current"
	number "$directory/memory.current" "$current"
	for limit in memory.high memory.max; do
		value=$(cat "$directory/$limit") || fail "cannot read $directory/$limit"
		if [ "$value" != max ]; then
			number "$directory/$limit" "$value"
			headroom=$((value - current))
			[ "$headroom" -ge 0 ] || headroom=0
			[ "$headroom" -ge "$usable" ] || usable=$headroom
		fi
	done
	group=${group%/*}
done
[ "$usable" -ge $((RESERVE_BYTES + JOB_MEMORY_BYTES)) ] || fail "memory: $usable bytes usable, $((RESERVE_BYTES + JOB_MEMORY_BYTES)) needed for one build job"

# One job, and no jobserver inherited from whatever started this entry.
unset MAKEFLAGS CARGO_MAKEFLAGS MFLAGS
# Claim a new target atomically. A previous version may have used this
# pathname without claiming it. Only the agreed migration may remove it.
parent=$root/target
mkdir -p -- "$parent"
[ "$(realpath -e -- "$parent")" = "$parent" ] || fail "noncanonical bootstrap parent"
exec 8<"$parent"
flock -x 8 || fail "cannot lock bootstrap parent"
owner_id() {
	stat -c '%d:%i' -- "$root" "$1"
}
if [ ! -e "$target" ] && [ ! -L "$target" ]; then
	pending=$(mktemp -d -- "$parent/.bootstrap-owner-XXXXXXXX")
	trap 'rm -f -- "$pending/OWNER"; rmdir -- "$pending"' EXIT
	owner_id "$pending" > "$pending/OWNER"
	mv -nT -- "$pending" "$target"
	[ ! -d "$pending" ] || fail "bootstrap target appeared during claim"
	trap - EXIT
fi
[ -d "$target" ] && [ ! -L "$target" ] && [ -f "$target/OWNER" ] && [ ! -L "$target/OWNER" ] || fail "unowned bootstrap target"
owner_id "$target" | cmp -s - "$target/OWNER" || fail "bootstrap owner changed"
exec 9<"$target"
flock -nx 9 || fail "bootstrap target in use"
exec 8<&-
CARGO_INCREMENTAL=0 CARGO_BUILD_BUILD_DIR="$target" CARGO_BUILD_JOBS=1 cargo +1.98.1 build --locked --release -p rnx-final --message-format=json-render-diagnostics --target-dir "$target" > "$target/bootstrap-build.jsonl" || fail "the controller did not build"
artifact="$target/release/rnx-final"
artifact_sha=$(sha256sum "$artifact") || fail "cannot hash the controller"
artifact_sha=${artifact_sha%% *}
digest "the controller digest" "$artifact_sha"
built=$(cut -d' ' -f1 /proc/uptime) || fail "cannot read /proc/uptime"

handoff="$target/handoff"
mkdir -p "$handoff"
record="$handoff/$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n').json"
cat > "$record.pending" <<EOF
{"schema": 1, "started": $started, "built": $built, "candidate": "$candidate", "clean": true,
 "cargo_lock_sha256": "$lock_sha", "toolchain_sha256": "$toolchain_sha", "jobs": 1,
 "recipe": ["cargo", "+1.98.1", "build", "--locked", "--release", "-p", "rnx-final", "--target-dir", "$target"],
 "artifact": "$artifact", "artifact_sha256": "$artifact_sha", "memory_usable_bytes": $usable, "disk_free_bytes": $free,
 "fixture": $fixture}
EOF
mv "$record.pending" "$record"
exec "$artifact" --bootstrap "$record" "$@"
