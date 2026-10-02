#!/usr/bin/env bash
# Record 0148 (review round 1, R1): the driver's handling of the frozen HF
# check. A sentinel stands in for every downstream step; for hf-frozen it
# behaves per scenario. A completed PASS or FAIL (exit 0 or 3 with a
# matching completion artifact) must continue with its true label; a silent
# exit 1, a traceback exit 1, a missing or malformed artifact, or an
# artifact disagreeing with the status must stop before any later step.
#
#   driver_controls.sh D2_JSON NLI_DIR
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
d2=$(realpath "$1") nli=$(cd "$2" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cat > "$work/stub" <<'STUB'
#!/usr/bin/env bash
echo "$1" >> "$WORK/began"
[ "$1" = hf-frozen ] || exit 0
art() { printf '%s' "$1" > "$RNX_0148_RESULT"; }
case "$SCENARIO" in
	pass) art '{"check": "hf-frozen", "completed": true, "result": "PASS", "pairs": 2290}'; exit 0 ;;
	fail) art '{"check": "hf-frozen", "completed": true, "result": "FAIL", "pairs": 2290}'; exit 3 ;;
	silent1) exit 1 ;;
	traceback1) printf 'Traceback (most recent call last):\n  File "hf_full.py", line 1\nImportError: no module\n' >&2; exit 1 ;;
	missing3) exit 3 ;;
	malformed3) art '{"check": "hf-frozen", "complet'; exit 3 ;;
	incomplete3) art '{"check": "hf-frozen", "completed": false, "result": "FAIL", "pairs": 2290}'; exit 3 ;;
	mismatch0) art '{"check": "hf-frozen", "completed": true, "result": "FAIL", "pairs": 2290}'; exit 0 ;;
	crash2) exit 2 ;;
esac
STUB
chmod +x "$work/stub"
ok=1
for sc in pass fail silent1 traceback1 missing3 malformed3 incomplete3 mismatch0 crash2; do
	rm -rf "$work/out" "$work/began"
	log=$(WORK="$work" SCENARIO=$sc RNX_0148_STUB="$work/stub" "$here/run.sh" "$d2" "$nli" python3 /dev/null "$work/out" 2>&1)
	code=$?
	began=$(tr '\n' ' ' < "$work/began")
	label=$(grep -o 'gate 3 as frozen.*' <<< "$log" | head -1)
	stop=$(grep -o 'STOP: .*' <<< "$log" | head -1)
	case $sc in
		pass | fail)
			want=$([ $sc = pass ] && echo PASS || echo FAIL)
			if [ $code -eq 0 ] && [ "$began" = "u6 twin validate hf-frozen hf-tokenizers hf-model evaluate " ] && grep -q "completed, $want" <<< "$label"; then
				echo "continues: $sc: $label; every later step began"
			else
				echo "WRONG: $sc: exit $code, began '$began', '$label'"; ok=0
			fi ;;
		*)
			if [ $code -ne 0 ] && [ "$began" = "u6 twin validate hf-frozen " ] && [ -n "$stop" ]; then
				echo "stops: $sc: $stop; no later step began"
			else
				echo "WRONG: $sc: exit $code, began '$began', '$stop'"; ok=0
			fi ;;
	esac
done
[ $ok -eq 1 ] && echo "all driver controls behave" || { echo "CONTROLS FAILED"; exit 1; }
