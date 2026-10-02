"""Record 0152, gate 0: 0151's provenance gate (D1, both models, both
frozen query files, the split), unchanged, then the reused 0151 helpers
pinned byte for byte to their contents at 43711c4 (0151 closed). Nothing
downstream runs unless all of it holds.

  preflight.py D1 MINILM_DIR CE_DIR"""
import hashlib, pathlib, subprocess, sys

HERE = pathlib.Path(__file__).resolve().parent
P0151 = HERE.parent / "0151"
PINNED = {  # sha256 of `git show 43711c4:probes/0151/<file>`
    "bm25.py": "42c3880fcbb1253a786ed7f4ee42ccad5d19811241be0545aeb3d68483bc9832",
    "evaluate.py": "8c1c9bad4a6a04f16ba96b8c7ecc06becb25a3ed9cddc35d53b544641235bda5",
    "validate.py": "e28f066a94f2086712a61d8ae6ca25ae677e55a255827ac95b61137c03196bcd",
    "preflight.py": "9deb10750e5301f52c73e7df58ac3af5880f7713b70917c2cbc05b269ae4dbc0",
    "split.py": "3321434f1f3a9cf6e1c014e851ef35ea9ba36fce79f70cf87be08ad042974136",
}
problems = []
for f, h in PINNED.items():
    p = P0151 / f
    if not p.is_file():
        problems.append(f"0151/{f} missing")
    elif hashlib.sha256(p.read_bytes()).hexdigest() != h:
        problems.append(f"0151/{f} changed since 43711c4")
if problems:
    for p in problems:
        print(f"FAIL: {p}")
    sys.exit(1)
r = subprocess.run([sys.executable, str(P0151 / "preflight.py"), *sys.argv[1:4]])
if r.returncode != 0:
    sys.exit(1)
print(f"pins: {', '.join(PINNED)} match 43711c4")
