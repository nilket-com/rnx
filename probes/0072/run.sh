#!/usr/bin/env bash
# Record 0072, one command: document both releases in both configurations,
# prove the extractor on the control crate, inventory and classify each
# configuration, prove the sample runner can fail, generate/compile/execute
# the samples for both releases under the same frozen generator, and print
# the tables the evidence quotes. Any failing step fails the command.
#
#   probes/0072/run.sh            # everything
#   probes/0072/run.sh report     # only rebuild the tables from existing output
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
extract="$root/target/0072/extract/debug/surface"
inv55="$here/out/0.55.2-adapter/result/inventory.json"
inv54="$here/out/0.54.4-adapter/result/inventory.json"
if [ "${1:-}" != report ]; then
  ( cd "$here/extract" && CARGO_TARGET_DIR="$root/target/0072/extract" cargo build -q )
  "$here/control/run.sh"
  for spec in "0.55.2 adapter" "0.55.2 full" "0.54.4 adapter" "0.54.4 full"; do
    "$here/inventory/doc.sh" $spec
  done
fi
for d in "$here"/out/*-*/; do
  [ -f "$d/pins.json" ] || { echo "FAIL $(basename "$d"): no pins.json (documentation incomplete)"; exit 1; }
  "$extract" "$d" polars "$d/result" > /dev/null
done
if [ "${1:-}" != report ]; then
  # Self-check: an injected wrong oracle must make the runner fail.
  if python3 "$here/samples/gen.py" "$inv55" --build --inject-failure s000 > "$root/target/0072-selfcheck.log" 2>&1; then
    echo "FAIL: the sample runner reported success with an injected wrong oracle"; exit 1
  fi
  grep -q "executed_mismatch" "$root/target/0072-selfcheck.log" || { echo "FAIL: injected failure was not reported as a mismatch"; exit 1; }
  echo "self-check ok: injected wrong oracle fails the run"
  if python3 "$here/samples/gen.py" "$inv55" --build --inject-broken-reuse s001 > "$root/target/0072-selfcheck-reuse.log" 2>&1; then
    echo "FAIL: the sample runner reported success with an injected broken second result"; exit 1
  fi
  grep -q "receiver_reuse_failed" "$root/target/0072-selfcheck-reuse.log" || { echo "FAIL: broken reuse was not reported as such"; exit 1; }
  echo "self-check ok: injected broken reuse fails the run"
  python3 "$here/samples/gen.py" "$inv55" --build --release 0.55.2
  python3 "$here/samples/gen.py" "$inv54" --build --release 0.54.4
  python3 "$here/samples/gen.py" "$inv55" > /dev/null   # re-pin the harness to 0.55.2
fi
python3 "$here/report.py" "$here"
