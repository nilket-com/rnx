#!/usr/bin/env bash
# Record 0135 part 1 (0133 F4b): U1 composed as `rnx project run`, with
# Polars and Candle, halting at the default budget and passing with --budget.
#   project_budget.sh RNX MODEL WORK
# RNX_UAT_DATA names 0133's data root (d1/plans). The project's runtime is
# this checkout, so the tree must be committed (path builds refuse changes).
set -euo pipefail
rnx=$(realpath "$1"); model=$(realpath "$2"); work=$3
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../.." && pwd)
data=${RNX_UAT_DATA:?}
rm -rf "$work"; mkdir -p "$work/app" "$work/out" "$work/twin"
cat > "$work/app/rnx.toml" <<TOML
format=1
[application]
entry='main.rn'
[runtime]
path='$root'
TOML
{ cat "$root/probes/0133/workflow-u1.rn"; printf '\npub fn main(args) { run(args) }\n'; } > "$work/app/main.rn"
m="$work/app/rnx.toml"
"$rnx" project add --manifest "$m" polars candle
"$rnx" project lock --manifest "$m"
t=$(date +%s); "$rnx" project build --manifest "$m"; echo "build: $(( $(date +%s) - t )) s"
args=("$data/d1/plans" "$model" "$root/probes/0133/rubric.tsv" "$work/out")

echo "== default budget: must halt and name the project flag"
set +e; "$rnx" project run --manifest "$m" -- "${args[@]}" > "$work/default.out" 2> "$work/default.err"; code=$?; set -e
cat "$work/default.err"
[ "$code" -ne 0 ] || { echo "FAIL: default budget completed"; exit 1; }
grep -q 'instructions exceeded; rnx project run --budget N raises it' "$work/default.err" || { echo "FAIL: halt text"; exit 1; }
echo "ok: halted with code $code, naming rnx project run --budget"

echo "== --budget 2000000000: must complete"
t=$(date +%s)
"$rnx" project run --manifest "$m" --budget 2000000000 -- "${args[@]}" > "$work/budget.out"
echo "run: $(( $(date +%s) - t )) s"
grep -q 'WORKFLOW OK u1' "$work/budget.out" || { echo "FAIL: no marker"; exit 1; }
cp "$root/probes/0133/out/u1-twin.tsv" "$work/twin/u1.tsv"
for u in u2 u3; do cp "$root/probes/0133/out/$u-twin.tsv" "$work/out/$u.tsv"; cp "$root/probes/0133/out/$u-twin.tsv" "$work/twin/$u.tsv"; done
python3 "$root/probes/0133/compare_outputs.py" "$work/out" "$work/twin" | grep -E '^u1|vs twin'
