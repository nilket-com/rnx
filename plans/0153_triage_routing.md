# rnx 0153: triage routing, the design and development half (a fresh Rhai holdout for 0154)

**Status:** plan. **The user (2026-10-02):** triage next. The evaluation should measure useful routing, and the trade-off between errors and review workload. Fresh annotations on previously scored tickets aren't unseen inputs, so the holdout strategy comes first. The design was agreed with Codex before this plan.

**Amended after Codex's design review (R1–R3),** before the freeze:
- **R1:** mandatory REVIEW and the edge-case policy;
- **R2:** the numerical and ordering contract;
- **R3:** metadata provenance and annotation-file validation.

**Why two records:** the same shape as 0151 and 0152.
- **This record** freezes the design, the data and both blind holdout annotations. It then develops mechanically on development data only, and its impl commits the selected policy.
- **0154 runs it once on the holdout,** with the outcome rule frozen **here,** verbatim.

## 0. What an analyst does with the output

A triager receives each new issue in one of five places:

| queue | what the triager does |
|---|---|
| **bug** (performance folded in) | the maintainers' defect triage: reproduce, prioritize, fix |
| **feature** | the roadmap backlog: discussed and scheduled, not urgent |
| **question** | support: answer, link docs or move to discussions |
| **documentation** | the docs backlog |
| **REVIEW** | a human reads the issue and routes it by hand |

**The trade-off:**
- **Every ticket auto-routed** saves a human review.
- **Every misroute** costs more than a review. A bug sitting in the question queue is handled late, and a question in the bug queue wastes maintainer time.

**How it's reported:**
- **Workload:** the share of tickets sent to REVIEW.
- **Misroute rate:** wrong queues among the auto-routed tickets.
- **A cost view:** cost = W + c × misroutes / N, with c ∈ {2, 5}. A misroute costs c reviews. Both values are declared now and reported descriptively, without fitting any cost.

## 1. The data and the holdout strategy

**D2,** rune-rs/rune's 252 issues (0133):
- **Every ticket has already been scored,** by 0139, 0148 and 0149. **D2 is never called unseen.**
- **Here it's diagnostic only:** the selected policy is reported on 0139's 40-ticket sample against its two existing annotation sets.
- **It plays no part in selection,** so pooling can't hide Rhai errors behind Rune's class mix.

**D3, the new corpus:** `rhaiscript/rhai` issues, a sibling embedded scripting language for Rust. **No system has scored any of them.**
- **The fetch** (`probes/0153/fetch_d3.sh`): 0133's method, unauthenticated GitHub REST.
  - `state=all`, pull requests excluded, and full cursor pagination following `rel="next"` to the end.
  - It stores each issue's number, title, body (CRLF normalized), labels, state and timestamps.
  - **The text stays local** in `$RNX_UAT_DATA/d3`, as `issues.json` sorted by number. Only the manifest is committed, with three columns per issue:
    - the **canonical decimal** issue number;
    - **content:** the SHA-256 of the normalized title + "\n" + body;
    - **labels (R3):** the SHA-256 of the issue's label names, sorted and joined by "\n". The names themselves aren't committed, so annotators can't see them in the repository.
  - **`d3-meta.json`** records the fetch time, the maximum `updated_at`, and the SHA-256 of the whole normalized snapshot: `json.dumps(issues, sort_keys=True, separators=(",", ":"), ensure_ascii=False)` as UTF-8.
  - **Gate 0 checks all three** against the local snapshot, so any change to the text, labels or state is drift and stops the run.
  - **The snapshot** is the fetch; nothing fetched later enters.
- **The split** (`probes/0153/split_d3.py`, replayable):
  - **D3-hold:** the 150 tickets with the smallest `sha256("rnx-0153-hold:" + n)`.
    - **n** is the canonical decimal issue number: ASCII, no sign, no leading zeros.
    - **The order** is the lowercase hex digest, ascending, with exact ties to the smaller number.
  - **D3-dev:** every other ticket.
  - **Each file** is ordered by ticket number. The script refuses duplicates, omissions and drift.

**What the result is scoped to:** routing **Rhai** issues. It says nothing about generalizing to Rune or to support-ticket populations. The tickets are a repository's issues, not a random sample of anything wider.

## 2. Labels, queues and annotation

**The annotation scheme:** 0139's five labels and rules, with one change. The descriptions name **Rhai** where 0139's named Rune (`probes/0153/frozen/labels.tsv`):

| label | description |
|---|---|
| bug | A defect report: something in Rhai behaves incorrectly, crashes, panics, or produces a wrong result. |
| feature | A feature request: a proposal to add new functionality, syntax, or an API to Rhai. |
| question | A question asking how to do something with Rhai, or asking for help or clarification. |
| documentation | A documentation issue: missing, unclear, or incorrect documentation, examples, or comments. |
| performance | A performance issue: Rhai is slow, uses too much memory, or compiles or runs inefficiently. |

**The rules** (`frozen/rules.md`):
- **Pinned source:** 0139's five **classification rules**, verbatim, from `probes/0139/frozen/rules.md` at SHA-256 `8aa28e2af45288406023237baa3a193e30bf816450e9181c0f124be876a4c928`.
- **Not carried over:** that file's sample-specific format paragraph ("all 40", "sample.tsv's order").
- **The new annotation format (R3):**
  - a header line `number\tlabel`, then exactly one row per ticket of the split being annotated;
  - D3-hold: **150 rows**; D3-dev: **every D3-dev ticket**;
  - rows in the split file's order (ascending number);
  - each label exactly one of the five, lowercase; no blank, missing, extra or duplicate rows.
- **The validator** (`probes/0153/frozen_check.py`) enforces all of this for every annotation file, and gate 0 runs it.

**Queues:** performance → bug, and every other label is its own queue. **Folding changes routing, not annotation semantics:**
- annotators assign the five labels;
- agreement, errors and the reference all use the **four queues,** after folding.
- Five-label agreement is reported as well.

**The annotators' view** (`probes/0153/annotator_view.py`) shows exactly the ticket number, the title and the body, in split order.
- **Hidden from annotators:** maintainer labels, state, any system output and the other annotator's labels.
- **Labels are made from that text alone,** independently, and are never revised after any score.

**Who annotates what:**
- **D3-hold (150): Claude and Codex, independently.** Both sets are committed before any variant runs on any D3 ticket.
- **D3-dev (346 if D3 has 496 issues): Claude alone.**
  - This is the development reference, a single agent annotator, and it's disclosed as such.
  - Development is therefore judged against a less strict reference than the holdout's worst case below.
  - It's committed before any variant runs.

**Maintainer labels** are an external comparison, never ground truth. They're mapped as follows (`frozen/maintainer_map.tsv`):

| maintainer labels | mapped queue |
|---|---|
| bug, regression | bug |
| enhancement, new feature | feature |
| question | question |
| docs | documentation |

- **The labels are matched case-insensitively.**
- **A ticket whose mapped labels name two or more queues** is `multi`.
- **No mapped label** means `none`.
- **`multi` and `none`** are excluded from the maintainer comparison and counted.

## 3. The freeze sequence (review: design before any holdout text is read)

1. **This plan file is the design.** Codex's acceptance records its SHA-256. It's never edited again; the later amends add files only.
2. **After acceptance:**
   - fetch D3 and commit the manifest and split;
   - generate the annotator views;
   - Codex and Claude annotate D3-hold independently;
   - Claude annotates D3-dev.
3. **The plan commit is amended** with `probes/0153/frozen/` (manifest, split, both holdout annotation files, the dev annotation file, labels, rules, the maintainer map). This keeps the plan + impl cadence; Codex judged that an amend with a byte-identical design file is acceptable, and otherwise a transparent extra freeze commit.
   - **Gate 0 checks:**
     - the design file's SHA-256 against the accepted hash;
     - 0139's `rules.md` pin;
     - every annotation file, through `frozen_check.py`;
     - every frozen file's hash, as listed in `frozen/SHA256SUMS`;
     - the D3 manifest (content and label hashes) and the snapshot hash against the local snapshot;
     - the split and fold replays.
4. **Only then does any variant run,** on D3-dev and D2 only. D3-hold isn't read by any system until 0154.

**All design discretion is frozen above and below,** before either annotator reads holdout text: the regexes, the representations, the hypotheses, the confidences, the threshold grid, the selection and the targets. Development is mechanical.

## 4. The bounded menu

**Eligible for selection: K, Z, N and C.**

**MAJ is diagnostic only:** everything goes to the most frequent queue in Claude's D3-dev labels, with ties to the queue order bug, feature, question, documentation. It's reported but never selected.

**ALL-HUMAN** sends everything to REVIEW. It's the explicit fallback when no eligible policy qualifies.

**K, frozen keyword rules** on the **lowercased title only:**

| queue | regex (Python `re.search`) |
|---|---|
| bug | `\b(panic(s\|ked)?\|crash(es\|ed)?\|segfault\|errors?\|fail(s\|ed\|ing\|ure)?\|broken\|bug\|wrong\|incorrect\|regression\|unexpected(ly)?\|doesn'?t work\|does not work\|overflow\|memory leak\|slow(er)?\|performance)\b` |
| feature | `\b(feature request\|support for\|add(ing)? support\|would be nice\|allow(ing)?\|proposal\|propose\|request\|implement\|new feature\|enhancement\|ability to)\b` |
| question | `(\?\s*$)\|\b(how (do\|can\|to\|should)\|is (it\|there) (possible\|a way)\|what is\|why (does\|is)\|question)\b` |
| documentation | `\b(docs?\|documentation\|readme\|typo\|book\|examples?)\b` |

- **The `|` in this table is escaped as `\|` for Markdown.** `frozen/keywords.tsv` holds the same regexes unescaped, and gate 0 checks that it equals this table with `\|` read as `|`.
- **Exactly one queue matching** gives that queue, with confidence 1.
- **None, or two or more,** gives mandatory REVIEW.
- **K's only threshold is 1.**

**Z, 0139's zero-shot:**
- **The representation is unchanged:** title + "\n" + body, `enc.chunk(text, 0)`, `candle::segment_mean`, renormalized, with the pinned MiniLM.
- **The scores:** cosine to each of the five frozen descriptions, embedded.
- **A queue's score** is the maximum over its member labels.
- **The prediction** is the arg-max queue, with ties to the queue order.
- **Confidence** is the top queue score minus the second.

**N, 0148's NLI:**
- **The premises** are the same passages; the hypotheses are each frozen description, verbatim and untemplated.
- **A label's score** is the maximum softmax p(entailment) over the ticket's passages.
- **A queue's score** is the maximum over its member labels. The prediction is the arg-max, with ties to the queue order.
- **Confidence** is the top queue score.

**C, a nearest centroid over Z's unit ticket embeddings,** trained only on D3-dev's labels:
- **A centroid** is the mean of a queue's unit embeddings, renormalized.
- **A queue is absent** when it has no training ticket, or its mean has norm 0.
- **The prediction** is the arg-max cosine over present queues, with ties to the queue order. **Confidence** is the top cosine minus the second.
- **Fewer than two present queues** send the ticket to REVIEW.
- **Development is cross-fit:**
  - **Folds:** `rank(sha256("rnx-0153-fold:" + number)) mod 5` over D3-dev.
  - **Each fold** is scored by centroids from the other four.
  - So no ticket is scored against a centroid that contains its own embedding or label.
- **For 0154:** the centroids from all of D3-dev are frozen as an artifact in 0153's impl (f32 bits). The rules for missing queues and ties are the same.

**Dropped (review):**
- **G, 0149's generator:** there's no well-defined multiclass confidence, and it has a known option-order bias. It needs its own contract in a future record.
- **Fusion:** search work, parked.

## 5. The routing policy and selection

- **The representation (R1):** each system gives each ticket either a prediction (one of the four queues) with a confidence, or **mandatory REVIEW,** with no confidence.
- **Auto-routing** requires **both:** a queue prediction **and** a finite confidence ≥ t. Everything else, mandatory REVIEW included, goes to REVIEW at every t, t = 0 and the lowest candidate included.
- **Denominators:** every ticket of the split counts in N, workload and A. Mandatory-REVIEW tickets are never omitted. Each system's mandatory-REVIEW count is reported by cause.
- **The threshold candidates for Z, N and C:** the distinct finite confidences of the **predicted** tickets on D3-dev, ascending. Mandatory-REVIEW tickets contribute none. Tickets with equal confidence move together, as a tie block.
- **K's only candidate** is t = 1.
- **On D3-dev,** against Claude's dev labels:
  - auto-share A(t) = auto-routed / N;
  - misroute rate E(t) = misrouted / auto-routed.
  - **E is undefined when nothing is auto-routed.** An undefined E is never treated as 0 and can't meet a target.
- **Each eligible system's dev threshold** is the **lowest** candidate t with a defined E(t) ≤ **0.10**. A system with no such t is ineligible.
- **Selection:** the eligible system with the **highest dev A(t)** at its threshold. Exact ties go to the simpler system, in the order **K, Z, C, N**.
- **If no system is eligible, the selection is ALL-HUMAN.** 0154 then reports it, and its target can't be met.
- **0153's impl commits** the selected system, its threshold and its artifacts (C's centroids if C is selected).

## 5a. The edge cases (R1), frozen

Each case is either a declared mandatory REVIEW, counted in every denominator, or a named stop of the whole record before evaluation.

| case | Z / C | N | K |
|---|---|---|---|
| title + body yields **no passages** | mandatory REVIEW (no embedding) | mandatory REVIEW | unaffected (title regex) |
| ticket embedding has **norm 0** or isn't finite | mandatory REVIEW | unaffected | unaffected |
| `NliModel` refuses a ticket's pair **by name for its 512-token pair limit** (scored per ticket, so one refusal marks that ticket) | unaffected | mandatory REVIEW for that ticket | unaffected |
| any **other** adapter error | **stop** | **stop** | n/a |
| a non-finite logit, cosine, probability or confidence where an embedding exists | **stop** (a numerical fault) | **stop** | n/a |
| C has **fewer than two present queues** | mandatory REVIEW | n/a | n/a |

- **No ticket is silently omitted,** and no workaround is chosen after D3 is seen.
- **The stop rule applies to the whole record:** fix the plumbing, disclose it, and send it for review before any replay.

## 5b. The numerical and ordering contract (R2), frozen

**Z:**
- The cosines are the f32 values rnx returns.
- A queue's score is the maximum of its members' f32 values.
- The margin is (top − second), computed in f64 from those f32 values.
- Ties are exact f32 equality, broken by the queue order.

**N:**
- The logits are f32, promoted to f64, in `id2label` order (contradiction, entailment, neutral).
- **Stable softmax:** m = max of the three; e_i = exp(l_i − m) in f64; p(entail) = e_entail / (e_contra + e_entail + e_neutral), summed in that order.
- A label's score is the f64 maximum over the ticket's passages, and a queue's score the maximum over its members.
- Confidence is the f64 top queue score. Ties are exact f64 equality, broken by the queue order.

**C:**
- **Input:** each ticket's unit embedding, 384 f32 values from the validated retained evidence.
- **A centroid's sum:** per component, the f64 sum of the training tickets' values, in **ascending ticket number** order.
- **Its norm:** the square root of the f64 sum of squares, in component order.
- **Absent queues:** a queue is absent if it has no training ticket, or its norm is 0 or not finite.
- **The stored centroid:** each component is sum / norm in f64, **rounded to f32**. Both the cross-fit centroids and the full-dev artifact are produced by this same function. The artifact stores the f32 bits; it's used by reading those bits.
- **Cosine:** the f64 sum, in component order, of (ticket component as f64) × (centroid component as f64). The ticket embedding is unit already and isn't renormalized.
- **Confidence** is (top − second) in f64. Ties are exact f64 equality, broken by the queue order.
- **Folds:** over D3-dev, sort by (`sha256("rnx-0153-fold:" + n)` hex ascending, then number ascending), with n canonical decimal. A ticket's fold is its **zero-based** rank mod 5.

**Fixtures and controls, run before any D3 inference:**
- **Centroids, folds and ties:** a small synthetic fixture, independently computed. The production code is pure Python; the check uses numpy with explicit f64 and f32 casts. It covers a missing queue, a zero-norm queue, exact ties and the f32 rounding.
- **Threshold boundaries:**
  - confidence == t auto-routes; the next lower representable value doesn't;
  - mandatory REVIEW never auto-routes at t = 0 or at the lowest candidate;
  - zero auto-routes leave E undefined, so the system is ineligible;
  - tie blocks move together.
- **Replays:** the split and the folds.
- **Frozen-file corruption:** each refused by gate 0. The cases are a changed design file, a changed rules, labels, keywords or map file, an annotation file with a missing, extra, duplicate, out-of-order, blank or out-of-vocabulary row, a changed manifest, and drift in the snapshot or its labels.

## 6. Measures

**Development, D3-dev** (Claude's labels), per system:
- the full risk–coverage curve, A(t) against E(t);
- the cost view, c = 2 and c = 5;
- misroutes by reference queue × predicted queue at the chosen threshold;
- the confusion at full coverage.

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

**D2 diagnostic:** the selected policy on 0139's 40-ticket sample, against both existing annotation sets, folded to queues. Descriptive only; it was seen before.

## 7. Implementation and parity, gated

**The producers** emit retained evidence; all decisions are made by a Python evaluator from **validated** retained scores, as in 0151.
- **The session script** (`probes/0153/triage.rn`, a session paste) runs over D3-dev and D2:
  - the passages, by ticket and byte range;
  - the per-ticket unit embeddings (Z and C's input);
  - the five description cosines;
  - every (passage, hypothesis) NLI logit triple.
- **The twin** (`twin0133`, new mode `t3`) produces the same independently.
- **The gates, fail-closed** (gate 0 as in section 3):
  - each producer validated against D3/D2 and its own evidence;
  - the producers bit-equal;
  - **semantic checks against HF:** sentence-transformers MiniLM, within 1e-4, and the NLI logits fed the pinned token ids, within 1e-4, as 0148's model-level gate. Native-tokenizer agreement is reported separately, as in 0148;
  - completion artifacts with exit 0 or 3, and dimensions verified;
  - corruption controls on both producers plus one one-sided.
- **The rehearsal before anything runs on D3:** all plumbing first runs on D2. This is mechanical verification; D2 is spent.
- **The stop rule:** an unexplained semantic mismatch, or a need for new adapter behaviour, stops the record.
- **Nothing new ships,** so there's no after-push `:dep` check.

## 8. The hand-off to 0154

0153's impl ends with:
- the selected system, its threshold and its frozen artifacts;
- the development tables;
- D3-hold's manifest and both annotation files re-verified unchanged.

**0154's plan** states:
- the selection;
- K and ALL-HUMAN unchanged as comparators;
- one locked run on D3-hold;
- section 6's measures and outcome rule, **verbatim**;
- repairs only to plumbing, sent for review before any replay, and every attempt disclosed;
- no change after results;
- a negative result closes as a negative result.

## Out of scope

- G, or any generative classifier.
- Fusion.
- New labels.
- Tuning on D3-hold.
- Generalization claims beyond Rhai issues.
