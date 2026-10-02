#!/usr/bin/env bash
# Record 0151: the development run, in order. Gate 0 (provenance) runs first
# and stops everything on failure. The HF check writes a completion artifact,
# validated by hf_result.py before the run continues. The holdout is read only
# by gate 0: hashed, and read by split.py to replay the frozen split. It is
# never passed to retrieval, scoring or evaluation.
#
#   run.sh D1 MINILM_DIR CE_DIR HF_PYTHON OUT
#
# RNX_0151_STUB, when set, replaces every downstream step with that command
# (called with the step's name; for hf-pool it writes RNX_0151_RESULT).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
d1=$1 minilm=$2 ce=$3 hfpy=$4 out=$5
dev=$here/dev.tsv
cd "$root"

# gate 0: provenance
python3 "$here/preflight.py" "$d1" "$minilm" "$ce"

step() {
	if [ -n "${RNX_0151_STUB:-}" ]; then "$RNX_0151_STUB" "$1"; return; fi
	shift; "$@"
}
runner=adapters/polars/target/release/runner0134
twin=probes/0133/twin/target/release/twin0133
mkdir -p "$out/u8" "$out/u8t"
chmod 700 "$out/u8" "$out/u8t"
step bm25-controls python3 "$here/bm25.py"
step u8 env RNX_SESSION_ARGS=repl python3 probes/0133/session.py $runner probes/0151/u8_rerank_dev.rn "$out/u8/session.log" "$d1" "$minilm" "$ce" "$dev" "$out/u8"
step twin $twin u8 "$d1" "$minilm" "$ce" "$dev" "$out/u8t"
step validate python3 "$here/validate.py" "$out/u8" "$out/u8t" "$dev" "$d1"
step order bash -c "$runner run --budget 2000000000 probes/0151/u8_rerank_dev.rn '$out/u8/u8-retrieval.tsv' '$out/orders.tsv' && python3 '$here/order_check.py' '$out/u8' '$out/orders.tsv' '$dev' '$d1'"
export RNX_0151_RESULT="$out/hf-pool.json"
rm -f "$RNX_0151_RESULT"
set +e
step hf-pool bash -c "cd probes/0151 && '$hfpy' hf_pool.py '$ce' '$out/u8' '$dev' '$d1' '$RNX_0151_RESULT'"
status=$?
set -e
"$hfpy" "$here/hf_result.py" hf-pool "$RNX_0151_RESULT" "$status" --require-pass
step evaluate python3 "$here/evaluate.py" "$out/u8" "$dev" "$d1" "$out/selected.json"
