#!/usr/bin/env bash
# Record 0079 gate 2: builds the probe, runs the in-process contract test
# and the subprocess controls, every one in its own session under a
# watchdog that tracks the whole process group, not just its leader: on
# timeout the group is killed and reaped; after any exit a surviving group
# member fails the control; a timeout is a result ("deadlock"), never a
# pass for anything but the experiments that predict one. Writes
# out/*.json and out/report.md; exits nonzero unless every control held.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
probe="$here/probe"; out="$here/out"; mkdir -p "$out"
target="${PROBE_TARGET:-$root/target/0079-probe}"
CARGO="${PROBE_CARGO:-cargo}"
fail=0
say() { echo "$*"; }

( cd "$probe" && CARGO_TARGET_DIR="$target" "$CARGO" build -q --locked --release ) || { echo "FAIL: probe build"; exit 1; }
bin="$target/release/callback-probe"

# Runs `cmd...` in its own session (setsid; the leader's pid lands in
# $pidfile through `$$` of the wrapping shell, which execs the command).
# Waits up to $secs seconds; on timeout kills the group and records
# "deadlock". After any exit, lists the group's survivors, kills them, and
# reports them. Sets: status, survivors, line (the command's last output line).
watch_group() {
  local name="$1" secs="$2"; shift 2
  local log="$out/$name.log"; local pidfile="$out/$name.pid"; rm -f "$pidfile"
  ( cd "$probe" && setsid bash -c 'echo $$ > "$0"; exec "$@"' "$pidfile" "$@" > "$log" 2>&1 ) &
  local job=$!
  status="?"; survivors=""
  local waited=0
  while kill -0 "$job" 2>/dev/null; do
    sleep 0.2; waited=$((waited + 1))
    if [ $((waited / 5)) -ge "$secs" ]; then status="deadlock"; break; fi
  done
  local leader; leader="$(cat "$pidfile" 2>/dev/null)"
  if [ "$status" = "?" ]; then wait "$job"; status="exit $?"; fi
  # the group is the leader's session id (setsid): everything spawned inside it, whether or not the leader is alive
  local members=""
  [ -n "$leader" ] && members="$(pgrep -s "$leader" 2>/dev/null | tr '\n' ' ')"
  if [ -n "$members" ]; then
    survivors="$members"
    kill -KILL $members 2>/dev/null; sleep 0.3
    local left; left="$(pgrep -s "$leader" 2>/dev/null | tr '\n' ' ')"
    [ -n "$left" ] && { say "$name: group members survived SIGKILL: $left"; fail=1; }
  fi
  [ "$status" = deadlock ] && wait "$job" 2>/dev/null
  line="$(tail -1 "$log" 2>/dev/null)"
}

record() {
  python3 - "$out/subprocess.json" "$@" <<'PY'
import json, sys, os
p, name, expect, status, verdict, survivors, line = sys.argv[1:]
d = json.load(open(p)) if os.path.exists(p) else {}
try: parsed = json.loads(line)
except Exception: parsed = line
d[name] = {"expected": expect, "status": status, "verdict": verdict, "survivors": survivors.split(), "output": parsed}
json.dump(d, open(p, "w"), indent=1)
PY
}

# One probe-script control: name, timeout seconds, expected outcome, env
# assignments, script, extra args. Expected: completed | deadlock | refused |
# interrupted | survivor (the negative control: the driver must catch a
# surviving child after a normal exit).
run_control() {
  local name="$1" secs="$2" expect="$3" envs="$4" script="$5"; shift 5
  if [ -n "${PROBE_ONLY:-}" ] && [ "$PROBE_ONLY" != "$name" ]; then return; fi
  local status survivors line
  watch_group "$name" "$secs" env $envs "$bin" "scripts/$script" "$@"
  local verdict="FAIL"
  case "$expect" in
    completed) [[ "$status" == "exit 0" && -z "$survivors" && "$line" == *'"result":"completed'* ]] && verdict=ok ;;
    deadlock) [[ "$status" == deadlock ]] && verdict=ok ;;
    refused) [[ "$status" == "exit 0" && -z "$survivors" && "$line" == *'is a routed binding and may not be called from a callback'* ]] && verdict=ok ;;
    interrupted)
      # the value intact, and the signal delivered while the call was running (after its start, before its end)
      if [[ "$status" == "exit 0" && -z "$survivors" && "$line" == *'"result":"completed len=20000 first=1"'* && "$line" == *'"interrupted":true'* ]]; then
        python3 - "$line" <<'PY' && verdict=ok
import json, sys
d = json.loads(sys.argv[1])
sys.exit(0 if 0 < d["interrupted_at_ms"] < d["elapsed_ms"] else 1)
PY
      fi ;;
    survivor) [[ "$status" == "exit 0" && -n "$survivors" ]] && verdict=ok ;;
  esac
  [ "$verdict" = ok ] || fail=1
  say "$name: $verdict (expected $expect, got $status; survivors: ${survivors:-none}; $line)"
  record "$name" "$expect" "$status" "$verdict" "$survivors" "$line"
}

rm -f "$out/subprocess.json"
# the in-process contract test, under the same watchdog (it contains deliberately infinite callbacks)
if [ -z "${PROBE_ONLY:-}" ] || [ "$PROBE_ONLY" = contract ]; then
  status="?"; survivors=""; line=""
  watch_group contract 900 env PROBE_OUT="$out" CARGO_TARGET_DIR="$target" "$CARGO" test -q --locked --release --test contract
  if [[ "$status" == "exit 0" && -z "$survivors" ]]; then say "contract test: ok"; else say "FAIL: contract test ($status; survivors: ${survivors:-none}; see out/contract.log)"; fail=1; fi
  record contract completed "$status" "$([[ "$status" == "exit 0" && -z "$survivors" ]] && echo ok || echo FAIL)" "$survivors" "$line"
fi
# decision 3's experiments: denial off, pool sizes 1 and 2; inner work that
# needs the pool deadlocks (the plan's prediction), and a nested
# callback-bearing call whose inner operation happens to submit no pool work
# completes: which is why one completed nested call proves nothing
run_control starve_pool1 15 deadlock "POLARS_MAX_THREADS=1" starve.rn
run_control starve_pool2 15 deadlock "POLARS_MAX_THREADS=2" starve.rn
run_control starve_nested_callback_pool2 15 completed "POLARS_MAX_THREADS=2" starve_nested_callback.rn
# the same shape with the denial in force: refused, no hang
run_control starve_denied_pool2 15 refused "POLARS_MAX_THREADS=2" starve_denied.rn
# a standalone SIGINT handler that only sets a flag (the session's handler,
# not an rnx session): the signal lands while the callback loop runs, the
# call completes, the value is intact, the flag is seen afterwards
if [ -z "${PROBE_ONLY:-}" ] || [ "$PROBE_ONLY" = ctrlc ]; then
  ( sleep 0.8; kill -INT "$(cat "$out/ctrlc.pid" 2>/dev/null)" 2>/dev/null ) &
fi
run_control ctrlc 60 interrupted "" ctrlc.rn --watch-interrupt
# controls of the controls: a leader that must time out is a deadlock; a
# leader that exits normally leaving a child in its group is caught
run_control watchdog_control 2 deadlock "" ctrlc.rn
run_control survivor_control 15 survivor "" starve_denied.rn --fork-survivor

python3 "$here/report.py" "$out" > "$out/report.md" || fail=1
[ "$fail" = 0 ] && say "gate 2: all controls held" || say "gate 2: FAILED"
exit $fail
