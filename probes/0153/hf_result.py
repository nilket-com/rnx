"""Record 0153: validates the semantic check's completion before the driver
continues (0148/0152's pattern). A completed check exits 0 (PASS) or 3
(FAIL) AND leaves a well-formed artifact naming hf-t3, completed, whose
result agrees with the status, and whose counts equal the counts recomputed
here from the validated evidence (tickets, embeddings, cosines, NLI pairs).
Anything else stops the run; with --require-pass a completed FAIL also does.

  hf_result.py RESULT_JSON EXIT_STATUS DIR ISSUES_JSON SPLIT_TSV LABELS_TSV NLI_TOKENIZER_JSON [--require-pass]"""
import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

path, status, d, issues_path, split_path, labels_path, tok_path = sys.argv[1:8]
require = "--require-pass" in sys.argv[8:]
want = {"0": "PASS", "3": "FAIL"}.get(status)
if want is None:
    sys.exit(f"STOP: hf-t3 exited {status}: the check itself did not complete")
try:
    r = json.load(open(path))
except (OSError, ValueError) as e:
    sys.exit(f"STOP: hf-t3 exited {status} without a well-formed completion artifact ({e!r})")
if not (isinstance(r, dict) and r.get("check") == "hf-t3" and r.get("completed") is True and r.get("result") == want):
    sys.exit(f"STOP: hf-t3 exited {status} with an artifact that does not record a completed {want}: {str(r)[:200]}")
from validate import descriptions, labels, nli_tokenizer, split_numbers, validate
issues = {i["number"]: i for i in json.load(open(issues_path))}
numbers = split_numbers(split_path)
names = labels(labels_path)
_, _, _, E, C, L = validate(d, issues, numbers, names, descriptions(labels_path), nli_tokenizer(tok_path))
counts = {"tickets": len(numbers), "embeddings": len(E), "cosines": len(C) * len(names), "nli_pairs": len(L)}
got = {k: r.get(k) for k in counts}
if got != counts:
    sys.exit(f"STOP: hf-t3's artifact records {got}; the validated evidence has {counts}")
if require and want != "PASS":
    sys.exit("STOP: hf-t3 completed, FAIL: the record stops here (plans/0153 section 7)")
print(f"hf-t3: completed, {want} (exit {status}); counts {counts} verified")
