# rnx 0148 evidence: NLI classification for ticket triage (U6), against 0139's frozen sample

**The result:**
- **The new type:** `candle::NliModel` loads the pinned `cross-encoder/nli-deberta-v3-xsmall` under a closed geometry. It returns (contradiction, entailment, neutral) logits for (premise, hypothesis) pairs.
- **U6:** triages all 252 tickets by the rule frozen in plan section 4. The NLI scoring of 2,290 pairs takes **107 s** in the final run (110 s in the earlier one).
- **Parity, gated, reported apart from quality:**
  - **BIT-EQUAL** to an independent Rust twin, after each producer is validated against 2,290 identities reconstructed from the verified inputs;
  - **the frozen native-HF check FAILED** (max logit difference 0.188; ticket 975's decision), and stays recorded as failed;
  - **the cause is tokenization:** 15 pairs tokenize differently in transformers 5.18 than under the pinned `tokenizer.json`;
  - **the amended model-level check passes:** PyTorch's DeBERTa on the pinned ids agrees within **1.33e-5** on all 2,290 pairs, with identical decisions (plan 6a).
- **Quality, reported and not gated, as frozen:**
  - **The frozen rule abstains almost always:** coverage **4 / 40,** with all 4 correct; 236 of the 252 tickets go to review.
  - **At full coverage, NLI scores lower than 0139:** 15 and 16 of 40, against 19 and 20.
  - **NLI finds questions** (7 / 9 and 7 / 7, where 0139 found none), but reads feature requests as questions (14 of 17).
  - **Paired McNemar p is 0.57 and 0.59:** no established difference. Nothing is tuned on this.

## 0. Plan corrections and amendments, in order

1. **Before any score, a contract wording correction** (sent to Codex during implementation):
   - **The plan said** `vocab_size` equals the tokenizer's size.
   - **The pinned files say otherwise:** the tokenizer has 128,001 entries (ids 0 to 128,000; `[MASK]` is the last), while `config.json` pads its vocabulary to 128,100.
   - **As written, the contract would have refused the pinned model.** The implementation uses 0146's rule instead: every tokenizer id is below `vocab_size`, and the closed values are the tokenizer's exact size of 128,001 with `[PAD]` 0, `[CLS]` 1 and `[SEP]` 2.
2. **During the first run, a validator fix:**
   - **The problem:** the twin prints its f32 logits in their shortest round-trip form, the script as their exact f64 values. My first check required the exact form and stopped the run.
   - **The fix:** a logit is now read as the f32 its text rounds to, and the recompute uses that exact value, as the runtime does. The tolerance didn't change.
3. **After the frozen full HF check failed, the stop-rule amendment** (plan section 6a, agreed with Codex): see section 4 here.
   - **Nothing in the evaluation rule changed:** the hypotheses, aggregation, f64 softmax, 0.5 threshold, tie rule, sample, tolerance and quality comparison are all as frozen.
4. **Review round 1** (Codex), two gate fixes; the outputs come from a fresh run after them:
   - **R1, the frozen check's completion:** the driver had accepted any exit 1 as the recorded failure, but a traceback also exits 1.
     - **The fix:** `hf_full.py` now writes a completion artifact, `hf-frozen.json`, and exits 0 (PASS) or 3 (a completed mismatch, FAIL). `hf_result.py` requires the artifact to be well formed, completed, and to agree with the status before the driver continues; anything else stops it.
     - **Controls** (`driver_controls.sh`, `out/driver-controls.txt`):
       - a completed PASS and a completed FAIL each continue with their true label;
       - a silent exit 1, a traceback exit 1, exit 3 with no artifact, a malformed artifact, an incomplete artifact, exit 0 with a FAIL artifact, and exit 2 each stop before any later step.
     - **The actual frozen result is preserved:** completed, FAIL.
   - **R2, decisions at a boundary:** one "near" flag had let a producer choose any labels.
     - **The fix:**
       - each producer's decisions must follow its **own** retained f64 scores exactly (ties to `labels.tsv` order, below 0.5 to review, exactly 0.5 assigned);
       - its scores must be within 1e-12 of the recompute;
       - any remaining difference from the recomputed decisions is allowed only where a perturbation of the recomputed scores within 1e-12 would yield it (`validate.explained`), and is named.
       - **The same explainability test,** at 1e-4, replaces "near" in `hf_full.py` and `hf_model.py`.
     - **Controls** (`validate.py --controls`, `out/decision-controls.txt`):
       - exactly 0.5 (bug's entailment from logits [0, 0, −1000]) is assigned, and **Codex's forged row** (question / question / documentation at that boundary) is refused;
       - just below 0.5 abstains, and assigning it anyway is refused;
       - an exact tie goes to the first label, and giving it to the second is refused;
       - **a genuine rounding case** (the recompute 1e-13 below 0.5, the producer's own score exactly 0.5, assigned) is accepted and named;
       - an unrelated label at that ambiguity is refused.

## 1. The binding (`adapters/candle/src/text/nli.rs`)

**`NliModel::load(dir)`:**
- **The closed contract:**
  - `config.json` must hold exactly the production geometry (or, only in a `test-support` build, the tiny fixture's), and exactly the fixed flags;
  - `attention_head_size`, `embedding_size`, `conv_*`, `cls_dropout` and any unknown key are refused by name;
  - metadata is ignored, but `torch_dtype` must be `float32`;
  - the sentence-transformers activation, if present, must be the identity.
- **The tokenizer:**
  - 0146's pair contract, now shared (`rerank::pair_tokenizer`): the pair template, special tokens defined as exactly themselves, a complete probe pair, truncation and padding disabled, every id below `vocab_size`;
  - plus the exact tokenizer size and special ids.
- **The weights, checked before any tensor is read:**
  - the 203 expected tensors and their shapes are derived from the geometry with checked arithmetic;
  - their byte total must equal the file's data section (283,328,524 bytes for the pinned file).
- **Then by name:** each key, dtype and shape, and the I64 `position_ids` holding 0 to 511.
- **Loading:** the encoder loads under `deberta.`; Candle takes the pooler and classifier from the root.

**`nli.score(premises, hypotheses)`:**
- **Returns** a `Dense` of N × 3 logits named `contradiction`, `entailment`, `neutral`; `nli.labels()` returns the same names.
- **Shared with `CrossEncoder` (factored, its six tests unchanged):** the borrowed input checks before any proportional allocation, the S1 planner (`rerank::plan_pairs`), and `execute`.
- **No token types:** `type_vocab_size` is 0, so none are passed.
- **The memory estimate** (`estimate_nli`) is a checked sum, each term cited to its lines in candle-transformers 0.11's `debertav2.rs`:
  - the relative-position build;
  - the 4-D mask;
  - per layer, q/k/v and their head-major copies, the [B, s, s] tensors (scores, c2p's and p2c's gathers and indices, XSoftmax's masks), and the two [B, s, P] pre-gather products;
  - the relative projections and their repeats;
  - the feed-forward;
  - the head.
  - **Every tensor a layer creates is charged as live at once,** so it's an upper bound.
- **The activation caps** add `B × s × P` to `embed`'s three.

**Controls:**
- **Unit tests** (`nli.rs`, 5, release and debug; a tiny generated DeBERTa-v2 in the fixture geometry):
  - pairs score, and a pair alone matches its padded row within 1e-5;
  - refusals by name (unequal, empty, non-string, 32 tokens accepted and 33 refused by index);
  - **the closed contract, one refusal per item:** 15 fixed values plus `torch_dtype` and the activation, 6 refused keys, 3 geometry changes, an absent key, the tokenizer's size, the template, a multi-id `[CLS]`, a missing and an extra tensor (by the byte check), a renamed tensor, a transposed classifier, an F16 bias, a U32 buffer of equal bytes, a zeroed buffer;
  - a FIFO config refused at once;
  - the production geometry's arithmetic equal to the pinned file's (203 tensors, 283,328,524 bytes).
- **Allocation** (`tests/nli_alloc.rs`): 32,769 pairs; 32,768 against 32,767; an over-long hypothesis; and per-text-valid inputs over the combined total. Each is refused under 256 KiB.

## 2. Calibration, fail-closed (`tests/nli_model.rs`, the pinned model, `out/calibration.txt`)

| cell | planner's shapes | peak | admitted estimate | headroom |
|---|---|---:|---:|---:|
| 16 tokens, 1 pair | (1, 16) | 4.3 MB | 7.5 MB | 1.74× |
| 16 tokens, 8 pairs | (8, 16) | 17.2 MB | 26.9 MB | 1.56× |
| 16 tokens, 8 × 32 at once | 29 × 8, then 6, 9, 9 (the S1 tail), all at 16 | 242.6 MB | 859.8 MB in flight; **32 ran at once** | 3.54× |
| 128 tokens, 1 pair | (1, 128) | 6.8 MB | 24.8 MB | 3.64× |
| 128 tokens, 32 at once | 32 × 1 | 131.4 MB | 795.0 MB in flight; **32 ran at once** | 6.05× |
| 512 tokens, 1 pair | (1, 512) | 47.5 MB | 199.2 MB | 4.20× |
| 512 tokens, 32 at once | 32 × 1 | 220.9 MB | 996.2 MB in flight; **the budget admitted 5 at once** | 4.51× |

- **Overlap is what ran:** the "at once" numbers are `max_running`, the overlap the budget actually admitted, as Codex asked; requesting workers doesn't count.
- **Named skips:**
  - 16 tokens × 32 single pairs: the planner groups them into eights, so it's not a shape it forms;
  - the "largest batch" cell at 128 and 512 tokens: only single pairs are admitted there.
- **The S1 tail:** one call can't form 32 batches of exactly 8, because the tail halves from what remains. The concurrent cell reports the shapes it formed and bounds the peak by the estimates actually admitted.
- **A lowered limit** (one below a 512-token pair's estimate) is refused by the planner after allocating 181 KB, against an estimate of 199 MB.
- **The minimum admissible budget:** four 512-token pairs complete one at a time, with every permit released.

## 3. The early semantic stop gate (before any D2 score; `out/early-gate.txt`)

- **The pairs:** six hand-written, non-D2 pairs (`gate_pairs.json`) of 18 to 344 tokens.
- **The result:** scored as one padded batch and each alone, Candle and Hugging Face agree within **4.77e-6** on every logit, all finite. **PASS.**

## 4. U6 and parity (`probes/0148/run.sh`, `out/run.txt`)

0. **Provenance, first and fatal:**
   - D2 matches 0133's manifest (252);
   - 0139's frozen labels, rules, sample and two annotations, its baseline table, and 0136's ranges match plan section 1;
   - the NLI model matches its three pinned SHA-256s.
   - **Controls** (`out/preflight-controls.txt`), each refused by name with the sentinel showing no step began:
     - a ticket's body changed, a ticket removed, two reordered;
     - each of 0139's files changed (labels, sample, an annotation, the baseline) and the ranges changed;
     - a weight byte changed, the tokenizer missing.
     - The unmodified inputs reach all seven steps.
1. **The session** (`out/u6-session.txt`, runner0134, `repl`):
   - 252 tickets, 2,290 pairs, **NLI scoring 107.2 s** in the final run (110.1 s and 107.1 s in the two earlier runs);
   - it shows the class counts, the review queue and each label's most confident tickets.
   - **All three runs produced byte-identical pairs and tables.**
2. **Validation, then parity** (`validate.py`): each producer is checked on its own:
   - **the identities:** 2,290, in the order the validator reconstructs from verified D2, the ranges and the labels;
   - **the logits:** finite, each read as the f32 its text rounds to;
   - **the ticket tables:** recomputed in Python f64, each `s_label` within 1e-12, and the decisions identical. None was within 1e-12 of a boundary.
   - **Then compared: BIT-EQUAL** to `twin0133 u6`, every logit in f32 bits and every ticket row (s_label in f64 bits).
     - **The twin** has its own pair encoding, partition (the same 1,896 × 1, 149 × 2 and 24 × 4 batches), f64 softmax and rule.
     - **Its limit, stated:** it uses the same Candle DeBERTa code, so it checks rnx's binding, partition and rule, not Candle's model.
   - **Controls** (`out/validate-controls.txt`), all refused:
     - **on both producers:** a passage swapped, a ticket swapped, a label swapped, a missing, an extra and a duplicate identity, and a class with its scores forged consistently;
     - **on one producer:** one bit of a logit in a pair that isn't its ticket's maximum, so the tables are unchanged.
3. **Gate 3 as frozen: FAIL** (`hf_full.py`, a completed mismatch, exit 3, with its artifact `out/hf-frozen.json`; kept as the record of the frozen check):
   - **The check:** sentence-transformers 6.1.0's `CrossEncoder` with its native tokenizer (transformers 5.18.0), on the 2,290 reconstructed pairs.
   - **The result:**
     - the **max |rnx − HF| logit is 0.188** (bound 1e-4);
     - **ticket 975** is `question` for HF (best 0.5204) but `review` for U6;
     - ticket 771 is within 1e-4 of its second label, with decisions that agree.
   - **HF `predict` takes 124.6 s** (median of 126.8, 123.4, 124.6).
   - **The listing:** ticket 975's difference is "NOT explained within 1e-4" by `validate.explained`.
4. **The tokenizer diagnostics** (`tokenizer_diag.py`, plan 6a):
   - **The versions:** tokenizers 0.23.2, transformers 5.18.0, sentence-transformers 6.1.0, sentencepiece 0.2.2, torch 2.14.1+cpu.
   - **Native HF's tokenizer** is `DebertaV2Tokenizer`, with a rebuilt normalizer: `Replace(\s{2,}|[\n\r\t] → " ")`, then `NFC`, then a right `Strip`.
   - **The pinned `tokenizer.json`** normalizes with `Strip`, then the `Precompiled` charsmap carried from SentencePiece.
   - **`spm.model`** is pinned by SHA-256 `c679fbf9…86fd`.
   - **15 of 2,290 pairs** have different native-HF ids, each listed with its identity and differing ids:
     - ticket 200, passage 3 (×5 labels): "µ" in "231.437µs" is `μ` (13158) under the pinned file and `[UNK]` (3) natively;
     - ticket 344, passage 2 (×5): a run of differences from position 177 on;
     - ticket 975, passage 0 (×5): differences from position 221 on.
     - **None of the three tickets is in the 40-ticket sample.**
   - **Accounted for:** on the 2,275 pairs whose ids agree, native HF agrees with U6 within **1.2e-5** (shapes, finiteness and alignment checked). Every disagreement is a differing-id pair.
   - **The three tokenizations, over the 463 distinct texts:**
     - the pinned file equals SentencePiece on 459;
     - native HF equals SentencePiece on 456;
     - the pinned file equals native HF on 460.
     - On "µ", SentencePiece gives `μ`, as the pinned file does. Where the pinned file differs from SentencePiece, it gives `[UNK]` for box-drawing characters and an emoji, which SentencePiece byte-encodes.
   - **So rnx follows the pinned `tokenizer.json`.** It doesn't fully match native HF or SentencePiece.
5. **Gate 3 as amended: PASS** (`hf_model.py`):
   - **The check:** transformers' PyTorch DeBERTa, fed the pinned tokenizer's exact ids, masks and pair order, one pair at a time, for all 2,290 reconstructed inputs.
   - **The result:**
     - every output is shaped (1, 3), finite and aligned;
     - max |rnx − HF model| is **1.33e-5** (bound 1e-4);
     - **every decision is identical** (ticket 771 is listed as within 1e-4 of its second label, and agrees).
   - **This checks Candle's DeBERTa semantics.** Native tokenization remains a disclosed end-to-end difference.
- **The statuses are checked separately:** `run.sh` validates the frozen check's completion (exit 3 with a completed FAIL artifact, after review round 1), stops on anything else, and then requires the diagnostics and the model-level check to pass.
- **Launch** (`launch.py`, 0129's method; `rnx-candle` at 0146's `240fa54`): deltas of −0.24, +0.24 and −0.57 ms. **Within noise.**

## 5. Quality, reported and not gated (`evaluate.py`, `out/evaluation.txt`)

**An agent-labelled evaluation, not human or user acceptance.** 40 tickets, so everything here is descriptive only.

| against | measure | NLI (0148) | 0139 baseline |
|---|---|---|---|
| Claude's labels | coverage | **4 / 40** | 32 / 40 |
| | accuracy over assigned | 4 / 4 | 18 / 32 = 0.56 |
| | full coverage, no abstention | 15 / 40 = 0.38 | 19 / 40 = 0.47 |
| | recall at full coverage: question, bug, feature, documentation, performance | **7/9**, 4/9, 1/17, 3/4, 0/1 | 0/9, 7/9, 11/17, 0/4, 1/1 |
| Codex's labels | coverage | **4 / 40** | 32 / 40 |
| | accuracy over assigned | 4 / 4 | 18 / 32 = 0.56 |
| | full coverage, no abstention | 16 / 40 = 0.40 | 20 / 40 = 0.50 |
| | recall at full coverage | **7/7**, 5/12, 1/16, 3/4, 0/1 | 0/7, 8/12, 11/16, 0/4, 1/1 |

- **Paired, at full coverage:**
  - by Claude's labels: both right 3, NLI only 12, 0139 only 16, both wrong 9; exact McNemar p = 0.572;
  - by Codex's labels: 3, 13, 17 and 7; p = 0.585.
- **Confusion at full coverage** (both annotators, in `evaluation.txt`): NLI reads **feature requests as questions** (14 of 17 by Claude's labels, 13 of 16 by Codex's) and some bugs as questions or documentation. 0139 read questions as features.
- **Abstention:**
  - **the frozen 0.5 threshold assigns only 16 of the 252 tickets:** question 8, bug 5, documentation 2, feature 1;
  - in the sample, the four assigned (one bug, three questions) are all correct.
- **What it says:**
  - The model ranks the verbatim descriptions very differently from 0139's embedding similarity: it detects asking-for-help intent, and confuses requests with questions.
  - Its entailment scores for these descriptions are mostly far below 0.5, so the frozen rule abstains on nearly everything.
  - **Neither order of results establishes that one approach is better** on this sample.
  - **No hypothesis wording, aggregation or threshold was changed after these scores.** A different rule would be a new record, on tickets outside this sample, with fresh annotations.

## 6. Findings

- **F1, the tokenizer stacks disagree:** the model's own SentencePiece file, the pinned `tokenizer.json` derived from it, and transformers 5.18's native tokenizer differ on a handful of texts with unusual characters (section 4.4). A native-HF comparison of DeBERTa-v3 models must therefore separate tokenization from the model. The amended gate does that.
- **F2, verbatim descriptions as hypotheses** give low entailment scores. The frozen 0.5 threshold, chosen before any score, covers 4 of 40.
- **F3, the S1 tail:** one call can't form uniform concurrent batches (section 2). The calibration reports the shapes formed.
- **F4, cost:** on the same 2,290 pairs, rnx's NLI scoring took 107 to 110 s, against HF `predict`'s 125 s. HF used batches of 32 with padding; rnx used S1's mostly single-pair batches, concurrently.

## Gates

- **Suites:**

| suite | passed |
|---|---:|
| Candle, default (release) | 90 |
| Candle, `test-support` (release) | 92 |
| `nli` unit tests, debug | 5 |
| `rerank` unit tests, debug (unchanged after the factoring) | 6 |

- **Explicit runs:** the pinned-model gate test and the calibration grid (`#[ignore]`, run explicitly) pass.
- **Tooling:** clippy is clean on the touched files, and fmt and `git diff --check` are clean.
- **The twin** (`probes/0133/twin`) builds with `u6`; its fmt drift predates this record.
- **`:dep candle` and `:dep polars candle`, after push** (a clean worktree binary at the pushed `11fbd3f`, a fresh private cache; `out/dep/`):
  - **`:dep candle`** (the cold `:dep` took 79.4 s): `dep_candle.rn` loads `NliModel` alone and scores the six early-gate pairs one at a time. All 18 logits are BIT-EQUAL in f32 bits to the saved early gate.
  - **`:dep polars candle`** (the `:dep` took 386.8 s): the full U6 validates against the 2,290 reconstructed identities, with the recompute within 1e-12, and is BIT-EQUAL to the saved twin.
- **Two claims, kept distinct:**
  - **the native-HF end-to-end check, as frozen, FAILED;**
  - **the pinned-tokenizer model-level check PASSED.**
