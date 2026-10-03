#!/usr/bin/env bash
# Record 0154, gate 0's controls: 0153's freeze controls (a sample) plus
# 0154's policy pins (plans/0154 section 3). Each case corrupts
# one input on a copy (frozen files in a scratch worktree of HEAD; D3, D2 and
# the models by symlink farms with one real corrupted file) and runs run.sh
# (mode d2) with a sentinel standing in for every downstream step. A case
# passes when run.sh refuses it by name and no step began; the unmodified
# inputs must reach every step.
#
#   preflight_controls.sh D3_DIR D2_JSON MINILM_DIR NLI_DIR HF_PYTHON
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
d3=$(cd "$1" && pwd) d2=$(realpath "$2") minilm=$(cd "$3" && pwd) nli=$(cd "$4" && pwd) hfpy=$5
work=$(mktemp -d)
trap 'git -C "$root" worktree remove --force "$work/tree" >/dev/null 2>&1; rm -rf "$work"' EXIT
git -C "$root" worktree add -q --detach "$work/tree" HEAD
cat > "$work/stub" <<STUB
#!/bin/sh
echo "\$1" >> "$work/began"
[ "\$1" = hf-t3 ] && printf '{"check": "hf-t3", "completed": true, "result": "PASS"}' > "\$RNX_0154_RESULT"
exit 0
STUB
chmod +x "$work/stub"
farm() { # farm SRC DST: a tree of symlinks to SRC's files
	mkdir -p "$2"
	(cd "$1" && find . -type f -not -path './pages/*') | while read -r f; do
		mkdir -p "$2/$(dirname "$f")"
		ln -s "$1/$f" "$2/$f"
	done
}
real() { cp --remove-destination "$(readlink "$1")" "$1"; }
flip() { real "$1"; printf '\x00' | dd of="$1" bs=1 seek=100 conv=notrunc status=none; }
frozen() { python3 - "$work/tree/probes/0153/frozen/$1" "$2" <<'PY'
import sys
p, how = sys.argv[1:3]
lines = open(p).read().split("\n")
if how == "append": open(p, "a").write("x")
elif how == "drop": del lines[2]
elif how == "dup": lines.insert(2, lines[1])
elif how == "swap": lines[1], lines[2] = lines[2], lines[1]
elif how == "blank": lines[2] = ""
elif how == "oov": lines[2] = lines[2].split("\t")[0] + "\tbugs"
if how != "append": open(p, "w").write("\n".join(lines))
PY
}
issue() { real "$work/c/d3/issues.json"; python3 - "$work/c/d3/issues.json" "$1" <<'PY'
import json, sys
p, how = sys.argv[1:3]
v = json.load(open(p))
if how == "text": v[3]["body"] += " drift"
elif how == "labels": v[3]["labels"] = sorted(v[3]["labels"] + ["question"])
elif how == "state": v[3]["state"] = "open" if v[3]["state"] == "closed" else "closed"
json.dump(v, open(p, "w"))
PY
}
ok=1
case_() {
	local name=$1 expect=$2; shift 2
	rm -rf "$work/c" "$work/began"
	farm "$d3" "$work/c/d3"; farm "$minilm" "$work/c/minilm"; farm "$nli" "$work/c/nli"
	mkdir -p "$work/c/d2"; ln -s "$d2" "$work/c/d2/issues.json"
	(cd "$work/tree" && git checkout -q -- . && git clean -qfd probes/0154)
	mkdir -p "$work/tree/probes/0154"
	cp "$here"/*.py "$here"/*.sh "$work/tree/probes/0154/"
	"$@"
	local log code began=""
	log=$(RNX_0154_STUB="$work/stub" "$work/tree/probes/0154/run.sh" "$work/c/d3" "$work/c/d2/issues.json" "$work/c/minilm" "$work/c/nli" "$hfpy" "$work/c/out" 2>&1)
	code=$?
	[ -f "$work/began" ] && began=$(tr '\n' ' ' < "$work/began")
	if [ -z "$expect" ]; then
		if [ $code -eq 0 ] && [ "$began" = "fixtures-0153 fixtures-0154 handler twin-controls session twin validate hf-t3 evaluate " ]; then
			echo "pass: $name: every step reached ($began)"
		else
			echo "WRONG: $name: exit $code, began '$began': $log"; ok=0
		fi
	elif [ $code -ne 0 ] && [ -z "$began" ] && grep -qF "$expect" <<< "$log"; then
		echo "refused: $name: $(grep -F "$expect" <<< "$log" | head -1); no step began"
	else
		echo "ACCEPTED: $name: exit $code, began '$began': $(tail -2 <<< "$log")"; ok=0
	fi
}
case_ "unmodified" ""
case_ "the design file changed" "the design file is not the accepted design" bash -c "printf x >> '$work/tree/plans/0153_triage_routing.md'"
case_ "a holdout annotation row missing" "annotations-codex-hold.tsv" frozen annotations-codex-hold.tsv drop
case_ "D3 text drifted" "D3 drifted" issue text
case_ "a MiniLM weight byte changed" "MiniLM: model.safetensors changed" flip "$work/c/minilm/model.safetensors"
case_ "the frozen centroids changed" "0153/out/dev/centroids.tsv changed" bash -c "printf x >> '$work/tree/probes/0153/out/dev/centroids.tsv'"
case_ "selected.json changed" "0153/out/dev/selected.json changed" bash -c "printf ' ' >> '$work/tree/probes/0153/out/dev/selected.json'"
case_ "0153's evaluate.py changed" "0153/evaluate.py changed" bash -c "printf '#' >> '$work/tree/probes/0153/evaluate.py'"
case_ "0153's validate.py changed" "0153/validate.py changed" bash -c "printf '#' >> '$work/tree/probes/0153/validate.py'"
[ $ok -eq 1 ] && echo "all provenance and freeze controls behave" || { echo "CONTROLS FAILED"; exit 1; }
