#!/bin/bash
# Sample bootstrap and release with the same device grouping as the controller.
set -euo pipefail
mode=$1 run=$2 out=$3
state=$run/space.json
declare -A seen=()
rows=()
for role in build receipts; do
	case $role in build) path=$run ;; receipts) path=$out ;; esac
	device=$(stat -c %d -- "$path")
	[[ ! -v 'seen[$device]' ]] || continue
	seen[$device]=1
	read -r blocks blocksize < <(stat -f -c '%a %S' -- "$path")
	free=$((blocks * blocksize))
	[[ $mode == release ]] || (( free >= 21474836480 )) || { echo "low free space at $path" >&2; exit 1; }
	rows+=("$role" "$device" "$free")
done
group=$(sed -n 's/^0:://p' /proc/self/cgroup)
shmem=$(awk '$1=="shmem"{print $2}' "/sys/fs/cgroup$group/memory.stat")
[[ $shmem =~ ^[0-9]+$ ]]
if [[ $mode == start ]]; then
	jq -n --argjson shmem "$shmem" --args '$ARGS.positional as $a | reduce range(0; $a|length; 3) as $i
		({}; .[$a[$i]]={device:$a[$i+1],start:($a[$i+2]|tonumber),peak:0}) |
		.memory={start:$shmem,peak:0}' "${rows[@]}" >"$state"
fi
jq --argjson shmem "$shmem" --args '$ARGS.positional as $a | reduce range(0; $a|length; 3) as $i (.;
	.[$a[$i]] |= (if .device != $a[$i+1] then error("filesystem changed") else . end |
	.peak=([.peak,.start-($a[$i+2]|tonumber)] | max))) | .memory.peak=([.memory.peak,$shmem-.memory.start]|max)' "${rows[@]}" <"$state" >"$state.new"
mv "$state.new" "$state"
if [[ $mode == start && $(cat "$run/limits") != null ]]; then
	read -r available < <(awk '$1=="MemAvailable:"{printf "%.0f\n",1024*$2}' /proc/meminfo)
	while [[ -n $group ]]; do
		dir=/sys/fs/cgroup$group
		current=$(cat "$dir/memory.current")
		for bound in memory.max memory.high; do
			limit=$(cat "$dir/$bound")
			if [[ $limit != max && $((limit-current)) -lt $available ]]; then available=$((limit-current)); fi
		done
		group=${group%/*}
	done
	jq -en --argjson available "$available" --slurpfile sample "$state" --slurpfile limits "$run/limits" '
		($sample[0]|keys)==($limits[0].bytes|keys) and
		all($sample[0]|to_entries[]; (if .key=="memory" then $available else .value.start end) >= $limits[0].bytes[.key])' >/dev/null
fi
