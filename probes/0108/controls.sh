#!/usr/bin/env bash
# Record 0108: negative controls on build.sh's no-default stage. A fake cargo
# stands in for the test run; outputs go to a temporary directory, so no
# measured output is touched. Needs the scratch adapter and locks/adapter.lock.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
fake() { printf '#!/usr/bin/env bash\n%s\n' "$1" > "$tmp/cargo-$2"; chmod +x "$tmp/cargo-$2"; }
fake 'echo "test result: FAILED. 0 passed; 1 failed"; exit 101' failing
fake 'echo "test result: ok. 1 passed; 0 failed"; exit 0' passing
fake 'echo "error: could not compile"; exit 101' silent
bad=0
run() { # label, cargo, expected exit (0 or nonzero)
  PROBE_STAGE=no_default PROBE_OUT="$tmp/out-$2" PROBE_CARGO="$tmp/cargo-$2" "$here/build.sh" > "$tmp/$2.log" 2>&1; local got=$?
  if { [ "$3" = 0 ] && [ "$got" = 0 ]; } || { [ "$3" != 0 ] && [ "$got" != 0 ]; }; then echo "ok   $1 (exit $got)"; else echo "FAIL $1 (exit $got)"; cat "$tmp/$2.log"; bad=1; fi
}
mkdir -p "$tmp"/out-{failing,passing,silent}
run "a failing test run makes the stage exit nonzero" failing 1
run "a run that never reaches a test result exits nonzero" silent 1
run "a passing test run exits zero" passing 0
grep -q '"no_default": "tests failed' "$tmp/out-failing/build-status.json" || { echo "FAIL the failure is not recorded in build-status.json"; bad=1; }
exit $bad
