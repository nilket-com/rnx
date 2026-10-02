"""Record 0148 (review round 1, R1): validates the frozen HF check's
completion before the driver continues. A completed comparison exits 0
(PASS) or 3 (FAIL) AND leaves a well-formed artifact whose result agrees
with that status; anything else is the checker failing, and stops the run.

  hf_result.py RESULT_JSON EXIT_STATUS"""
import json, sys

path, status = sys.argv[1], sys.argv[2]
want = {"0": "PASS", "3": "FAIL"}.get(status)
if want is None:
    sys.exit(f"STOP: the frozen HF check exited {status}: the check itself did not complete")
try:
    r = json.load(open(path))
except (OSError, ValueError) as e:
    sys.exit(f"STOP: exit {status} without a well-formed completion artifact ({e!r})")
if not (isinstance(r, dict) and r.get("check") == "hf-frozen" and r.get("completed") is True
        and r.get("result") == want and isinstance(r.get("pairs"), int)):
    sys.exit(f"STOP: exit {status} with an artifact that does not record a completed {want}: {str(r)[:200]}")
print(f"gate 3 as frozen (native HF CrossEncoder): completed, {want} (exit {status})"
      + (" - recorded; plan section 6a" if want == "FAIL" else ""))
