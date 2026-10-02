#!/usr/bin/env bash
# Record 0147: the whole evaluation, in order. Gate 0 (provenance) runs first
# and stops everything on failure: no session, twin or HF step begins.
#
#   run.sh D1 MINILM_DIR CE_DIR HF_PYTHON OUT
#
# RNX_0147_STUB, when set, replaces every downstream step with that command
# (called with the step's name); preflight_controls.sh uses it as a sentinel.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
d1=$1 minilm=$2 ce=$3 hfpy=$4 out=$5
rubric=$here/rubric.tsv
cd "$root"

# gate 0: provenance
python3 "$here/preflight.py" "$d1" "$minilm" "$ce"

# gate 1: the frozen rubric (the accepted plan's hash) and its kinds
want=ee31b55db02970a9aa4a714c50eef35df29a9b15a4fb2ac041670d8def819326
got=$(sha256sum "$rubric" | cut -d' ' -f1)
[ "$got" = "$want" ] || { echo "FAIL: rubric.tsv is $got, frozen $want"; exit 1; }
python3 "$here/kinds.py" "$rubric" "$d1"

# gate 2: the pipelines are byte-identical to their committed versions
for spec in a959f21:probes/0136/workflow-u1c.rn 240fa54:probes/0146/u5_rerank.rn; do
	[ "$(git rev-parse "$spec")" = "$(git hash-object "${spec#*:}")" ] || { echo "FAIL: ${spec#*:} differs from ${spec%%:*}"; exit 1; }
done
echo "scripts: workflow-u1c.rn as at a959f21, u5_rerank.rn as at 240fa54"

step() {
	if [ -n "${RNX_0147_STUB:-}" ]; then "$RNX_0147_STUB" "$1"; return; fi
	shift; "$@"
}
runner=adapters/polars/target/release/runner0134
twin=probes/0133/twin/target/release/twin0133
mkdir -p "$out/u1" "$out/u5" "$out/u5t"
chmod 700 "$out/u1" "$out/u5" "$out/u5t"
step u1 env RNX_SESSION_ARGS=repl python3 probes/0133/session.py $runner probes/0136/workflow-u1c.rn "$out/u1/session.log" "$d1" "$minilm" "$rubric" "$out/u1" 0
step u5 env RNX_SESSION_ARGS=repl python3 probes/0133/session.py $runner probes/0146/u5_rerank.rn "$out/u5/session.log" "$d1" "$minilm" "$ce" "$rubric" "$out/u5"
step twin $twin u5 "$d1" "$minilm" "$ce" "$rubric" "$out/u5t"
step compare python3 probes/0146/compare_u5.py "$out/u5" "$out/u5t" "$rubric" "$d1"
step hf bash -c "cd probes/0146 && '$hfpy' hf_check.py '$ce' '$out/u5' '$rubric' '$d1'"
step evaluate python3 "$here/evaluate.py" "$out/u5" "$out/u1/u1.tsv" "$rubric" "$d1"
