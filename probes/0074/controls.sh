#!/usr/bin/env bash
# Record 0074 negative controls on the runner: each must make run.sh exit
# nonzero at the right stage, and nothing an earlier run produced may be
# reported. They use the real scripts with an alternative output
# directory (the rc2 documentation is linked in, since it is skipped).
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
fails=0
check() { if [ "$1" = 0 ]; then echo "CONTROL FAIL: $2"; fails=1; else echo "control ok: $2"; fi; }
tmp="$(mktemp -d)"
fresh() { rm -rf "$tmp/out" "$tmp/locks"; mkdir -p "$tmp/out" "$tmp/locks"; ln -s "$here/out/rc2" "$tmp/out/rc2"; cp "$here/locks/"*.lock "$tmp/locks/"; }
common=(PROBE_SKIP_DOC=1 PROBE_OUT="$tmp/out" PROBE_LOCKS="$tmp/locks" PROBE_SCRATCH="$tmp/scratch")

# 1. a changed frozen input is refused before any tool runs
fresh
python3 - "$here/frozen.json" "$tmp/frozen.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1])); d["inputs"]["generator"]["sha256"] = "0" * 64
json.dump(d, open(sys.argv[2], "w"))
PY
env "${common[@]}" PROBE_FROZEN="$tmp/frozen.json" "$here/run.sh" > "$tmp/c1.log" 2>&1; check $? "a generator whose digest differs from frozen.json is refused"
grep -q "RUN INVALID at frozen" "$tmp/c1.log" || { echo "CONTROL FAIL: not refused at the frozen stage"; fails=1; }

# 2. a missing lock is refused on replay
fresh; rm -f "$tmp/locks/adapter.lock"
env "${common[@]}" "$here/run.sh" > "$tmp/c2.log" 2>&1; check $? "a replay without the committed adapter lock is refused"
grep -q "RUN INVALID at locks" "$tmp/c2.log" || { echo "CONTROL FAIL: not refused at the locks stage"; fails=1; }

# 3. stale oracle output plus failing tests: the run is invalid and nothing stale is reported
fresh
echo '{"cases": 1, "tally": {"match": 999}, "results": [], "run_id": "stale"}' > "$tmp/out/oracle-results-rc2.json"
shim="$tmp/cargo"; printf '#!/usr/bin/env bash\nif [ "${1:-}" = test ]; then echo "error: injected test failure" >&2; exit 7; fi\nexec cargo "$@"\n' > "$shim"; chmod +x "$shim"
env "${common[@]}" PROBE_CARGO="$shim" "$here/run.sh" > "$tmp/c3.log" 2>&1; check $? "failing oracle controls invalidate the run"
grep -q "RUN INVALID at oracle_controls" "$tmp/c3.log" || { echo "CONTROL FAIL: not invalidated at the oracle controls stage"; fails=1; }
grep -q "999" "$tmp/c3.log" && { echo "CONTROL FAIL: a stale oracle tally was reported"; fails=1; }
[ -f "$tmp/out/oracle-results-rc2.json" ] && { echo "CONTROL FAIL: the stale oracle results survived"; fails=1; }

# 4. a failing generator invalidates the run before any build
fresh
env "${common[@]}" PROBE_GEN=/bin/false "$here/run.sh" > "$tmp/c4.log" 2>&1; check $? "a failing generator invalidates the run"
grep -q "RUN INVALID at generate" "$tmp/c4.log" || { echo "CONTROL FAIL: not invalidated at the generate stage"; fails=1; }
[ -f "$tmp/out/build.json" ] && { echo "CONTROL FAIL: a build ran after the generator failed"; fails=1; }

rm -rf "$tmp"
[ "$fails" = 0 ] && echo "all controls ok"
exit $fails
