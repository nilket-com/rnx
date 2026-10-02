#!/usr/bin/env bash
# Record 0149: the driver's handling of every expected-outcome check. A
# sentinel stands in for every downstream step; for the three checks it
# writes an artifact and exits per scenario. The steps that began, and the
# run's exit, must match each scenario's expectation.
#
#   driver_controls.sh D2_JSON MODEL_DIR HF_PYTHON
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
d2=$(realpath "$1") model=$(cd "$2" && pwd) hfpy=$3
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cat > "$work/stub" <<'STUB'
#!/usr/bin/env bash
echo "$1" >> "$WORK/began"
art() { printf '%s' "$1" > "$RNX_0149_RESULT"; }
ok() { art "{\"check\": \"$1\", \"completed\": true, \"result\": \"PASS\"}"; exit 0; }
case "$1" in
	early-gate) case "$SCENARIO" in
		early-fail) art '{"check": "early-gate", "completed": true, "result": "FAIL"}'; exit 3 ;;
		*) ok early-gate ;; esac ;;
	3a-native) case "$SCENARIO" in
		3a-fail-explained) art '{"check": "3a-native", "completed": true, "result": "FAIL", "counts": {"identical": 290, "i": 1, "ii": 1, "iii": 0}}'; exit 3 ;;
		3a-fail-unexplained) art '{"check": "3a-native", "completed": true, "result": "FAIL", "counts": {"identical": 291, "i": 0, "ii": 0, "iii": 1}}'; exit 3 ;;
		3a-traceback) echo "Traceback (most recent call last):" >&2; exit 1 ;;
		3a-no-counts) art '{"check": "3a-native", "completed": true, "result": "FAIL"}'; exit 3 ;;
		3a-wrong-name) art '{"check": "3b-model", "completed": true, "result": "PASS"}'; exit 0 ;;
		*) ok 3a-native ;; esac ;;
	3b-model) case "$SCENARIO" in
		3b-fail) art '{"check": "3b-model", "completed": true, "result": "FAIL"}'; exit 3 ;;
		3b-mismatch) art '{"check": "3b-model", "completed": true, "result": "FAIL"}'; exit 0 ;;
		*) ok 3b-model ;; esac ;;
esac
exit 0
STUB
chmod +x "$work/stub"
all="gate-rnx early-gate u7 twin validate 3a-native 3b-model evaluate "
ok=1
expect() { # expect SCENARIO EXIT(0|nonzero) BEGAN
	local sc=$1 want=$2 began_want=$3
	rm -rf "$work/out" "$work/began"
	log=$(WORK="$work" SCENARIO=$sc RNX_0149_STUB="$work/stub" "$here/run.sh" "$d2" "$model" "$hfpy" "$work/out" 2>&1)
	local code=$? began
	began=$(tr '\n' ' ' < "$work/began")
	local stop=$(grep -o 'STOP: .*' <<< "$log" | head -1)
	if { [ "$want" = 0 ] && [ $code -eq 0 ]; } || { [ "$want" != 0 ] && [ $code -ne 0 ]; }; then
		if [ "$began" = "$began_want" ]; then
			if [ -n "$stop" ]; then echo "stops: $sc: $stop"; else echo "continues: $sc: every later step began"; fi
			return
		fi
	fi
	echo "WRONG: $sc: exit $code, began '$began', want '$began_want'; $stop"; ok=0
}
expect all-pass 0 "$all"
expect 3a-fail-explained 0 "$all"
expect early-fail 1 "gate-rnx early-gate "
expect 3a-fail-unexplained 1 "gate-rnx early-gate u7 twin validate 3a-native "
expect 3a-traceback 1 "gate-rnx early-gate u7 twin validate 3a-native "
expect 3a-no-counts 1 "gate-rnx early-gate u7 twin validate 3a-native "
expect 3a-wrong-name 1 "gate-rnx early-gate u7 twin validate 3a-native "
expect 3b-fail 1 "gate-rnx early-gate u7 twin validate 3a-native 3b-model "
expect 3b-mismatch 1 "gate-rnx early-gate u7 twin validate 3a-native 3b-model "
[ $ok -eq 1 ] && echo "all driver controls behave" || { echo "CONTROLS FAILED"; exit 1; }
