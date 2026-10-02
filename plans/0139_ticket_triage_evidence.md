# rnx 0139 evidence: zero-shot triage of support tickets (U4)

**The result:**
- **The deliverable:** U4 runs as a session paste and displays the triage summary, a 20-ticket review queue and per-label examples for the project's 252 support tickets. It takes 8.4 s in total; the embedding and scoring take 5.9 s (the saved run, `out/session-display.txt`).
- **Parity:** its complete per-ticket table is **bit-equal** to a direct-Rust twin.
- **Quality, reported and not gated:** on the frozen 40-ticket sample, coverage is 32 of 40 and accuracy over assigned tickets 18 of 32 (0.56), against each of two agent annotators.
  - **The clear failure:** "question" is never predicted. Questions are read as feature requests.
  - **Not an rnx artifact:** PyTorch's sentence-transformers, on the same passages, pooling and descriptions, predicts exactly the same per-label counts before abstention, also with no questions.
- **No new tensor API was needed.**

## 1. The freeze (`probes/0139/frozen/`, committed in the plan commit `c9d69d5` at 2026-10-01T19:30:21, before any score)

- **What's frozen:**
  - the five labels and their one-line descriptions (`labels.tsv`);
  - the annotation rules with tie-breaks (`rules.md`);
  - the sample: 40 ticket numbers, every 6th of the sorted 252 from index 0, the first 40 (`sample.tsv`);
  - the margin rule (abstain below 0.02) and the tie rules, in the plan.
- **The annotators,** each working from the ticket text alone, without scores or each other's labels:
  - **Claude:** `annotations-claude.tsv`;
  - **Codex:** `codex-labels-with-reasons.tsv`, with its reasons, SHA-256 `80d7acbf…e079` as published by Codex, and `annotations-codex.tsv` in the frozen schema, derived from it.
- **These are agent-labelled evaluations, not human or user acceptance.**
- **The first U4 session started at 19:30:28,** after the freeze. Three runs failed in the script's Polars display code, before any result was shown:
  1. a missing `DataFrame::clone`;
  2. a fallible `with_order_descending_multi` without `?`;
  3. a fallible `slice` without `?`.
  - **No frozen item changed.** The fourth run is the one reported. The second and third failures read as hashes, not method names: 0133's F15, still open.

## 2. The session (`out/session-display.txt`, runner0134 with this tree's adapters, `repl`)

**Tickets per label** (review means a margin below 0.02):

| triage | tickets |
|---|---:|
| feature | 99 |
| bug | 79 |
| review | 52 |
| performance | 19 |
| documentation | 3 |

- **The review queue:** the 20 lowest margins, shown as two 10-row frames, because the preview shows at most 10 rows (0124). Its margins run from 0.0004 to 0.0084. Release and process tickets dominate the head (#453 "0.12.2 release did not get completed", #372 "yanked futures-core", #768 "New 0.13.x release?"): they fit none of the five labels well.
- **The most label-like tickets:**
  - bug: #527 formatting misfire, #745 LSP crashes, #15 type-hash conflicts;
  - performance: #1051 benchmark maintenance, #273 memory leak;
  - documentation: #648 "Documentation enhancements";
  - question: #45 "Community Site", #444 "Project questions", #165 "Can you describe the VM design?". These are question-like, but each scores higher on another label.
- **Cost:** 252 tickets and 458 passages; embedding and scoring 5,921 ms, and the session run 8.4 s (the saved run). An earlier run of the same script measured 5.8 s and 8.2 s.

## 3. Measures (`out/`)

**Parity, gated** (`parity.txt`): **each table is first validated on its own** (review round 1).
- **The checks:**
  - the header is exactly the five fixed columns, then `labels.tsv`'s five score columns, in order;
  - the tickets are exactly the committed D2 manifest's 252, each once;
  - triage is a frozen label or "review"; best and second are frozen labels, and differ;
  - every number is finite.
- **Only then are the two compared:** triage, best and second exactly, the margin as an f64, and the five scores as f32 bits.
- **The result:** all 252 tickets valid and **BIT-EQUAL** to `twin0133 u4`, which uses its own chunker, the S1 partition, the same Candle pooling and scoring calls, the same tie rules, and margins computed in f64 from the f32 scores.
- **Corruption controls** (`compare_u4.py --controls`), each refused:
  - review round 1's three cases, where agreement alone had passed: header-only on both sides, the same ticket missing on both, and the same score column missing on both;
  - a duplicate row, a non-finite score, one changed score bit, an unknown triage label, and best equal to second.

**Quality, reported** (`evaluation.txt`; coverage and accuracy with numerators and denominators; the confusion table covers assigned sample tickets only):

| against | coverage | accuracy over assigned | abstentions by reference label |
|---|---|---|---|
| Claude's labels | 32 / 40 | 18 / 32 = 0.56 | documentation 3, feature 3, bug 1, question 1 |
| Codex's labels | 32 / 40 | 18 / 32 = 0.56 | documentation 3, bug 3, feature 2 |

- **Confusion (both annotators):**
  - **question:** the model **never predicts question.** Of the question-labelled tickets it assigned, 6 went to feature, and the rest to bug or performance (1 and 1 by Claude's labels; 1 and 0 by Codex's).
  - **feature** is mostly right (11 of 14).
  - **bug** is 6 of 8 (Claude's labels) or 6 of 9 (Codex's).
  - **documentation:** its one assigned ticket went to feature.
- **All 14 errors are listed in `evaluation.txt`, with titles and their two top labels.** Most are help requests read as features (#434 "Custom state?", #486 "Proper way to call Rust from Rune Scripts", #629 "Modify a vm function").
- **The annotators agree on 37 of 40, Cohen's κ 0.89.** The disagreements: #400 (Claude question, Codex bug), #909 (question, bug), #962 (feature, bug). They're kept as frozen and weren't resolved after scoring.

**Cost against PyTorch, reported** (`torch.txt`; `torch_u4.py`):
- **The setup:** sentence-transformers 6.1.0, torch 2.14.1, CPU, 20 threads, batch 32. It uses the same 458 passages (0136's committed D2 byte ranges), the same mean pooling and renormalization, and the same model and descriptions.
- **The time:** embedding and scoring take **4.78, 4.81 and 4.91 s** (the saved runs; median 4.81), against rnx's 5.9 s (about 1.23×).
- **The predictions:** its top-label counts are **exactly rnx's** before abstention: bug 101, feature 109, question 0, documentation 5, performance 37.

## 4. What the result says

**Zero-shot triage by one-line descriptions sorts these tickets by topic:**
- defect, new capability and efficiency language is picked up reasonably;
- **asking for help is a matter of intent, not topic,** and the model reads help requests about features as features.

**The margin is an uncalibrated signal:**
- at 0.02 it routes 52 of 252 tickets (21%) to review;
- its lowest values flag tickets that fit no category (releases, process).

**Improving question detection** (different descriptions, a sixth "other" label, or a supervised head) would be a new, separately frozen run. It's not done here.

## Gates

- **The freeze** was committed before the first run: the plan commit `c9d69d5` at 19:30:21, and the first session at 19:30:28.
- **U4** passes as a session paste, with the marker and a final `Ok(())`, and displays its frames. It's bit-equal to its twin.
- **The code changed** only in `probes/0139` and `probes/0133/twin` (the twin's `u4` command); no adapter or core code changed. The twin builds.
- **`:dep polars candle`, after push** (`out/dep/`): a clean worktree binary at the pushed `c9abc26`, a fresh cache, the datasets verified, and an ordinary `:dep polars candle` session.
  - **The session:** U4 passes on its marker with a final `Ok(())`. Embedding and scoring take 5,935 ms, and the run 8.5 s. The first `:dep` took 391 s to build the adapters.
  - **Parity:** its per-ticket table is complete and valid for all 252 manifest tickets, and **BIT-EQUAL** to the twin's.
