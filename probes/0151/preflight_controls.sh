#!/usr/bin/env bash
# Record 0151, gate 0's controls. Each case corrupts one input on a copy (D1
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
[ "\$1" = hf-pool ] && printf '{"check": "hf-pool", "completed": true, "result": "PASS"}' > "\$RNX_0151_RESULT"
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
	cp "$here"/*.py "$here"/*.sh "$here"/*.rn "$work/tree/probes/0151/"
	"$@"
	local log code began=""
	log=$(RNX_0151_STUB="$work/stub" "$work/tree/probes/0151/run.sh" "$work/c/d1" "$work/c/minilm" "$work/c/ce" python3 "$work/c/out" 2>&1)
	code=$?
	[ -f "$work/began" ] && began=$(tr '\n' ' ' < "$work/began")
	if [ -z "$expect" ]; then
		if [ $code -eq 0 ] && [ "$began" = "bm25-controls u8 twin validate order hf-pool evaluate " ]; then
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
[ $ok -eq 1 ] && echo "all provenance controls behave" || { echo "CONTROLS FAILED"; exit 1; }
