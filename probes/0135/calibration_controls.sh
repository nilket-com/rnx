#!/bin/sh
# Record 0135: the calibration gates fail closed.
#
#   calibration_controls.sh PROBE MODEL PASSAGES
#
# MODEL is the pinned model; MODEL384 must exist beside it (the same weights,
# max_seq_length 384), so one asked shape is ruled out by the caps.
# 1. calibrate passes on asked shapes, skipping by name what the caps rule out;
# 2. calibrate fails when the S0 pin is replaced (a split batch), when the
#    estimate is scaled below the peak, and when the texts are too short to
#    reach the full length (MODEL64, max_seq_length 64, must exist too);
# 3. configs fails with a configuration the loader refuses (an unexpected
#    construction error), and when the estimate is scaled below the peak.
set -u
probe=$1; model=$2; passages=$3
fails=0
expect() {
	want=$1; label=$2; shift 2
	"$@" > /tmp/calibration-control.$$ 2>&1
	code=$?
	if { [ "$want" = pass ] && [ $code -eq 0 ]; } || { [ "$want" = fail ] && [ $code -ne 0 ]; }; then
		echo "ok: $label ($want, exit $code): $(grep '^#' /tmp/calibration-control.$$ | tail -1)"
	else
		echo "WRONG: $label (wanted $want, exit $code)"
		fails=$((fails + 1))
	fi
}
expect pass "calibrate, 256 tokens" "$probe" calibrate "$model" "$passages" 1 4 32
expect pass "calibrate, 384 tokens with 32 ruled out by the caps" "$probe" calibrate "${model}384" "$passages" 4 32
expect fail "calibrate with the S0 pin replaced (C = 32)" "$probe" calibrate "$model" "$passages" 4 32 --concurrency 32
expect fail "calibrate with the estimate scaled to 0.5" "$probe" calibrate "$model" "$passages" 4 --scale 0.5
# texts too short to reach the full length: a shorter sequence must not pass
short=/tmp/calibration-short.$$.json
python3 -c 'import json; print(json.dumps(["a"] * 40))' > "$short"
expect fail "calibrate on texts shorter than the full length" "$probe" calibrate "${model}64" "$short" 4
rm -f "$short"
expect fail "configs with a refused configuration" "$probe" configs --bad
expect fail "configs with the estimate scaled to 0.5" "$probe" configs --scale 0.5
rm -f /tmp/calibration-control.$$
[ $fails -eq 0 ] && echo "calibration controls: all passed" || { echo "calibration controls: $fails WRONG"; exit 1; }
