"""Record 0154, gate 0: 0153's gate 0 unchanged (the freeze, D3, the split
and folds, D2, the model pins), then 0154's pins: the frozen policy's
artifacts and 0153's scoring and validation code at their 064aa52 contents.

  preflight.py D3_DIR D2_JSON MINILM_DIR NLI_DIR"""
import hashlib, pathlib, subprocess, sys

HERE = pathlib.Path(__file__).resolve().parent
P0153 = HERE.parent / "0153"
PINNED = {  # sha256 at 064aa52 (0153 closed)
    "out/dev/centroids.tsv": "5dd804d3d44297f4e81de9d2ec888c7136c9bd99cf1cf1231172ad2474e7c972",
    "out/dev/selected.json": "022543f0a6855375987b2239a9dfa48b1d9b57d8a4ad81172b285077c4017406",
    "evaluate.py": "3e710af554ef10d6e864f55af5a7012f6ba413781ac158de9fec1a5b278ba825",
    "validate.py": "74c517da7e45fe2568198c69da681dd963f9ed89c7ae8c1053d2214806ae1633",
}
problems = []
for f, h in PINNED.items():
    p = P0153 / f
    if not p.is_file():
        problems.append(f"FAIL: 0153/{f} missing")
    elif hashlib.sha256(p.read_bytes()).hexdigest() != h:
        problems.append(f"FAIL: 0153/{f} changed since 064aa52")
if problems:
    print("\n".join(problems))
    sys.exit(1)
r = subprocess.run([sys.executable, str(P0153 / "preflight.py"), *sys.argv[1:5]])
if r.returncode != 0:
    sys.exit(1)
print(f"pins: 0153's {', '.join(PINNED)} match 064aa52")
