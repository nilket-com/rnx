#!/usr/bin/env bash
# Record 0154: the development-only rehearsal (plans/0154 section 3, review
# R1). Plumbing only: no D3-hold input. 0153's committed D3-dev evidence is
# decompressed and validated; the evaluator runs path (a), 0153's cross-fit
# predictions (must give 74 / 7), and path (b), the frozen full-dev centroid
# policy (must equal reference_b.py's independent computation, 88 / 7).
#
#   rehearse.sh HF_PYTHON NLI_TOKENIZER OUT
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
hfpy=$1 tok=$2 out=$3
cd "$root"
mkdir -p "$out/dev/s"
for f in probes/0153/out/dev/s/*.gz; do gunzip -c "$f" > "$out/dev/s/$(basename "$f" .gz)"; done
"$hfpy" "$here/fixtures.py" | tail -1
"$hfpy" "$here/reference_b.py" "$out/dev/s" | tee "$out/reference-b.txt"
"$hfpy" "$here/evaluate.py" rehearse "$out/dev/s" "$HOME/.local/share/rnx-uat-0133/d3/issues.json" "$tok" "$out" | tee "$out/rehearsal.txt"
"$hfpy" - "$out" <<'PY'
import json, sys
r = json.load(open(f"{sys.argv[1]}/rehearsal.json"))
ref = open(f"{sys.argv[1]}/reference-b.txt").read()
a, b = r["a"], r["b"]
ok = (a["auto"], a["possible"], a["certain"]) == (74, 7, 7)
ok &= f"{b['auto']} auto-routed of 346, {b['possible']} errors" in ref and (b["auto"], b["possible"]) == (88, 7)
print("rehearsal: (a) reproduces 0153's 74 / 7, (b) equals the independent reference 88 / 7" if ok else "REHEARSAL FAILED")
sys.exit(0 if ok else 1)
PY
