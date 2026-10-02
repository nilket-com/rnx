# rnx 0153: triage routing, the evidence (design, freeze and development)

**Result:** the frozen selection rule picks **C, the nearest-centroid classifier.**
- **Its threshold:** t = 0.09553107383376902.
- **Its development result:** it auto-routes **74 of 346** D3-dev tickets (A = 0.214), with **7 misroutes** among them (E = 0.095), against Claude's development labels.

That result is development evidence, against a single annotator's reference. **0154 is the claim:** one locked run on the 150 blind-annotated holdout tickets, under section 6's rule, carried verbatim.

## 1. The order of events (plans/0153 section 3)

1. **The design file was accepted and frozen** at SHA-256 `b1b75573…` (plan commit 8512257). It was never edited again.
2. **D3 was fetched:** `rhaiscript/rhai`, 496 issues, pull requests excluded.
   - The manifest holds each issue's content hash and label hash; the meta file holds the snapshot hash.
   - **The split:** 150 holdout and 346 development tickets. **The development folds:** 70/69/69/69/69.
3. **The views** were generated (number, title and body only).
   - **Codex** annotated D3-hold blind: its file hash is `f9541f24…`.
   - **Claude** annotated D3-hold (`93017bd7…`) and D3-dev (`6aa75fd3…`), without opening Codex's file.
4. **The plan commit was amended** with `probes/0153/frozen/` (f9bc252), with the design file byte-identical, **before any system ran on any D3 ticket.**
5. **The implementation was committed** (97fb139). Then the official runs ran from that clean tree: the D2 rehearsal first, then D3-dev.
6. **Codex's review** (R1, R2; section 1a) found two gaps. They were fixed, sent for review **before** any replay, and the replay was approved. Both chains were then replayed from the clean committed tree a338a9f, and both proved unchanged.

**D3-hold has not been read by any system.**

**A disclosure:** for the last ~40 long D3-dev tickets, Claude read a view with backtrace and sanitizer frame lines collapsed to a count. Titles and every other body line were read in full. The holdout was read uncollapsed.

## 1a. Review round 1 (Codex): R1 and R2, and the approved replay

**R1, embedding validity (5a):**
- **The gap:** the producers normalized the pooled embeddings unconditionally, so a zero or non-finite pooled row would have produced NaN or failed the whole run. Section 5a requires Z/C mandatory REVIEW for that ticket instead, with K and N unaffected.
- **The fix in the producers** (`embed_tickets`; the twin's `t3_embed_tickets`):
  - The division and the matmul still run **batched**, so an invalid intermediate row may exist. The arithmetic is row-independent, so it can't affect other rows.
  - Each row is then classified from its pooled values and norm: `zero_norm`, `nonfinite_embedding` or `ok`.
  - Invalid rows are excluded before retention, routing and training. A new `t3-embedding-status.tsv` gives every ticket's status.
- **Downstream:**
  - The validator requires embedding and cosine rows for exactly the `ok` tickets.
  - The evaluator takes Z and C's mandatory causes from that file, and C's training excludes invalid embeddings.
  - `hf_t3.py` requires every claimed-invalid embedding to be degenerate in HF too. A forged `zero_norm` claim is a completed FAIL, exit 3 (`out/review-r1-r2/hf-t3-forged-zero-norm.json`).
- **Tests:**
  - **Both producers:** the session's `main` and the twin's `t3-controls` pass a synthetic pooled tensor, with a zero row and a NaN row, through the production function. Both now run before any inference.
  - **The evaluation path,** 9 fixtures: Z and C mandatory REVIEW; K and N unaffected; all-ticket denominators; no Z or C candidates; excluded from training; a non-finite derived score with a valid embedding still stops.

**R2, refusal binding:**
- **The gap:** a forged `nli_refused` with that ticket's NLI rows deleted was accepted.
- **The fix:** the validator recomputes every pair's length with the **pinned** `tokenizer.json` (no truncation, no padding). `nli_refused` must hold exactly when a pair is over 512 tokens. The validator, the evaluator and `hf_result.py` now run under the HF Python.
- **Controls:**
  - the forged refusal with its rows deleted, both-sided, is refused;
  - **a genuine over-512 ticket** goes through both real producers on a three-ticket synthetic set (`out/review-r1-r2/`). Ticket 2 is ten 120-character words: each is one [UNK] token for MiniLM but about 80 DeBERTa tokens. It's `nli_refused` in the session and the twin, BIT-EQUAL, the validator agrees, and HF passes.
  - On that set, 21 validation corruptions are refused and the completion-artifact controls behave.

**The replay, as approved:**
- **Why it was needed:** the saved evidence predated `t3-embedding-status.tsv`.
- **What was kept:** it was moved, unchanged, to `out/pre-replay/` with a SHA256SUMS list.
- **What ran:** both chains, from a338a9f.
- **The result** (`out/replay-compare.txt`), for both producers on D2 and D3-dev:
  - passages and NLI statuses are exact;
  - embeddings, cosines and NLI logits are identical in f32 bits;
  - `selected.json`, `curves.json` and `centroids.tsv` are **byte-identical**;
  - every new embedding status is `ok`: 252 on D2, 346 on D3-dev.
- **Nothing moved,** so the results in section 3 stand as first computed.
- **A disclosure:** the D3-dev replay began with one untracked file in the tree, `replay_compare.py`, written during the D2 replay. It isn't used by `run.sh`. It's committed in this amend.

## 2. Gates and controls

| check | result |
|---|---|
| gate 0 (`preflight.py`) | All hold, for both runs: the freeze (the design hash, the 0139 rules pin, the design tables, all three annotation files, SHA256SUMS), D3's content, labels and snapshot, the split and folds, D2 and 0139's sample and labels, and the MiniLM and NLI pins. |
| gate 0 controls (`out/preflight-controls.txt`, rerun for the new step order) | The unmodified inputs reach every step, now including the twin's controls before the session. 21 corruptions are refused before any step: the design file; 0139's rules; the labels, keywords, map and rules files; annotation rows missing, duplicated, reordered, blank, out of vocabulary or extra; the manifest; the split; D3 text, label and state drift; a D2 byte; 0139's sample; a MiniLM weight; the NLI tokenizer. |
| fixtures (`out/fixtures.txt`) | 35 of 35, including R1's 9: C's centroids checked bit for bit against an independent numpy computation, including missing and zero-norm queues, fewer than two queues, ties, the fold rule, the cross-fit isolation and the stable softmax. The boundaries are checked too: confidence equal to t routes and the next value down doesn't; tie blocks move together; mandatory REVIEW never routes and adds no candidate; zero auto-routes leave E undefined and make a system ineligible; K's single threshold; Wilson clamping. |
| producer controls (both, before any inference) | Synthetic zero and NaN pooled rows are classified by the production function, with other rows unaffected. NLI handler: a short pair scores. An over-512-token pair is the named refusal. One over-limit pair marks its whole ticket. Any other error is returned, not absorbed. |
| producers (D2, 252 tickets; D3-dev, 346; the replay) | **BIT-EQUAL** after each producer was validated against its issues, the split and its own evidence, with refusals bound to the pinned tokenizer. D3-dev: 774 passages, 346 × 384 embeddings, 346 × 5 cosines and 3,870 NLI pairs × 3 logits. Every ticket's NLI status and embedding status is `ok`: no ticket had no passages, no named NLI refusal occurred, and no embedding was invalid. |
| validation controls (`out/validate-controls.txt` on D2 before R1/R2; `out/review-r1-r2/validate-controls.txt` after) | Before: 14 both-sided and 3 one-sided. After: 18 both-sided, including the forged refusal and three embedding-status corruptions, and 3 one-sided lowest-bit flips (an embedding, a cosine, a logit). |
| semantic check (`hf_t3.py`; any claimed-invalid embedding must be degenerate in HF, and none were claimed) | D3-dev: MiniLM embeddings within 2.77e-7 and cosines within 9.55e-7; NLI logits within 2.3e-5 (pinned ids, model-level, bound 1e-4). D2: 2.52e-7, 4.43e-7 and 1.39e-5. Both completed PASS, with the counts verified. **Native-tokenizer differences, reported and not gated:** 5 pairs on D3-dev and 15 on D2 (0148 also found 15). |
| completion-artifact controls (`out/hf-result-controls.txt`) | 11 refused: missing, malformed, another check's, not completed, a status mismatch, an exception, a completed FAIL, and wrong or missing counts. |
| reproduction | `evaluate.py` over the committed, gzipped evidence reproduces `selected.json`, `curves.json`, `centroids.tsv` and every table. |

## 3. Development results (D3-dev, Claude's labels folded to queues)

**The reference:** bug 146 (139 bugs plus 7 performance tickets), feature 101, question 86, documentation 13.

| system | threshold | auto | A | E | misroutes | cost, c = 2 | cost, c = 5 |
|---|---:|---:|---:|---:|---:|---:|---:|
| K | its only threshold, 1 | 136 | 0.393 | 0.147 | 20 | — | — |
| Z | 0.1803 | 33 | 0.095 | 0.091 | 3 | 0.922 | 0.948 |
| **C** (cross-fit) | **0.0955** | **74** | **0.214** | **0.095** | **7** | **0.827** | **0.887** |
| N | none qualifies | — | — | — | — | — | — |
| MAJ (bug, diagnostic only) | — | 346 | 1.000 | 0.578 | 200 | 1.156 | 2.890 |
| ALL-HUMAN (fallback) | — | 0 | 0 | undefined | 0 | 1.000 | 1.000 |

- **K is ineligible:** its only threshold gives E = 0.147, above 0.10. 210 tickets went to mandatory REVIEW (no keyword, or several).
- **N is ineligible:** it never reaches E ≤ 0.10. At full coverage it predicts question for 216 tickets and documentation for 74, from a reference with 86 questions and 13 documentation tickets.
- **The selection:** the highest dev A among the eligible systems, Z and C, is **C's.**
- **C's misroutes at its threshold:**
  - feature → documentation: 3;
  - bug → documentation: 1;
  - question → bug: 1;
  - question → documentation: 1;
  - question → feature: 1.

  Five of the seven routed tickets into documentation, the smallest class (13 reference tickets).

The full risk-coverage curves, the confusions at full coverage and the misroutes by queue are in `out/dev/run.txt` and `out/dev/curves.json`.

## 4. What is frozen for 0154

- **The selected policy:** C with the full-dev centroids, the f32 bits in `out/dev/centroids.tsv`, produced by section 5b's function. A ticket auto-routes when its confidence ≥ 0.09553107383376902; otherwise it goes to REVIEW.
- **A property of the frozen design, disclosed:** the threshold was chosen on **cross-fit** confidences (centroids from four folds). The holdout is scored with centroids from **all** development tickets, so holdout confidences come from a slightly different model than the one the threshold was set on. Nothing about this changes.
- **The frozen inputs:** D3-hold's manifest, split and both annotation files, verified unchanged by gate 0 in both runs.

**A pre-holdout observation, stated plainly so 0154's result isn't a surprise:**
- **The development A is 0.214.** Section 6's frozen holdout rule requires the Wilson lower bound of the holdout A to reach **≥ 0.25**.
- **Unless the holdout's auto-route share is well above development's, the target can't be met,** and 0154 would report "not demonstrated".
- **The targets, the threshold and the system stay as frozen;** this note changes nothing. "Not demonstrated" doesn't imply no practical utility.

## 5. Holdout annotator agreement (computed from the frozen files; no system involved)

- **Four queues:** 140 of 150 agree (0.933), Cohen's κ 0.902.
- **Five labels:** 138 of 150 (0.920), κ 0.888.
- **Queue disagreements:** 10.

| Claude \| Codex | count |
|---|---:|
| feature \| question | 4 |
| question \| feature | 2 |
| bug \| feature | 1 |
| bug \| question | 1 |
| question \| bug | 1 |
| feature \| documentation | 1 |

These 10 tickets are where the worst-case E_possible and E_certain will differ in 0154.

## 6. D2 diagnostic (seen before; descriptive only)

- **Routed:** C at the frozen threshold auto-routes 3 of 0139's 40 sample tickets.
- **Misroutes:** none of the 3, against either of 0139's annotators.
- The rune tickets are a different repository's issues, and their confidences fall mostly below the threshold.

## 7. Costs

| run | session (embedding, cosines and per-ticket NLI) | twin | HF check |
|---|---|---|---|
| D3-dev | 12.9 s embedding and cosines; 258.8 s for 3,870 NLI pairs, scored per ticket | about 15 min (NLI per ticket, sequential) | MiniLM 8 s; NLI 222 s, one pair at a time |
| D2 | 7.9 s; 160.6 s for 2,290 pairs | — | NLI 128 s |

K and C's scoring time is negligible beside the embeddings.

## 8. Files

- **Code:**
  - `probes/0153/`: `triage.rn`, `validate.py`, `hf_t3.py`, `hf_result.py`, `evaluate.py`, `fixtures.py`, `preflight.py`, `run.sh` and the controls;
  - `twin0133 t3`.
- **Outputs:** `probes/0153/out/`
  - `d2/` and `dev/`: the replayed evidence of record (both producers, gzipped), with `hf-t3.json` and `run.txt`;
  - `pre-replay/`: the first runs' evidence, kept for the comparison;
  - `review-r1-r2/`: the synthetic over-512 set and the R1/R2 controls;
  - `replay-compare.txt`;
  - `dev/selected.json`, `curves.json` and `centroids.tsv`;
  - the control logs.
