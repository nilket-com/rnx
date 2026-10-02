# rnx 0147: a frozen 34-query evaluation of search over rnx's records, scoring U1′ and U5 unchanged

Status: plan. The user (2026-10-02) set the order: **a frozen evaluation record first, then NLI classification widening.**

**Amended after Codex's review (round 1), before the freeze:**
- **R1:** E28 also counts 0131, whose evidence measures and attributes the concurrency gain.
- **R2:** the D1 manifest and both models' pinned checksums become fatal gates before any run, with controls proving no downstream run begins.
- **R3:** candidate recall@20 is described only as what it measures.
- **The detail:** Δr is censored beyond rank 20.

**Why this record exists:**
- **0146's comparison was inconclusive.** On its five queries, retrieval alone already had hit@1 5/5, so re-ranking could only hold or lose first place. It lost two.
- **Five queries can't establish a cause** for those losses. In particular, they don't show that the rubric's size caused them.
- **The tested pipeline has a limitation:** the re-ranker sees only each document's best passage by embedding. That's a property of U5, and worth measuring later. This record doesn't change it.

**What it does:**
- **Freezes a larger rubric** before any run.
- **Runs U1′ and U5 unchanged** on it.
- **Reports paired per-query changes,** candidate recall@20, MRR and latency.
- **Reports Rust parity separately** from quality.
- **Promises no improvement.**

## 1. The rubric (`probes/0147/rubric.tsv`, frozen by this commit)

**SHA-256** `ee31b55db02970a9aa4a714c50eef35df29a9b15a4fb2ac041670d8def819326`. The format is 0133's: `query`, `id`, `text`, `supporting_records`, plus a fifth column, `kind`. U1′, U5 and the twin read only columns 0, 2 and 3.

**The corpus:** D1 is unchanged: rnx's records 0001–0132 and the parity map, 321 files.

**How the queries were written:**
- **From analyst tasks:** the questions someone using or maintaining rnx asks of its records, such as why a decision was made, how to do something, or what was measured. They spread across the corpus:
  - the session;
  - processes;
  - the batteries;
  - packaging;
  - notebooks;
  - adapters and launch;
  - Polars;
  - Candle.
- **From the corpus alone:** file titles, each plan's status paragraph, full-text search and reading.
- **No retriever has been run on any of them:** not U1′, U5, the twin, BM25 or any embedding. So no retrieval result for them has been seen, and **no query was chosen, dropped or reworded because of how a retriever handles it.**
- **0146's Q1–Q5 are excluded,** because their results have been seen.

**Harder tasks are included by construction, not by selection:**
- **16 queries need two or more records.** E10 needs five.
- **10 queries are paraphrases** (by the rule below): they share no distinctive word with any supporting record's title.
- **Several ask for a specific detail** inside a record: E08, E20, E24.

**The judgments:**
- **What supports a query:** a record supports it when the record directly answers or decides the matter asked. A record that merely mentions it doesn't count.
- **Identity:** a record is its number, so its plan, evidence and upstream-draft files all count (0133's rule). A record's rank is its earliest document's.
- **Relevance is binary,** and several valid answers are allowed.
- **Each judgment was checked by reading the record,** not by matching a word. E03, for instance, went to 0002 (Ctrl-C between instruction slices) and 0032 (which keeps that working for async scripts). 0021's budget was ruled out: it limits a run, it doesn't stop one on request.

| query | task | supporting | why |
|---|---|---|---|
| E01 | memory refusal and what's counted | 0001 0005 | 0001 decision 3 sets the ceiling; 0005 decides what the figure counts |
| E02 | a script halts on instructions | 0021 | the operator chooses the budget |
| E03 | stop a never-ending computation | 0002 0032 | Ctrl-C checked between slices; kept by the async driver |
| E04 | licence texts to distribute | 0028 0029 | the provenance inventory; the notice file |
| E05 | child timeouts on Windows | 0025 | the same contracts on Windows |
| E06 | a child in another folder with extra env | 0044 | working directory and child-only environment |
| E07 | launch arguments and environment | 0036 | what the process was given |
| E08 | a large JSON integer comes back wrong | 0033 | the `json_parse` contract (and its unsigned upstream draft) |
| E09 | how a notebook runs rnx code | 0046 0047 0048 | the worker; the kernel; the bounded transport |
| E10 | problems reported against Rune itself | 0006 0019 0030 0032 0033 | the eight upstream drafts |
| E11 | functions from a file alongside | 0050 | file modules |
| E12 | where a project lists its extensions | 0057 0062 | the project declaration; adapter names in it |
| E13 | slow verified launch, then cheaper | 0059 0065 | 155 → 29 ms; each native file checked once |
| E14 | delete old build output safely | 0066 | removal as an explicit ownership decision |
| E15 | fetch without an unbounded response | 0034 | the bounded request |
| E16 | clocks and time zones | 0038 | named clocks and the calendar |
| E17 | choose the session's colours | 0043 0045 | colours in a Rune file; the number's colours |
| E18 | a hash where a method name should be | 0014 0041 | naming the missing method in `run`, then everywhere |
| E19 | child output that isn't text | 0016 | a child's bytes |
| E20 | timed out, yet returns late | 0023 | a capture that ends when the call does |
| E21 | which Polars ships, and why not 2.0 rc | 0128 | the shipping decision |
| E22 | bindings without hand-writing each | 0073 0114 0115 | the generator; its split; its registry |
| E23 | a script's function inside Polars | 0079 0080 | the callback contract; its bindings |
| E24 | large unsigned values read back wrong | 0092 0093 | found in 0092; checked read-back in 0093 |
| E25 | load CSV or JSON into a frame | 0125 | loading |
| E26 | long rows into one column per category | 0127 | pivot |
| E27 | which analytics tasks failed end to end | 0122 | the workflow probe |
| E28 | faster embeddings on the CPU | 0131 0132 | 0131's evidence measures 20 concurrent calls (10.0 s against 60.8 s) and attributes the gain to concurrency; 0132 makes batches concurrent inside `embed` (review round 1) |
| E29 | model output into frame columns | 0129 0131 | Dense interchange; embeddings into a table |
| E30 | a second project doesn't recompile | 0061 0069 | shared executables; shared compilation |
| E31 | the Arrow arrays under Polars columns | 0119 0120 | the Arrow value layer; concrete arrays |
| E32 | a short quit or clear | 0042 | the prompt a person lives at |
| E33 | what Tab completes | 0003 | completion |
| E34 | read piped data | 0012 | standard input |

**`kind`** (`probes/0147/kinds.py` recomputes it and must agree):
- **The rule:** a query is `direct` when it shares a distinctive content word with the first-line title of any of its supporting records' files, and `paraphrase` otherwise.
- **Words:** runs of 4 or more letters or digits, minus a fixed stoplist, compared by their first 5 characters.
- **Distinctive:** at most 5% of D1's titles contain the word. That excludes, for example, "Polars", which appears in 31 of 321 titles.
- **The result:** 24 `direct` and 10 `paraphrase` (E01, E02, E07, E10, E15, E21, E23, E26, E27, E34).
- **Use:** the labels are descriptive strata only. Nothing is tuned on them.

**The freeze:**
- **The accepted plan commit holds the rubric and its hash.** Codex's review may amend the queries or judgments; if it does, the hash above is updated in the same amendment.
- **No retriever runs on these queries until the plan is accepted.** The implementation's first gate checks the rubric and its hash, and that `kinds.py` passes.
- **No query, judgment or label changes after any run.** A judgment error found later is reported in the evidence as a finding, scored both as frozen and as corrected, and never silently fixed.

**Evaluation only:**
- **This rubric is never used for tuning.** That covers passage choice, several passages per document, score blending, thresholds and candidate counts.
- **Any later tuning uses a separate, newly written query set.**
- **Later records cite this rubric by its hash** when they report against it.

## 2. The runs, all unchanged

| run | what | from |
|---|---|---|
| **U1′** | 0136's `workflow-u1c.rn` at overlap 0: the top 5 documents per query | byte-identical to 0136's committed script |
| **U5** | 0146's `u5_rerank.rn`: 20 candidates, re-scored by the cross-encoder, with the retained retrieval evidence | byte-identical to `240fa54` |
| **twin** | `twin0133 u5` on the new rubric | unchanged since 0146 |
| **HF** | `hf_check.py` on U5's pairs (34 × 20 = 680) | unchanged since 0146 |

- **Sessions:** both run as session pastes on `runner0134`, as 0146's did.
- **No adapter or core code changes** in this record, so there is nothing new to ship and no `:dep` check.
- **The new files are the rubric and its scripts:**
  - `kinds.py` and `evaluate.py`, with `evaluate.py`'s controls;
  - the provenance gate, `preflight.py`, with its controls;
  - the driver, `run.sh`.

**Consistency, a gate:** U1′'s top 5 must equal U5's retrieval top 5 for every query (the same chunking, model and scores).
- **The one allowed difference:** U1′ sorts without `with_maintain_order`. Where scores tie exactly at a rank, the two may order those documents differently.
- **Any difference is checked** against the retained f32 scores. A tie is named in the evidence; anything else fails the gate.

## 3. The measures (`probes/0147/evaluate.py`)

**The input:** a U5 directory validated by `compare_u5.validate`, so the candidates are bound to the retained evidence and D1, and the rubric. Each query is scored under two orders: retrieval (U5's candidate order) and re-ranked.

**Per query and order:**
- **r:** the rank of the first supporting record (its earliest document among the 20), or "> 20".
- **hit@1 and hit@5.**
- **recall@5:** the share of supporting records with a document in the top 5.
- **RR = 1/r,** or 0 beyond 20. MRR is therefore truncated at 20, and the evidence says so.
- **Candidate recall@20:** the share of supporting records among the 20 candidates. It's the same for both orders, because re-ranking only permutes.
  - **What it measures:** omissions at the candidate stage, and an upper bound on the recall@5 re-ranking can reach.
  - **What it doesn't:** whether the passage chosen for a document represents it well to the cross-encoder (review round 1, R3).

**Paired changes, per query:**
- **Δr,** when both ranks are within 20. When either is beyond 20 it's reported as censored (for example "> 20 → 3"), never as a subtraction from an invented rank (review round 1);
- ΔRR and Δrecall@5;
- a win, loss or tie by RR.

**Aggregates:**
- **Overall:** mean RR (MRR), mean recall@5, mean candidate recall@20, and the hit@1 and hit@5 counts.
- **By stratum:** `kind` (direct, paraphrase) and single against multi-record.
- **Two descriptive statistics,** with no threshold that declares improvement:
  - the exact two-sided sign test on RR wins against losses (ties dropped);
  - a paired bootstrap 95% interval on mean ΔRR (10,000 resamples of queries, seed 147).
  - With 34 queries the interval will be wide, and the evidence says what it can't distinguish.
- **The per-query table:** query, kind, supporting records, r under each order, and candidate recall@20.

**The single-passage limitation stays unmeasured here.** Checking whether the cross-encoder would rank a supporting document higher on another of its passages means scoring every passage. That's a tuning question for a later record, with a separate query set.

**Latency, reported separately:**
- U1′ end to end;
- U5 retrieval;
- U5 re-ranking of 680 pairs;
- HF `predict` on the same pairs (median of 3).

## 4. Gates

0. **Provenance, fatal and first** (`probes/0147/preflight.py`, review round 1, R2). The driver `probes/0147/run.sh` runs it first and stops on failure, before any session, twin or HF run begins:
   - **D1:** the ordered `(name, sha256)` list of its `.md` files must equal 0133's frozen `probes/0133/d1-manifest.tsv` (321 files). A changed, missing or extra file fails.
   - **MiniLM:** the six files 0131 pins (`probes/0131/fetch.sh`'s SHA-256s) must match.
   - **The cross-encoder:** the three files 0146 pins (`probes/0146/fetch.sh`'s SHA-256s) must match.
   - **Controls** (`probes/0147/preflight_controls.sh`), each on a temporary copy, through `run.sh`:
     - a D1 file with one byte changed, one removed, one extra `.md`;
     - a MiniLM weight byte changed;
     - a cross-encoder tokenizer byte changed;
     - a model file missing.
     - **Each must be refused by name,** and a sentinel standing in for the session, twin and HF steps must show that **none of them began.** The unmodified copies must pass.
1. **The rubric is unchanged:**
   - its SHA-256 is as stated above;
   - `kinds.py` passes;
   - every supporting record exists in D1.
2. **The scripts are unchanged:** U1′'s and U5's files are byte-identical to their committed versions (`git hash-object` against `0136`'s and `240fa54`'s).
3. **Both sessions succeed:** the marker, `Ok(())`, and no errors.
4. **The top 5s agree:** U1′'s and U5's retrieval top 5s, as in section 2.
5. **Parity, reported apart from quality:**
   - `compare_u5.py` (each producer validated against its retained evidence and D1, then compared) is **BIT-EQUAL** to the twin;
   - `hf_check.py` agrees within 1e-4, with identical orders except between candidates whose scores differ by less than 1e-4, each listed.
6. **The evaluator is checked:**
   - **Its own controls:** synthetic rankings with known answers:
     - a supporting record at rank 1, at 5, at 6 and absent;
     - a multi-record query with one record found and one absent;
     - a record whose second document ranks first;
     - an empty supporting set, which is refused.
   - **An independent agreement check:** on the real run, it must reproduce U5's own in-trace hit@1, hit@5, recall@5 and MRR for every query, in both orders, exactly.

**Reported as measured.** No outcome is a failure of the record, except a failed gate.

## Out of scope

- **Any change** to chunking, the embedding model, the candidate count, passage choice, the re-ranker or how scores combine.
- **Tuning of any kind** (see section 1).
- **A second, independent judge.** Codex's review of the judgments in this plan is the second look; disagreements are settled before acceptance, which is the freeze.
- **NLI classification:** the next record. It keeps 0139's frozen sample and reports classification quality separately from Rust parity.
