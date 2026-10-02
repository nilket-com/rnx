#!/usr/bin/env bash
# Record 0153: the triage run, in order (plans/0153 sections 3, 7). Gate 0
# (the freeze and provenance) runs first and stops everything on failure;
# the fixtures and the NLI handler's controls run next, before any inference.
#
#   run.sh d2  D3_DIR D2_JSON MINILM NLI HF_PYTHON OUT
#       the rehearsal on spent D2 (every ticket): producers, validation,
#       semantic check. Its evidence also serves the frozen D2 diagnostic.
#   run.sh dev D3_DIR D2_JSON MINILM NLI HF_PYTHON OUT D2_OUT
#       the same chain over D3-dev, then the evaluation and selection.
#
# D3-hold is never an input here. RNX_0153_STUB, when set, replaces every
# downstream step with that command (called with the step's name; for
# hf-t3 it writes RNX_0153_RESULT).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
mode=$1 d3=$2 d2=$3 minilm=$4 nli=$5 hfpy=$6 out=$7
labels=$here/frozen/labels.tsv
case "$mode" in
	d2)
		issues=$d2
		split=$out/d2-all.tsv
		;;
	dev)
		issues=$d3/issues.json
		split=$here/frozen/d3-dev.tsv
		d2out=$8
		;;
	*) echo "mode: d2 or dev" >&2; exit 1 ;;
esac
cd "$root"

# gate 0: the freeze and provenance
python3 "$here/preflight.py" "$d3" "$d2" "$minilm" "$nli"

step() {
	if [ -n "${RNX_0153_STUB:-}" ]; then "$RNX_0153_STUB" "$1"; return; fi
	shift; "$@"
}
runner=adapters/polars/target/release/runner0134
twin=probes/0133/twin/target/release/twin0133
mkdir -p "$out/s" "$out/t"
chmod 700 "$out/s" "$out/t"
if [ "$mode" = d2 ]; then
	python3 -c "
import json, sys
ns = sorted(i['number'] for i in json.load(open(sys.argv[1])))
open(sys.argv[2], 'w').write('number\n' + ''.join(f'{n}\n' for n in ns))" "$d2" "$split"
fi
step fixtures python3 "$here/fixtures.py"
step handler $runner run --budget 2000000000 probes/0153/triage.rn "$nli"
step twin-controls $twin t3-controls
step session env RNX_SESSION_ARGS=repl python3 probes/0133/session.py $runner probes/0153/triage.rn "$out/s/session.log" "$issues" "$split" "$labels" "$minilm" "$nli" "$out/s"
step twin $twin t3 "$issues" "$split" "$labels" "$minilm" "$nli" "$out/t"
tok=$nli/tokenizer.json
step validate "$hfpy" "$here/validate.py" "$out/s" "$out/t" "$issues" "$split" "$labels" "$tok"
export RNX_0153_RESULT="$out/hf-t3.json"
rm -f "$RNX_0153_RESULT"
set +e
step hf-t3 "$hfpy" "$here/hf_t3.py" "$minilm" "$nli" "$out/s" "$issues" "$split" "$labels" "$RNX_0153_RESULT"
status=$?
set -e
if [ -n "${RNX_0153_STUB:-}" ]; then
	python3 -c "import json, sys; r = json.load(open(sys.argv[1])); assert r['check'] == 'hf-t3' and r['completed'] and r['result'] == 'PASS'" "$RNX_0153_RESULT"
else
	"$hfpy" "$here/hf_result.py" "$RNX_0153_RESULT" "$status" "$out/s" "$issues" "$split" "$labels" "$tok" --require-pass
fi
if [ "$mode" = dev ]; then
	step evaluate "$hfpy" "$here/evaluate.py" "$out/s" "$issues" "$d2out/s" "$d2" "$out" "$tok"
fi
