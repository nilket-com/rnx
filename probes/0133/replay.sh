#!/bin/sh
# Record 0133: the full replay.
#
#   RNX_UAT_DATA=DIR replay.sh RNX MODEL_DIR OUT_DIR [PYTHON]
#
# 1. the datasets verify against their manifests, or nothing is scored;
# 2. the model's six files verify against fetch.sh's pinned SHA-256s;
# 3. U1, U2 and U3 run in ordinary `:dep polars candle` sessions (RNX must be
#    a binary whose commit is pushed, as :dep fetches the adapters at it), each
#    passing only on its WORKFLOW OK marker with no error after the call;
# 4. the twins run;
# 5. the complete outputs are compared exactly, with the fail-closed controls
#    (and session_controls.sh proves the session gate fails closed);
# 6. with PYTHON (a Python with tokenizers, torch, sentence-transformers), the
#    diagnostics: truncation counts and the PyTorch timing.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
rnx=$1; model=$2; out=$3; python=${4:-}
data=${RNX_UAT_DATA:?set RNX_UAT_DATA}
mkdir -p "$out/session" "$out/twin"
python3 "$here/manifest.py" check "$data"
"$here/manifest_controls.sh"
# an explicit, fatal gate: nothing downstream runs unless the model verifies
if ! (cd "$model" 2>/dev/null && sed -n '/^[0-9a-f]\{64\}  /p' "$here/../0131/fetch.sh" | sha256sum -c --quiet --strict); then
	echo "model NOT verified: $model"
	exit 1
fi
echo "model verified"
twin=${TWIN:-$here/twin/target/release/twin0133}
python3 "$here/session.py" "$rnx" "$here/workflow-u1.rn" "$out/session/u1.log" "$data/d1/plans" "$model" "$here/rubric.tsv" "$out/session"
python3 "$here/session.py" "$rnx" "$here/workflow-u2.rn" "$out/session/u2.log" "$data/d1/plans" "$data/d2/issues.json" "$model" "$out/session"
python3 "$here/session.py" "$rnx" "$here/workflow-u3.rn" "$out/session/u3.log" "$data/d2/issues.json" "$model" "$out/session"
"$twin" u1 "$data/d1/plans" "$model" "$here/rubric.tsv" > "$out/twin/u1.tsv"
"$twin" u2 "$data/d1/plans" "$data/d2/issues.json" "$model" > "$out/twin/u2.tsv"
"$twin" u3 "$data/d2/issues.json" "$model" > "$out/twin/u3.tsv"
python3 "$here/compare_outputs.py" "$out/session" "$out/twin" --controls
"$here/session_controls.sh" "$rnx"
if [ -n "$python" ]; then
	"$python" "$here/diagnostics/truncation.py" "$model" "$data"
	"$python" "$here/diagnostics/torch_timing.py" "$model" "$data"
fi
echo "replay: all checks passed"
