# rnx 0147 evidence: the frozen 34-query evaluation, U1′ and U5 scored unchanged

**The result:**
- **The comparison on the frozen rubric is inconclusive.**
  - **MRR:** 0.747 for retrieval alone, 0.701 re-ranked.
  - **hit@1:** 23 → 20 of 34.
  - **hit@5:** 27 → 29.
  - **RR:** re-ranking wins on 8 queries, loses on 11 and ties on 15 (sign test p = 0.65).
  - **Mean ΔRR:** −0.046, with a 95% bootstrap interval of **[−0.190, +0.099]**, which contains zero.
- **These descriptive tests don't establish an overall difference** between the two orders on this task set. The orders differ in where they put supporting records (review round 1).
- **They don't establish equivalence either.** The set is hand-written from analyst tasks, not a random sample, so it can't establish general equivalence.
- **Parity holds, reported separately:**
  - BIT-EQUAL to the twin over 34 × 20 candidates and 680 pairs;
  - Hugging Face within **1.19e-5**, with identical orders.
- **Latency:** re-ranking 680 pairs takes **14.0 s**, on top of 69.8 s of retrieval.

The rubric is `probes/0147/rubric.tsv`, SHA-256 `ee31b55d…`, frozen by Codex's acceptance of plan `1507737`. Nothing in it changed after that.

## 1. The gates (`probes/0147/run.sh`, `out/run.txt`)

0. **Provenance, first and fatal:**
   - D1 matches 0133's 321-file manifest;
   - MiniLM's six files and the cross-encoder's three match their pinned SHA-256s.
   - **Controls** (`out/preflight-controls.txt`), all refused by name with the sentinel showing **no session, twin or HF step began:**
     - a D1 byte changed, a D1 file removed, an extra `.md`;
     - a MiniLM weight byte changed, a cross-encoder tokenizer byte changed;
     - a model file missing.
     - The unmodified copies reached every step.
1. **The rubric:** the hash is the frozen one, and `kinds.py` passes.
2. **The scripts:** `workflow-u1c.rn` is byte-identical to `a959f21` (0136) and `u5_rerank.rn` to `240fa54` (0146).
3. **The sessions:** U1′ and U5 both returned to the prompt with the marker, `Ok(())` and no errors.
4. **U1′ against U5, validated first** (review round 1, R1):
   - **U1′ is checked on its own** against U5's retained evidence:
     - the exact header, row width, query set and ranks 1 to 5;
     - finite scores and distinct documents;
     - each pid is in range and holds the retained path and the f32 score for that query;
     - each pid is its document's best passage.
   - **Then against U5:** each rank's score must be bit-equal to U5's at that rank. A different document there is then necessarily an equal-score choice, and is named.
   - **The result:** for all 34 queries U1′'s top 5 equals U5's retrieval top 5, with **no equal-score choices.**
   - **Controls** (`out/u1-controls.txt`, on the real outputs), all refused:
     - Codex's two reproducers: a nonexistent document and pid with the score kept, and a NaN score with the identity kept;
     - an out-of-range pid;
     - a valid pid with another document's path;
     - a forged finite score one f32 step away;
     - another passage of the same document with its own retained score (refused as not the best passage);
     - a duplicated document; an unexpected query; an extra row; a missing row; a wrong header.
     - The unmodified U1′ passes.
     - **A genuine tie** is controlled synthetically (`evaluate.py --controls`), because the real run has none: it passes and is named.
5. **Parity:** `compare_u5.py` is **BIT-EQUAL**.
   - **What's compared:** each producer is validated against its retained evidence and D1, its candidates recomputed, then compared with the other. That covers 3,461 passages, their 34 query scores, 34 × 20 candidates, the 680 pairs' texts, the re-ranked orders and the metrics.
   - **The twin derived the same partition:** 672 single-pair batches and 4 two-pair batches. Some pairs are short enough to share the S1 share.
   - **HF:** max |d| **1.19e-5** (bound 1e-4), and the re-ranked orders are identical.
6. **The evaluator:**
   - its controls pass (`out/evaluate-controls.txt`);
   - on the real run, it reproduces U5's own in-trace hit@1, hit@5, recall@5 and MRR for all 34 queries, in both orders, exactly;
   - rerun from the saved outputs with round 1's evaluator, it reproduces round 0's tables byte for byte. Only the U1′ summary line changed.
- **`out/run.txt` is the first run's log,** with round 0's evaluator. `out/evaluation.md` is regenerated with round 1's.
- **0146's corruption controls, rerun on 0147's outputs:** all 26 are refused, after one fix to a control (F1).

## 2. Per query (`out/evaluation.md`)

- **r** is the rank of the first supporting record (its earliest document) among the 20.
- **Δr is censored** when either rank is beyond 20 (there are none here).
- **Candidate recall@20** is the same for both orders. It measures the candidate stage's omissions and bounds the recall@5 re-ranking can reach. It says nothing about how well the chosen passage represents a document.

| query | kind | supporting | retrieval r | re-ranked r | delta r | delta RR | recall@5 | candidate recall@20 |
|---|---|---|---:|---:|---:|---:|---|---:|
| E01 | paraphrase | 0001 0005 | 3 | 1 | -2 | +0.667 | 1.00 -> 1.00 | 1.00 |
| E02 | paraphrase | 0021 | 3 | 1 | -2 | +0.667 | 1.00 -> 1.00 | 1.00 |
| E03 | direct | 0002 0032 | 1 | 1 | 0 | +0.000 | 0.50 -> 0.50 | 1.00 |
| E04 | direct | 0028 0029 | 1 | 1 | 0 | +0.000 | 1.00 -> 0.50 | 1.00 |
| E05 | direct | 0025 | 1 | 4 | 3 | -0.750 | 1.00 -> 1.00 | 1.00 |
| E06 | direct | 0044 | 1 | 1 | 0 | +0.000 | 1.00 -> 1.00 | 1.00 |
| E07 | paraphrase | 0036 | 1 | 5 | 4 | -0.800 | 1.00 -> 1.00 | 1.00 |
| E08 | direct | 0033 | 1 | 1 | 0 | +0.000 | 1.00 -> 1.00 | 1.00 |
| E09 | direct | 0046 0047 0048 | 1 | 3 | 2 | -0.667 | 0.67 -> 0.67 | 1.00 |
| E10 | paraphrase | 0006 0019 0030 0032 0033 | 3 | 11 | 8 | -0.242 | 0.20 -> 0.00 | 0.40 |
| E11 | direct | 0050 | 8 | 14 | 6 | -0.054 | 0.00 -> 0.00 | 1.00 |
| E12 | direct | 0057 0062 | 1 | 4 | 3 | -0.750 | 1.00 -> 0.50 | 1.00 |
| E13 | direct | 0059 0065 | 7 | 5 | -2 | +0.057 | 0.00 -> 0.50 | 1.00 |
| E14 | direct | 0066 | 1 | 1 | 0 | +0.000 | 1.00 -> 1.00 | 1.00 |
| E15 | paraphrase | 0034 | 2 | 1 | -1 | +0.500 | 1.00 -> 1.00 | 1.00 |
| E16 | direct | 0038 | 1 | 1 | 0 | +0.000 | 1.00 -> 1.00 | 1.00 |
| E17 | direct | 0043 0045 | 1 | 2 | 1 | -0.500 | 1.00 -> 0.50 | 1.00 |
| E18 | direct | 0014 0041 | 1 | 1 | 0 | +0.000 | 1.00 -> 1.00 | 1.00 |
| E19 | direct | 0016 | 8 | 1 | -7 | +0.875 | 0.00 -> 1.00 | 1.00 |
| E20 | direct | 0023 | 1 | 1 | 0 | +0.000 | 1.00 -> 1.00 | 1.00 |
| E21 | paraphrase | 0128 | 1 | 1 | 0 | +0.000 | 1.00 -> 1.00 | 1.00 |
| E22 | direct | 0073 0114 0115 | 10 | 2 | -8 | +0.400 | 0.00 -> 0.33 | 0.33 |
| E23 | paraphrase | 0079 0080 | 6 | 7 | 1 | -0.024 | 0.00 -> 0.00 | 0.50 |
| E24 | direct | 0092 0093 | 6 | 1 | -5 | +0.833 | 0.00 -> 0.50 | 0.50 |
| E25 | direct | 0125 | 1 | 2 | 1 | -0.500 | 1.00 -> 1.00 | 1.00 |
| E26 | paraphrase | 0127 | 16 | 7 | -9 | +0.080 | 0.00 -> 0.00 | 1.00 |
| E27 | paraphrase | 0122 | 1 | 6 | 5 | -0.833 | 1.00 -> 0.00 | 1.00 |
| E28 | direct | 0131 0132 | 1 | 1 | 0 | +0.000 | 1.00 -> 1.00 | 1.00 |
| E29 | direct | 0129 0131 | 1 | 2 | 1 | -0.500 | 1.00 -> 0.50 | 1.00 |
| E30 | direct | 0061 0069 | 1 | 1 | 0 | +0.000 | 0.50 -> 0.50 | 1.00 |
| E31 | direct | 0119 0120 | 1 | 1 | 0 | +0.000 | 0.50 -> 0.50 | 0.50 |
| E32 | direct | 0042 | 1 | 1 | 0 | +0.000 | 1.00 -> 1.00 | 1.00 |
| E33 | direct | 0003 | 1 | 1 | 0 | +0.000 | 1.00 -> 1.00 | 1.00 |
| E34 | paraphrase | 0012 | 1 | 1 | 0 | +0.000 | 1.00 -> 1.00 | 1.00 |

## 3. Aggregates, all descriptive

MRR is truncated at 20. Each interval is a paired bootstrap of mean ΔRR (10,000 resamples of queries, seed 147). The sign test is exact, two-sided, with ties dropped.

| stratum | queries | hit@1 | hit@5 | mean recall@5 | MRR (to 20) | mean candidate recall@20 | RR wins / losses / ties | sign test p | mean delta RR, 95% bootstrap |
|---|---:|---|---|---|---|---:|---|---:|---|
| all | 34 | 23 -> 20 | 27 -> 29 | 0.717 -> 0.691 | 0.747 -> 0.701 | 0.919 | 8 / 11 / 15 | 0.648 | [-0.190, +0.099] |
| direct | 24 | 19 -> 15 | 19 -> 23 | 0.715 -> 0.729 | 0.819 -> 0.754 | 0.931 | 4 / 7 / 13 | 0.549 | [-0.224, +0.098] |
| paraphrase | 10 | 4 -> 5 | 8 -> 6 | 0.720 -> 0.600 | 0.573 -> 0.574 | 0.890 | 4 / 4 / 2 | 1.000 | [-0.317, +0.301] |
| single-record | 18 | 13 -> 12 | 15 -> 15 | 0.833 -> 0.833 | 0.786 -> 0.741 | 1.000 | 4 / 5 / 9 | 1.000 | [-0.251, +0.158] |
| multi-record | 16 | 10 -> 8 | 12 -> 14 | 0.585 -> 0.531 | 0.703 -> 0.657 | 0.827 | 4 / 6 / 6 | 0.754 | [-0.249, +0.162] |

## 4. What it says, without promising improvement

- **Inconclusive overall.** Every stratum's interval contains zero, and no sign test is near any conventional threshold. With 34 queries, a mean ΔRR anywhere from about −0.19 to +0.10 is consistent with the data.
- **The two orders trade places rather than one dominating:**
  - re-ranking lost first place on 8 queries (E05, E07, E09, E12, E17, E25, E27, E29);
  - it gained first place on 5 (E01, E02, E15, E19, E24).
  - hit@1 falls by 3 while hit@5 rises by 2.
- **The largest changes:**
  - **Gains:** E19 (rank 8 → 1), E22 (10 → 2), E24 (6 → 1) and E26 (16 → 7).
  - **Losses:** E10 (3 → 11), E27 (1 → 6), E11 (8 → 14) and E07 (1 → 5).
- **Paraphrase queries:** MRR is flat (0.573 → 0.574), but mean recall@5 falls (0.720 → 0.600). That's 10 queries, too few to say more.
- **The candidate stage misses some supporting records:** mean candidate recall@20 is 0.919. Five queries lose a supporting record before re-ranking can see it:
  - E10, 0.40: two of its five records were found;
  - E22, 0.33;
  - E23, 0.50; E24, 0.50; E31, 0.50.
  - **Re-ranking can't recover these:** they're outside the candidate set, whatever passage the cross-encoder scores.
  - **Candidate construction and the cross-encoder's passage representation are separate questions** (review round 1). This record measures neither beyond candidate recall@20.
- **What it doesn't say:** whether a cross-encoder helps in general, or whether a different passage choice, candidate count or score blend would. Those are tuning questions for a separately written query set.
- **This rubric stays evaluation-only.**

## 5. Latency, reported separately

| step | time |
|---|---:|
| U1′ end to end (load 4.8 s, embed 3,461 passages 61.6 s) | 66.7 s |
| U5 retrieval (load, chunk, embed, similarity, top 20) | 69.8 s |
| U5 re-ranking, 680 pairs | **14.0 s** |
| Hugging Face `CrossEncoder.predict`, the same 680 pairs | 10.6 s (median of 9.4, 10.6 and 11.2 s, rerun from the saved outputs; the first run's median was 11.3 s) |

Re-ranking scales with the pairs: 2.0 s for 100 pairs in 0146, 14.0 s for 680 here.

## 6. Findings

- **F1, a no-op corruption control in 0146:**
  - **The control:** "a passage moved to another document" moved candidate 1's passage to its neighbouring pid's document.
  - **The no-op:** on 0147's data, candidate 1 (pid 94) is inside a document, so both neighbours belong to the same document and nothing changed. The comparer rightly accepted the unchanged data, and the control reported the acceptance.
  - **Not a validator gap:** on 0146's data the candidate sat at a document boundary, so the move was real.
  - **The fix:** the control now moves the passage to the first document that isn't its own. It's refused on both records' outputs (`probes/0146/out/compare-controls.txt` is updated).
- **F2:** `compare_u5.py`'s success message said "the 100 pairs'" whatever the count. It now reports the count.
- **R1, U1′ was unbound** (review round 1): round 0's evaluator compared U1′ with U5 only at ranks where they differed, and accepted any differing row with an equal score as a "tie". Codex showed it accepting a nonexistent document and a NaN score. Fixed as in gate 4.
- **No judgment errors are reported.** The judgments weren't revisited after the run. Any error found later is reported both as frozen and as corrected.

## Files

- **Scripts** (`probes/0147/`): `run.sh`, `preflight.py`, `preflight_controls.sh`, `kinds.py`, `evaluate.py` and `u1_controls.py`.
- **Outputs** (`probes/0147/out/`):
  - `run.txt`, `gates.txt`;
  - `evaluation.md` and the four control files;
  - `u1/u1.tsv`;
  - `u5-script/` and `u5-twin/`. Their retained passages are byte-identical to 0146's, a single git object.
  - the session excerpts `u1-session.txt` and `u5-session.txt`.
