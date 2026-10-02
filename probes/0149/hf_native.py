"""Record 0149, check 3a as frozen (plans/0149 section 6): native end to
end. For every U7 chat (252 triage, 40 summaries), transformers'
apply_chat_template and tokenizer build the prompt, and generate (F32;
do_sample=False, repetition_penalty=1.0, no temperature/top-p/top-k; EOS
151645 and 151643; the frozen max_new_tokens) produces the ids; they must be
identical to U7's. Any difference is diagnosed at its first differing token
(diagnose.classify): (i) tokenizer/template, (ii) an explained numeric
argmax difference on the same prefix, (iii) unexplained. The 3a outcome is
kept as it is; (iii) also stops the record (the driver checks the artifact).

A completed comparison writes RESULT_JSON and exits 0 (all identical) or 3
(a completed mismatch); anything else is the checker failing.

  hf_native.py MODEL_DIR U7_DIR D2_JSON RANGES LABELS SAMPLE RESULT_JSON"""
import json, sys
import numpy as np, tokenizers, torch, transformers
from transformers import AutoModelForCausalLM, AutoTokenizer
from diagnose import classify
from validate import LIMITS, inputs, template

model_dir, d, d2, ranges, labels, sample, result_path = sys.argv[1:8]
TOL = 1e-3
chats, names = inputs(d2, ranges, labels, sample)
pinned = tokenizers.Tokenizer.from_file(f"{model_dir}/tokenizer.json")
tok = AutoTokenizer.from_pretrained(model_dir)
model = AutoModelForCausalLM.from_pretrained(model_dir, dtype=torch.float32).eval()
rows = {}
for task, fname in (("triage", "u7-triage.tsv"), ("summary", "u7-summaries.tsv")):
    for line in open(f"{d}/{fname}").read().splitlines()[1:]:
        f = line.split("\t")
        rows[(task, f[0])] = [int(x) for x in f[2].split(",")]
results, counts = [], {"identical": 0, "i": 0, "ii": 0, "iii": 0}
with torch.no_grad():
    for task, n, system, user in chats:
        msgs = [{"role": "system", "content": system}, {"role": "user", "content": user}]
        native = tok.apply_chat_template(msgs, add_generation_prompt=True, tokenize=True)
        native = list(native["input_ids"] if hasattr(native, "keys") else native)
        mine = pinned.encode(template(system, user), add_special_tokens=False).ids
        out = model.generate(torch.tensor([native]), attention_mask=torch.ones(1, len(native), dtype=torch.long),
                             do_sample=False, repetition_penalty=1.0, temperature=None, top_p=None, top_k=None,
                             max_new_tokens=LIMITS[task], eos_token_id=[151645, 151643], pad_token_id=151643)
        hf_ids = out[0, len(native):].tolist()
        rnx_ids = rows[(task, n)]

        def logits_at(k):
            seq = torch.tensor([mine + rnx_ids[:k]])
            v = model(seq).logits[0, -1].numpy()
            if not np.isfinite(v).all():
                raise SystemExit(f"FAIL: a non-finite HF logit at {task} #{n} step {k}")
            return v.tolist()

        kind, k = classify(native, mine, hf_ids, rnx_ids, logits_at, TOL)
        counts[kind] += 1
        if kind != "identical":
            results.append({"task": task, "ticket": n, "kind": kind, "first_difference": k,
                            "native_prompt_tokens": len(native), "rnx_prompt_tokens": len(mine),
                            "hf_ids": hf_ids, "rnx_ids": rnx_ids})
            print(f"  {task} #{n}: {kind} at step {k} (HF {hf_ids[:12]}, rnx {rnx_ids[:12]})")
ok = all(r["kind"] == "identical" for r in results) and not results
print(f"transformers {transformers.__version__}, torch {torch.__version__}")
print(f"3a native end to end, {len(chats)} chats: identical {counts['identical']}, (i) tokenizer/template {counts['i']}, "
      f"(ii) explained numeric {counts['ii']}, (iii) unexplained {counts['iii']}")
print("3a: PASS" if ok else "3a: FAIL (kept as it is; diagnosed above)")
json.dump({"check": "3a-native", "completed": True, "result": "PASS" if ok else "FAIL", "chats": len(chats),
           "counts": counts, "differences": results}, open(result_path, "w"), indent=1)
sys.exit(0 if ok else 3)
