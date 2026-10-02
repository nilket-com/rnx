"""Record 0149, check 3b (plans/0149 section 6): model-level, teacher-forced
on rnx's own prompt and generated ids, for every U7 chat and EVERY step.
transformers' F32 model scores prompt + rnx's ids; at each step k, HF's
logits at rnx's recorded top-5 ids must be finite and within 1e-3 of rnx's,
and HF's argmax must equal rnx's token, except where HF's logit for rnx's
token is within 2 × 1e-3 of HF's maximum (listed). Exit 0 PASS, 3 a
completed FAIL (which stops the record), anything else the checker failing.

  hf_model.py MODEL_DIR U7_DIR D2_JSON RANGES LABELS SAMPLE RESULT_JSON"""
import json, struct, sys
import numpy as np, tokenizers, torch, transformers
from transformers import AutoModelForCausalLM
from validate import inputs, template

model_dir, d, d2, ranges, labels, sample, result_path = sys.argv[1:8]
TOL = 1e-3
chats, names = inputs(d2, ranges, labels, sample)
# controls only: RNX_0149_ONLY="task:ticket,..." restricts the chats checked
import os
if os.environ.get("RNX_0149_ONLY"):
    only = {tuple(x.split(":")) for x in os.environ["RNX_0149_ONLY"].split(",")}
    chats = [c for c in chats if (c[0], c[1]) in only]
pinned = tokenizers.Tokenizer.from_file(f"{model_dir}/tokenizer.json")
model = AutoModelForCausalLM.from_pretrained(model_dir, dtype=torch.float32).eval()
rows = {}
for task, fname in (("triage", "u7-triage.tsv"), ("summary", "u7-summaries.tsv")):
    for line in open(f"{d}/{fname}").read().splitlines()[1:]:
        f = line.split("\t")
        rows[(task, f[0])] = [int(x) for x in f[2].split(",")]
steps = {}
for line in open(f"{d}/u7-steps.tsv").read().splitlines()[1:]:
    f = line.split("\t")
    steps.setdefault((f[0], f[1]), []).append(([int(x) for x in f[6].split(",")],
                                               [struct.unpack("<f", struct.pack("<f", float(x)))[0] for x in f[7].split(",")]))
worst, n_steps, listed, ok = 0.0, 0, [], True
with torch.no_grad():
    for task, n, system, user in chats:
        mine = pinned.encode(template(system, user), add_special_tokens=False).ids
        ids = rows[(task, n)]
        st = steps[(task, n)]
        if len(st) != len(ids):
            sys.exit(f"FAIL: {task} #{n}: {len(st)} steps for {len(ids)} ids")
        logits = model(torch.tensor([mine + ids[:-1]])).logits[0, len(mine) - 1:].numpy()
        if logits.shape != (len(ids), 151936) or not np.isfinite(logits).all():
            print(f"  {task} #{n}: HF logits shape {logits.shape} or non-finite")
            ok = False
            continue
        for k, (top_ids, top_logits) in enumerate(st):
            n_steps += 1
            diff = max(abs(float(logits[k, i]) - x) for i, x in zip(top_ids, top_logits))
            worst = max(worst, diff)
            if diff > TOL:
                ok = False
                listed.append(f"{task} #{n} step {k}: |rnx - HF| {diff:.3g} at the top 5 (bound {TOL})")
            hf_top = int(np.argmax(logits[k]))
            if hf_top != ids[k]:
                explained = float(logits[k, ids[k]]) >= float(logits[k].max()) - 2 * TOL
                listed.append(f"{task} #{n} step {k}: HF argmax {hf_top}, rnx {ids[k]} "
                              f"({'explained' if explained else 'NOT explained'} within 2 x {TOL})")
                ok &= explained
for l in listed:
    print(f"  {l}")
print(f"transformers {transformers.__version__}, torch {torch.__version__}")
print(f"3b teacher-forced, {len(chats)} chats, {n_steps} steps: max |rnx - HF| at the recorded top 5 = {worst:.3g} (bound {TOL}); "
      f"{'every argmax equal' if not any('argmax' in l for l in listed) else 'argmax differences listed'}")
print("3b: PASS" if ok else "3b: FAIL (stops the record)")
json.dump({"check": "3b-model", "completed": True, "result": "PASS" if ok else "FAIL", "chats": len(chats),
           "steps": n_steps, "max_diff": worst, "listed": listed}, open(result_path, "w"), indent=1)
sys.exit(0 if ok else 3)
