#!/bin/sh
# Record 0136: U1 prime (token-aware chunking) at overlap 0 and 32.
#
#   RNX_UAT_DATA=DIR replay.sh RNX MODEL_DIR OUT_DIR
#
# RNX is a binary with this tree's adapters (probes/0134/runner, sessions
# started with `repl`: RNX_SESSION_ARGS=repl), or, after push, a pushed
# binary with RNX_SESSION_ARGS unset (:dep polars candle).
# 1. data and model verify (0133's checks, fatal);
# 2. per overlap: U1 prime as a session paste, passing only on its marker;
#    the twin, chunking independently (TWIN0136_OVERLAP) on 0135's partition
#    (TWIN0135_CONCURRENCY=32); gate 1, the complete output bit for bit;
#    the rubric's hit@1 and hit@5, reported (not gated).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
u=$here/../0133
rnx=$1; model=$2; out=$3
data=${RNX_UAT_DATA:?set RNX_UAT_DATA}
python3 "$u/manifest.py" check "$data"
if ! (cd "$model" 2>/dev/null && sed -n '/^[0-9a-f]\{64\}  /p' "$here/../0131/fetch.sh" | sha256sum -c --quiet --strict); then
	echo "model NOT verified: $model"
	exit 1
fi
echo "model verified"
twin=${TWIN:-$u/twin/target/release/twin0133}
for o in 0 32; do
	mkdir -p "$out/o$o/session" "$out/o$o/twin"
	python3 "$u/session.py" "$rnx" "$here/workflow-u1c.rn" "$out/o$o/session/u1.log" "$data/d1/plans" "$model" "$u/rubric.tsv" "$out/o$o/session" "$o"
	TWIN0136_OVERLAP=$o TWIN0135_CONCURRENCY=32 "$twin" u1 "$data/d1/plans" "$model" "$u/rubric.tsv" > "$out/o$o/twin/u1.tsv"
	echo "overlap $o, gate 1: U1 prime against the twin"
	python3 "$here/../0135/compare.py" exact "$out/o$o/session" "$out/o$o/twin" u1
	echo "overlap $o, the rubric (reported, not gated)"
	python3 "$here/score.py" "$out/o$o/session/u1.tsv" "$u/rubric.tsv"
done
echo "replay: all checks passed"
