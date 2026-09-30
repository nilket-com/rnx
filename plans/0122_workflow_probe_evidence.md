# rnx 0122 evidence: the workflow probe, and the first workflow fix

The probe has two parts, both run through the real `rnx-polars` binary at v2 and compared with Rust twins:
- 39 standalone steps across eight notebook workflows;
- the eight workflows composed end to end.

**Before this record's fix:**
- **steps:** 27 work, 0 differ, 11 blocked, and 1 presentation step is checked by its markers;
- **composed workflows:** 1 works and 7 are blocked.

**The top-ranked blocker family** was a leftover limit in the hand-written `collect` from record 0058. It accepted only String, Int64, Float64 and Boolean results, so every query that counted, ranked or parsed dates failed.

**After the fix (Stage 2):**
- **steps:** 31 work, 0 differ, 7 blocked, 1 presented;
- **composed workflows:** 3 work (W4 derive, W7 windows, W8 output) and 5 are blocked;
- the four enabled steps and the two newly completed composed workflows each match their Rust twin exactly.

`probes/0122/replay.sh` recomputes the twins and all three outcome tables, and compares each with the committed one:
- v2 before the fix, rebuilt from the pre-fix source by `before_lib.py`;
- v2 after the fix;
- 0.55.2.

## The probe

**Steps and workflows:** 39 steps in `probes/0122/steps.py`, written as a notebook user would write them, plus eight composed workflows (`WORKFLOWS`) that run a workflow's steps in sequence on one frame.

| workflow | steps |
|---|---|
| W1 load | CSV (plain, with schema, from bytes), Parquet (file, bytes, scan), JSON |
| W2 inspect | schema, head, shape, null counts, summary |
| W3 clean | cast, rename, fill nulls, drop nulls, dedupe, filter |
| W4 derive | string, parse date, weekday, conditional, arithmetic |
| W5 group and aggregate | eager, lazy, two keys |
| W6 reshape | eager and lazy join, pivot, unpivot, concat |
| W7 windows | over, cumulative sum, rolling mean, rank |
| W8 output | CSV, Parquet, JSON, preview |

**How a step runs:**
- **Standalone:** each step builds its own input from the committed fixtures (`probes/0122/data`), so an early missing API cannot hide a later blocker.
- **Through the real binary:** each step runs through `rnx-polars run`, the same binary a user runs (rnx's `fs`, `env` and printing plus the adapter).
- **Printed result:** each step prints `polars::oracle_repr(result)`, a test-support function added here that renders a frame or series with the oracle's structural text. It is compiled only with `test-support`.
- **Compared:** `twins.rs`, run as a v2 adapter test, does the same operations in Polars' Rust API and renders them the same way.

**Comparison policy**, fixed in `probe.py`:
- a step's whole printed result, and a composed workflow's final line (its frame), compared as exact text, except as listed below;
- `w6.join_lazy`, `w5.eager_group_by` and the composed W6: rows compared as a multiset (a left join without `maintain_order`, and an eager group-by, which does not order its groups);
- `w8.present`, a presentation step with no Rust analogue: checked by the markers its output must contain, and counted apart as "presented" (review, round 1);
- `w2.summary`, compared to 12 significant digits. **Polars' own `std` is not bit-reproducible run to run:** the same binary printed 2.1602468994692865 and 2.160246899469287 across 12 runs. That is a Polars finding, not an adapter difference.

**Writing the steps honestly, and what I separated out:**
- **`?` on fallible calls.** A first draft without `?` failed almost everywhere on "Expected Expr but found Result". That hid every real blocker, so the steps use `?` where the API is fallible, and that friction is reported below as an ergonomic finding.
- **Other script-side ergonomics,** reported and not ranked: `select` is a Rune keyword, so the generator names it `select_`; `DataFrame` has no `clone` in Rune; `SchemaRef` has no `get`.

## The ranking (v2, before the fix)

The rule was fixed in the plan before probing:
1. the steps that fixing only that family would enable;
2. then the composed workflows that fixing only that family would complete, meaning workflows whose attributed blockers are that family alone and which were actually run as scripts;
3. then risk.

Every blocked step and every blocked composed workflow is attributed in `probe.py`'s committed tables (`BLOCKERS`, `WF_BLOCKERS`). The probe asserts that each table equals the observed blocks.

| family | kind | risk | steps enabled | composed workflows completed |
|---|---|---|---:|---|
| **A. hand-written `collect` accepts only 4 dtypes** | host | low | 4 (`w4.parse_date`, `w4.temporal`, `w5.lazy_group_by`, `w7.rank`) | W4, W7 |
| E. `SchemaRef` has no display | presentation | low | 1 (`w2.schema`) | W2 |
| F. `Expr::cast` takes `DataTypeExpr`; a `DataType` is not converted | surface | medium | 1 (`w3.cast`) | W3 |
| B. hand-written `read_csv` requires a schema | host | low | 1 | none: W1 also needs C and D |
| C. no JSON reader over bytes (`JsonReader` is a generic owner) | surface | medium | 1 | none |
| D. `scan_parquet` takes the internal `PlRefPath`, not a string | surface | medium | 1 | none |
| G. eager `DataFrame::group_by` (lifetime-bearing `GroupBy`; at the pin it offers `count`, `groups`, `keys`, `apply`) | surface | high | 1 (`w5.eager_group_by`) | none (W5 also needs A) |
| H. `LazyFrame::pivot`'s `Arc<DataFrame>` argument | surface | medium | 0; `w6.pivot` is multiply blocked | none |
| I. arity: the pinned `pivot` takes 8 arguments besides the receiver, and the generator binds at most 5 in all | surface | medium | 0; `w6.pivot` is multiply blocked | none |

**Pivot is multiply blocked** (review, round 1). The pinned `LazyFrame::pivot(on, on_columns, index, values, agg, maintain_order, separator, column_naming)` needs both H and I, so neither is credited. Its step and twin now make the real eight-argument call.

The callable counts are small (one to three rows per surface family) and were not used to rank.

## Stage 2: family A

**The limit.** Record 0058 refused every collected frame with a dtype outside its four (`files::validate` in `collect`), deliberately, when the hand-written surface was four dtypes wide (0058 files evidence: "a bool sum producing an unsupported dtype is also refused at collect"). Since then, generated eager operations return frames of every dtype; `null_count`'s UInt32 frame worked in the probe before the fix. Only the hand-written `collect` still refused them.

**Admission, against the plan's rule:**
- no new trait, lifetime or callback model;
- 0 generated bindings, against the at-most-40 cutoff;
- the existing ownership rules unchanged.

**The fix.** `collect` returns the frame Polars produced. The hand-written readers and writers keep their four-dtype schema contract, and `preview` still refuses a dtype it cannot show. Both are tested in `tests/collect_dtypes.rs`: a group count collects as UInt32 and its preview is refused, and a four-dtype query previews as before.

**Probe after the fix (v2):** steps 31 work, 0 differ, 7 blocked and 1 presented; composed workflows 3 work and 5 are blocked.
- The four family-A steps flip from blocked to works, and the composed W4 and W7 run end to end. Each matches its Rust twin exactly; `probe.py --fixed` asserts this.
- The ranking of what remains is E (completes W2), F (W3), G (now completes W5), then B, C and D (W1 needs all three), then H and I (W6 needs both).

## 0.55.2 (reported, not ranked)

22 steps work, 16 are blocked and 1 is presented; no composed workflow completes. The extra blocks are mostly the narrow 0.55.2 build's missing Polars features: strings, cumulative operations, rank, rolling, JSON and pivot.

**One product finding:** `w1.csv_bytes` with `try_parse_dates` makes Polars panic during schema inference ("activate one of dtype-date, dtype-datetime, dtype-time features"). It surfaces as an engine-thread panic rather than a clean error. The v2 build has those features and reads the dates.

## Findings for later records

- **Fallible values in expression position.** `polars::lit(...)` returns a Result, and so do `over`, `rank`, `std`, `cols` and the hand-written `group_by`/`agg`/`sort`. At v2 several of these are fallible in Rust too, so this matches Polars, but it is the most common friction in natural code.
- **Presentation:** frames with dtypes outside the four collect now, but `preview`/display still refuses them. That is the next presentation gap, and it matters as much as E.
- **Script ergonomics:** `select_`, the missing `clone` on DataFrame, and the missing `get` on SchemaRef.
- **Polars:** `std` is not bit-reproducible run to run. The 0.55.2 date-inference panic is noted above.

## Review round 1 (Codex)

1. **"Workflows completed" was inferred, not run.** The probe now composes all eight workflows as scripts with Rust twins. A family is credited with a workflow only when that composed workflow's sole attributed blocker is the family. W4 and W7 are verified end to end.
2. **Three steps broke the twin contract.**
   - `w5.eager_group_by`: the pinned eager `GroupBy` has no `sum`, so the step is `group_by(...).count()`, and its twin is the same call.
   - `w6.pivot`: it now makes the real eight-argument call, with its twin, and records both blockers (H and I).
   - `w8.present`: it is a presentation step, checked by its markers and counted apart.
3. **The before table was not replayable.** `before_lib.py` rebuilds the pre-fix source from the plan commit's `lib.rs` plus only the test-support `oracle_repr`. `replay.sh` builds it, probes it without `--fixed`, and byte-compares it with `outcomes-v2-before.json` before building and probing the fixed source.

## Tests, suites and launch

- **`tests/collect_dtypes.rs`:** 2 of 2.
- **Production suites:** release default (19), release test-support (226) and debug generated plus test-support (227) all pass. The debug oracle run's only differences were unordered `unique` flips, and the committed results are unchanged.
- **Surface and freeze:** unchanged. No generated binding moved; the change is in hand-written code.
- **Launch** against 0121 (fa3d2ca), default release builds, three rounds of 60 interleaved launches: median deltas −0.06, +0.07 and −0.63 ms, so no change (`launch-results-*.json`).
