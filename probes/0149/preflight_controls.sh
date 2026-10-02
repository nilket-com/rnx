#!/usr/bin/env bash
# Record 0149, gate 0's controls. Each case corrupts one input on a copy (D2
# and the model by symlinked copies; the repository's frozen files in a
# scratch worktree of HEAD), and runs run.sh with a sentinel standing in for
# every downstream step. A case passes when run.sh refuses it by name and the
# sentinel shows no step began; the unmodified inputs must reach every step.
#
#   preflight_controls.sh D2_JSON MODEL_DIR
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
d2=$(realpath "$1") nli=$(cd "$2" && pwd)
work=$(mktemp -d)
trap 'git -C "$root" worktree remove --force "$work/tree" >/dev/null 2>&1; rm -rf "$work"' EXIT
git -C "$root" worktree add -q --detach "$work/tree" HEAD
# the probe scripts under test, as they are in this working tree
cp "$here"/*.py "$here"/run.sh "$here"/fetch.sh "$work/tree/probes/0149/"
cat > "$work/stub" <<STUB
#!/bin/sh
echo "\$1" >> "$work/began"
# a completed PASS for the frozen HF check, so the unmodified case reaches
# every step (driver_controls.sh covers that check's other outcomes)
# a completed PASS for each check, so the unmodified case reaches every step
# (driver_controls.sh covers the checks' other outcomes)
case "\$1" in early-gate | 3a-native | 3b-model) printf '{"check": "%s", "completed": true, "result": "PASS"}' "\$1" > "\$RNX_0149_RESULT" ;; esac
exit 0
STUB
chmod +x "$work/stub"

ok=1
case_() { # case_ NAME EXPECT EDIT...
	local name=$1 expect=$2; shift 2
	rm -rf "$work/c" "$work/began"
	mkdir -p "$work/c/nli"
	cp "$d2" "$work/c/issues.json"
	for f in "$nli"/*; do ln -s "$f" "$work/c/nli/$(basename "$f")"; done
	(cd "$work/tree" && git checkout -q -- probes)
	cp "$here"/*.py "$here"/run.sh "$here"/fetch.sh "$work/tree/probes/0149/"
	"$@"
	local log code began=""
	log=$(RNX_0149_STUB="$work/stub" "$work/tree/probes/0149/run.sh" "$work/c/issues.json" "$work/c/nli" python3 "$work/c/out" 2>&1)
	code=$?
	[ -f "$work/began" ] && began=$(tr '\n' ' ' < "$work/began")
	if [ -z "$expect" ]; then
		if [ $code -eq 0 ] && [ "$began" = "gate-rnx early-gate u7 twin validate 3a-native 3b-model evaluate " ]; then
			echo "pass: $name: every step reached ($began)"
		else
			echo "WRONG: $name: exit $code, began '$began': $log"; ok=0
		fi
	elif [ $code -ne 0 ] && [ -z "$began" ] && grep -qF "$expect" <<< "$log"; then
		echo "refused: $name: $(grep -F "$expect" <<< "$log" | head -1); no step began"
	else
		echo "ACCEPTED: $name: exit $code, began '$began'"; ok=0
	fi
}
real() { cp --remove-destination "$(readlink "$1")" "$1"; }
flip() { real "$1"; printf '\x00' | dd of="$1" bs=1 seek=100 conv=notrunc status=none; }
d2edit() { python3 - "$work/c/issues.json" "$1" <<'PY'
import json, sys
p, what = sys.argv[1:3]
v = json.load(open(p))
if what == "body": v[0]["body"] += " edited"
if what == "drop": v.pop(0)
if what == "swap": v[0], v[1] = v[1], v[0]
json.dump(v, open(p, "w"))
PY
}
tree() { echo "x" >> "$work/tree/$1"; }
case_ "unmodified" ""
case_ "a D2 ticket's body changed" "D2: #" d2edit body
case_ "a D2 ticket removed" "missing" d2edit drop
case_ "two D2 tickets reordered" "D2: the order differs" d2edit swap
case_ "0139's labels changed" "probes/0139/frozen/labels.tsv changed" tree probes/0139/frozen/labels.tsv
case_ "0139's sample changed" "probes/0139/frozen/sample.tsv changed" tree probes/0139/frozen/sample.tsv
case_ "an annotation changed" "probes/0139/frozen/annotations-codex.tsv changed" tree probes/0139/frozen/annotations-codex.tsv
case_ "0139's baseline changed" "probes/0139/out/u4-session.tsv changed" tree probes/0139/out/u4-session.tsv
case_ "the byte ranges changed" "probes/0136/out/ranges-o0.tsv changed" tree probes/0136/out/ranges-o0.tsv
case_ "0148's U6 table changed" "probes/0148/out/u6-script/u6-tickets.tsv changed" tree probes/0148/out/u6-script/u6-tickets.tsv
case_ "a weight byte changed" "model: model.safetensors changed" flip "$work/c/nli/model.safetensors"
case_ "the generation config changed" "model: generation_config.json changed" flip "$work/c/nli/generation_config.json"
case_ "the tokenizer missing" "model: tokenizer.json missing" rm "$work/c/nli/tokenizer.json"
[ $ok -eq 1 ] && echo "all provenance controls behave" || { echo "CONTROLS FAILED"; exit 1; }
