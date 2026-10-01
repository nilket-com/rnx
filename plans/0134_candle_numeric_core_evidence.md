# rnx 0134 evidence: Candle's numeric-analysis core

**The measure is met.** The adapter now covers **97 of Candle 0.11.0's 220 core `Tensor` methods (44.1%)**, counted mechanically by `probes/0134/count.py` against the pinned source, plus 3 candle-nn functions and 1 derived helper.
- **One set of family contracts** covers them, implemented through family-level helpers in `adapters/candle/src/tensor_ops.rs`.
- **The examples:** a script can now build, transform, reduce, rank and read tensors back. Three examples compose them into useful answers, and each matches its direct-Rust twin.

## 1. The surface (`probes/0134/manifest.tsv`)

| family | identities | script surface |
|---|---:|---|
| A, construction and readback | 21 | `from_vec`, `zeros`, `ones`, `full`, `arange`, `arange_step`, `eye`, `zeros_like`, `ones_like`; `to_vec`, `to_vec1`, `to_vec2`, `to_scalar`; `to_dtype`; `dims`, `rank`, `elem_count`; `contiguous`, `is_contiguous`, `flatten_all`; 0129's `shape` and `dtype` |
| C, arithmetic and broadcasting | 47 | `add`, `sub`, `mul`, `div`, `maximum`, `minimum`; the Rune operators `+ - * /` with tensors or numbers; 12 `broadcast_*`; `eq` to `ge` with a tensor or a number; `affine`, `powf`, `clamp`, `where_cond`; 19 unary math functions |
| D, reductions | 18 | `sum`, `mean`, `max`, `min`, `argmax`, `argmin` (each with `_keepdim`), the 4 `_all` forms, `var` and `var_keepdim` |
| E, F, matmul and ranking | 4 | `matmul`, `broadcast_matmul`, `arg_sort_last_dim`, `sort_last_dim` |
| B, minimal shape | 7 | `reshape`, `t`, `transpose`, `squeeze`, `unsqueeze`, `narrow`, `flatten` |
| nn, counted apart | 3 of 19 | `candle::softmax`, `log_softmax`, `softmax_last_dim` |
| derived, counted apart | 1 | `t.topk(k, asc)`, giving `(values, indices)` |

**Operators return a `Result`,** following 0113's precedent for Polars, so a script writes `(a + b)?`.

## 2. The contracts, and the controls that prove them

**Every family is tested against the direct Candle call** (`tensor_ops.rs`, 8 groups of controls, bit for bit where the kernels are shared), and **every refusal is named before any work**.

**Shapes and allocation:**
- **Rank** is at most 6.
- **Checked before Candle runs:** every count and stride product, including for shapes with zeros.
- **Every output is at most 2²⁴ values,** inferred before the call, for:
  - constructors, `arange` (§2a), `eye`;
  - broadcasting;
  - `matmul`;
  - `reshape`, `contiguous`, `flatten`;
  - reductions;
  - `to_vec2`'s outer rows (`[MAX + 1, 0]` is refused: zero values, but too many rows).
- **`broadcast_matmul`'s expanded operands are checked first:** `[4096, 4096]` against a batch of 2 is refused naming "the expanded left operand [2, 4096, 4096]", though the result is small. Against a batch of 1, the same operands fit.
- **The allocation control** (`tests/tensor_alloc.rs`, one test per binary): broadcast, expanded matmul, `eye`, `arange` and `zeros`, each oversized, are refused with an allocator peak under 1 MiB. The expanded matmul's peak is its 16 MiB of inputs plus under 1 MiB. The positive control, an accepted 4 MiB tensor, is visible.

**Dtypes:**
- **No implicit promotion:** for example "dtypes differ (f32 and f64)".
- **Strict constructors:**
  - f32: a finite value outside the f32 range is refused;
  - u8: 0 to 255; u32: 0 to 2³² − 1;
  - i64: from a float, the value must be integral and below 2⁶³. **The control:** 2⁶³ is refused, the f64 just below it is accepted, and `i64::MAX` from an integer round-trips.
- **`to_dtype`** is Candle's own Rust `as` conversion, as stated.

**IEEE:** `log(0)` = −inf and `sqrt(−1)` = NaN are shown, and refused only at `to_dense`.

**Reductions:**
- negative axes; repeated axes refused;
- `var` is unbiased: `[1, 2, 3]` gives 1, and an axis of 1 is refused;
- an empty axis: `sum` gives zeros; `mean`, `max` and `argmax` refuse; `max_all` refuses an empty tensor;
- `softmax` rows sum to 1;
- `log_softmax` equals candle-nn's bit for bit.

### 2a. `arange`: the loop is proven before it runs

A bounded preflight simulates Candle's own `while current < end { push; current += step }` in the selected dtype, the terminal increment included.

**Equal to Candle bit for bit:**
- f32 0 to 1 by 0.1;
- i64 10 to 0 by −3 gives `[10, 7, 4, 1]`;
- u8 0 to 5.

**Refused by name:**
- **non-progress:** f32 16777216 to 16777220 by 1;
- **overflow:** i64 `MAX − 1` to `MAX` by 2, and u8 254 to 255 by 2;
- a zero step; a wrong-signed step ("never reaches"); non-finite endpoints or steps;
- more than 2²⁴ values.

An empty range is a valid empty tensor.

### 2b. Ranking: the upstream sort defect

Pinned candle-core's `ArgSort::asort` ignores a view's start offset.

**The control reproduces it:** `[0, 1, 2, 9, 3, 6].narrow(0, 3, 3)` reports itself contiguous, yet **Candle's raw `sort_last_dim` returns `[9, 3, 6]`**.

**The fix:** the binding first copies every ranking input into fresh offset-zero storage (`force_contiguous`). It then returns `[3, 6, 9]` with indices `[1, 2, 0]`, and a transposed input sorts its logical rows.

**Also refused:** NaN ("1 NaN values, whose order Candle leaves unspecified") and an empty last axis.

**A top-k cut inside a run of ties** (`[5, 1, 5, 3, 5]`, top 2) returns distinct indices, each holding 5, without requiring a particular subset.

### 2c. The display, for every result kind

**What each kind shows:**
- a scalar shows its value;
- a vector shows its first 8 values;
- a matrix shows its 8 × 8 corner;
- ranks 3 to 6 show the first 2-D slice, labelled "(the [0] slice)";
- **u8 values faithfully** (`0 | 1 | 255`);
- NaN and inf as such;
- an empty tensor as "(empty)".

**Bounded:** a 4096 × 4096 tensor stays within 2,048 bytes.

**0129's display test changed on purpose:** a vector now shows its values.

**From a script** (`tests/tensor_script.rs`):
- build, then operators with tensors and numbers;
- `mean_keepdim(-1)`, `broadcast_sub`, `sqr`, `sum_all`, `to_scalar`;
- `topk` destructured into `(v, i)`;
- a `gt` mask read back with `to_vec2`;
- softmax with z-scores and correlation;
- refusals as script `Err`s;
- arguments reused after errors.

### 2d. Review round 1: four fixes, each with its controls

**R1, integer scalars keep their precision.** A number compared with a tensor, or used as a `clamp` bound, now keeps its Rune integer form until it meets the receiver's dtype. It becomes a rank-0 tensor of that dtype by the constructors' strict rules, broadcast as a view. Before, it went through f64, so an i64 holding 2⁵³ + 1 didn't `eq` itself.
- **The controls:**
  - 2⁵³, 2⁵³ + 1 and 2⁵³ + 2 (big − 1, big, big + 1 with big = 2⁵³ + 1) under all six comparisons, against direct Candle with a typed i64 scalar;
  - `i64::MIN` and `i64::MAX`;
  - `clamp` with two numbers, and with a tensor and a number, exact and equal to Candle's;
  - a u8 comparison or clamp against 300 or −1 is refused ("u8 range");
  - an f32 receiver takes an integer by the f32 rule.

**R2, stride products are checked behind a zero.** The shared shape check now also follows Candle's contiguous-stride computation (`shape.rs` `stride_contiguous`), a running product from the last dimension through the first, and refuses any overflowing step. A zero dimension can't hide it.
- **The new shapes of `t`, `transpose`, `unsqueeze` and `flatten` are checked too:** a permutation can move a zero forward.
- **`flatten`'s merged segment is checked on its own** (review round 2). Zeros at both ends of a shape (`[0, 2³³, 2³³, 0]`) make the whole count and every stride valid, yet the merged middle product is 2⁶⁶. Before, that wrapped to `[0, 0, 0]` in release, and in debug it panicked on the calling thread before reaching the joined worker. Now it is refused, naming the axes. Release and debug controls cover it, plus an ordinary valid flatten and the receiver's reuse.
- **The controls:**
  - `zeros([0, i64::MAX, i64::MAX])` and `zeros([0, 2³³, 2³³])` are refused (the first wrapped in release and panicked in debug before);
  - a valid `[2³³, 0, 2³³]` refuses `transpose(0, 1)` but allows `transpose(0, 2)`, `unsqueeze` and `flatten`;
  - a rank-4 case;
  - **all in release and debug.**

**R3, `arange` refuses an increment that reaches ±infinity.** The float step now rejects any non-finite result, not only NaN and stagnation.
- **The controls:** f32 `3e38` to `3.4e38` by `3e38`, and f64 `1e308` to `1.5e308` by `1e308`, both refused, with their negative mirrors.
- **Unchanged:** an ordinary large f32 range still equals Candle's, bit for bit.

**R4, `topk` validates before any work.** The rank, a non-empty last axis and k are now checked before the fresh copy and the NaN scan.
- **The allocation control** (`tests/tensor_alloc.rs`): a 1024 × 1024 input is built first, then k = −1, 0 and 1025 are each refused with an allocator peak under 1 MiB. Before, there was a 13.6 MB peak. The same input then takes `topk(3)`.

**Unaffected:** E2's output (still bit for bit with its twin after the rebuild), and E1's and E3's ranking paths.

## 3. The examples, each against its twin (`probes/0134/twin`)

**E2: z-scores and correlation, Candle only** (`e2_stats.rn`; 0.4 s).
- **The data:** six statistics over 321 records (words, passages, code fences, table rows, headings, title length).
- **The method:**
  - standardize each column (`mean_keepdim`, unbiased `var_keepdim`, `sqrt`, `broadcast_sub`, `broadcast_div`);
  - correlate with `t().matmul()` and `* (1 / (n − 1))`;
  - mask the strict upper triangle with `arange`, `unsqueeze`, `broadcast_gt`, `abs` and `where_cond`;
  - rank the pairs with `flatten_all().topk(5)`;
  - find the most unusual documents with `abs().max(1).topk(5)`.
- **The answer:**
  - **the strongest correlations:** words and passages +0.992; passages and headings +0.784; words and headings +0.784; words and table rows +0.525;
  - **the most unusual documents:** 0025's Windows evidence (max |z| 11.5) and 0072's binding-surface evidence (9.1), both long tables of results.
- **Parity:** **equal bit for bit** with the twin: all 36 correlation values as f64 bits, the 5 pairs and the 5 unusual documents with their score bits (`compare_e.py e2`).

**E1: semantic search, Candle only** (`e1_search.rn`; no DataFrame library):
- **The steps:**
  - read and chunk 0133's D1 with `fs` and Rune;
  - `TextEncoder::embed`, then `Tensor::from_dense`, for 3,658 passages and the 5 rubric queries;
  - **one `matmul`** (unit rows, so a dot product is the cosine) gives a 3,658 × 5 score tensor, displayed bounded;
  - `t().topk(60, false)` gives the top passages per query;
  - `to_vec2` reads back values and indices;
  - Rune keeps each document's first hit (a flag per document, since Rune's `Vec` has no `contains`: another Rune std gap, alongside 0133's F1 to F3).
- **The answer:** per query, the five documents with score, record, title and a 120-character excerpt. **They are the same documents as 0133's U1** (for example Q2: 0130's plan, then 0130's evidence; Q3: 0126's plan and evidence).
- **The scores differ slightly from U1's** (0.48185458 against 0.48185435), because E1 takes a plain `matmul` of the unit embeddings where `similarity` normalizes in f64 first.
- **Time:** 374 s, almost all of it the embedding (0133's F13).
- **Parity:** **equal bit for bit** with the twin, `twin0134 e1`: the same embedding, matmul, `force_contiguous` and `sort_last_dim`, then the first five distinct documents among the top 60. All 25 rows match in document and f32 score bits (`compare_e.py e1`).

**E3: Polars and Candle** (`e3_rank.rn`):
- **The steps:** 0133's U1 ranking done on tensors (`q.matmul(e.t())` then `topk(60)`); the hits back into a Polars frame; a `left_join` to the titles; 0124's preview.
- **Parity:** **equal bit for bit with the same twin,** all 25 rows. It is the same tensor ranking, read through Polars.
- **Time:** 403 s.
- **A small presentation note:** the frame shows the scores as f64, because they were rebuilt from readback numbers with `from_iter_f64`.

## 4. Gates

**Launch** (`probes/0134/launch.py`, 0129's method: 3 rounds of 60 interleaved launches against 0132's `24f87d5`): Candle's deltas are +0.30, −0.14 and +0.68 ms, so **about +0.3 ms** for about 100 more registrations. The absolute times (9.7 to 10.4 ms) were raised by concurrent load during the run; the interleaved deltas are the measure.

**Suites:**

| suite | passed |
|---|---:|
| Candle, release | 41 (after review round 1) |
| Candle, release `test-support` | 41 |
| Candle, debug `test-support` | 41 |
| Polars, release default | 41 (Polars is unchanged) |
| project tool | 81 |
| `rnx` core | 391 |

The 41 Candle tests include `tensor_ops`' 11 control groups, `tensor_script`'s 3, `tensor_alloc`'s 1, and 0129 to 0132's tests. Clippy is clean with `test-support`.

**The examples, run before push:** with `probes/0134/runner`, a probe binary that is `rnx::main_with` plus the working tree's Polars and Candle extensions, the composition a project assembles. It's needed because `:dep` fetches pushed commits, and project builds refuse untracked files. **Owed after push:** E1 and E2 in a plain `:dep candle` session, and E3 in `:dep polars candle`.

**After push** (`probes/0134/out/dep/`; a clean worktree binary at the pushed commit, a fresh cache, `probes/0133/session.py` with `RNX_DEP` naming the adapters):
- **At `feaeebf`:** E2 passed and equalled its twin bit for bit. **E1 and E3 failed at the paste, not in Candle:** a line ending in a `//` comment (`let scores = e.matmul(q.t()?)?;  // passages x queries`) closed the input mid-function, giving "Expected expression but got eof".
- **The cause:** `session::completeness`. Rune places the zero-width end-of-input error before trailing comments and whitespace, so `end < input.len()` called an open block complete. The pre-push runs used `rnx run`, which is why it escaped.
- **The fix, folded into this impl** (`b86e715`, accepted by Codex): the error is at the end when only trivia follows it, judged by Rune's own parser (`Parser::is_eof` on the suffix); the grown-input check is unchanged.
  - **Controls:** an open function ending in a trailing comment, with and without a newline, is now incomplete (both were complete before the fix); `let x = ; // note` stays a complete, diagnosed input.
- **At `b86e715`:** all three pass, each with `WORKFLOW OK` and a final `Ok(())`, and each equals its twin bit for bit:

| example | session | `:dep` | run | rows |
|---|---|---:|---:|---:|
| E1 | `:dep candle` | 29.8 s | 360.3 s | 25 |
| E2 | `:dep candle` | 16.1 s | 0.4 s | 16 |
| E3 | `:dep polars candle` | 273.9 s | 356.5 s | 25 |

  The `:dep` times include building the adapters at the new commit (Polars dominates E3's).

**Coverage, against the stated denominator:** 97 of 220 core (44.1%, `count.py`), 3 of 19 nn, and 1 derived helper.

## 5. Not done (0135 and later)

- `index_select`, `gather`, `cat` and `stack`.
- The other 16 nn functions.
- Model composition.
- 0133's friction families, which are their own records.
