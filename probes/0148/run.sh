#!/usr/bin/env bash
# Record 0148: the whole evaluation, in order. Gate 0 (provenance) runs first
# and stops everything on failure: no session, twin or HF step begins.
#
#   run.sh D2_JSON NLI_DIR HF_PYTHON SPM_MODEL OUT
#
# RNX_0148_STUB, when set, replaces every downstream step with that command
# (called with the step's name); preflight_controls.sh uses it as a sentinel.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
d2=$1 nli=$2 hfpy=$3 spm=$4 out=$5
ranges=$root/probes/0136/out/ranges-o0.tsv
labels=$root/probes/0139/frozen/labels.tsv
cd "$root"

# gate 0: provenance
python3 "$here/preflight.py" "$d2" "$nli"

step() {
	if [ -n "${RNX_0148_STUB:-}" ]; then "$RNX_0148_STUB" "$1"; return; fi
	shift; "$@"
}
runner=adapters/polars/target/release/runner0134
twin=probes/0133/twin/target/release/twin0133
mkdir -p "$out/u6" "$out/u6t"
chmod 700 "$out/u6" "$out/u6t"
step u6 env RNX_SESSION_ARGS=repl python3 probes/0133/session.py $runner probes/0148/u6_nli.rn "$out/u6/session.log" "$d2" "$ranges" "$labels" "$nli" "$out/u6"
step twin $twin u6 "$d2" "$ranges" "$labels" "$nli" "$out/u6t"
step validate python3 probes/0148/validate.py "$out/u6" "$out/u6t" "$d2" "$ranges" "$labels"
# gate 3 as frozen: replayed, its completion validated on its own (review
# round 1, R1): exit 0 or 3 with a matching completion artifact is a
# completed PASS or FAIL; anything else stops the run
export RNX_0148_RESULT="$out/hf-frozen.json"
rm -f "$RNX_0148_RESULT"
set +e
step hf-frozen bash -c "cd probes/0148 && '$hfpy' hf_full.py '$nli' '$out/u6' '$d2' '$ranges' '$labels' '$RNX_0148_RESULT'"
frozen=$?
set -e
python3 probes/0148/hf_result.py "$RNX_0148_RESULT" "$frozen"
# gate 3 as amended: every disagreement accounted for, then the model itself
step hf-tokenizers bash -c "cd probes/0148 && '$hfpy' tokenizer_diag.py '$nli' '$spm' '$out/u6' '$d2' '$ranges' '$labels'"
step hf-model bash -c "cd probes/0148 && '$hfpy' hf_model.py '$nli' '$out/u6' '$d2' '$ranges' '$labels'"
step evaluate python3 probes/0148/evaluate.py "$out/u6/u6-tickets.tsv" probes/0139/out/u4-session.tsv probes/0139/frozen "$d2"
