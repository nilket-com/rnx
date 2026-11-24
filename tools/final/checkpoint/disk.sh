# Source-only disk admission: preparing the published revision must never
# compile the candidate, even to query its catalogue. disk-plan.json is the
# tested output of the candidate controller's checkpoint-disk-plan command.
disk_routes() {
	jq -cs --slurpfile spec "$1" '
		(map({key, value:[.value.lane,.value.cold_bytes]})|from_entries)
		| delpaths($spec[0].removed|map([.])) + $spec[0].added
		| to_entries | group_by(.value[0]) | map({
			lane:.[0].value[0], cold:.[0].value[1], keys:map(.key)
		})' "$mechanism/ee52b405.jsonl"
}

# Same headroom observation as job_budget::memory::final_jobs. Do not use
# the bootstrap's raw memory.current: it excludes reclaimable page cache.
disk_tokens() {
	local group dir current credit head limit key reserve tokens cpus
	local available
	available=$(awk '/^MemAvailable:/ && $3=="kB" {print $2; found=1} END {exit !found}' /proc/meminfo)
	available=$((available * 1024))
	group=$(sed -n 's/^0:://p' /proc/self/cgroup)
	[[ $group == /* && $group != */../* ]]
	while [[ -n $group && $group != / ]]; do
		dir=/sys/fs/cgroup$group
		current=$(cat "$dir/memory.current")
		credit=$(awk '{v[$1]=$2} END {
			n=split("file shmem inactive_file file_dirty file_writeback file_mapped", keys, " ")
			for (i=1;i<=n;i++) if (!(keys[i] in v)) exit 1
			f=v["file"]-v["shmem"]; if (f>v["inactive_file"]) f=v["inactive_file"]
			f-=v["file_dirty"]+v["file_writeback"]+v["file_mapped"]
			printf "%.0f\n", (f>0 ? f : 0)
		}' "$dir/memory.stat")
		(( credit <= current )) || credit=$current
		for key in memory.high memory.max; do
			limit=$(cat "$dir/$key")
			[[ $limit != max ]] || continue
			head=$((limit - current + credit))
			(( available <= head )) || available=$head
		done
		group=${group%/*}
	done
	reserve=$((available / 4))
	(( reserve >= 2147483648 )) || reserve=2147483648
	tokens=$(((available - reserve) / 3221225472))
	(( tokens >= 1 )) || { echo 'insufficient memory for a final job' >&2; return 1; }
	cpus=$(awk '/^Cpus_allowed_list:/ {
		n=split($2,list,","); for (i=1;i<=n;i++) {
			last=split(list[i],span,"-"); count+=span[last]-span[1]+1
		}
		print count; found=1
	} END {exit !found}' /proc/self/status)
	(( tokens <= cpus )) || tokens=$cpus
	echo "$tokens"
}

disk_apparent() {
	# scheduler::tree_bytes counts every regular pathname, including hard
	# links, and omits the two cache metadata files.
	find -P "$1" \( -name .rnx-cache-owner.json -o -name .rnx-marks.json \) -prune -o \
		-type f -printf '%s\n' | awk '{s+=$1} END {printf "%.0f\n", s}'
}

disk_lane_growth() {
	local dir=$1 cold=$2 keys=$3 warm=$4 share=$5 size bytes verdict
	if [[ -e $dir/.rnx-marks.json && $keys != null ]]; then
		verdict=$(jq -er --argjson keys "$keys" '.recipes as $r |
			if ($r|type)!="object" then error("invalid marks") else
			([$keys[] | ($r[.] // {}) | if type=="object" then length>0 else error("invalid recipe marks") end] | all | tostring) end' "$dir/.rnx-marks.json")
	else
		verdict=false
	fi
	if [[ $keys != null && $verdict == false ]]; then
		bytes=0
		if [[ -d $dir ]]; then bytes=$(du -sB1 -- "$dir" | cut -f1); fi
		size=$((cold - bytes))
	else
		bytes=0
		if [[ -d $dir ]]; then bytes=$(disk_apparent "$dir"); fi
		if (( bytes == 0 )); then echo "$cold"; return; fi
		size=$(awk -v bytes="$bytes" -v share="$share" 'BEGIN {printf "%.0f\n", int(bytes*share)}')
	fi
	(( size >= warm )) || size=$warm
	echo "$size"
}

disk_scratch() {
	local dir=$1 path marker
	for path in "$dir" /var/tmp; do
		dir=$path
		while :; do
			for marker in .git Cargo.toml .cargo rust-toolchain rust-toolchain.toml; do
				if [[ -e $dir/$marker || -L $dir/$marker ]]; then continue 3; fi
			done
			[[ $dir != / ]] || break
			dir=$(dirname -- "$dir")
		done
		[[ $(realpath -e "$path") == "$path" ]]
		echo "$path"
		return
	done
	echo 'no neutral stage scratch parent' >&2
	return 1
}

disk_free() {
	df -B1 --output=avail -- "$1" | tail -1 | tr -d ' '
}

# Charge the same reserve once per filesystem. Emit the binding and both
# readings on refusal; callers persist the full decision before returning.
disk_sites() {
	local root=$1 out=$2 checkout=$3 output=$4 scratch=$5 floor=$6
	local parent path dev free needed i
	parent=$(disk_scratch "$out")
	local -a paths=("$root" "$out" "$parent") amounts=("$checkout" "$output" "$scratch")
	local -A seen=() totals=()
	for i in 0 1 2; do
		dev=$(stat -c %d -- "${paths[i]}")
		totals[$dev]=$((${totals[$dev]:-0} + amounts[i]))
	done
	for path in "${paths[@]}"; do
		dev=$(stat -c %d -- "$path")
		[[ ! -v seen[$dev] ]] || continue
		seen[$dev]=1
		free=$(disk_free "$path"); needed=$((floor + totals[$dev]))
		jq -nc --arg path "$path" --argjson free "$free" --argjson needed "$needed" \
			'{path:$path,free_bytes:$free,needed_bytes:$needed,pass:($free >= $needed)}'
	done
}

checkpoint_disk() (
	set -euo pipefail
	shopt -s inherit_errexit
	export LC_ALL=C
	local phase=$1 spec=$mechanism/disk-plan.json warm share floor temp stages cold
	local a b c routes lane keys size add=0 prep=0 tokens extra
	local baseline_spec=$root/tools/final/checkpoint/disk-plan.json removed='[]'
	read -r a b c floor cold warm share temp stages < <(jq -er '[.bulk[],.floor,.cold,.warm,.share,.temp,.stages]|@tsv' "$spec")
	routes=$(disk_routes "$spec")
	tokens=$(disk_tokens)
	(( tokens <= stages )) || tokens=$stages
	if [[ $phase == before-preparation ]]; then
		# The cold high-water mark (all classes, including anchor scratch)
		# and first-stage lane growth come from FINAL-v1. Round the observed
		# high water up to a GiB. Charge changed lanes once at their declared
		# cold size beyond this calibration, on each side independently.
		prep=$(( ((112738422784 + 1073741823) / 1073741824) * 1073741824 ))
		# Isolate the lane term from the failed first stage: the live
		# receipt counted one floor, 47+2+8 GiB bulk and 13 temp slots.
		add=$((124669975202 - 21474836480 - 57 * 1073741824 - 13 * 134217728))
		if [[ $baseline != ee52b405e9cea581c514ec87463faaa34d803b13 ]]; then
			removed=$(jq -ec .removed "$baseline_spec")
			if [[ $mode == promoted ]]; then
				# The verified generation retains its lanes across preparation.
				# Cap the credit at the measured footprint of those lanes,
				# leaving the bootstrap, dependency home and anchor growth.
				local kept
				kept=$(du -sxB1 -- "$lanes" | cut -f1)
				(( kept <= 110403301376 )) || kept=110403301376
				prep=$((prep - kept))
			fi
			extra=$(jq -er '[.added[]] | group_by(.[0]) | map(map(.[1])|max) | add // 0' "$baseline_spec")
			prep=$((prep + extra))
		fi
		# A candidate can also reintroduce an adoption recipe its
		# published parent had removed. Its lane is cold at admission.
		extra=$(jq -ner --argjson routes "$routes" --argjson removed "$removed" --slurpfile spec "$spec" '
			(($spec[0].added|keys) + $removed) as $changed |
			[$routes[] | select(any(.keys[]; . as $k | $changed|index($k)!=null)) | .cold] | add // 0')
		add=$((add + extra))
	else
		while IFS=$'\t' read -r lane size keys; do
			[[ -n $lane ]] || continue
			size=$(disk_lane_growth "$lanes/$lane" "$size" "$keys" "$warm" "$share")
			add=$((add + size))
		done <<<"$(jq -r '.[]|[.lane,.cold,(.keys|tojson)]|@tsv' <<<"$routes")"
	fi
	local raw
	while IFS= read -r -d '' raw; do
		[[ $raw != /* && $raw != *..* ]] || { echo 'invalid raw lane path' >&2; return 1; }
		size=$cold
		if [[ $phase == before-clock ]]; then size=$(disk_lane_growth "$root/$raw" "$cold" null "$warm" "$share"); fi
		add=$((add + size))
	done < <(jq -j '.other[]|., "\u0000"' "$spec")
	local decisions
	decisions=$(disk_sites "$root" "$out" "$((a + prep + add))" "$b" "$((c + tokens * temp))" "$floor")
	jq -sn --arg phase "$phase" --argjson prep "$prep" --argjson lanes "$add" --argjson tokens "$tokens" \
		--argjson sites "$(jq -s . <<<"$decisions")" \
		'{phase:$phase,preparation_growth_bytes:$prep,lane_growth_bytes:$lanes,tokens:$tokens,sites:$sites}' \
		| publish_file "$proof/$phase.json"
	jq -e 'all(.sites[];.pass)' "$proof/$phase.json" >/dev/null || {
		jq -r '.sites[]|select(.pass|not)|"disk admission at \(.path): \(.free_bytes) bytes free, \(.needed_bytes) needed"' "$proof/$phase.json" >&2
		return 1
	}
)

# The positive and negative paths share the clock boundary. Refusal is
# published by the same EXIT handler that owns preparation and its lease.
prepare_and_admit() {
	outcome=before-preparation
	checkpoint_disk "$outcome"
	outcome=preparation
	prepare_checkpoint
	outcome=before-clock
	checkpoint_disk "$outcome"
	admitted=$(uptime_seconds)
}
