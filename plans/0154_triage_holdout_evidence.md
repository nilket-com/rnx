# rnx 0154: triage holdout, the evidence

**Outcome (the rule frozen in 0153 section 6 and carried verbatim): not demonstrated.** Both conditions failed:
- **Workload:** the policy, C at t = 0.09553107383376902, auto-routed **25 of 150** holdout tickets. A = 0.167, Wilson 95% [0.116, 0.234]. **The lower bound, 0.116, is below the 0.25 target.**
- **Errors:** among those 25, the worst-case misroute rate was **E_possible = 0.200** (5 of 25), Wilson [0.089, 0.391]. **The upper bound, 0.391, is above the 0.20 target.**

**What this means:**
- On these 150 frozen Rhai issues, the routing policy didn't demonstrate useful routing under the frozen targets.
- That doesn't imply no practical utility: with certain errors only, E is 0.120.
- This is an agent-reference evaluation on a repository's issues, not a random sample and not human acceptance.

There was **one attempt,** from the committed, clean tree at **d68814c**, after Codex reviewed the rehearsal. Every gate passed and nothing was repaired. **The holdout is spent.**

## 1. Gates (attempt 1, `out/holdout/run.txt`)

| gate | result |
|---|---|
| gate 0 | 0153's freeze and provenance hold. 0154's pins match 064aa52: the centroids, `selected.json`, and 0153's `evaluate.py` and `validate.py`. |
| controls, before any holdout input | 0153's 35 fixtures, 0154's 22 measure fixtures, the NLI handler controls, and the twin's synthetic embedding controls. |
| producers | **BIT-EQUAL:** 150 tickets, 312 passages, 150 embeddings × 384, 150 × 5 cosines and 1,560 NLI pairs, with refusals bound to the pinned tokenizer. All NLI and embedding statuses are `ok`, so there was no mandatory REVIEW from the edge cases. |
| semantic check | MiniLM embeddings within 2.22e-7 and cosines within 5.94e-7; NLI logits within 1.82e-5. Completed PASS, with the counts verified. The native tokenizer matched the pinned ids on every pair. |
| reproduction | `evaluate.py holdout` over the committed, gzipped evidence reproduces `result.json` byte for byte. |

The rehearsal and the gate-0 controls, from before the run, are in `out/rehearsal/`. Codex reviewed them before approving the run.

## 2. The holdout results (all 150 tickets count)

| | C (the selected policy) | K (keyword rules) | ALL-HUMAN |
|---|---:|---:|---:|
| auto-routed | 25 | 51 | 0 |
| A [Wilson 95%] | 0.167 [0.116, 0.234] | 0.340 [0.269, 0.419] | 0 |
| workload (REVIEW share) | 0.833 | 0.660 | 1.000 |
| **E_possible** (primary) [Wilson] | **0.200** (5) [0.089, 0.391] | 0.275 (14) [0.171, 0.409] | undefined |
| E_certain [Wilson] | 0.120 (3) [0.042, 0.300] | 0.255 (13) [0.155, 0.389] | undefined |
| E on consensus tickets | 0.091 (2 of 22) | 0.260 (13 of 50) | — |
| either-label-correct | 22 of 25 (0.880) | 38 of 51 (0.745) | — |
| cost, possible misroutes, c = 2 / 5 | 0.900 / 1.000 | 0.847 / 1.127 | 1.000 |
| cost, certain misroutes, c = 2 / 5 | 0.873 / 0.933 | 0.833 / 1.093 | 1.000 |
| mandatory REVIEW | 0 | 99 (no match, or several) | all |

**C's misroutes,** as (Claude, Codex) → predicted:
- (question, question) → documentation: 2;
- (feature, question) → documentation: 1;
- (feature, documentation) → documentation: 1;
- (bug, feature) → bug: 1.

**Four of the five possible errors go to documentation,** and **all three certain errors** do. In development, five of seven did. Two of the five possible errors are annotator disagreements on which C matched one annotator.

**K's misroutes:**
- (feature, feature) → question: 6;
- (feature, feature) → bug: 4;
- (bug, bug) → question: 2;
- (question, feature) → question: 1;
- (question, question) → bug: 1.

**The cost view** (descriptive, with c declared in advance):
- **At c = 2,** C costs less than ALL-HUMAN (0.900 and 0.873 against 1.000).
- **At c = 5 with possible misroutes,** C costs exactly as much as ALL-HUMAN (1.000).
- **K costs more than ALL-HUMAN at c = 5** under both counts.

**Development (0153, cross-fit, Claude's labels) against the holdout, for C:**

| | development | holdout |
|---|---:|---:|
| A | 0.214 | 0.167 |
| E | 0.095 | 0.200 (E_possible), 0.120 (E_certain) |

0153 section 4 had already disclosed that development's A was below the target's 0.25 lower bound. The holdout share came out lower still.

## 3. Annotators and the maintainers

**Annotator agreement,** from the frozen files:
- **Four queues:** 140 of 150 (κ 0.902). **Five labels:** 138 of 150 (κ 0.888).
- **The 10 queue disagreements:**
  - feature | question: 4;
  - question | feature: 2;
  - bug | feature: 1;
  - bug | question: 1;
  - question | bug: 1;
  - feature | documentation: 1.

**The maintainer comparison** (external and descriptive, never ground truth):
- **Coverage:** 87 holdout tickets have exactly one mapped maintainer queue; 59 have none and 4 have several, and those are excluded.
- **Agreement with the maintainers:** Claude's queue matches on 57 of 87, Codex's on 55 of 87.
- **C's auto-routes:** 14 have a mapped maintainer queue, and C agrees on 8 of them.

## 4. What this closes, and what it leaves

- **Closed as a negative result:** the frozen routing policy didn't meet the frozen targets on the Rhai holdout. Nothing changed after results: not the policy, the targets, the measures nor the labels.
- **What isn't established:**
  - that triage routing is useless;
  - that a different threshold, system or target would succeed. Any of those would need fresh, unseen issues.
- **Observations, not findings:**
  - **C is cautious:** it routes few tickets.
  - **Its errors cluster on the documentation queue,** the smallest class, both in development and on the holdout.
  - **The keyword rules route twice as many** tickets, at more than twice the certain error rate.

## 5. Costs

- **The session:** 5.2 s for embedding and cosines; 96.5 s for the NLI pairs, which were retained and gated but not scored. It returned to a prompt after 102.6 s.
- **The HF check:** 85 s.
- **C's scoring:** negligible.

## 6. Files

- **Code:** `probes/0154/`: `evaluate.py`, `fixtures.py`, `reference_b.py`, `preflight.py`, `run.sh`, `rehearse.sh` and `preflight_controls.sh`. The producers and the validator are 0153's, unchanged.
- **`out/holdout/`:**
  - both producers' gzipped evidence;
  - `result.json`, `hf-t3.json`, `run.txt` and `session-excerpt.txt`.
- **`out/rehearsal/`:** the pre-run rehearsal, the fixtures and the gate-0 controls.
