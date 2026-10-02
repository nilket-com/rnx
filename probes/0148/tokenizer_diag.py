"""Record 0148, the tokenizer diagnostics (plans/0148 section 6a). Records
the library versions, the native HF backend's normalizer configuration and
the pinned spm.model's hash; lists every reconstructed pair whose native-HF
ids differ from the pinned tokenizer.json's, with its identity and the
differing ids; proves the original check's disagreement is accounted for:
on every pair whose ids agree, native HF CrossEncoder logits agree with U6
within 1e-4 (shapes, finiteness and identity alignment checked); and
compares the three tokenizations (pinned tokenizer.json, native HF,
SentencePiece) over every distinct text.

  tokenizer_diag.py MODEL_DIR SPM_MODEL U6_DIR D2_JSON RANGES LABELS"""
import hashlib, json, math, struct, sys
from validate import identities, num

SPM_SHA = "c679fbf93643d19aab7ee10c0b99e460bdbc02fedf34b92b05af343b4af586fd"
model_dir, spm_path, d, d2, ranges, labels = sys.argv[1:7]
if hashlib.sha256(open(spm_path, "rb").read()).hexdigest() != SPM_SHA:
    sys.exit("FAIL: spm.model is not the pinned file")
ids, numbers, names = identities(d2, ranges, labels)
import sentencepiece, tokenizers, torch, transformers
from transformers import AutoTokenizer
from sentence_transformers import CrossEncoder, __version__ as st_version

pinned = tokenizers.Tokenizer.from_file(f"{model_dir}/tokenizer.json")
native = AutoTokenizer.from_pretrained(model_dir)
spm = sentencepiece.SentencePieceProcessor(model_file=spm_path)
backend = getattr(native, "backend_tokenizer", None) or getattr(native, "_tokenizer", None)
print(f"versions: tokenizers {tokenizers.__version__}, transformers {transformers.__version__}, "
      f"sentence-transformers {st_version}, sentencepiece {sentencepiece.__version__}, torch {torch.__version__}")
print(f"native HF tokenizer: {type(native).__name__}; backend normalizer {backend.normalizer.__getstate__().decode() if backend else None}")
print(f"pinned tokenizer.json normalizer: {json.dumps(json.load(open(f'{model_dir}/tokenizer.json'))['normalizer'])[:160]}...")
print(f"spm.model: SHA-256 {SPM_SHA}")
differ = []
for r, i in enumerate(ids):
    a, b = pinned.encode(i[3], i[4]).ids, native(i[3], i[4])["input_ids"]
    if a != b:
        at = [k for k in range(max(len(a), len(b))) if k >= min(len(a), len(b)) or a[k] != b[k]]
        differ.append((r, at, [(a[k] if k < len(a) else None, b[k] if k < len(b) else None) for k in at]))
print(f"pairs whose native-HF ids differ from the pinned tokenizer.json's: {len(differ)} of {len(ids)}")
for r, at, pairs in differ:
    print(f"  pair {r + 1} {ids[r][:3]}: positions {at}, (pinned, native) ids {pairs}")
tickets = sorted({ids[r][0] for r, _, _ in differ})
print(f"  tickets: {tickets}")
# the identical-id subset: native HF CrossEncoder against U6
same = [r for r in range(len(ids)) if r not in {x[0] for x in differ}]
rows = open(f"{d}/u6-pairs.tsv").read().splitlines()[1:]
if len(rows) != len(ids):
    sys.exit("FAIL: U6 pairs and identities differ in number")
for r in same:
    f = rows[r].split("\t")
    if (f[0], f[1], f[2]) != (str(ids[r][0]), str(ids[r][1]), ids[r][2]):
        sys.exit(f"FAIL: U6 pair {r + 1} misaligned")
ce = CrossEncoder(model_dir, device="cpu")
hf = ce.predict([(ids[r][3], ids[r][4]) for r in same], batch_size=32, show_progress_bar=False,
                activation_fn=None, convert_to_numpy=True)
if hf.shape != (len(same), 3) or not all(math.isfinite(float(x)) for x in hf.flatten()):
    sys.exit(f"FAIL: native HF outputs shape {hf.shape} or non-finite")
ours = [[struct.unpack("<f", struct.pack("<f", num(x, "logit")))[0] for x in rows[r].split("\t")[3:]] for r in same]
worst = max(abs(a - float(b)) for x, y in zip(ours, hf) for a, b in zip(x, y))
print(f"identical-id subset: {len(same)} pairs; native HF CrossEncoder max |rnx - HF| = {worst:.3g} (bound 1e-4)")
# the three tokenizations, over every distinct text
texts = sorted({i[3] for i in ids} | {i[4] for i in ids})
P = lambda x: pinned.encode(x, add_special_tokens=False).ids
N = lambda x: native(x, add_special_tokens=False)["input_ids"]
S = lambda x: spm.encode(x)
print(f"{len(texts)} distinct texts: pinned == SentencePiece {sum(P(x) == S(x) for x in texts)}, "
      f"native == SentencePiece {sum(N(x) == S(x) for x in texts)}, pinned == native {sum(P(x) == N(x) for x in texts)}")
ok = worst <= 1e-4
print("accounted for: every disagreement is a differing-id pair" if ok else "NOT accounted for")
sys.exit(0 if ok else 1)
