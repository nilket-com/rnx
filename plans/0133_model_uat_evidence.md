# rnx 0133 evidence: user-acceptance workflows with a model, on real documents

**The answer to the record's question:** useful model tasks run end to end in an ordinary `:dep polars candle` session today.
- **Semantic search over rnx's own 321 records** gives useful answers: hit@1 3 of 5, hit@5 5 of 5, with titles, scores and excerpts.
- **Near-duplicate detection** surfaces templated sibling records and plan/evidence pairs.
- **Ticket grouping** runs, but the frozen threshold leaves 98% of tickets alone.
- **Every computed answer matches its direct-Rust twin.**

**The friction is real, and specific.** 18 findings in 7 families (§5):
1. one blocker, for the `project run` path only;
2. a CPU embedding cost on long passages, 387 s against PyTorch's 49 s, that comes from our own budget, not the CPU;
3. heavy truncation under the frozen chunking rule (39% of passages);
4. no numeric readback;
5. session ergonomics for pasted scripts;
6. error messages that print hashes;
7. thresholds that don't fit the model.

**The judgements of usefulness are the agent's,** labelled as such, and are not user acceptance until the user reviews them.

## 1. Data, pinned and verified

**D1:** `plans/*.md` at `24f87d5` (`git archive`), 321 files.
- **Manifest:** `probes/0133/d1-manifest.tsv` (path and SHA-256).

**D2:** the `rune-rs/rune` issues: 252, with pull requests excluded.
- **Fetched** 2026-10-01 11:35 UTC; the newest `updated_at` is 2026-09-08 05:04 UTC.
- **Cursor pagination:** the issues endpoint refuses page numbers past 1,000 results (HTTP 422, "use cursor based pagination"), so `fetch_issues.sh` follows the `Link: rel="next"` headers through all 11 pages.
- **Manifest:** `d2-manifest.tsv` (number and normalized-content SHA-256) and `d2-meta.json`.
- **The snapshot** is retained locally under `$RNX_UAT_DATA/d2/`.
- **Every replay** runs `manifest.py check`, which reports "datasets verified", or unavailable, or drift with the changed numbers.

**The model:** 0131's pinned `all-MiniLM-L6-v2`. **The rubric:** `rubric.tsv`, frozen before scoring.

## 2. The workflows in ordinary sessions

**How each was run:** a stock `rnx` session started under a pty (`probes/0133/session.py`); then `:dep polars candle` (15 to 17 s, attached); then the script pasted line by line; then `run([...])`. The transcripts are kept under `out/`.

**U1, semantic search (D1)** (`workflow-u1.rn`):
- **The steps, each of which works:**
  - load: `fs::read_dir`, `fs::read`, title from the first line;
  - chunk: the frozen rule, in Rune;
  - frames: `Series::from_iter_*`, then `DataFrame::new`;
  - embed: `TextEncoder::embed` on 3,658 passages;
  - similarity against 5 queries;
  - per-document best passage: `sort`, `unique_stable`, `head(Some(5))`;
  - `left_join` the titles;
  - an excerpt from `strings`, cut in Rune;
  - display: 0124's preview, with long cells marked `…[truncated]`.
- **Time:** load 341 ms; **embedding 387 s**; ranking under 1 s.
- **Display:** a readable frame per query, with record, title, score and the excerpt's first 300 characters. It is actionable: the excerpt is the passage that answers.

**U2, near-duplicates (D1 and D2)** (`workflow-u2.rn`):
- **D2:** tickets parsed with `json::parse`; embedding; all-pairs `similarity`; band extraction.
- **D1:** passage embeddings averaged per document (a lazy `group_by` with 384 `col("e{i}").mean()` expressions, built in a loop); `to_dense`; all-pairs; bands.
- **Band extraction** works around three gaps: a per-column mask loop (no unpivot, F11), filtering twice (no boolean AND, F16), and casting to string, `strings`, then parsing (no numeric readback, F12).
- **Time:** D2 26 s; D1 embedding 364 s; D1 means and pairs 1.3 s.

**U3, grouping tickets (D2)** (`workflow-u3.rn`):
- **The steps:** ticket embedding; all-pairs; edges at 0.80 or above; union-find in Rune; groups printed with numbers, labels and titles.
- **Time:** 26.6 s.

**Standalone steps:** every composed workflow completed once the script errors were fixed. No step was blocked, so no downstream step was hidden and no twin-fed diagnostic intermediate was needed. The failed attempts along the way (§5) are recorded as friction, not as blocked steps.

## 3. Calculation parity, replayed (`probes/0133/replay.sh`, `out/replay.txt`)

**The replay** (review round 1, R2): `RNX_UAT_DATA=… replay.sh RNX MODEL OUT [PYTHON]` runs every step and propagates any failure:
1. **The datasets** must verify (`manifest.py check`, R1), and `manifest_controls.sh` must refuse 7 corruptions:
   - a duplicated issue, a missing issue, changed content, a changed label, reordering;
   - missing D1, missing D2.
2. **The model's six files** must verify against `0131/fetch.sh`'s pinned SHA-256s. This is an explicit, fatal gate (review round 2, R2a). `replay_controls.sh` proves that an empty, missing or wrong model stops the replay before any session or twin runs; a stand-in executable records any downstream invocation, and none happens.
3. **U1, U2 and U3** run in ordinary `:dep polars candle` sessions on an `rnx` built at the pushed `24f87d5` (`:dep` fetches the adapters at the binary's commit). Each passes only when all of these hold after the call (review round 2, R2b):
   - its `WORKFLOW OK` marker appears;
   - the run's own final evaluation is `Ok(())`;
   - there is no compile or runtime error.

   `session_controls.sh` proves this gate fails closed. Each of these fails:
   - a run returning `Err`;
   - a script that doesn't compile;
   - a runtime error after the marker;
   - **a returned `Err` after the marker.**

   A clean run passes. The replay's three saved session logs each end in `[n] Ok(())` after their marker, so they also pass the stricter gate.
4. **The twins** (`probes/0133/twin`, written directly against candle-transformers and tokenizers) write the same machine-readable formats.
5. **`compare_outputs.py`** compares the **complete** outputs exactly, with fail-closed controls.
6. **With a reference Python:** the diagnostics `diagnostics/truncation.py` and `diagnostics/torch_timing.py`.

**The result: every check passed.**

| workflow | rows (session = twin) | compared |
|---|---:|---|
| U1 | 25 | every (query, rank, path, passage id) and the score's f32 bits |
| U2 | 459 | the complete set of (band, path or issue, path or issue) pairs, every score's f32 bits |
| U3 | 254 | every ticket's component (singletons included), and both edges with their score bits |

**Session against twin: EQUAL.** U1's ranked documents carry stable paths (a record number alone would conflate plan and evidence) and the passage each ranking chose.

**The comparison's controls each fail as they must:**
- a changed score on a non-top U2 row;
- a wrong U1 document;
- a singleton moved to another U3 component;
- a dropped U3 singleton.

**Withdrawn:** the earlier inference that D1's Polars group-by means are bit-equal to the twin's. The complete U2 pair comparison above is the evidence; nothing about the means themselves is claimed beyond it.

## 4. Semantic quality (the agent's judgement, kept apart from parity)

**U1, against the frozen rubric (`rubric.tsv`):**

| query | top result | rubric record's rank | hit@1 | hit@5 |
|---|---|---:|:-:|:-:|
| Q1, adding an adapter with `:dep` | 0001 (first release; off target) | 0070 3rd, 0063 5th | no | yes |
| Q2, the rejected thread-local limit | 0130 plan | 1 | yes | yes |
| Q3, the borrowed group-by | 0126 plan | 1 | yes | yes |
| Q4, the dense block's limits | 0082 (borrowed-slice bounds) | 0129 2nd | no | yes |
| Q5, the bounded preview | 0068 | 1 | yes | yes |

**hit@1 3 of 5, hit@5 5 of 5.** The two misses are near misses with plausible neighbours (both concern bounds and limits). Each excerpt shows the answering passage; for example Q2's excerpt opens "Rejected: a thread-local override…".

**U2:**
- **D1 duplicate candidates (≥ 0.95):** 3 pairs, all within the near-template 0101 to 0105 "owned snapshots" family. **These are templated sibling records, not true duplicates.** That is topical overlap, as the plan anticipated.
- **D1 related (0.85 to 0.95):** 456 pairs. The top 20 are dominated by plan/evidence pairs (0127, 0079, 0025, 0117, …) and by sibling records (0101/0104, 0092/0095). Every one inspected is a genuine relation.
- **False positives at ≥ 0.85:** none found in the top 20.
- **D2:** 0 pairs in either band (F17).

**U3:**
- **The result:** 250 groups for 252 tickets; **98% singletons**; the largest group has 2.
- **The two groups are coherent:** macros (#521/#525) and protocols (#23/#583).
- **But as a grouping it isn't useful at the frozen 0.80:** the reference model's maximum similarity across all 31,626 ticket pairs is 0.832, and genuine near-duplicates sit just below the edge (#588 "Cannot mutate nested field" with #835 "Problem when change a inner field in a mutable struct", at 0.799).
- **Per the plan, this is reported, not tuned.** A recalibration would be a labelled follow-up, evaluated on a separate subset.

**Truncation, counted with the pinned tokenizer, special tokens included** (`diagnostics/truncation.py`, run by the replay; `out/replay.txt`):

| | inputs | over 256 tokens | tokens lost |
|---|---:|---:|---:|
| D1 passages (180-word rule) | 3,658 | 1,426 (39.0%); median 235, p90 336, max 2,079 | 89,477 of 843,014 (10.6%) |
| D2 tickets (title + 180 words) | 252 | 85 (33.7%); median 185, p90 508, max 706 | 14,132 of 57,386 (24.6%) |

- **The rubric's records were affected:** 128 of their 313 passages lost a tail.
- **"Truncated but computable" is not counted as model-ready chunking.** It is finding F14.

## 5. Findings and the friction ranking

**The findings log** (`probes/0133/findings.md`), each with its status:

| # | status | finding |
|---|---|---|
| F1 to F3 | friction | Rune has no `String::split_whitespace`, `Vec::join` or `String::strip_prefix` |
| F4a | friction | `rnx run` defaults to 2,000,000 instructions, and the document loops need about 50M; `--budget` works |
| **F4b** | **blocked** (the `project run` path) | `rnx project run` accepts no budget option, yet its halt message says "--budget N raises it". Sessions allow 2×10⁹ per input |
| F5 | friction | every edit to the entry script needs `project lock` and `build` (about 1.6 s with attach) |
| F6 | friction | no `:load`, so a workflow is pasted |
| F7 | friction | `main` is reserved in a session |
| F8 | friction | pasting a multi-line function redraws the whole input on every keystroke: 3 MB of terminal output and minutes for a 100-line script |
| F9 | friction | a compile error inside a pasted definition makes every later line its own input, so errors cascade |
| F10 | friction | `select` is a Rune keyword; the bindings are `select_`, and the error reads "Unsupported field access" |
| F11 | friction | no `unpivot` in the shipped 0.55.2 |
| F12 | friction | no numeric readback (Dense is opaque; Polars columns reach Rune only through `strings`) |
| F13 | friction (runtime) | embedding long passages: 387 s, against PyTorch's 49 s, because 0132's budget serializes long batches |
| F14 | friction (quality) | 39% of D1 passages and 34% of D2 tickets are truncated; there is no token-aware chunking |
| F15 | friction | session errors name methods by hash; a `.collect()` on a fallible chain reads "Missing instance function `0x…` for `Result`" |
| F16 | friction | two `BooleanChunked` masks can't be ANDed |
| F17 | finding (quality) | the frozen thresholds don't fit MiniLM on real tickets (maximum 0.832) |
| F18 | friction | a macro call split across lines ends a pasted input early |

**Every step, with its status, in the composed session runs:**

| workflow | step | status | findings |
|---|---|---|---|
| U1, U2 | load the D1 files (`fs::read_dir`, `fs::read`) | works | F1, F2, F3 |
| U1, U2 | chunk by the frozen rule | works | F1, F2, F14 |
| U1, U2 | build frames (`from_iter_*`, `DataFrame::new`) | works | |
| U1, U2 | embed the passages | works | F13 (runtime), F14 (quality) |
| U1 | embed the queries; similarity | works | |
| U1 | each document's best passage (`sort`, `unique_stable`, `head`) | works | F10 |
| U1 | join titles; excerpt; display | works | |
| U1, U2, U3 | write results (cast to strings, then parse) | works | F12 |
| U2, U3 | ticket inputs (`json::parse`) | works | F14 |
| U2, U3 | all-pairs similarity | works | |
| U2, U3 | band extraction (per-column masks, filtering twice) | works | F11, F12, F15, F16 |
| U2 | document means (lazy `group_by` with 384 expressions) | works | F15 |
| U3 | union-find and groups | works | F17 (quality) |
| all | the session: paste, the reserved `main`, multi-line macros | works | F6, F7, F8, F9, F18 |
| all | **the `project run` path** | **blocked** | **F4b** |
| all | the `rnx run` path | works with `--budget` | F4a |

**All blockers:** F4b, and only on the `project run` path. **No step of a composed session workflow is blocked.**

**Credits under the frozen rules,** where a family is credited only with what it enables alone, within the plan's measure (an ordinary session):
- **every family enables 0 workflows and 0 steps,** because all three composed session workflows already work;
- **outside the measure:** F4b's family alone would enable all 3 workflows on the `project run` path. That is a path credit, reported here and not used in the order.

Friction reductions are not counted as enabled workflows or steps.

**The frozen ranking** is therefore decided by its tie-breakers: the smaller estimated fix first, then discovery order.

| rank | family | findings | size |
|---:|---|---|---|
| 1 | Rune std gaps | F1, F2, F3 | S |
| 2 | budgets on every run path | F4a, F4b | S |
| 3 | numeric readback | F12 | S (0134's `to_vec` covers Candle) |
| 4 | long-passage embedding cost | F13 | S |
| 5 | session scripts | F6, F7, F8, F9, F18 | M |
| 6 | error naming | F10, F15 | M |
| 7 | token-aware chunking | F14 | M |
| — | the Polars 0.55.2 surface | F11, F16 | outside rnx's control: with the v2 switch or a pivot record |
| — | thresholds | F17 | guidance and calibration, not code |

**Separately, the agent's own priority by user impact** (labelled; not the frozen ranking):
1. budgets (F4b blocks a path, and its message misleads);
2. long-passage embedding cost (F13: 387 s against 49 s);
3. token-aware chunking (F14: 39% and 34% of inputs truncated);
4. session scripts;
5. numeric readback;
6. error naming;
7. the Rune std gaps.

**A plan departure, stated:** the plan called for each step to be run standalone as well as composed, so that an upstream blocker couldn't hide a downstream one. All three composed workflows ran to completion, so no step was hidden, and **the standalone runs were not performed**. This is declared as a departure, not claimed as a gate.

## 6. Timing, and the GPU criterion

**U1 and U2 spend more than 360 s embedding D1,** which is too slow for an interactive question. But the cause (F13) is our budget serializing long batches: PyTorch on the same CPU takes 49 to 52.5 s (`diagnostics/torch_timing.py`, run by the replay). **So a useful workload isn't yet shown to be too slow for the CPU;** the fix is family 3. GPU stays deferred under the user's criterion.

## 7. Not done

- **The fixes:** every family above is a follow-up record.
- **A threshold recalibration** for U2 and U3, which would be a labelled follow-up.
