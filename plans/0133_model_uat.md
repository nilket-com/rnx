# rnx 0133: user-acceptance workflows with a model, on real documents

Status: plan, the user's direction after 0132:
- UAT-style examples of useful calculations with models are worth more now than GPU speed;
- revisit GPU only when a useful workload is demonstrably too slow on the CPU.

**What this record does:** finds out which useful model tasks rnx supports end to end today, and where the friction is. It runs each task as a user would, ranks the friction, and fixes in follow-up records the blockers it reveals.

**This record is the probe** (like 0122's workflow probe). **It changes no rnx core or adapter code.** Every fix, however small, is a follow-up record.

## The success measure (per workflow, unchanged from 0129–0132)

1. It runs as an ordinary script in a plain `:dep polars candle` session. Nothing is precomputed outside rnx except fetching the data and the model.
2. Its computed answer equals a direct-Rust twin: bit for bit where the same kernels run, otherwise within a stated tolerance.
3. It displays a useful answer: what a user would act on (titles, scores, excerpts, groups), not raw vectors.

**Each step is recorded with its status** (works, friction or blocked) and its cause:
- **works:** runs as written;
- **friction:** runs, but awkwardly (a workaround, a missing convenience, a confusing error);
- **blocked:** can't be done in rnx today.

## The data (real, local or fetched; nothing synthetic), pinned and replayable

**D1, rnx's own records:** the `plans/*.md` files **at commit `24f87d5`**, the last commit before this plan. The 0133 plan is excluded, and so is any later file.
- **The corpus:** 321 Markdown documents (plans and evidence), each with a `# title` first line. They are real technical prose of very different lengths, mostly far beyond the model's 256-token window.
- **Reading:** the probe exports them with `git archive 24f87d5 plans/`, never from the working tree.
- **The manifest:** `probes/0133/d1-manifest.tsv` (path and SHA-256, committed). A replay verifies every file against it.

**D2, real support tickets:** the `rune-rs/rune` GitHub issues (an Apache-2.0 repository): titles, bodies and labels. They are bug reports, questions and feature requests in users' own words.
- **The fetch** (`probes/0133/fetch_issues.sh`):
  - `GET /repos/rune-rs/rune/issues?state=all&per_page=100&page=N`, paginated until an empty page;
  - entries carrying `pull_request` are excluded.
- **The manifest** (`probes/0133/d2-manifest.tsv`, committed): one row per issue, giving its number and the SHA-256 of its normalized content. Normalized means canonical JSON of the number, the title, the body (CRLF folded to LF) and the label names, sorted. Also committed: the fetch's UTC date, the issue count and the API's `updated_at` maximum.
- **The snapshot:** the fetched JSON is **retained locally** under `$RNX_UAT_DATA/d2/`, and is not committed, since the text is other people's writing. Every replay verifies it against the manifest.
  - **A missing snapshot** is reported as **dataset unavailable**.
  - **A refetch whose hashes differ** is reported as **dataset drift**, with the changed issue numbers.
  - New data is never silently compared with the recorded results.

**The model:** 0131's pinned `all-MiniLM-L6-v2` (`probes/0131/fetch.sh`).

## Frozen before any scoring

**Chunking (U1, U2, D1):**
- the text after the title line is split on blank lines into paragraphs;
- consecutive paragraphs are packed into passages of at most **180 words** (whitespace-separated);
- a paragraph longer than 180 words is cut into consecutive 180-word windows.

**180 words is a heuristic, not a guarantee** that a passage fits MiniLM's 256 WordPiece tokens. Technical prose tokenizes densely, and anything over is truncated by the encoder (0131).
- **Counted:** the evidence counts truncation with the pinned tokenizer, special tokens included, for every D1 passage and every D2 ticket input. It reports the count, the share, and the content lost from each truncated tail, and whether a rubric-relevant passage lost its tail.
- **Two separate things:** "truncated but computable" is kept apart from full-content coverage. Truncation can itself rank as chunking friction; it isn't credited as model-ready chunking.

**Tickets (U2 and U3, D2):** an issue is embedded as its title, a newline, and the first 180 words of its body.

**Document vectors (U2):** the mean of a document's passage embeddings, renormalized to unit length.

**Thresholds,** cosine:

| band | range |
|---|---|
| U2 duplicate candidates | ≥ 0.95 |
| U2 related | 0.85 to 0.95, reported separately from duplicates |
| U3 graph edge | ≥ 0.80 |

None of these are tuned on the results. If one proves useless, that is a finding, and any recalibration is labelled and evaluated on a separate subset.

**U1's queries and the records that should answer them** (`probes/0133/rubric.tsv`, committed). A record is identified by its number, so its plan and evidence files both count:

| # | query | supporting records |
|---|---|---|
| Q1 | how does a session add a native adapter with :dep? | 0063, 0067, 0070 |
| Q2 | why was the thread-local test limit rejected? | 0130 |
| Q3 | how is a borrowed Polars group-by kept valid during aggregation? | 0126 |
| Q4 | what limits does the neutral dense block enforce? | 0129 |
| Q5 | how is a large dataframe shown at the prompt without printing everything? | 0068, 0124 |

## The workflows

**U1, semantic search over a document table (D1):**
- **Load and chunk:** load the documents into a Polars frame (`path`, `record`, `title`, `text`) from the exported directory, and chunk them by the frozen rule.
- **Embed and rank:** embed the passages, score them against each query, and keep each document's best passage (the document score is the maximum).
- **The answer:** the top 5 documents, with **title, score, and an excerpt** (the best passage's first 300 characters).
- **Semantic quality:** hit@1 and hit@5 against the rubric, with the ranked answers shown in full.

**U2, near-duplicate detection (D1 and D2):**
- **The method:** document vectors, then all-pairs similarity, then the pairs in each band.
- **The answer:** pairs with both titles and the score, sorted.
- **What was found** is reported as three separate counts and samples:
  - duplicate candidates (≥ 0.95);
  - related pairs (0.85 to 0.95);
  - false-positive examples found by inspection.
- **Plan and evidence pairs, and the repeated snapshot records, are topical overlap, not duplicates.** They are reported as relatedness.

**U3, grouping similar support tickets (D2):**
- **The method:** ticket vectors, then edges at 0.80 or above, then connected components.
- **The answer:** the groups by size, each with its titles, issue numbers and labels.
- **Grouping is judged on more than parity:**
  - the number of groups;
  - the **singleton share**;
  - the **largest group's size**;
  - a sampled coherence read of the 10 largest groups, with examples.
- **A giant component is not a success** merely because it matches Rust.

**Two judgements, kept apart:**
- **Calculation parity** (rnx against its twin) is pass or fail against the tolerance.
- **Semantic quality** uses the rubric and inspection.
- **The quality judgements are the agent's,** labelled so, and are not called user acceptance until the user has reviewed them.

**The twins** (`probes/0133/twin`) implement the same loading, chunking, pooling, similarity, thresholds and grouping directly in Rust against Polars, Candle and `tokenizers`.

**Steps standalone, and composed:**
- **Composed:** each workflow is run as one script.
- **Standalone:** each of its steps is also run alone, with its inputs prepared, so an upstream blocker can't hide a downstream one.
- **Labelling:** a standalone step fed from a twin-prepared intermediate is labelled **diagnostic** and is never credited as end-to-end session success.

## What the probe expects to meet (hypotheses to confirm or refute, not fixes)

| step | why it may be friction |
|---|---|
| building a frame from a directory of files | rnx's `fs` module and Polars' constructors exist, but no "read a directory into a frame" path |
| chunking long text | the encoder truncates at 256 tokens; splitting by paragraphs or by token budget needs string work in Rune or a tokenizer-aware helper |
| reading individual scores and rows | `Dense` is read-only and opaque to scripts, so values reach Rune only through Polars (`with_dense`, then frame operations) |
| per-document aggregation | the best passage per document: a group-by over a `Dense`-derived column |
| pairwise similarity above 4,096 documents | `similarity` names one column per row of `b`, capped at 4,096 columns |
| graph grouping | connected components need loops over pairs in Rune, possibly slow or awkward |
| excerpts | cutting a passage for display (0124's preview truncates cells already) |

## Gates

**Per step:** works, friction or blocked, with its cause, in the composed run and standalone.

**Per workflow:**
- the session transcript;
- the parity result;
- the semantic-quality result, kept separate;
- display usability (is the answer readable and actionable?);
- the wall time.

These stay four distinct kinds of evidence; one is never folded into another.

**The friction ranking:** every blocker and friction is recorded and grouped into fix families.
- **Credit:** a family is credited only with the steps and workflows that **fixing it alone** would enable. A step blocked by more than one family credits none of them.
- **Order, fixed before probing:**
  1. workflows enabled;
  2. steps enabled;
  3. the smaller estimated fix (S, M or L, estimated before building);
  4. discovery order.

**Timing:** each workflow's wall time in the session, which is the user's GPU criterion.

**No regressions:** no rnx code changes in this record. If a probe-only helper is added, the suites still pass.

## Stop rules

- If a workflow can't be completed without a new feature, record the blocker and continue with its remaining steps standalone. Don't build the feature here.
- If D2 is unavailable or has drifted, report that, and don't score against a different dataset.
- If the twin and rnx disagree beyond the tolerance, stop and report.

## Out of scope

- GPU.
- Training and fine-tuning.
- A vector index.
- Models other than 0131's.
- Building the friction fixes, which are follow-up records.
