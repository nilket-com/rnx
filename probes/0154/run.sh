#!/usr/bin/env bash
# Record 0154: the locked holdout run (plans/0154 sections 3, 4). Gate 0
# runs first and stops everything on failure; every control runs before
# the producers touch D3-hold. Once any producer has scored a holdout
# ticket, the holdout is spent (section 4).
#
#   run.sh D3_DIR D2_JSON MINILM NLI HF_PYTHON OUT
#
# RNX_0154_STUB, when set, replaces every downstream step with that command
# (called with the step's name; for hf-t3 it writes RNX_0154_RESULT).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
d3=$1 d2=$2 minilm=$3 nli=$4 hfpy=$5 out=$6
p53=$root/probes/0153
labels=$p53/frozen/labels.tsv
split=$p53/frozen/d3-hold.tsv
issues=$d3/issues.json
tok=$nli/tokenizer.json
cd "$root"

# gate 0: the freeze, provenance and the policy pins
"$hfpy" "$here/preflight.py" "$d3" "$d2" "$minilm" "$nli"

step() {
	if [ -n "${RNX_0154_STUB:-}" ]; then "$RNX_0154_STUB" "$1"; return; fi
	shift; "$@"
}
runner=adapters/polars/target/release/runner0134
twin=probes/0133/twin/target/release/twin0133
mkdir -p "$out/s" "$out/t"
chmod 700 "$out/s" "$out/t"
step fixtures-0153 "$hfpy" "$p53/fixtures.py"
step fixtures-0154 "$hfpy" "$here/fixtures.py"
step handler $runner run --budget 2000000000 probes/0153/triage.rn "$nli"
step twin-controls $twin t3-controls
# from here the producers score D3-hold: the holdout is spent
step session env RNX_SESSION_ARGS=repl python3 probes/0133/session.py $runner probes/0153/triage.rn "$out/s/session.log" "$issues" "$split" "$labels" "$minilm" "$nli" "$out/s"
step twin $twin t3 "$issues" "$split" "$labels" "$minilm" "$nli" "$out/t"
step validate "$hfpy" "$p53/validate.py" "$out/s" "$out/t" "$issues" "$split" "$labels" "$tok"
export RNX_0154_RESULT="$out/hf-t3.json"
rm -f "$RNX_0154_RESULT"
set +e
step hf-t3 "$hfpy" "$p53/hf_t3.py" "$minilm" "$nli" "$out/s" "$issues" "$split" "$labels" "$RNX_0154_RESULT"
status=$?
set -e
if [ -n "${RNX_0154_STUB:-}" ]; then
	python3 -c "import json, sys; r = json.load(open(sys.argv[1])); assert r['check'] == 'hf-t3' and r['completed'] and r['result'] == 'PASS'" "$RNX_0154_RESULT"
else
	"$hfpy" "$p53/hf_result.py" "$RNX_0154_RESULT" "$status" "$out/s" "$issues" "$split" "$labels" "$tok" --require-pass
fi
step evaluate "$hfpy" "$here/evaluate.py" holdout "$out/s" "$issues" "$tok" "$out"
