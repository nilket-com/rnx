# rnx 0139: zero-shot triage of support tickets (a UAT workflow)

Status: plan, agreed in direction with Codex after 0137 (0138 maintenance first). It follows the user's direction: useful, model-assisted analytics in an ordinary `:dep` session. No new tensor API is expected. A blocker becomes a finding, not scope growth.

**The question an analyst asks:** of the project's 252 support tickets (0133's D2), which are bugs, feature requests, questions, documentation and performance? Which need a human look first?

**The answer U4 gives:**
- **No training:** the model compares each ticket with a short description of each category.
- **The output:** a Polars summary (the count per category), a **review queue** of the least certain tickets, and per-category examples, displayed in the session.

## 1. Frozen before any score is computed

Everything in this section is committed, in this plan and `probes/0139/frozen/`, before the workflow first runs: the plan commit is amended with the frozen files, the annotations included, before the impl's first run. The evidence shows the commit order.

**The data and model:** 0133's D2 (`probes/0133/d2-manifest.tsv`, the 252 tickets) and 0131's pinned MiniLM. Both are verified by 0133's checks.

**The label set and descriptions** (`frozen/labels.tsv`). One line each is embedded as the label's text:

| label | description (embedded) |
|---|---|
| bug | "A defect report: something in Rune behaves incorrectly, crashes, panics, or produces a wrong result." |
| feature | "A feature request: a proposal to add new functionality, syntax, or an API to Rune." |
| question | "A question asking how to do something with Rune, or asking for help or clarification." |
| documentation | "A documentation issue: missing, unclear, or incorrect documentation, examples, or comments." |
| performance | "A performance issue: Rune is slow, uses too much memory, or compiles or runs inefficiently." |

**The ticket text:** title, a newline and the whole body, chunked by `enc.chunk(text, 0)` (0136) and pooled per ticket by `candle::segment_mean` (0137), then renormalized. That's E4's representation.

**The scoring:**
- **Cosine scores:** every ticket against every label (a matmul of unit rows).
- **The predicted label** is the highest-scoring one. **Equal scores** go to the label earlier in the frozen order (bug, feature, question, documentation, performance).
- **The margin** is the top score minus the second.

**Abstention:** a ticket whose margin is below **0.02** is marked "review" rather than assigned.
- **What the margin is:** an uncalibrated ranking signal, not a probability. 0.02 is chosen before any score is seen, as a round value, and is reported, not tuned.
- **Coverage:** the share not abstained is reported alongside accuracy.

**The review queue** holds the 20 lowest-margin tickets, abstained or not, in margin order. **Equal margins** go by ascending ticket number.

**The evaluation sample** (`frozen/sample.tsv`): 40 tickets, chosen deterministically before any score. The ticket numbers are sorted ascending and every ⌊252/40⌋ = 6th is taken from index 0, giving the first 40 such. The rule and the resulting numbers are committed.

**The annotation rules** (`frozen/rules.md`), one primary label per ticket, with tie-breaks for the overlapping categories:
1. A ticket reporting that something that should work doesn't is **bug**, even if slow or poorly documented.
2. A slowness, memory or efficiency complaint without incorrect behaviour is **performance**.
3. A report that the docs are missing or wrong, with no code defect claimed, is **documentation**.
4. A request for new behaviour is **feature**. A request for help using existing behaviour is **question**.
5. If two still fit, the label matching the **title's** main ask wins.

**The annotators:**
- **Claude** labels the 40 tickets by these rules from their text alone, before any score.
- **Codex,** if it agrees, labels the same 40 independently.

Both sets are committed before the run. **These are agent-labelled evaluations, not human or user acceptance,** and the evidence says so.

## 2. The workflow (`probes/0139/workflow-u4.rn`)

**As a session paste in an ordinary `:dep polars candle` session:**
1. Load the tickets.
2. Chunk, embed, pool and normalize (as E4).
3. Embed the five descriptions.
4. Score, predict, compute the margins and abstain.
5. Build a Polars frame (ticket, title, predicted, margin, abstained, and the score per label).
6. Display:
   - the counts per predicted label, abstentions included;
   - the review queue (20 rows: ticket, title, the top two labels, the margin);
   - three examples per label (the highest-scoring).
7. Write the complete per-ticket table for comparison.

**Display is the deliverable:** the session transcript's actual frames are kept in the evidence (0123/0124's lesson: computation alone isn't a usable result).

## 3. Measures, kept separate

| measure | how | gated? |
|---|---|---|
| **Numerical parity** | the complete per-ticket table (all five scores' f32 bits, the prediction, the margin, the abstention) against a direct-Rust twin (`twin0133`'s chunker, S1 embedding, the same Candle pooling and scoring calls), bit for bit | **gated** |
| **Session success** | the marker, a final `Ok(())`, the frames displayed | **gated** |
| **Quality against each annotator** | on the 40: **coverage** (assigned / 40, numerator and denominator shown); **accuracy over assigned sample tickets** (correct / assigned); a 5 × 5 confusion table **of assigned sample tickets only** (reference by prediction); **abstentions reported separately, by reference label**; and every error listed with its title and its two top labels | reported, **not gated** |
| **Annotator agreement** (if Codex labels) | the two agents' agreement on the 40, and Cohen's κ | reported |
| **Cost** | U4's session time against **sentence-transformers** on the same CPU, with the same texts, chunking, pooling and normalization (mean of passage embeddings, renormalized), the same model, and disclosed thread and batch settings | reported |

## Gates

- Everything in §1 is committed before the first run (the commit order shown).
- U4 passes as a session paste, displays its frames, and is bit-equal to its twin.
- **The suites, after push:** core, Polars, Candle and project; `:dep polars candle`.

## Out of scope

- tuning the descriptions, the margin or the label set after seeing scores (any later change is a new, separately frozen run);
- new tensor APIs;
- any claim that zero-shot triage is accurate in general.
