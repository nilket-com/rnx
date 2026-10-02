# rnx 0152: search holdout, one locked evaluation of 0151's selection

**Status:** plan. This is the second half of the user's 2026-10-02 direction: **fresh development queries and a separate frozen holdout.**
- 0151 (closed at 43711c4) froze both query sets, developed eight systems on the 30 development queries only, and selected one by a frozen rule. **The selection was B, untuned BM25.**
- This record scores B **once** on the 40 holdout queries, against R, the unchanged embedding retrieval.
- **The holdout has not been run.** Gate 0 hashed it and `split.py` read it only to replay the split. No retriever, BM25 or model has seen it.

## 1. What is frozen by this plan

**The queries:** `probes/0151/holdout.tsv`, 40 queries (35 direct, 5 paraphrase), at SHA-256 `f0e6f3455b4258f3ef350c179f9893ef0aba31ceef37ace8deed8b572bb3ff7a`, with the supporting records as committed in 0151's plan. **There are no new judgments, and no judgment is changed after results.**

**The two systems,** and no others:
- **B, the system 0151 selected:** BM25 under the `probes/0151/bm25.py` contract, unchanged.
  - The units are passages, and a document scores its best passage.
  - Tokens are lowercase `[a-z0-9]` runs.
  - k1 is 1.2 and b is 0.75; the f64 accumulation runs in lexicographic term order.
  - Ties go to the lower pid; the depth is 40.
  - `out/selected.json` in 0151 records this configuration.
- **R, the unchanged baseline:** MiniLM retrieval exactly as in 0151 (U5's retrieval, overlap 0: each document's best passage by cosine). Its ranking is the candidate order, the first 40 distinct documents.
  - The scores are sorted by `order()` in `probes/0151/u8_rerank_dev.rn`, the numeric stable descending sort corrected in 0151's R1.

**The passages:** both systems read the same passages, D1 chunked by MiniLM's tokenizer at 256 tokens, overlap 0, exactly as 0151.

**No cross-encoder is used.** No 0151 pipeline (P1–P6) is scored, and no fusion is tried. Those are outside this record whatever the result.

## 2. Measures and the outcome

**Per query and system, exactly as 0151:**
- r, the rank of the first supporting record, with 0147's record dedup;
- hit@1 and hit@5;
- recall@5;
- RR@20;
- candidate recall at 40.

**The primary comparison** is B against R on RR@20, paired by query, with:
- wins, losses and ties;
- an exact two-sided sign test;
- the mean ΔRR (B − R) with a bootstrap 95% interval (10,000 resamples). The bootstrap seed is 151, the same as 0151, so as not to choose a new one.

**The outcome category is fixed now,** from the bootstrap interval of the mean ΔRR@20:
- **B better than R on this holdout:** the interval lies entirely above 0.
- **B worse than R:** the interval lies entirely below 0.
- **Inconclusive:** the interval includes 0.

**What each category supports:**
- **"B better"** supports a descriptive claim **on these 40 frozen tasks:** untuned BM25 ranked their supporting records better than the MiniLM retrieval did. These are hand-written analyst tasks, not a random sample (0151's limitation), so it isn't a claim about this corpus's queries in general or about any query population.
- **"Inconclusive"** is a negative result for the proposed improvement. It's **not** evidence that the two are equivalent, nor that B is worse.
- **"B worse"** is a negative result, stated as such. This is one pre-declared comparison, so no multiplicity correction applies. The sign test is reported alongside, descriptively.

**Also reported, descriptively, with no claim:**
- the means of every measure for both systems;
- the full per-query table;
- the direct and paraphrase subsets (35 and 5; 5 can't support a conclusion);
- 0151's development figures beside the holdout figures, to show the selection's optimism.

## 3. Implementation and parity, gated

**Producers:** retrieval only, because no cross-encoder pair is needed.
- **The script:** `probes/0151/u8_rerank_dev.rn` gains a retrieval-only mode, an optional sixth argument `retrieval`.
  - The code path up to the pool is unchanged: loading, chunking, embedding, similarity, `order()`, and the candidate and passage slots.
  - The cross-encoder isn't loaded and no pair is scored.
  - It writes the same `u8-passages.json` and `u8-retrieval.tsv`, and the pool without the `ce` column.
  - `order_controls()` still runs first.
- **The twin:** `twin0133 u8` gains the same mode.
- **The default mode doesn't change,** so 0151's run stays replayable.

**Conditions from Codex's acceptance (folded in before any holdout attempt):**
- **The plumbing is rehearsed first,** on the already-spent development data and on synthetic fixtures, before the locked attempt. This verifies the plumbing; it isn't a variant or a selection.
  - **Default mode unchanged:** a full default-mode U8 run on development reproduces 0151's retained outputs, script and twin.
  - **Retrieval mode agrees:** retrieval-only outputs equal 0151's retained passages and retrieval scores, and the pool equals 0151's minus the `ce` column.
  - **The checks work:** the validator, the order step and the HF check, with their controls, all run on development or synthetic data.
- **Gate 0 pins** `probes/0151/bm25.py` and `probes/0151/evaluate.py` (the measures and statistics helpers) byte for byte to their 43711c4 contents. `validate.py` stays untouched and is pinned too; the retrieval mode lives in `probes/0152/validate_retrieval.py`, which imports the pinned helpers.
- **The semantic check binds both axes:**
  - the exact ordered query ids and texts, and the passage ids and texts;
  - a complete 3,461 × 40 matrix, its shape and counts checked before any elementwise comparison, and every value finite.
  - **The artifact** records the verified dimensions and count. An exception, or a malformed or missing artifact, never counts as a completed comparison.
  - **Its controls** reject a missing, extra, duplicated or reordered row or column, a wrong query or passage binding, a non-finite score, and a genuine over-tolerance difference.

**The run** (`probes/0152/run.sh`), fail-closed:

| step | what it checks |
|---|---|
| gate 0, provenance | 0151's `preflight.py`: D1, both models (the cross-encoder only because the preflight is reused unchanged), both query files and the split. The provenance controls are rerun with 0152's step list. |
| BM25 contract controls | `bm25.py --controls` |
| the session | the script on `holdout.tsv`, in retrieval mode. Its timing line reports retrieval and pool time. |
| the twin | the same independently, in retrieval mode |
| validation | Each producer is validated against D1 and its own retained evidence, with the pool recomputed numerically from the retained scores. Then the producers are compared bit for bit (passages, retrieval scores, pool rows). `probes/0152/validate_retrieval.py` does this with six pool columns and no cross-encoder scores, reusing the pinned `validate.py` helpers. Its corruption controls are rerun in that mode, both-sided plus one one-sided. |
| order | 0151's step over the retained holdout matrix: the script's own `order()` equals the numeric stable sort for every query, and the retained pool is the pool it builds. |
| semantic check of retrieval (new) | sentence-transformers MiniLM on PyTorch CPU, the same pinned files, embeds the retained passages and the 40 holdout queries. The cosine scores are compared with the retained retrieval matrix, max \|rnx − HF\| within **1e-4**. It writes a completion artifact (exit 0 PASS, 3 completed FAIL), validated by `hf_result.py`. This is parity, reported separately from quality. |
| evaluation | `probes/0152/evaluate.py` reuses 0151's `measures`, `sign_test`, `bootstrap` and `bm25.rank` by import. It computes B and R only, from the validated retained evidence, and writes the tables and `result.json` with the outcome category. |

**Costs:** the session's retrieval and pool time, and BM25's scoring time over the retained passages.

## 4. The locked run

- **The holdout runs once,** after this plan is accepted.
- **Mechanical or parity failures** may be repaired, with plumbing changes only. Before the replay, each repair is sent to Codex. The replay runs on the identical frozen pipeline, and every attempt and its outputs are disclosed.
  - **What repairs may never change:** `bm25.py`, the depth, the passages, R's retrieval and ordering, the judgments, the measures, the statistics or the outcome rule.
- **The stop rule:** an unexplained semantic mismatch, or a need for new adapter behaviour, stops the record unscored.
- **After results are seen, no rule or configuration changes.** Ideas the per-query table suggests, such as fusing B with a re-ranker, or T58-like misses, go to a later record with fresh queries. **The holdout is spent by this run** and isn't reused for tuning.
- **Nothing new ships,** so there's no after-push `:dep` check. The retrieval-only mode is probe code.

## 5. Evidence

`plans/0152_search_holdout_evidence.md` will report:
- the gates;
- every attempt;
- both systems' tables;
- the paired comparison and the outcome category;
- the development-to-holdout comparison;
- the costs.

The retained outputs go in `probes/0152/out/`, with large files gzipped.

## Out of scope

- Any system other than B and R.
- Any change to BM25 or to retrieval.
- New judgments.
- Triage usefulness, which is a later record with fresh annotations.
