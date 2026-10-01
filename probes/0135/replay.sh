#!/bin/sh
# Record 0135: 0133's workflows on the new partition, through both gates.
#
#   RNX_UAT_DATA=DIR replay.sh RNX MODEL_DIR OUT_DIR C [PYTHON]
#
# RNX is a binary with this tree's adapters (probes/0134/runner, sessions
# started with `repl`; :dep would fetch the pushed adapters), or, after push,
# a pushed binary with RNX_SESSION_ARGS unset. C is the adapter's
# CONCURRENCY, which the twin uses to derive the same partition on its own.
#
# 1. data and model verify, as in 0133 (fatal);
# 2. U1, U2 and U3 run as session pastes, each passing only on its marker;
# 3. the twins run on the S1 partition they derive independently;
# 4. gate 1: the complete outputs, bit for bit, against the twins;
# 5. gate 2: the outputs against 0133's retained (old-partition) outputs,
#    identities exact and scores within 1e-5, with the mutation controls;
# 6. with PYTHON, the contemporaneous PyTorch timing.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
u=$here/../0133
rnx=$1; model=$2; out=$3; c=$4; python=${5:-}
data=${RNX_UAT_DATA:?set RNX_UAT_DATA}
mkdir -p "$out/session" "$out/twin" "$out/old"
python3 "$u/manifest.py" check "$data"
if ! (cd "$model" 2>/dev/null && sed -n '/^[0-9a-f]\{64\}  /p' "$here/../0131/fetch.sh" | sha256sum -c --quiet --strict); then
	echo "model NOT verified: $model"
	exit 1
fi
echo "model verified"
twin=${TWIN:-$u/twin/target/release/twin0133}
python3 "$u/session.py" "$rnx" "$u/workflow-u1.rn" "$out/session/u1.log" "$data/d1/plans" "$model" "$u/rubric.tsv" "$out/session"
python3 "$u/session.py" "$rnx" "$u/workflow-u2.rn" "$out/session/u2.log" "$data/d1/plans" "$data/d2/issues.json" "$model" "$out/session"
python3 "$u/session.py" "$rnx" "$u/workflow-u3.rn" "$out/session/u3.log" "$data/d2/issues.json" "$model" "$out/session"
TWIN0135_CONCURRENCY=$c "$twin" u1 "$data/d1/plans" "$model" "$u/rubric.tsv" > "$out/twin/u1.tsv"
TWIN0135_CONCURRENCY=$c "$twin" u2 "$data/d1/plans" "$data/d2/issues.json" "$model" > "$out/twin/u2.tsv"
TWIN0135_CONCURRENCY=$c "$twin" u3 "$data/d2/issues.json" "$model" > "$out/twin/u3.tsv"
for n in u1 u2 u3; do cp "$u/out/$n-session.tsv" "$out/old/$n.tsv"; done
echo "gate 1: the new outputs against the twins on the identical partition"
python3 "$here/compare.py" exact "$out/session" "$out/twin"
echo "gate 2: the new outputs against 0133's retained outputs"
python3 "$here/compare.py" tolerance "$out/old" "$out/session"
echo "the gates' mutation controls"
python3 "$here/compare.py" controls "$out/old" "$out/session" "$out/twin"
if [ -n "$python" ]; then
	"$python" "$u/diagnostics/torch_timing.py" "$model" "$data"
fi
echo "replay: all checks passed"
