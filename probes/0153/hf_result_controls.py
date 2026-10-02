"""Record 0153: controls for hf_result.py (the semantic check's completion
validation). An exception or a malformed, missing, mismatched or miscounted
artifact never counts as a completed comparison.

  hf_result_controls.py GOOD_ARTIFACT DIR ISSUES_JSON SPLIT_TSV LABELS_TSV NLI_TOKENIZER_JSON"""
import json, pathlib, subprocess, sys, tempfile

here = pathlib.Path(__file__).parent
good_path, d, issues, split, labels, tok = sys.argv[1:7]
good = json.load(open(good_path))
cases = {
    "the well-formed PASS artifact": (good, "0", True),
    "a missing artifact": (None, "0", False),
    "a malformed artifact": ("{not json", "0", False),
    "another check's artifact": ({**good, "check": "hf-pool"}, "0", False),
    "not completed": ({**good, "completed": False}, "0", False),
    "PASS recorded with exit 3": (good, "3", False),
    "an exception (exit 1) with a PASS artifact": (good, "1", False),
    "a completed FAIL": ({**good, "result": "FAIL"}, "3", False),
    "the wrong ticket count": ({**good, "tickets": good["tickets"] - 1}, "0", False),
    "the wrong NLI pair count": ({**good, "nli_pairs": good["nli_pairs"] + 1}, "0", False),
    "the wrong cosine count": ({**good, "cosines": good["cosines"] - 5}, "0", False),
    "a count missing": ({k: v for k, v in good.items() if k != "embeddings"}, "0", False),
}
ok = True
with tempfile.TemporaryDirectory() as t:
    for name, (art, status, want) in cases.items():
        p = pathlib.Path(t, "a.json")
        if p.exists():
            p.unlink()
        if isinstance(art, dict):
            p.write_text(json.dumps(art))
        elif isinstance(art, str):
            p.write_text(art)
        r = subprocess.run([sys.executable, here / "hf_result.py", p, status, d, issues, split, labels, tok, "--require-pass"],
                           capture_output=True, text=True)
        line = (r.stdout + r.stderr).strip().splitlines()[-1]
        good_case = (r.returncode == 0) == want
        ok &= good_case
        print(f"{('pass' if want else 'refused') if good_case else 'WRONG'}: {name}: {line[:150]}")
print("all completion-artifact controls behave" if ok else "CONTROLS FAILED")
sys.exit(0 if ok else 1)
