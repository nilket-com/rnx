# rnx 0154: triage holdout, one locked evaluation of 0153's routing policy

**Status:** plan, amended for Codex's review (R1, R2 and the cost wording) before acceptance. This is the second half of the triage evaluation the user asked for: useful routing and the trade-off between errors and review workload, on a holdout fixed before development.
- 0153 (closed at 064aa52) froze the design, D3 and both blind holdout annotations before any system ran. It then selected a policy mechanically on D3-dev.
- **This record scores that policy once, on the 150 D3-hold tickets.**
- **D3-hold has not been read by any system.**

## 1. What is frozen (nothing here changes after any holdout result)

**The policy,** C, the nearest centroid:
- **The centroids:** the full D3-dev centroids, as f32 bits in `probes/0153/out/dev/centroids.tsv` (SHA-256 `5dd804d3…74e7c972`).
- **The threshold:** a ticket auto-routes when its confidence ≥ **0.09553107383376902**; otherwise it goes to REVIEW.
- **The record:** `probes/0153/out/dev/selected.json` (SHA-256 `022543f0…c4017406`).
- **The scoring:** 0153's design sections 4, 5, 5a and 5b, unchanged.
  - The confidence is the f64 cosine of the top queue minus the second, with ties to the queue order.
  - Mandatory REVIEW applies to fewer than two present queues, and to a ticket whose embedding has no passages, is zero or is non-finite.
- **The design file:** `plans/0153_triage_routing.md` at SHA-256 `b1b75573…`.

**The comparators, unchanged:**
- **K:** the frozen keyword rules. Exactly one queue matching routes; none or several give REVIEW.
- **ALL-HUMAN.**

**The inputs:**
- **The tickets:** D3-hold, `probes/0153/frozen/d3-hold.tsv` (150 tickets).
- **The labels:** both blind annotation files, `annotations-claude-hold.tsv` and `annotations-codex-hold.tsv`, verified through `frozen/SHA256SUMS` (`c0f9163f…`).
- **The source text:** the D3 snapshot, through the manifest and the snapshot hash.

## 2. The measures and the outcome rule (0153 section 6, carried verbatim)

**The holdout (0154), on D3-hold,** with both annotators; **all 150 tickets count**:
- **Workload:** REVIEW / 150. **Auto-share** A = auto / 150.
- **Errors among the auto-routed,** with the annotations folded to queues:
  - **certain error:** the prediction matches **neither** annotator;
  - **possible error:** the prediction differs from **at least one** annotator. This is the worst case.
- **The primary misroute rate** is E_possible = possible errors / auto-routed.
- **Secondary:** E_certain; E on consensus tickets only; and an either-label-correct analysis.
- **Annotator agreement:** on four queues and on five labels, with Cohen's κ and the disagreement counts.
- **Also reported:** misroutes by queue; the maintainer comparison; the cost view; K and ALL-HUMAN beside the selection.
- **All intervals** are Wilson 95% (z = 1.96).

**The 0154 outcome rule, frozen here and carried verbatim:**
- **"Routing target met"** if the Wilson upper bound of the holdout **E_possible ≤ 0.20** **and** the Wilson lower bound of the holdout **A ≥ 0.25**.
- **Otherwise "not demonstrated".**
- **ALL-HUMAN,** or zero auto-routes, is "not demonstrated".

**What the targets are:** operational design targets, frozen now. They aren't empirically justified or calibrated guarantees, and "not demonstrated" doesn't imply no practical utility. This is an agent-reference evaluation on a non-random task set, not human acceptance.

**The details these measures need, fixed now** (they apply section 6; they don't change it):
- **The cost view:** two separate costs, never added together, for c ∈ {2, 5}:
  - W + c × possible errors / 150;
  - W + c × certain errors / 150.
- **E on consensus tickets only:** the auto-routed tickets where both annotators agree on the queue. Its denominator is those tickets.
- **Either-label-correct:** a routed ticket is correct if it matches either annotator. That makes it 1 − E_certain, reported with its count.
- **Misroutes by queue:** for every possible error, (Claude's queue, Codex's queue) → predicted queue.
- **The maintainer comparison:** each ticket's maintainer labels are mapped by `frozen/maintainer_map.tsv`.
  - **`multi` and `none` tickets are excluded and counted.**
  - The agreement of the selection's auto-routes, and of each annotator, with the mapped queue is reported.
  - This is external and descriptive, never ground truth.
- **K** gets the same measures at its only threshold.
- **The development comparison:** 0153's development A 0.214 and E 0.095 beside the holdout figures.
- **The pre-holdout observation,** already disclosed in 0153 section 4: the development A is below the target's 0.25 lower bound. Nothing changes because of it.

## 3. Implementation and parity, gated

**The producers:** 0153's producers, unchanged.
- **The session** (`probes/0153/triage.rn`) and **the twin** (`twin0133 t3`) run over `d3-hold.tsv`.
- The NLI evidence they also produce is retained and gated, but **not scored**: N isn't a comparator.

**The run** (`probes/0154/run.sh`), fail-closed, in order:

| step | what it does |
|---|---|
| gate 0 | 0153's `preflight.py` (the freeze, D3, the split and folds, D2, the model pins), plus 0154's pins: the centroids, `selected.json`, and 0153's `evaluate.py` and `validate.py` at their 064aa52 contents. |
| controls | 0153's fixtures and both producers' synthetic controls; 0154's measure fixtures (below). |
| producers | the session, then the twin, over D3-hold. |
| validation | 0153's `validate.py`: each producer validated, refusals bound to the pinned tokenizer, BIT-EQUAL. |
| semantic check | 0153's `hf_t3.py`, with its completion artifact and counts verified. |
| evaluation | `probes/0154/evaluate.py`: C (from the frozen centroid bits, read as bits), K and ALL-HUMAN; every measure in section 2; the outcome category. It imports 0153's scoring functions; nothing is reimplemented. |

**The rehearsal first,** before the locked run (0152's practice, review R1). It's plumbing verification only, never a development result and never used for selection. The new evaluator runs on D3-dev's validated evidence, with Claude's dev labels standing in for both annotators, along two separate paths:
- **(a) 0153's cross-fit predictions:** they must reproduce **74 auto-routed, 7 errors** (A 0.214, E 0.095), with every consensus-path measure equal to 0153's.
- **(b) The actual frozen policy:** the full-dev centroid bits at the frozen threshold, the holdout scoring path itself. It must reproduce an independent reference computation, which Codex found to be **88 auto-routed, 7 errors** (A 0.254, E 0.080) on the development inputs.
  - Those are in-sample figures (the centroids were trained on these tickets), so they're optimistic.
  - **They are not a development result** and change nothing.
- **Measure fixtures:**
  - hand-built cases for certain, possible and consensus-only errors and either-label-correct;
  - the folding of performance into bug;
  - the maintainer mapping's `multi` and `none`;
  - Wilson bounds at 0 and n;
  - the outcome rule at its boundaries: the upper bound exactly 0.20, the lower bound exactly 0.25, and zero auto-routes.
- **Gate-0 controls:** the pins, rerun with the new step list.

## 4. The locked run

- **D3-hold is scored once,** after this plan is accepted and the rehearsal reviewed.
- **Repairs:** only to plumbing, sent for review before any replay. Every attempt is disclosed.
- **The stop rule (review R2):** an unexplained semantic mismatch, or a need for new adapter behaviour, stops the record, which then reports the failed gate.
  - **Once any system has scored a holdout ticket, the holdout is spent,** even if a later validation or semantic check fails.
  - A repair or replay still needs prior review and disclosure, and never changes the policy, the targets or the measures.
- **After results are seen, nothing changes:** not the policy, the targets, the measures, the labels or the outcome rule.
- **A negative result closes as a negative result.** The holdout is spent by this run.
- **Nothing new ships,** so there's no after-push `:dep` check.

## 5. Evidence

`plans/0154_triage_holdout_evidence.md` will report:
- the gates, and every attempt;
- the policy's and K's full measures, ALL-HUMAN, and the outcome category;
- agreement, the maintainer comparison, and the costs.

The retained outputs go in `probes/0154/out/`.

## Out of scope

- Any other system or threshold.
- New labels or adjudication.
- Generalization beyond Rhai issues.
- N or G as comparators.
