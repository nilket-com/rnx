#!/usr/bin/env bash
# Record 0149: the whole evaluation, in order. Gate 0 (provenance) runs first
# and stops everything on failure. Every expected-outcome check writes a
# completion artifact, validated by hf_result.py before the run continues.
#
#   run.sh D2_JSON MODEL_DIR HF_PYTHON OUT
#
# RNX_0149_STUB, when set, replaces every downstream step with that command
# (called with the step's name and, for the checks, writing the artifact
# named by RNX_0149_RESULT); the controls use it as a sentinel.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
d2=$1 model=$2 hfpy=$3 out=$4
ranges=$root/probes/0136/out/ranges-o0.tsv
labels=$root/probes/0139/frozen/labels.tsv
sample=$root/probes/0139/frozen/sample.tsv
cd "$root"

# gate 0: provenance
python3 "$here/preflight.py" "$d2" "$model"

step() {
	if [ -n "${RNX_0149_STUB:-}" ]; then "$RNX_0149_STUB" "$1"; return; fi
	shift; "$@"
}
checked() { # checked NAME ARTIFACT MODE COMMAND...: run a check, validate its completion
	local name=$1 art=$2 mode=$3; shift 3
	export RNX_0149_RESULT=$art
	rm -f "$art"
	set +e
	step "$name" "$@"
	local status=$?
	set -e
	"$hfpy" probes/0149/hf_result.py "$name" "$art" "$status" $mode
}
runner=adapters/polars/target/release/runner0134
twin=probes/0133/twin/target/release/twin0133
mkdir -p "$out/u7" "$out/u7t" "$out/gate"
chmod 700 "$out/u7" "$out/u7t" "$out/gate"
# gate 1: the early stop gate, before any D2 generation
step gate-rnx env RNX_GEN_MODEL="$model" RNX_GEN_GATE_OUT="$out/gate/gate.json" \
	cargo test -q --release --manifest-path adapters/candle/Cargo.toml --test generate_model the_early_gate -- --ignored
checked early-gate "$out/gate/early-gate.json" --require-pass \
	bash -c "cd probes/0149 && '$hfpy' hf_gate.py '$model' '$out/gate/gate.json' '$out/gate/early-gate.json' gate_chats.json"
# U7 and its twin
step u7 env RNX_SESSION_ARGS=repl python3 probes/0133/session.py $runner probes/0149/u7_generate.rn "$out/u7/session.log" "$d2" "$ranges" "$labels" "$sample" "$model" "$out/u7"
step twin $twin u7 "$d2" "$ranges" "$labels" "$sample" "$model" "$out/u7t"
step validate "$hfpy" probes/0149/validate.py "$out/u7" "$out/u7t" "$d2" "$ranges" "$labels" "$sample" "$model/tokenizer.json"
# 3a as frozen (kept as it is; an unexplained difference stops), then 3b
checked 3a-native "$out/3a.json" --no-unexplained \
	bash -c "cd probes/0149 && '$hfpy' hf_native.py '$model' '$out/u7' '$d2' '$ranges' '$labels' '$sample' '$out/3a.json'"
checked 3b-model "$out/3b.json" --require-pass \
	bash -c "cd probes/0149 && '$hfpy' hf_model.py '$model' '$out/u7' '$d2' '$ranges' '$labels' '$sample' '$out/3b.json'"
step evaluate python3 probes/0149/evaluate.py "$out/u7/u7-triage.tsv" probes/0139/out/u4-session.tsv probes/0148/out/u6-script/u6-tickets.tsv probes/0139/frozen "$d2"
