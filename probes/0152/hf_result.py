"""Record 0152 (0151's, plus --dims): validates a check's completion before the driver continues
(0148's review lesson). A completed check exits 0 (PASS) or 3 (FAIL) AND
leaves a well-formed artifact naming that check, completed, whose result
agrees with the status; anything else stops the run. With --require-pass,
a completed FAIL also stops it; with --no-unexplained (3a), a completed FAIL
continues only when no difference is unexplained (iii).

With --dims P QUERIES_TSV (0152), the artifact must also record exactly P
passages, the file's query count, P x that count values, and the file's
ordered query ids.

  hf_result.py CHECK RESULT_JSON EXIT_STATUS [--require-pass | --no-unexplained] [--dims P QUERIES_TSV]"""
import json, sys

check, path, status = sys.argv[1:4]
rest = sys.argv[4:]
dims = None
if "--dims" in rest:
    i = rest.index("--dims")
    dims = (int(rest[i + 1]), rest[i + 2])
    del rest[i:i + 3]
mode = rest[0] if rest else None
want = {"0": "PASS", "3": "FAIL"}.get(status)
if want is None:
    sys.exit(f"STOP: {check} exited {status}: the check itself did not complete")
try:
    r = json.load(open(path))
except (OSError, ValueError) as e:
    sys.exit(f"STOP: {check} exited {status} without a well-formed completion artifact ({e!r})")
if not (isinstance(r, dict) and r.get("check") == check and r.get("completed") is True and r.get("result") == want):
    sys.exit(f"STOP: {check} exited {status} with an artifact that does not record a completed {want}: {str(r)[:200]}")
if dims:
    ids = [l.split("\t")[0] for l in open(dims[1]).read().splitlines()[1:] if l]
    got = (r.get("passages"), r.get("queries"), r.get("values"), r.get("query_ids"))
    if got != (dims[0], len(ids), dims[0] * len(ids), ids):
        sys.exit(f"STOP: {check}'s artifact records {got[:3]}{'' if got[3] == ids else ' with other query ids'}; want ({dims[0]}, {len(ids)}, {dims[0] * len(ids)}) and the file's")
if mode == "--require-pass" and want != "PASS":
    sys.exit(f"STOP: {check} completed, FAIL: the record stops here (plans/0152 section 4)")
if mode == "--no-unexplained" and want == "FAIL":
    counts = r.get("counts")
    if not isinstance(counts, dict) or not isinstance(counts.get("iii"), int):
        sys.exit(f"STOP: {check}'s artifact has no diagnosis counts")
    if counts["iii"] > 0:
        sys.exit(f"STOP: {check} has {counts['iii']} unexplained difference(s) (iii): the record stops here")
print(f"{check}: completed, {want} (exit {status})")
