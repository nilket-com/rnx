# rnx 0152: search holdout, the evidence

**Outcome (the rule frozen in plans/0152 section 2): inconclusive.**
- On the 40 frozen holdout tasks, the mean ΔRR@20 of B (BM25, 0151's selection) minus R (the unchanged MiniLM retrieval) is **−0.040**, with a 95% bootstrap interval of **[−0.189, +0.117]**. The interval includes 0.
- **This is a negative result for the proposed improvement:** BM25's development advantage didn't hold up on the holdout.
- **It isn't evidence that the two are equivalent, nor that B is worse.** R is ahead on the mean and on hit@1, and the sign test leans to R descriptively (9 wins, 18 losses, 13 ties, p 0.122). By the frozen rule, none of that is a finding.
- **The scope is these 40 hand-written analyst tasks** (0151's limitation): not a random sample, and not a query population.

There was **one attempt,** the locked run from the committed, clean tree at **19521f8**. Every gate passed and nothing was repaired. **The holdout is now spent.**

## 1. Gates (attempt 1, `out/holdout/run.txt`)

| gate | result |
|---|---|
| 0, provenance and pins | D1's 321 files, both models and both query files match, and the split replays. `bm25.py`, `evaluate.py`, `validate.py`, `preflight.py` and `split.py` match 43711c4 byte for byte. |
| BM25 contract controls | 11/11 |
| the session (retrieval mode) | OK: 40 queries and 3,461 passages; retrieval and pool in 70.7 s. The ordering control ran first. |
| twin parity | **BIT-EQUAL** after each producer was validated against D1 and its own evidence: the passages, all 3,461 × 40 retrieval scores, and 4,796 pool rows. |
| order | The script's own `order()` equals the numeric stable sort for all 40 queries, and the retained pool is exactly the pool it builds. |
| semantic check of retrieval | Bound to the validated passages and the queries file's ordered ids and texts: 3,461 × 40 = 138,440 scores, **max \|rnx − HF\| 7.63e-7** against a bound of 1e-4, none over. Completed, PASS, with the dimensions verified by `hf_result.py --dims`. |
| reproduction | `evaluate.py` over the committed `out/holdout/u8-script` reproduces `result.json` exactly. |

**The twin's log line** still reads "u8: 40 queries, 0 pairs scored one at a time" in retrieval mode. It's cosmetic, and I left it alone: nothing changes after results.

## 2. The plumbing rehearsal on development, before the holdout (Codex's conditions)

Everything is in `out/rehearsal/`; `unchanged-paths.txt` has the details.

**The default mode is unchanged:**
- **The twin:** its full development run is byte-identical to 0151's retained twin outputs, cross-encoder scores included.
- **The script:**
  - its retrieval scores and pool, cross-encoder scores included, are byte-identical;
  - its passages are identical in content. Their two JSON keys came out in the other order, because Rune objects don't keep key order. Every validator parses the JSON.

**Retrieval mode agrees with 0151:** both producers ran with a cross-encoder directory that doesn't exist, so it provably isn't loaded.
- The passages and retrieval scores are byte-identical to 0151's.
- The pool equals 0151's minus the `ce` column.

**The checks:**
- **Validation controls in retrieval mode:** 12 both-sided corruptions refused, plus one one-sided lowest-bit flip.
- **Semantic-check controls:** 15 corruptions refused at the expected layer:
  - missing, extra, duplicated or reordered rows and columns;
  - wrong query and passage bindings;
  - a non-finite score;
  - a genuine +2e-4 over-tolerance difference outside the pool;
  - an unmet passage count.

  Eleven completion-artifact cases behave as well: missing, malformed, another check's, not completed, a status mismatch, an exception, a completed FAIL, and wrong passage counts, value counts or query ids.
- **Provenance and pin controls:** 9 refused before any step, run in rehearsal mode so that no control reads the holdout beyond gate 0.

**The full driver on development** exited 0 and reproduced 0151's B and R figures exactly (0.569 and 0.412; 16 / 5 / 9).

**A Rune quirk found in the rehearsal:** `if let Some(ce) = ce` fails to compile with "Missing variable". It's worked around by naming the binding `model`. It's a candidate upstream report, not filed.

## 3. The holdout results

| system | mean RR@20 | hit@1 | hit@5 | mean recall@5 | mean candidate recall@40 |
|---|---:|---:|---:|---:|---:|
| B | 0.525 | 15 | 29 | 0.666 | 0.900 |
| R | 0.564 | 19 | 26 | 0.613 | 0.947 |

**The primary comparison, B − R on RR@20:** −0.040, with a bootstrap interval of [−0.189, +0.117] (10,000 resamples, seed 151). **The outcome is inconclusive.**

**Descriptive only:**
- **The sign test:** B wins 9, loses 18 and ties 13 (p 0.122).
- **Direct tasks (35):** B 0.540 against R 0.556.
- **Paraphrase tasks (5):** B 0.418 against R 0.622. Five queries can't support a conclusion.
- **B is ahead at hit@5** (29 against 26) and recall@5, and **behind at hit@1** (15 against 19) and candidate recall.

**Development (0151, selection-biased) against holdout, mean RR@20:**

| system | development | holdout |
|---|---:|---:|
| B | 0.569 | 0.525 |
| R | 0.412 | 0.564 |

B's development lead of +0.156 became −0.040 on the holdout. Most of the arithmetic swing is R's higher holdout mean. **The cause of the reversal isn't established** by this single fixed split. **The supported conclusion:** B's development lead didn't translate into a demonstrated improvement on the holdout. Neither equivalence nor R's superiority was established.

**Per query** (r of the first supporting record; "-" means beyond 20), from `out/holdout/run.txt`:
- **Missed by both:** T34 (0081) and T46 (0097, a paraphrase).
- **Missed by B only:** T47 and T49.
- **Missed by R only:** T52, T63 and T64.
- **Large B wins:** T24, T26, T35, T41 and T54.
- **Large R wins:** T06, T16, T47 and T49.

The two systems still fail on different tasks, as in development.

## 4. What this closes, and what it leaves

- **Closed, negative:** "untuned BM25 beats MiniLM retrieval on these frozen tasks" isn't supported. Nothing changes in rnx. No system, rule or judgment was changed after results.
- **Not shown:** that R is better, or that the two are equivalent.
- **Carried forward, as ideas only:**
  - **Fusion.** The complementary failures (development and holdout alike) make a lexical-plus-embedding fusion the obvious candidate. Testing it needs **fresh** development and holdout tasks, because both of these sets are now spent.
  - **Larger task sets.** A larger fresh task set is an option, not a demonstrated remedy. No power calculation was done, so the sample size a future comparison needs isn't established.
- **Next, per the user's direction:** triage usefulness, with fresh annotations.

## 5. Costs

| system | time | model calls |
|---|---:|---|
| R | 70.7 s for retrieval and pool, embedding 3,461 passages and 40 queries | MiniLM |
| B | 0.32 s for the index and 40 rankings | none |

No cross-encoder was used.

## 6. Files

- **Code:**
  - `probes/0152/run.sh`, `preflight.py`, `validate_retrieval.py`, `order_check.py`, `hf_retrieval.py`, `hf_result.py` and `evaluate.py`;
  - the controls, `validate_controls.py`, `hf_controls.py` and `preflight_controls.sh`;
  - the retrieval-only mode in `probes/0151/u8_rerank_dev.rn` and `twin0133 u8`.
- **`out/holdout/`:**
  - both producers' pools, with gzipped passages and retrieval scores;
  - `run.txt`, `session-excerpt.txt`, `hf-retrieval.json`, `orders.tsv` and `result.json`.
- **`out/rehearsal/`:** the development rehearsal and all control logs.
