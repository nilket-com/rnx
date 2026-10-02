#!/usr/bin/env bash
# Record 0152: the locked holdout run, in order. Gate 0 (provenance and the
# 43711c4 pins) runs first and stops everything on failure. Both producers
# run retrieval only; the HF check writes a completion artifact validated by
# hf_result.py (with its dimensions) before the run continues.
#
#   run.sh D1 MINILM_DIR CE_DIR HF_PYTHON OUT
#
# RNX_0152_QUERIES=dev rehearses the identical driver on the spent
# development queries (plumbing only); unset, it runs the holdout.
# RNX_0152_STUB, when set, replaces every downstream step with that command
# (called with the step's name; for hf-retrieval it writes RNX_0152_RESULT).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
d1=$1 minilm=$2 ce=$3 hfpy=$4 out=$5
case "${RNX_0152_QUERIES:-holdout}" in
	holdout) queries=$root/probes/0151/holdout.tsv ;;
	dev) queries=$root/probes/0151/dev.tsv ;;
	*) echo "RNX_0152_QUERIES: holdout or dev" >&2; exit 1 ;;
esac
passages=3461
cd "$root"

# gate 0: provenance and pins
python3 "$here/preflight.py" "$d1" "$minilm" "$ce"
echo "queries: $(basename "$queries")"

step() {
	if [ -n "${RNX_0152_STUB:-}" ]; then "$RNX_0152_STUB" "$1"; return; fi
	shift; "$@"
}
runner=adapters/polars/target/release/runner0134
twin=probes/0133/twin/target/release/twin0133
mkdir -p "$out/u8" "$out/u8t"
chmod 700 "$out/u8" "$out/u8t"
step bm25-controls python3 probes/0151/bm25.py
# the cross-encoder directory is passed but never loaded in retrieval mode
step u8 env RNX_SESSION_ARGS=repl python3 probes/0133/session.py $runner probes/0151/u8_rerank_dev.rn "$out/u8/session.log" "$d1" "$minilm" "$ce" "$queries" "$out/u8" retrieval
step twin $twin u8 "$d1" "$minilm" "$ce" "$queries" "$out/u8t" retrieval
step validate python3 "$here/validate_retrieval.py" "$out/u8" "$out/u8t" "$queries" "$d1"
step order bash -c "$runner run --budget 2000000000 probes/0151/u8_rerank_dev.rn '$out/u8/u8-retrieval.tsv' '$out/orders.tsv' && python3 '$here/order_check.py' '$out/u8' '$out/orders.tsv' '$queries' '$d1'"
export RNX_0152_RESULT="$out/hf-retrieval.json" RNX_0152_QFILE="$queries"
rm -f "$RNX_0152_RESULT"
set +e
step hf-retrieval "$hfpy" "$here/hf_retrieval.py" "$minilm" "$out/u8" "$queries" "$d1" $passages "$RNX_0152_RESULT"
status=$?
set -e
python3 "$here/hf_result.py" hf-retrieval "$RNX_0152_RESULT" "$status" --require-pass --dims $passages "$queries"
step evaluate python3 "$here/evaluate.py" "$out/u8" "$queries" "$d1" probes/0151/out/selected.json "$out/result.json"
