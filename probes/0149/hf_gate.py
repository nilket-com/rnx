"""Record 0149: the early semantic stop gate, before any D2 generation.
On the frozen non-D2 gate chats:
- the template: rnx's prompt ids equal transformers' apply_chat_template
  (add_generation_prompt=True) ids;
- greedy: transformers' generate (F32; do_sample=False,
  repetition_penalty=1.0, no temperature/top-p/top-k; EOS 151645 and
  151643; the same max_new_tokens) produces exactly rnx's ids;
- logits: teacher-forced on rnx's prompt and generated ids, HF's complete
  logit vector at every step is finite and within 1e-3 of rnx's.
A failure stops the record and is reported; the tolerance is not loosened.
Exit 0 PASS, 3 a completed FAIL (with RESULT_JSON); anything else is the
checker failing.

Review round 1 (R2): before any model is loaded, the artifact is bound to
the frozen gate_chats.json: exactly its five chats in order, with their
exact system, user and token limit; prompt ids reconstructed with the
pinned tokenizer; nonempty generated ids under the stopping policy; steps
equal to the ids; vocab 151,936; and the raw vectors exactly contiguous,
non-overlapping and complete (no truncation, extra or missing vectors, or
arbitrary offsets). Anything else exits 1: the gate did not complete.

  hf_gate.py MODEL_DIR RNX_GATE_JSON RESULT_JSON GATE_CHATS_JSON"""
import json, math, sys
import numpy as np, tokenizers

model_dir, gate_path, result_path, frozen_path = sys.argv[1:5]
TOL = 1e-3
VOCAB = 151936
EOS = (151645, 151643)


def bad(msg):
    sys.exit(f"FAIL (the gate did not complete): {msg}")


frozen = json.load(open(frozen_path))
try:
    gate = json.load(open(gate_path))
    raw = np.fromfile(gate_path + ".f32", dtype="<f4")
except (OSError, ValueError) as e:
    bad(f"the artifact is unreadable ({e!r})")
pinned = tokenizers.Tokenizer.from_file(f"{model_dir}/tokenizer.json")
if not isinstance(gate, list) or len(gate) != len(frozen["chats"]):
    bad(f"{len(gate) if isinstance(gate, list) else 'no'} chats, want exactly the {len(frozen['chats'])} frozen ones")
at = 0
for k, (c, f, m) in enumerate(zip(gate, frozen["chats"], frozen["max_new_tokens"])):
    if not isinstance(c, dict) or (c.get("system"), c.get("user"), c.get("max_new_tokens")) != (f["system"], f["user"], m):
        bad(f"chat {k} is not frozen chat {k} (its system, user or token limit differs)")
    want = pinned.encode(f"<|im_start|>system\n{f['system']}<|im_end|>\n<|im_start|>user\n{f['user']}<|im_end|>\n<|im_start|>assistant\n",
                         add_special_tokens=False).ids
    if c.get("prompt_ids") != want:
        bad(f"chat {k}: the prompt ids are not the pinned tokenizer's")
    ids = c.get("ids")
    if not isinstance(ids, list) or not ids:
        bad(f"chat {k}: no generated ids")
    eos_stop = ids[-1] in EOS and not any(i in EOS for i in ids[:-1]) and len(ids) <= m
    length_stop = len(ids) == m and not any(i in EOS for i in ids)
    if not (eos_stop or length_stop) or c.get("stop") != ("eos" if eos_stop else "length"):
        bad(f"chat {k}: the ids and stop {c.get('stop')!r} break the stopping policy")
    if c.get("steps") != len(ids) or c.get("vocab") != VOCAB:
        bad(f"chat {k}: {c.get('steps')} steps x {c.get('vocab')} for {len(ids)} ids, want {len(ids)} x {VOCAB}")
    if c.get("logits_at") != at:
        bad(f"chat {k}: its vectors start at {c.get('logits_at')}, want {at} (contiguous, non-overlapping)")
    at += len(ids) * VOCAB
if raw.size != at:
    bad(f"the raw vectors hold {raw.size} values, want exactly {at}")
print(f"artifact bound to the {len(gate)} frozen chats: prompts, ids, stops, steps and {at} contiguous values")
import torch, transformers
from transformers import AutoModelForCausalLM, AutoTokenizer
tok = AutoTokenizer.from_pretrained(model_dir)
model = AutoModelForCausalLM.from_pretrained(model_dir, torch_dtype=torch.float32).eval()
ok, rows = True, []
with torch.no_grad():
    for k, c in enumerate(gate):
        msgs = [{"role": "system", "content": c["system"]}, {"role": "user", "content": c["user"]}]
        native = tok.apply_chat_template(msgs, add_generation_prompt=True, tokenize=True)
        if hasattr(native, "input_ids"):
            native = native["input_ids"]
        template = list(native) == c["prompt_ids"]
        gen = model.generate(torch.tensor([c["prompt_ids"]]), attention_mask=torch.ones(1, len(c["prompt_ids"]), dtype=torch.long),
                             do_sample=False, repetition_penalty=1.0, temperature=None, top_p=None, top_k=None,
                             max_new_tokens=c["max_new_tokens"], eos_token_id=[151645, 151643], pad_token_id=151643)
        hf_ids = gen[0, len(c["prompt_ids"]):].tolist()
        greedy = hf_ids == c["ids"]
        seq = torch.tensor([c["prompt_ids"] + c["ids"][:-1]]) if len(c["ids"]) > 1 else torch.tensor([c["prompt_ids"]])
        logits = model(seq).logits[0, len(c["prompt_ids"]) - 1:].numpy()
        ours = raw[c["logits_at"]:c["logits_at"] + c["steps"] * c["vocab"]].reshape(c["steps"], c["vocab"])
        finite = bool(np.isfinite(logits).all() and np.isfinite(ours).all())
        worst = float(np.abs(logits[:c["steps"]] - ours).max())
        good = template and greedy and finite and worst <= TOL
        ok &= good
        rows.append({"chat": k, "prompt_tokens": len(c["prompt_ids"]), "template": template, "greedy": greedy,
                     "finite": finite, "max_logit_diff": worst, "steps": c["steps"]})
        print(f"chat {k}: {len(c['prompt_ids'])} prompt tokens, template {'equal' if template else 'DIFFERS'}, "
              f"greedy ids {'equal' if greedy else 'DIFFER ' + str(hf_ids)}, {c['steps']} steps x {c['vocab']} logits, "
              f"max |rnx - HF| {worst:.3g} (bound {TOL}){'' if finite else ', NON-FINITE'}")
print(f"transformers {transformers.__version__}, torch {torch.__version__}")
print("early gate: PASS" if ok else "early gate: FAIL (stop: report it)")
json.dump({"check": "early-gate", "completed": True, "result": "PASS" if ok else "FAIL", "chats": rows}, open(result_path, "w"), indent=1)
sys.exit(0 if ok else 3)
