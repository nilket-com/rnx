"""Record 0149 (review round 1, R2): corruptions of the early gate's
artifact. Each must make hf_gate.py stop before loading any model (exit 1,
no completion artifact), so the driver stops before D2 generation; the
unmodified artifact must complete with PASS.

  gate_controls.py MODEL_DIR GATE_JSON HF_PYTHON"""
import json, pathlib, shutil, subprocess, sys, tempfile
import numpy as np

here = pathlib.Path(__file__).parent
model, gate_path, hfpy = sys.argv[1:4]
gate = json.load(open(gate_path))
raw = np.fromfile(gate_path + ".f32", dtype="<f4")
V = 151936


def run(g, r):
    with tempfile.TemporaryDirectory() as t:
        p = pathlib.Path(t, "gate.json")
        p.write_text(json.dumps(g))
        r.astype("<f4").tofile(str(p) + ".f32")
        res = pathlib.Path(t, "result.json")
        out = subprocess.run([hfpy, "hf_gate.py", model, p, res, "gate_chats.json"], cwd=here, capture_output=True, text=True)
        lines = [l for l in (out.stdout + out.stderr).splitlines() if l.startswith(("FAIL", "early gate"))]
        return out.returncode, res.exists(), lines[-1] if lines else "(no message)"


def shorten(g):
    g = json.loads(json.dumps(g))
    c = g[2]
    c["ids"], c["steps"], c["stop"] = c["ids"][:-1], c["steps"] - 1, c["stop"]
    return g


cut = lambda g: [dict(c, logits_at=c["logits_at"] - (V if k > 2 else 0)) for k, c in enumerate(shorten(g))]
cases = {
    "empty": ([], raw[:0]),
    "a subset (four chats)": (gate[:4], raw[:sum(c["steps"] for c in gate[:4]) * V]),
    "a duplicated chat in place of another": ([gate[0], gate[0]] + gate[2:], raw),
    "a wrong chat (the user text changed)": ([dict(gate[0], user=gate[0]["user"] + " ")] + gate[1:], raw),
    "a shortened step count, vectors cut to match": (cut(gate), np.concatenate([raw[:(gate[2]["logits_at"] + (gate[2]["steps"] - 1) * V)], raw[gate[3]["logits_at"]:]])),
    "steps below the ids": ([dict(gate[2], steps=gate[2]["steps"] - 1) if k == 2 else c for k, c in enumerate(gate)], raw),
    "a truncated raw file": (gate, raw[:-1]),
    "an extra vector": (gate, np.concatenate([raw, raw[:V]])),
    "an arbitrary offset": ([dict(c, logits_at=c["logits_at"] + 1) if k == 4 else c for k, c in enumerate(gate)], raw),
    "a wrong vocab": ([dict(gate[0], vocab=V - 1)] + gate[1:], raw),
}
ok = True
for name, (g, r) in cases.items():
    code, art, msg = run(g, r)
    good = code not in (0, 3) and not art
    ok &= good
    print(f"{'stops' if good else 'ACCEPTED'}: {name}: exit {code}; {msg}")
code, art, msg = run(gate, raw)
ok &= code == 0 and art
print(f"{'pass' if code == 0 else 'WRONG'}: unmodified: exit {code}; {msg}")
print("all gate controls behave" if ok else "CONTROLS FAILED")
sys.exit(0 if ok else 1)
