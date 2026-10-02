#!/usr/bin/env bash
# Record 0152, gate 0's controls (0151's, with the 43711c4 pins). The stub
# runs in rehearsal mode (RNX_0152_QUERIES=dev), so no control reads the
# holdout beyond gate 0. Each case corrupts one input on a copy (D1
# and the two models by symlink farms with one real corrupted file; the
# frozen query files in a scratch worktree of HEAD) and runs run.sh with a
# sentinel standing in for every downstream step. A case passes when run.sh
# refuses it by name and the sentinel shows no step began; the unmodified
# inputs must reach every step.
#
#   preflight_controls.sh D1 MINILM_DIR CE_DIR
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
d1=$(cd "$1" && pwd) minilm=$(cd "$2" && pwd) ce=$(cd "$3" && pwd)
work=$(mktemp -d)
trap 'git -C "$root" worktree remove --force "$work/tree" >/dev/null 2>&1; rm -rf "$work"' EXIT
git -C "$root" worktree add -q --detach "$work/tree" HEAD
cat > "$work/stub" <<STUB
#!/bin/sh
echo "\$1" >> "$work/began"
[ "\$1" = hf-retrieval ] && python3 -c 'import json, os, sys; f = os.environ["RNX_0152_QFILE"]; ids = [l.split("\\t")[0] for l in open(f).read().splitlines()[1:] if l]; json.dump({"check": "hf-retrieval", "completed": True, "result": "PASS", "passages": 3461, "queries": len(ids), "values": 3461 * len(ids), "query_ids": ids}, open(os.environ["RNX_0152_RESULT"], "w"))'
exit 0
STUB
chmod +x "$work/stub"
farm() { # farm SRC DST: a directory of symlinks to SRC's files
	mkdir -p "$2"
	(cd "$1" && find . -type f) | while read -r f; do
		mkdir -p "$2/$(dirname "$f")"
		ln -s "$1/$f" "$2/$f"
	done
}
real() { cp --remove-destination "$(readlink "$1")" "$1"; }
flip() { real "$1"; printf '\x00' | dd of="$1" bs=1 seek=100 conv=notrunc status=none; }
tree() { printf 'x' >> "$work/tree/$1"; }
ok=1
case_() { # case_ NAME EXPECT EDIT...
	local name=$1 expect=$2; shift 2
	rm -rf "$work/c" "$work/began"
	farm "$d1" "$work/c/d1"; farm "$minilm" "$work/c/minilm"; farm "$ce" "$work/c/ce"
	(cd "$work/tree" && git checkout -q -- probes)
	mkdir -p "$work/tree/probes/0152"
	cp "$here"/*.py "$here"/*.sh "$work/tree/probes/0152/"
	"$@"
	local log code began=""
	log=$(RNX_0152_QUERIES=dev RNX_0152_STUB="$work/stub" "$work/tree/probes/0152/run.sh" "$work/c/d1" "$work/c/minilm" "$work/c/ce" python3 "$work/c/out" 2>&1)
	code=$?
	[ -f "$work/began" ] && began=$(tr '\n' ' ' < "$work/began")
	if [ -z "$expect" ]; then
		if [ $code -eq 0 ] && [ "$began" = "bm25-controls u8 twin validate order hf-retrieval evaluate " ]; then
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
first=$(cd "$d1" && ls *.md | head -1)
case_ "unmodified" ""
case_ "a D1 byte changed" "D1: $first changed" flip "$work/c/d1/$first"
case_ "a D1 file removed" "D1: $first missing" rm "$work/c/d1/$first"
case_ "a MiniLM weight byte changed" "MiniLM: model.safetensors changed" flip "$work/c/minilm/model.safetensors"
case_ "a cross-encoder tokenizer byte changed" "cross-encoder: tokenizer.json changed" flip "$work/c/ce/tokenizer.json"
case_ "the development queries changed" "dev.tsv changed" tree probes/0151/dev.tsv
case_ "the holdout queries changed" "holdout.tsv changed" tree probes/0151/holdout.tsv
case_ "bm25.py changed since 43711c4" "0151/bm25.py changed since 43711c4" tree probes/0151/bm25.py
case_ "0151's evaluate.py changed since 43711c4" "0151/evaluate.py changed since 43711c4" tree probes/0151/evaluate.py
case_ "0151's validate.py changed since 43711c4" "0151/validate.py changed since 43711c4" tree probes/0151/validate.py
[ $ok -eq 1 ] && echo "all provenance controls behave" || { echo "CONTROLS FAILED"; exit 1; }
