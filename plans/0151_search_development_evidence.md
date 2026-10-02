# rnx 0151: search development, the evidence

**Result:** the frozen selection rule picks **B, the untuned BM25 baseline**, with a development mean RR@20 of 0.569. B is the configuration 0152 takes to the holdout. Every gate passed, and none of the eight systems was changed after its results were seen.

**This is selection evidence only.** Eight systems were compared on 30 queries, so the winner's development score is biased upward. **The claim belongs to 0152's holdout.**

## 1. Gates

The run is `probes/0151/run.sh` (output in `out/run.txt`, with scratch paths written as `SCRATCH`). It exited 0. That run used the script before the R1 correction and had no `order` step; section 1a gives the correction and its checks over the same retained evidence.

| gate | result |
|---|---|
| 0, provenance | D1's 321 files match 0133's manifest. Both models match their pinned SHA-256s. `dev.tsv` and `holdout.tsv` match the plan, and `split.py` replays. |
| BM25 contract controls | 11 of 11 pass: hand-computed scores, distinct terms, case, an empty query, a token-free corpus, ranking and ties. |
| U8 session | OK, with 0 errors: 30 queries, 3,461 passages and 3,593 pairs, each scored alone. |
| producer validation | Each producer's pool is recomputed from its own retained scores over D1. |
| twin parity | **BIT-EQUAL:** passages, all 30 × 3,461 retrieval scores, and 3,593 pool rows with cross-encoder scores, compared in f32 bits. |
| semantic check (HF `CrossEncoder`) | Max \|rnx − HF\| is **1.24e-5**, under a bound of 1e-4, over the 3,593 reconstructed pairs. The completion artifact (`out/hf-pool.json`) records PASS. |
| validation controls (`out/validate-controls.txt`) | The unmodified pair passes. All 11 corruptions applied to both producers are refused, and so is the one-sided flip of a cross-encoder score's lowest bit. |
| provenance controls (`out/preflight-controls.txt`) | The unmodified inputs reach every step. All six corruptions are refused by name before any step begins: a D1 byte, a D1 file, a MiniLM weight byte, a cross-encoder tokenizer byte, `dev.tsv` and `holdout.tsv`. |
| ordering (review R1) | See section 1a. The corrected `order` gives the numeric order for all 30 queries over the retained matrix of both producers, and both retained pools are exactly the pools it builds (`out/order-check.txt`). |
| reproduction | `evaluate.py` over the committed `out/u8-script` reproduces the tables and `selected.json` byte for byte. |

## 1a. The ordering correction (Codex review, R1)

**The bug:** as first run, the U8 script cast each query's retrieval scores to text **before** sorting them. That sorts by text, not by the frozen descending numeric f32 order: `[-0.9, -0.1, 0.0, 0.2]` sorts as `0.2, 0.0, -0.9, -0.1`, and `10` sorts below `0.2`.

**The fix:** `order()` in `u8_rerank_dev.rn` now does a stable descending sort on the numeric column, with ties keeping pid order, and casts to text only afterwards, for readback. Nothing about scoring or selection changed.

**The ordering control:** `order_controls()` runs at the start of every U8 run, in the session before any model loads, and again in `rnx run` mode.
- **Its cases:** negative, zero and positive f32 scores, ties, `10.0` against `0.2`, and the scientific-notation values `1e-5` and `3.5e-8`.
- **It gives** `8,3,5,4,7,2,1,6,0`.
- **On the old text-first path, it refuses:** that path gives `7,8,3,5,4,2,0,1,6`.

**The retained evidence is unchanged:** the new `order` run step applies the script's own `order`, through `rnx run u8_rerank_dev.rn`, to each producer's retained 30-query matrix, with each score read back to its f32. `order_check.py` then confirms:
- every query's full order equals `validate.py`'s numeric stable sort;
- each retained pool is exactly the pool that order builds.

**Why the pool survived the bug:** the old text order differed from the numeric order in all 30 full query orders. Within the pools it didn't, because the pool draws only from the top of each query's order, where the scores share the positive `0.xxx` text form and so sort the same as text and as numbers. The 3,593 cross-encoder pairs, the tables and the selection therefore stand as computed. No re-embedding or rescoring was needed.

**0152 inherits the corrected script, and its control runs on every run.** The provenance controls were rerun with the new step and still behave.

## 2. Development results: every system, losers included

| system | candidates | mean RR@20 | hit@1 | hit@5 | mean recall@5 | mean candidate recall | logical CE pairs |
|---|---:|---:|---:|---:|---:|---:|---:|
| **B** | 40 | **0.569** | 12 | 23 | 0.733 | 0.967 | 0 |
| R | 40 | 0.412 | 9 | 16 | 0.483 | 0.933 | 0 |
| P1 | 20 | 0.479 | 11 | 19 | 0.583 | 0.817 | 600 |
| P2 | 20 | 0.540 | 13 | 20 | 0.617 | 0.817 | 1,200 |
| P4 | 40 | 0.476 | 11 | 18 | 0.600 | 0.933 | 1,200 |
| P3 | 20 | 0.524 | 12 | 20 | 0.617 | 0.817 | 1,797 |
| P5 | 40 | 0.556 | 13 | 21 | 0.683 | 0.933 | 3,593 |
| P6 | 40 | 0.546 | 13 | 20 | 0.617 | 0.933 | 3,593 |

**The selection:** B wins at 0.5689. No tie arose, so the declared tie order (B, R, P1, P2, P4, P3, P5, P6) didn't decide anything. `out/selected.json` records the frozen configuration: BM25 under the `probes/0151/bm25.py` contract, depth 40.

**Paired comparisons against R** (RR@20: wins / losses / ties, exact sign test, mean Δ with a 95% bootstrap at seed 151):
- B: 16 / 5 / 9, p 0.027, +0.156 [+0.050, +0.274]
- P1: 10 / 6 / 14, p 0.454, +0.066 [−0.051, +0.191]
- P2: 12 / 6 / 12, p 0.238, +0.128 [+0.016, +0.253]
- P4: 10 / 11 / 9, p 1.000, +0.063 [−0.058, +0.191]
- P3: 13 / 5 / 12, p 0.096, +0.112 [+0.001, +0.232]
- P5: 14 / 7 / 9, p 0.189, +0.143 [+0.016, +0.272]
- P6: 13 / 3 / 14, p 0.021, +0.133 [+0.049, +0.234]

**Paired comparisons against B:** every pipeline's mean Δ is negative, and every interval includes 0:
- P5: 6 / 10 / 14, p 0.454, −0.013 [−0.157, +0.137]
- P2: 7 / 11 / 12, p 0.481, −0.029 [−0.169, +0.115]
- P6: 7 / 11 / 12, p 0.481, −0.023 [−0.163, +0.118]

The rest are in `out/run.txt`.

**Reading the comparisons:**
- **Re-ranking helps the embedding retriever.** Every 40-candidate pipeline beats R on the mean.
- **None of them shows a gain over plain lexical search on this development set.**
- **Re-ranking more passages costs more pairs:** k = 2 or 3 passages beats k = 1 at the same candidate count, and costs 2–3 times the pairs.

## 3. Per query (r of the first supporting record; "-" means beyond 20)

| query | kind | supporting | B | R | P1 | P2 | P4 | P3 | P5 | P6 |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| T01 | direct | 0004 | 2 | 1 | 8 | 2 | 14 | 3 | 4 | 1 |
| T03 | paraphrase | 0007 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| T04 | direct | 0008 | 2 | 6 | 3 | 3 | 3 | 2 | 2 | 4 |
| T08 | direct | 0013 | 1 | 1 | 2 | 2 | 2 | 2 | 2 | 1 |
| T13 | paraphrase | 0024 | 3 | 11 | 11 | 14 | 20 | 9 | 19 | 11 |
| T14 | direct | 0026 | 2 | 4 | 1 | 1 | 1 | 1 | 1 | 1 |
| T15 | direct | 0027 | 1 | 2 | 3 | 1 | 3 | 1 | 1 | 2 |
| T17 | direct | 0031 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| T23 | direct | 0049 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| T25 | direct | 0052 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| T28 | direct | 0058 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| T31 | direct | 0071 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| T32 | paraphrase | 0072 | 1 | - | - | - | 3 | - | 3 | 11 |
| T33 | direct | 0074 | 6 | 7 | 6 | 6 | 10 | 6 | 2 | 6 |
| T36 | direct | 0083 0084 | 2 | 4 | 4 | 5 | 7 | 5 | 9 | 4 |
| T37 | paraphrase | 0085 0086 | 5 | 6 | 5 | 1 | 7 | 1 | 1 | 1 |
| T39 | direct | 0088 | - | 9 | 1 | 1 | 1 | 1 | 1 | 1 |
| T40 | direct | 0089 | - | 5 | 16 | 13 | - | 13 | - | 9 |
| T42 | direct | 0091 | 5 | - | - | - | 4 | - | 5 | 9 |
| T43 | direct | 0092 0095 | - | 4 | 6 | 6 | 8 | 6 | 8 | 12 |
| T50 | direct | 0108 | 2 | 6 | 3 | 3 | 4 | 3 | 4 | 3 |
| T51 | direct | 0109 | 2 | 6 | 1 | 1 | 1 | 2 | 2 | 2 |
| T53 | direct | 0111 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| T55 | direct | 0116 | 2 | 14 | 10 | 9 | 17 | 11 | 19 | 16 |
| T57 | direct | 0118 | 1 | 11 | 2 | 4 | 4 | 4 | 6 | 4 |
| T58 | direct | 0121 | - | - | - | - | - | - | - | - |
| T66 | direct | 0075 0076 | - | 4 | 5 | 2 | 6 | 2 | 1 | 2 |
| T67 | direct | 0077 | 6 | - | - | - | 6 | - | 8 | 15 |
| T68 | direct | 0078 | 2 | - | - | - | - | - | - | - |
| T70 | direct | 0019 | 1 | 2 | 1 | 1 | 1 | 1 | 1 | 1 |

**What the rows show** (observations only; none of them changes the selection):
- **B and the pipelines fail on different queries:**
  - B misses five queries (T39, T40, T43, T58 and T66), and every 40-candidate pipeline finds T39 and T66 in the top 2.
  - B alone finds T68, and B or P4/P5 find T32, T42 and T67, where R and the 20-candidate pipelines miss. In each of those cases, the record's words are in the query.
  - So a fusion of B with a re-ranker is a plausible later candidate. It **wasn't** on this record's frozen menu, and adding it now would be tuning on development. If it's ever tried, it needs fresh queries of its own.
- **T58 (record 0121) is missed by every system.** That's either a hard task or a judgment worth re-examining in a later record. It's reported as it stands and wasn't repaired.
- **Paraphrases are thin:** only 9 of the 70 tasks are paraphrases, 4 in development and 5 in the holdout. B did well on the 4 here (1, 3, 1 and 5), but 4 queries can't support any conclusion about lexical search under paraphrase. **The holdout, like development, mostly measures direct analyst phrasing.**

## 4. Costs

The plan requires the shared run's time to be reported once and not attributed to any single pipeline. The single U8 session (`out/session-excerpt.txt`) took:
- **Retrieval and pool construction:** 67.9 s for 321 documents, 3,461 passages and 30 queries. That includes MiniLM embedding every passage.
- **Re-ranking:** 3,593 pairs scored one at a time in **574.2 s**, a mean of about **160 ms per pair**. That's the cost of the one-pair-per-call policy, which keeps each score independent of its batch. In production the pairs would be batched.
- **The whole session:** it returned to a prompt after 642.3 s.

**BM25 needs no model calls:** its scores come from `bm25.py` over the same retained passages, in negligible time at this corpus size. **B and R need no cross-encoder calls.** That doesn't make their overall costs equal: R pays for MiniLM embedding of every passage and query, which falls inside the 67.9 s above, while B needs no model at all.

## 5. The hand-off to 0152

- **The selected system:** B, BM25 under the frozen `probes/0151/bm25.py` contract, depth 40, units = passages, with a document scored by its best passage.
- **Its frozen configuration:** `out/selected.json`.
- **The development tables:** above, and in `out/run.txt`.
- **The holdout file is unchanged:** `probes/0151/holdout.tsv` was re-verified at SHA-256 `f0e6f3455b4258f3ef350c179f9893ef0aba31ceef37ace8deed8b572bb3ff7a`, the hash the plan froze. Gate 0 hashed it, and `split.py` read it there only to replay the frozen split; its provenance control refuses a changed copy. It was never passed to retrieval, scoring or evaluation.

**0152 runs once on the holdout** with the same measures and paired statistics:
- B, the selected system;
- R, unchanged, as the baseline.

If B doesn't beat R on the holdout, the record closes negative.

## 6. Files

- **Code:**
  - `probes/0151/u8_rerank_dev.rn`, the session script;
  - `probes/0133/twin` (`u8`), the twin;
  - `probes/0151/bm25.py`, `validate.py`, `evaluate.py`, `hf_pool.py`, `hf_result.py`, `preflight.py`, `order_check.py` (R1) and `run.sh`;
  - the controls, `validate_controls.py` and `preflight_controls.sh`.
- **Outputs (`probes/0151/out/`):**
  - `u8-script/` and `u8-twin/`: the pools, plus gzipped passages and retrieval scores;
  - `run.txt`, `hf-pool.json`, `selected.json`, `session-excerpt.txt`, `order-check.txt` and both control logs.
