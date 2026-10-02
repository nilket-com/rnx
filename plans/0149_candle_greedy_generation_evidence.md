# rnx 0149 evidence: deterministic text generation (U7), triage and one-line summaries

**The result:**
- **The new type:** `candle::TextGenerator` loads the pinned `Qwen/Qwen2.5-0.5B-Instruct` under a closed contract. It generates greedily with `chat` and, concurrently within the budget, `chat_many`.
- **U7:**
  - triages all 252 tickets in **104 s** (504 tokens generated; 103.8 s in the final run);
  - writes one-line summaries of the 40 sample tickets in **36 s** (850 tokens).
- **Parity, gated:**
  - **the twin:** identical generated ids (1,354), with every top-5 logit equal in f32 bits;
  - **native end to end (3a): PASS,** all 292 chats identical to transformers' own template, tokenizer and `generate`;
  - **teacher-forced (3b): PASS,** within **8.96e-5** (bound 1e-3), every argmax equal;
  - **the early gate:** full logit vectors within **6.3e-5**.
- **Quality, reported as frozen, and poor:**
  - the model answers **"bug" for 244 of the 252 tickets.** Every answer parsed, so coverage is 40 / 40;
  - accuracy is **11 / 40 and 14 / 40** (Claude's and Codex's labels), below 0139 (19 and 20) and 0148 (15 and 16) at full coverage.
  - **Not tuned:** the prompt, parse and limits are as frozen.

## 0. Departures from the plan, in order

1. **The decoded-text bound:**
   - **The plan:** the plan put the bound at `max_new_tokens × L`, with L the longest single-token decode (measured at 128 bytes).
   - **Codex's acceptance:** it asked for a bound that holds for the pinned decoder's **concatenation**.
   - **The implementation:** each request's bound is **3 × the sum of its ids' token-string UTF-8 lengths,** computed from a per-id table before the text is allocated. It's at most `max_new_tokens × 3 × 256`, where 256 is checked at load as the longest token string.
   - **Why it holds:** the decoder is `ByteLevel` (checked exactly at load). It maps each character of a token string to one byte, then decodes UTF-8 lossily, at most one U+FFFD (3 bytes) per invalid byte. The decoded length is then checked against the bound.
2. **A policy the plan didn't state:**
   - **The gap:** the model's vocabulary (151,936) is padded past the tokenizer (151,665).
   - **The policy:** a generated id with no token is refused by name, never dropped from the text. The run produced none.
3. **Two session limits, met before any D2 generation, each worked around in the script without changing a frozen string:**
   - **A multi-line `println!` inside a pasted function submitted early** (F1).
   - **`const` items don't persist between pasted inputs,** so the system prompts became functions returning the same strings (F2).
4. **Timings:** the display reports the total time and tokens for each task, not prefill and decode separately as the plan's section 4 listed. That split isn't measured here.
5. **Launch:** six rounds rather than three, because the first three were noisy (section 6).
6. **Review round 1** (Codex): two blocking fixes and two clarifications. The outputs come from a fresh end-to-end run after them.
   - **R1, a sparse tokenizer panicked the load.**
     - **The cause:** an ordinary id moved into the padded range (150000 → 151900) kept the entry count, but indexing the per-id table panicked.
     - **The fix:** the loader now proves the ids are exactly the dense range 0 .. 151,665 (none at or past it, none shared, no hole), and refuses by name before any table is indexed; the table access is checked as well.
     - **Controls:** Codex's case, on the pinned files, is refused by name, not a panic (`a_sparse_tokenizer_is_refused_not_a_panic`). A shared id that leaves a hole is refused in the fixture.
   - **R2, the early gate wasn't bound to its frozen chats.** An empty artifact passed.
     - **The fix:** `hf_gate.py` now checks the artifact against `gate_chats.json` **before loading any model:**
       - exactly the five chats in order, with their exact system, user and token limit;
       - prompt ids reconstructed with the pinned tokenizer;
       - nonempty ids under the stopping policy;
       - steps equal to the ids, and vocab 151,936;
       - exactly contiguous, complete raw vectors.
     - **On any mismatch it exits 1** (the gate didn't complete), and the driver stops before D2 generation.
     - **Controls** (`gate_controls.py`, `out/gate-controls.txt`): empty, a subset, a duplicate, a wrong chat, a shortened step count with its vectors cut to match, steps below the ids, a truncated raw file, an extra vector, an arbitrary offset and a wrong vocab all stop. The unmodified artifact passes.
   - **(a), the output caps:**
     - **The problem:** `GEN_OUTPUT` covered only the step reservations, about 11.6 MB, though the result ids and top 5 coexist with them and the decoded texts were bounded only per request.
     - **The two caps now:**
       - **`GEN_OUTPUT`** bounds the staged records **and** the results' ids and top 5 together (`n × (2 × max_new + 1)` records, at most 23,113,728 bytes), checked before execution;
       - **`GEN_TEXT` (64 MiB)** bounds the sum of every request's decoded-text bound, checked after generation in a first pass, before any text is allocated.
     - **The load peak**, promised by the plan and missing before, is now measured (`the_load_peak_is_measured`): **2.998 GB at load; 2.010 GB resident after** (F32 weights 1.976 GB, plus the RoPE tables and handles).
   - **(b), mid-decode cleanup:**
     - **The problem:** round 0's failure and panic controls used `execute`'s hooks, which fire before `run_gen` and before any cache exists.
     - **The fix:** new test knobs fail or panic **inside a request after decode step 3**, when its clone's KV cache exists.
     - **The control:** a budget for one request at a time, so the others wait. After each injection: the lowest-index error, `max_running` 1, `inflight` 0 at the end, and identical reuse (`a_mid_decode_failure_or_panic_cleans_up`). The tracked live bytes return exactly to their starting figure after both (`tests/generate_cleanup.rs`).

## 1. The binding (`adapters/candle/src/text/generate.rs`)

**`TextGenerator::load(dir)`:**
- **The configuration is closed:**
  - exactly the production geometry (or the fixture's, in `test-support` builds);
  - the pinned **inactive** sliding window admitted exactly (32768, 21, false; review round 1);
  - `torch_dtype` must be `bfloat16`;
  - `rope_scaling` and quantization are refused by name, as are unknown keys and an untied head.
- **The tokenizer:**
  - exactly 151,665 tokens, every id below `vocab_size`;
  - `<|endoftext|>`, `<|im_start|>` and `<|im_end|>` are special at 151643 to 151645;
  - the decoder and post-processor are exactly `ByteLevel`, which adds no tokens;
  - a behavioural probe;
  - truncation and padding are disabled.
- **The weights:**
  - 290 BF16 tensors; the byte total (988,065,536) is checked against the data section before reading;
  - names, dtypes and shapes are checked;
  - a 1 GiB file bound;
  - converted to F32 once.

**Generation:**
- `chat(system, user, max_new_tokens)` and `chat_many(systems, users, max_new_tokens)` return `#{text, ids, prompt_tokens, stop, top_ids, top_logits}`.
- **The prompt is Qwen's template,** built by rnx; the gates check it.
- **Greedy:** the argmax, with ties to the lowest id, over finite logits only.
- **Stopping:** EOS is included in `ids` and must come last; otherwise generation stops at exactly the limit.

**Resources and lifecycle** (plan section 3a):
- **Checked in order:** the borrowed inputs; each prompt (≤ 1,024 tokens) and the payload; the staged outputs.
- **The shared executor,** now generic over its output record, runs the requests. Each is one planned item with its checked estimate (cited to `qwen2.rs`), and its row is a fixed reservation of `max_new_tokens + 1` records.
- **The model clone is made inside the run function, after admission,** so it's dropped before the permit, including on unwind.

**Controls:**
- **Unit tests** (`generate.rs`, 7, release and debug):
  - **the greedy policy on exact synthetic logits:** each EOS, the length, forced length, the tie to the lowest id, a non-finite logit;
  - **on a tiny generated Qwen2** (BF16, with a one-character ByteLevel BPE tokenizer):
    - generation;
    - `chat_many` equal to isolated and interleaved calls;
    - an injected failure and an injected panic **before** a request runs (`execute`'s hooks), each followed by identical reuse, with nothing left in flight;
    - **review round 1:** an injected failure and an injected panic **after decode step 3** (the KV cache exists), with budget waiters; the tracked live bytes are checked separately;
    - the bounds by name (count, lengths, `max_new_tokens` 0 and 257, a prompt over 1,024 tokens, a budget below one estimate);
    - the closed contract: 10 fixed values, 3 refused keys, the geometry, an absent key, the tokenizer's size, **a shared id (round 1),** decoder and post-processor and a special token, and the weights' bytes, a renamed tensor, a shape, a dtype and an `lm_head`;
    - a FIFO config;
    - the production arithmetic (290 tensors, 988,065,536 bytes).
- **Allocation** (`tests/generate_alloc.rs`): 1,025 chats, 1,024 against 1,023, an over-long text, the combined total, and `max_new_tokens` 0 and 257. Each is refused under 256 KiB.

## 2. Calibration, fail-closed (`tests/generate_model.rs`, the pinned model, `out/calibration.txt`)

- **The setup:**
  - prompts of **exactly** 16, 256 and 1,024 tokens;
  - **forced** decode lengths of 1, 64 and 256 (a test-support knob ignores EOS);
  - each alone, and as four at once.
- **The resident F32 weights,** outside the per-call budget: **1,976,131,072 bytes.**

| prompt | new | alone: peak / estimate | ×4: running at once (admitted) | ×4: peak / admitted in flight |
|---:|---:|---|---:|---|
| 16 | 1 | 2.7 / 4.4 MB (1.65×) | 4 | 9.6 / 17.7 MB (1.84×) |
| 16 | 64 | 4.8 / 8.0 MB (1.65×) | 4 | 18.5 / 32.0 MB (1.73×) |
| 16 | 256 | 11.3 / 18.8 MB (1.67×) | 4 | 42.2 / 75.4 MB (1.78×) |
| 256 | 1 | 25.2 / 61.9 MB (2.46×) | 4 | 93.6 / 247.7 MB (2.65×) |
| 256 | 64 | 25.2 / 65.5 MB (2.60×) | 4 | 98.4 / 262.0 MB (2.66×) |
| 256 | 256 | 25.2 / 76.3 MB (3.03×) | 4 | 94.2 / 305.4 MB (3.24×) |
| 1,024 | 1 | 231.8 / 385.3 MB (1.66×) | **2** | 463.6 / 770.6 MB (1.66×) |
| 1,024 | 64 | 231.8 / 388.8 MB (1.68×) | **2** | 463.6 / 777.7 MB (1.68×) |
| 1,024 | 256 | 231.8 / 399.7 MB (1.72×) | **2** | 461.5 / 799.4 MB (1.73×) |

- **A lowered limit** (one below a 1,024 + 256 request's estimate) is refused after allocating 341 KB, against an estimate of 400 MB, before any clone or cache.
- **The minimum budget:** four 1,024 + 8 chats complete one at a time, with nothing left in flight.

## 3. The early stop gate (before any D2 generation; `out/gate/`)

**The chats:** five hand-written, non-D2 chats (`gate_chats.json`), with prompts of 31 to **1,008** tokens. One stops by length mid-list.

| chat | prompt | template | greedy ids | max \|rnx − HF\|, full vectors |
|---:|---:|---|---|---:|
| 0 | 31 | equal | equal (2: "Paris", EOS) | 6.25e-5 |
| 1 | 47 | equal | equal (2) | 5.25e-5 |
| 2 | 50 | equal | equal (24, length) | 6.29e-5 |
| 3 | 1,008 | equal | equal (24, length) | 4.91e-5 |
| 4 | 31 | equal | equal (6, length) | 5.25e-5 |

All 151,936-wide vectors at every step are finite. **PASS** (bound 1e-3, frozen). The production path (`chat_strs`) gave the same ids as the full-logit path.

## 4. U7 and parity (`probes/0149/run.sh`, `out/run.txt`)

0. **Provenance, first and fatal:**
   - D2 matches 0133's manifest;
   - 0139's frozen files and baseline, 0136's ranges and **0148's U6 table** match;
   - the five model files match their SHA-256s.
   - **Controls** (`out/preflight-controls.txt`, 12 refusals and the unmodified pass), each refused by name with no step begun:
     - D2 edited, a ticket dropped, two reordered;
     - each frozen file changed, the ranges changed, the U6 table changed;
     - a weight byte changed, the generation config changed, the tokenizer missing.
1. **The early gate,** inside the driver: completed, PASS.
2. **The session** (`out/u7-session.txt`):
   - **triage:** 252 tickets, 504 tokens, 104.2 s;
   - **summaries:** 40, 850 tokens, 36.4 s;
   - it shows the class counts and the 40 summaries beside their tickets and triage.
3. **Validation, then parity** (`validate.py`, each producer checked on its own first, plan section 4a):
   - **the rows:** the exact ticket sets and order;
   - **the prompts:** reconstructed from verified inputs and the frozen strings, then tokenized;
   - **the steps:** one per id, with position binding, five distinct in-range ids, descending finite logits with ties by id, and the chosen id equal to the top-1 and to the row's id;
   - **the stop policy;**
   - **the text** as the decode, and **the class** as the parse.
   - **Then compared: IDENTICAL** to `twin0133 u7`, which has its own prompt construction, greedy loop and decoding, run sequentially. That covers 1,354 generated ids and every top-5 id and logit in f32 bits.
   - **Controls** (`out/validate-controls.txt`), all refused:
     - **on both producers:** a missing, an extra, a duplicate and a reordered step; a forged stop; a dropped id; a non-finite logit; a row-level consistent forgery (token, text and parse) whose steps disagree;
     - **on one producer:** one bit of a recorded logit.
     - **A fully consistent forgery, steps included,** is accepted by the validator, as it must be, since only the model can tell. **3b refuses it:** |Δ| 5.37 at the top 5, with the argmax not explained.
4. **3a, native end to end: PASS.** For all 292 chats, transformers' `apply_chat_template` and tokenizer, then `generate` (F32, greedy, `repetition_penalty` 1.0), gave **identical** ids.
   - **The diagnosis counts:** identical 292; (i) 0, (ii) 0, (iii) 0.
   - **The diagnosis itself is tested synthetically** (`out/diagnosis-controls.txt`): identical, a prompt-id difference (i), a near tie after an equal prefix (ii), the same with a clear margin (iii), and rnx stopping short on an equal prefix (iii).
5. **3b, teacher-forced on rnx's ids: PASS.** All 292 chats and 1,354 steps: HF's logits at the recorded top 5 are within **8.96e-5** of rnx's (bound 1e-3), and **every argmax is equal.**
- **Completion is checked for every expected-outcome check** (`hf_result.py`): the artifact must name its check, be completed, and agree with the exit status.
  - The early gate and 3b must pass.
  - 3a may complete with differences only when none is unexplained.
  - **Driver controls** (`out/driver-controls.txt`, 9 scenarios): all-pass and an explained 3a difference continue; an early-gate FAIL, an unexplained 3a difference, a 3a traceback, a 3a artifact without counts, a wrongly named artifact, a 3b FAIL and a 3b status mismatch each stop at the right step.

## 5. Quality, reported and not gated (`evaluate.py`, `out/evaluation.txt`)

**An agent-labelled evaluation, not human or user acceptance.** 40 tickets, so everything here is descriptive only.

| against | measure | U7 (0149) | 0139 | 0148 |
|---|---|---|---|---|
| Claude's labels | coverage | 40 / 40 | 32 / 40 | 4 / 40 |
| | accuracy over assigned | 11 / 40 = 0.28 | 18 / 32 = 0.56 | 4 / 4 |
| | full coverage | **11 / 40** | 19 / 40 | 15 / 40 |
| | recall: question, bug, feature, documentation, performance | 1/9, 9/9, 0/17, 1/4, 0/1 | 0/9, 7/9, 11/17, 0/4, 1/1 | 7/9, 4/9, 1/17, 3/4, 0/1 |
| Codex's labels | full coverage | **14 / 40** | 20 / 40 | 16 / 40 |
| | recall | 1/7, 12/12, 0/16, 1/4, 0/1 | 0/7, 8/12, 11/16, 0/4, 1/1 | 7/7, 5/12, 1/16, 3/4, 0/1 |

- **Pairwise McNemar, at full coverage:**

| against | annotator | both right / U7 only / other only / both wrong | p |
|---|---|---|---:|
| 0139 | Claude | 7 / 4 / 12 / 17 | 0.077 |
| 0139 | Codex | 8 / 6 / 12 / 14 | 0.238 |
| 0148 | Claude | 5 / 6 / 10 / 19 | 0.454 |
| 0148 | Codex | 6 / 8 / 10 / 16 | 0.815 |

- **Over all 252 tickets:** bug 244, feature 3, documentation 3, question 2.
- **What it says:**
  - **The 0.5B instruct model, given the frozen prompt, collapses to the first listed option.** Its confusion table is almost a single "bug" column.
  - **No prompt, label order or model change was tried:** that would be tuning on this sample. A different rule is a new record, on tickets outside it, with fresh annotations.
  - **The summaries** (displayed, not judged) are mostly faithful one-liners, with visible errors: #538's invents "Haskell", and #566's speculates about 2023.

## 6. Findings

- **F1, a session paste limit still present:**
  - **The symptom:** inside a pasted function, a `println!` whose first line ends in its format string and a comma (`println!("…",` then the arguments on the next line) submits at that line ("Expected close delimiter `)`").
  - **History:** 0144 fixed related macro-paste cases, but not this one.
  - **This record's workaround:** U7 builds the line with a template string first. A follow-up session record should take it.
- **F2, `const` items aren't kept between pasted inputs** (`Missing item TRIAGE_SYSTEM`); functions are. A follow-up, or a documented session rule.
- **F3, first-option collapse** (section 5): an observation about this model and prompt, not a defect.
- **F4, padded vocabulary:** 271 model ids have no token, and the binding refuses them by name if generated. None was generated.
- **F5, cost:** U7's 292 generations took 141 s. Most of a triage chat is its prefill (about 350 prompt tokens for a 2-token answer).

## Gates

- **Suites:**

| suite | passed |
|---|---:|
| Candle, default (release) | 97 |
| Candle, `test-support` (release) | 101 |
| unit tests, debug | `generate` 7, `nli` 5, `rerank` 6 |

- **Explicit runs:** the pinned-model gate test and the calibration grid pass.
- **Tooling:** clippy is clean on the touched files, and fmt and `git diff --check` are clean.
- **Launch** (`launch.py`, 0129's method; `rnx-candle` at 0148's `5af908f`), six rounds: +1.59, −0.13, +0.99, −0.28, −1.16 and +1.09 ms. The signs are mixed and the mean is +0.35 ms, against a baseline that itself varied from 7.5 to 11.8 ms. **Within noise.**
- **`:dep candle` and `:dep polars candle`, after push** (a clean worktree binary at the pushed `c9da020`, a fresh private cache; `out/dep/`):
  - **`:dep candle`** (the cold `:dep` took 80.2 s): `dep_candle.rn` regenerates the five gate chats with `chat`. The ids (58 tokens) are **IDENTICAL** to the saved early gate's.
  - **`:dep polars candle`** (the `:dep` took 391.0 s): the full U7 validates against the 292 reconstructed chats and is **IDENTICAL** to the saved twin, covering 1,354 ids and their top-5 logits in f32 bits.
