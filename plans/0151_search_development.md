# rnx 0151: making search measurably useful, the development half (fresh queries, a frozen holdout for 0152)

Status: plan. The user (2026-10-02): pursue model usefulness with **fresh development queries and a separate frozen holdout.** The design was agreed with Codex before this plan, including its bounds on the menu of variants.

**Why two records:**
- **This record develops:** it writes and freezes **both** query sets, iterates **only on the development set,** and ends with one configuration selected by a stated rule, committed in its impl.
- **0152 evaluates:** its plan freezes that selection, and its impl scores it **once** on the holdout against the unchanged baselines.
- **The boundary is a commit:** the selection is fixed before any holdout query is run, without extra commits.

**Amended after Codex's review (round 1), before the freeze:**
- **R1:** every pair is scored alone, so a score doesn't depend on its context, and 0152 carries the same policy.
- **R2:** BM25 is fully closed.
- **R3:** the paraphrase count is corrected to nine, and the split is replayable by a script.

**What 0146 and 0147 left open:**
- Re-ranking with one passage per document was **inconclusive** on 0147's 34 queries (MRR 0.747 → 0.701; interval [−0.19, +0.10]).
- 8% of supporting records weren't among the 20 candidates.
- The re-ranker saw only each document's best passage by embedding.

## 1. The queries (written and frozen by this commit; nothing has been run on them)

| file | queries | SHA-256 |
|---|---:|---|
| `probes/0151/dev.tsv` | 30 | `2442696a1acaac0ce8643e03a6cc5347a42f5586eea754cde88edfba149cbfee` |
| `probes/0151/holdout.tsv` | 40 | `f0e6f3455b4258f3ef350c179f9893ef0aba31ceef37ace8deed8b572bb3ff7a` |

**How they were written:** 0147's method, from the corpus only.
- **The sources:** titles, status paragraphs, full-text search and reading. **No retriever, BM25 or model has been run on any of them.**
- **The tasks:** 70 analyst tasks over D1 (records 0001 to 0132), **none of them 0146's or 0147's tasks.** Several land on the same records as earlier tasks; a supporting document may serve different tasks.
- **The judgments:** binary, with several valid answers allowed; a record supports a task when it directly answers or decides it. Each judgment was checked by reading. The full table is in the two files, with their text, supporting records and `kind`.

**The split is frozen and isn't chosen by hand:**
- **The task level:** no development task is a paraphrase of a holdout task. Each of the 70 is a distinct task.
- **The rule** (exact, review round 1, R3; `probes/0151/split.py` replays it):
  - **The strata:** (records, kind), with records "multi" (two or more supporting records) or "single". They're ordered by the string order of (records, kind): multi/direct, multi/paraphrase, single/direct, single/paraphrase.
  - **Within a stratum:** ordered by the SHA-256 hex digest of the **exact id string** (`T01` …), ascending.
  - **The allocation:** over the whole ordered sequence, position i (0-based) is development iff `floor((i + 1) × 30 / 70) > floor(i × 30 / 70)`. The other 40 are the holdout.
  - **Within each file,** rows are in id order.
  - **`split.py`** recomputes every kind and the split from the union of the two files, and requires both to be exactly the rule's, byte for byte. It refuses duplicates, omissions, any count other than 70 (30 + 40), ids outside T01 to T70, and kind drift.

| split | single, direct | single, paraphrase | multi, direct | multi, paraphrase |
|---|---:|---:|---:|---:|
| dev (30) | 23 | 3 | 3 | 1 |
| holdout (40) | 29 | 4 | 6 | 1 |

- **`kind`** is 0147's rule (`probes/0147/kinds.py`).
  - **Only 9 of the 70 tasks are paraphrases** by that rule (dev 3 + 1, holdout 4 + 1), so the set leans toward direct lookups. That's a stated limitation, not adjusted (corrected from "7", review round 1).
- **What it can support:** a hand-written, non-random holdout supports a **descriptive** result on these tasks. It doesn't show general performance on a population of queries, or demonstrated statistical power.

**The holdout is not looked at again:** not its queries, judgments or any score, until 0152. 0147's 34 queries and 0139's sample stay historical evaluations, never tuned on.

## 2. What is compared on development

**The fixed pieces, all at overlap 0:**
- 0136's chunking;
- MiniLM retrieval;
- each document's best passage for retrieval;
- the cross-encoder of 0146.

**Two baselines, both eligible to win:**
- **R, embedding retrieval alone:** U5's retrieval order (0136's U1′), the top 40 documents.
- **B, BM25,** an untuned lexical baseline with a closed contract (`probes/0151/bm25.py`, a small probe, no adapter change):
  - **The units:** the same 3,461 passages (0147's retained, D1-validated passages). Each document scores the **maximum** over its passages.
  - **Tokens:** lowercase runs of `[a-z0-9]`, with no stemming and no stopwords, for queries and passages alike.
  - **Scoring:** Okapi BM25 with k1 = 1.2 and b = 0.75 (review round 1, R2).
    - **The statistics are per passage:** N, each term's n (document frequency) and the average length are counted over passages.
    - **idf** = ln(1 + (N − n + 0.5) / (n + 0.5)).
    - **A passage's score** is the sum over the query's **distinct** terms (a repeated query term counts once) of idf × tf × (k1 + 1) / (tf + k1 × (1 − b + b × dl / avgdl)).
  - **Precision and order:** Python floats (IEEE f64), accumulated in ascending lexicographic order of the distinct query terms.
  - **Edge cases:**
    - **an all-token-free corpus** (average length 0) gives every passage a score of 0;
    - **so does an empty query.**
    - Rankings then follow the tie rule.
  - **Ties:** a document's passage ties go to the lower pid; document ties go to the lower pid of its best passage.
  - **Frozen** here, and carried unchanged to 0152.
  - **Controls:**
    - a hand-computed four-passage corpus;
    - ties;
    - an empty query;
    - a token-free passage;
    - **a repeated query term** (scores equal to the term once);
    - **an all-token-free corpus.**

**Six re-ranking pipelines,** Codex's bound menu, with no other variants and no weight sweep:

| pipeline | candidates (by R) | passages per candidate, k | document score |
|---|---:|---:|---|
| P1 (U5's ranking rule over this pool) | 20 | 1 | CE of the best passage |
| P2 | 20 | 2 | max CE over its top 2 passages |
| P3 | 20 | 3 | max CE over its top 3 |
| P4 | 40 | 1 | CE of the best passage |
| P5 | 40 | 3 | max CE over its top 3 |
| P6 | 40 | 3 | fixed rank fusion of R and P5 |

- **Passage selection is exact:** a candidate's top k passages by retrieval score, with ties to the lower pid; if a document has fewer than k passages, all of them.
- **Ordering:** re-ranked by document score, descending, with ties to the better retrieval rank.
- **P6's fusion, frozen, not tuned:** `1/(60 + r_R) + 1/(60 + r_P5)`, with both ranks over the same 40 candidates. Ties go to the better `r_R`. It's an experimental choice, not a promised benefit.
- **The pair scores are computed once, each pair alone** (review round 1, R1):
  - **The pool:** one run scores every distinct (query, passage) pair the six pipelines need, the top 3 passages of the top 40 candidates, in a fixed order: query order, then candidate rank, then passage rank.
  - **Every pair is scored by its own `ce.score([q], [p])` call,** a batch of exactly one pair with no padding. So a pair's score doesn't depend on which pairs share its batch, by construction. (In a batch, padding can change the last bits, as 0146 noted.)
  - **The twin** derives the same pool independently and also scores each pair alone.
  - **Each pipeline** reads its pairs from that pool.
  - **P1 is "U5's ranking rule over this pool,"** not claimed to be U5 itself.
  - **0152 carries the same policy:** the selected system's pairs are scored one at a time there too, so its holdout scores are computed exactly as its development scores were.
- **Costs:** each system's **logical** cost is reported as the number of pairs it reads. The shared run's measured time is reported once, with its mean per pair, and isn't attributed to any single pipeline.

## 3. Measures, selection, and what development may claim

**Per query and system:**
- r, the rank of the first supporting record, with 0147's record-id dedup (a record's rank is its earliest document's);
- hit@1 and hit@5;
- recall@5;
- **RR at a cutoff of 20 for every system,** the 40-candidate pipelines included;
- candidate recall at the system's candidate count.

**The selection rule:**
- **The criterion:** the highest **development mean RR@20** among the eight eligible systems, R, B and P1 to P6.
- **Exact ties** go to the simpler system, in this order, declared now: **B, R, P1, P2, P4, P3, P5, P6.** That's ascending cost (the number of cross-encoder pairs), with the fused system last.
- **No other selection:** the winner is the committed configuration for 0152. If R or B wins, that's the result; no model pipeline is forced.

**Reported for development:**
- every system's per-query table and aggregates;
- the paired comparisons of each pipeline against R and against B (wins, losses and ties; an exact sign test; a bootstrap of the mean ΔRR, seed 151);
- costs, in pairs scored and measured time, with retrieval and re-ranking reported apart;
- **every variant, including the losers and any failure.**
- **What development can't claim:** with 8 systems and 30 queries, the winner's development score is optimistically biased. It's reported as selection evidence only; **0152's holdout is the claim.**

## 4. Implementation and parity, gated

- **U8** (`probes/0151/u8_rerank_dev.rn`, a session paste, using only existing adapter calls) runs retrieval and scores every pair the menu needs.
  - **Retained:** the passages (0147's format), the retrieval scores, and every pair as (query, document, passage, pid, CE score).
- **The twin** (`twin0133 u8`) computes the same independently: retrieval, candidates, passage selection, and the pair pool scored one pair at a time.
- **Gate 0, provenance:** D1 against 0133's manifest, both models' pinned files, both query files' hashes, and `split.py`, before any session.
- **Validation:** each producer's retained evidence is checked against D1, its candidates are recomputed from its retained scores, and its pair identities are reconstructed; then the producers are compared **bit for bit.** Corruption controls are applied to both producers alike, and one is one-sided.
- **The semantic check:** HF's `CrossEncoder` scores the reconstructed pairs, within 1e-4. It writes a completion artifact (exit 0 PASS, 3 FAIL), checked by the driver.
- **The evaluator** computes all eight systems from the **validated** retained scores, so the same numbers come from either producer.
- **The stop rule:** an unexplained semantic mismatch, or a need for new adapter behaviour, stops the record.
- **Nothing new ships,** so there's no after-push `:dep` check, unless the implementation finds it needs adapter code, which would invoke the stop rule.

## 5. The hand-off to 0152

0151's impl ends with the selected system named, its frozen configuration, the development tables, and the holdout file's hash re-verified unchanged.

**0152's plan then states:**
- the selected system;
- R and B unchanged;
- one locked evaluation on the 40 holdout queries;
- the same measures and paired statistics, with no other system scored;
- **mechanical or parity failures repaired and replayed on the identical frozen pipeline, each attempt disclosed;**
- **no rule or configuration changes after holdout results are seen;**
- **a negative result closes as a negative result.**

## Out of scope

- Chunk overlap and retrieval changes (a separate retrieval record).
- Score weights, other fusion formulas, other models.
- Triage: a later record, with fresh annotations.
