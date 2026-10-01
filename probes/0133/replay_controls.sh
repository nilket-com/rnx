#!/bin/sh
# Record 0133: replay.sh's gates are fatal. With a missing or wrong model,
# replay must exit non-zero before running any session or twin; a stand-in
# executable records whether anything downstream was invoked.
set -u
here=$(cd "$(dirname "$0")" && pwd)
: "${RNX_UAT_DATA:?set RNX_UAT_DATA}"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail=0
printf '#!/bin/sh\ntouch "%s/invoked"\nexit 0\n' "$work" > "$work/standin"
chmod +x "$work/standin"
control() { # name, model directory
	rm -f "$work/invoked"
	TWIN="$work/standin" "$here/replay.sh" "$work/standin" "$2" "$work/out" >/dev/null 2>&1
	status=$?
	if [ "$status" -ne 0 ] && [ ! -e "$work/invoked" ]; then
		echo "ok: $1 stops the replay before any session or twin"
	else
		echo "FAIL: $1 (status $status, downstream invoked: $([ -e "$work/invoked" ] && echo yes || echo no))"
		fail=1
	fi
}
mkdir -p "$work/empty"
control "an empty model directory" "$work/empty"
control "a missing model directory" "$work/nowhere"
mkdir -p "$work/wrong/1_Pooling"
for f in config.json tokenizer.json model.safetensors modules.json 1_Pooling/config.json sentence_bert_config.json; do echo wrong > "$work/wrong/$f"; done
control "a model with wrong files" "$work/wrong"
exit $fail
