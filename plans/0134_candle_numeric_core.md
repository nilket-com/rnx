# rnx 0134: Candle's numeric-analysis core, as one family-sized batch

Status: plan. The user's direction (2026-10-01) is to widen rnx's Candle API as was done for Polars, aiming at model-assisted analytics. The batch was agreed with Codex (`.agent-chat/to-codex-085.md` and its reply). 0133's probe informs the order; 0135 (shape, indexing and nn ops) is provisionally next.

**The gap:**
- Candle 0.11.0's core `Tensor` has **220 public methods**, counted mechanically by `probes/0134/count.py`: the literal `pub fn`s in its `impl Tensor` blocks, plus the macro-defined element-wise and broadcast operations (`unary_op!`, `binary_op!`, `binary_op_scalar!`, `broadcast_binary_op!`), deduplicated. That is the inventory denominator. Operator trait impls and adapter helpers aren't in it.
- **Excluded from the denominator's target:** the autograd and in-place internals (13: `backward`, `inplace_op1` to `3`, `zero_set`, `one_set`, `const_set`, `force_contiguous`, `is_fortran_contiguous`, `mv`, `dot`, `embedding`, `unfold`) and the convolution and pooling methods.
- **rnx today maps 2 of them** (`Tensor::shape` and `Tensor::dtype`), plus the adapter-specific `from_dense` and `to_dense`, which are rnx names over Candle's `from_vec` and `to_vec1`, not core identities themselves. A script therefore can't build, transform, reduce, rank or read a tensor's values, and 0133's workflows fall back on Polars or hit dead ends.

**The measure:** **97 core identities (44.1% of 220)** are covered under one set of family contracts. They are listed one per row in the committed `probes/0134/manifest.tsv` and counted by `count.py` against the source, which reports none missing and no duplicates. The 3 candle-nn functions and 1 derived helper are counted separately. Two Candle-only `:dep candle` examples and one Polars+Candle example compose them into useful answers, each matched against a direct-Rust twin.

## 1. The manifest (every mapped Rust identity)

**Conventions:**
- `T` is a `candle::Tensor`.
- A *dims* argument is a Rune integer or a vector of them. Negative values count from the end, Python style, and are normalized and checked against the rank before Candle is called.
- *dtype* is one of `"f32"`, `"f64"`, `"i64"`, `"u32"` or `"u8"`.
- *values* is a Rune vector of numbers.

**A, construction and readback** (21 identities, including 0129's `shape` and `dtype`):

| script | Rust identity | notes |
|---|---|---|
| `Tensor::from_vec(values, shape, dtype)` | `Tensor::from_vec` | the constructor rules in §2.3 |
| `Tensor::zeros(shape, dtype)`, `ones` | `Tensor::zeros`, `ones` | the shape is checked first (§2.1) |
| `Tensor::full(value, shape, dtype)` | `Tensor::full` | the value follows the §2.3 rules |
| `Tensor::arange(start, end, dtype)`, `arange_step(start, end, step, dtype)` | `arange`, `arange_step` | a bounded, overflow-checked preflight of Candle's own repeated-add loop (§2.6) |
| `Tensor::eye(n, dtype)` | `Tensor::eye` | n² checked first |
| `t.zeros_like()`, `ones_like()` | the same | |
| `t.to_vec()` | `flatten_all` then `to_vec1` | flattened and bounded readback into Rune numbers (f64 for floats, i64 for integers) |
| `t.to_vec2()` | `to_vec2` | rank 2 only, bounded |
| `t.to_scalar()` | `to_scalar` | rank 0 only |
| `t.to_dtype(dtype)` | `to_dtype` | Candle's conversion semantics, stated in §2.3 |
| `t.dims()`, `rank()`, `elem_count()` | the same | |
| `t.contiguous()`, `t.is_contiguous()` | `Tensor::contiguous`, `Tensor::is_contiguous` (its own canonical identity) | |
| `t.flatten_all()` | the same | |

**C, arithmetic and broadcasting** (47 unique identities; `affine` is counted once, and Candle's operator impls aren't counted):

| script | Rust identity |
|---|---|
| `a.add(b)`, `sub`, `mul`, `div`, `maximum`, `minimum`, and the Rune operators `a + b`, `a - b`, `a * b`, `a / b` | the binary ops: equal shapes and dtypes |
| `a + 2.0` etc. (a tensor with a number) | Candle's `impl Add<f64> for Tensor` and friends, which are `affine` |
| `a.broadcast_add(b)`, `broadcast_sub`, `broadcast_mul`, `broadcast_div`, `broadcast_maximum`, `broadcast_minimum`, `broadcast_eq`, `broadcast_ne`, `broadcast_lt`, `broadcast_le`, `broadcast_gt`, `broadcast_ge` | the same |
| `a.eq(x)`, `ne`, `lt`, `le`, `gt`, `ge`, where x is a tensor or a number | the same (`TensorOrScalar`); the result is a u8 mask |
| `t.affine(mul, add)`, `t.powf(e)`, `t.clamp(min, max)` | the same |
| `mask.where_cond(on_true, on_false)` | the same; the mask must be u8 |
| `t.exp()`, `log`, `sqrt`, `sqr`, `abs`, `neg`, `recip`, `tanh`, `relu`, `gelu`, `gelu_erf`, `silu`, `sin`, `cos`, `floor`, `ceil`, `round`, `sign`, `erf` | the unary ops |

**D, reductions and normalization** (18 identities):

| script | Rust identity |
|---|---|
| `t.sum(dims)`, `sum_keepdim`, `mean(dims)`, `mean_keepdim` | the same |
| `t.max(dim)`, `max_keepdim`, `min`, `min_keepdim`, `argmax`, `argmax_keepdim`, `argmin`, `argmin_keepdim` | the same |
| `t.sum_all()`, `mean_all`, `max_all`, `min_all` | the same |
| `t.var(dim)`, `var_keepdim` | the same: **unbiased (n − 1)**, as Candle computes it |

**E and F, matmul and ranking** (4 identities):

| script | Rust identity |
|---|---|
| `a.matmul(b)`, `a.broadcast_matmul(b)` | the same |
| `t.arg_sort_last_dim(asc)` | the same |
| `t.sort_last_dim(asc)` | the same; returns `(values, indices)` |

**B, minimal shape operations** (7 identities, with `contiguous` and `flatten_all` counted in A): `t.reshape(shape)`, `t.t()`, `t.transpose(d1, d2)`, `t.squeeze(dim)`, `t.unsqueeze(dim)`, `t.narrow(dim, start, len)`, `t.flatten(start, end)`.

**Counted separately:**
- **candle-nn (3 of 19):** `candle::softmax(t, dim)`, `candle::log_softmax(t, dim)`, `candle::softmax_last_dim(t)`.
- **Derived (1):** `t.topk(k, asc)`, which is `sort_last_dim` then `narrow`; it returns `(values, indices)`.
- **Kept from 0129 and 0131:** `from_dense`, `to_dense`, `shape`, `dtype`, the models and `similarity`.

**Total:** 21 + 47 + 18 + 2 + 2 + 7 = **97 core identities**, plus 3 nn and 1 derived, counted mechanically.

## 2. The family contracts, set once

**Registration:** through family-level helpers in a new `adapters/candle/src/tensor_ops.rs`, not per-method code:
- a unary table;
- a binary table (same-shape);
- a broadcast table;
- a comparison table;
- a reduction table with dims normalization;
- a constructor set.

Each helper carries its family's checks, its conversion and its error naming.

### 2.1 Shapes and allocation: checked before Candle runs

**Rank:** at most **6**.
- Every dimension is checked, and integer conversions (`i64` to `usize`) are checked.
- **Rank 0 (scalars)** is supported: `to_scalar`, comparisons with numbers, reductions with `_all`.
- **Zero-length axes** are allowed in construction and in shape and arithmetic operations. Candle handles empty tensors.
  - **max, min, argmax and argmin over an empty axis** are refused by name, since Candle has no identity for them.
  - **sum over an empty axis** gives 0.
  - **mean over an empty axis** is refused by name, rather than returning 0/0.

**Every output is at most 2²⁴ elements,** `Dense`'s bound. **Every count and stride product is checked before Candle is called, including for shapes that contain a zero.** The output shape is **inferred and checked before the Candle call**, for every operation that can grow:
- constructors: the shape product, the `arange` count, `eye` n²;
- broadcasting: the broadcast shape of both operands, with checked arithmetic;
- `matmul`: the batch dimensions times m × n, with the inner dimensions checked to match;
- **`broadcast_matmul`:** Candle concretizes the broadcasted operands with `contiguous` (`tensor.rs:1545`). So the **expanded size of each operand, and its copy, are inferred and capped before the call**, as well as the output;
- **reductions:** the output shape is inferred and checked, including over zero-length shapes (for example `[N, 0]` summed over axis 1 produces N values);
- **`to_vec2`:** the outer row metadata is counted (`[N, 0]` has zero elements but N vectors), so N rows are bounded too;
- `reshape`: the element count must be equal;
- `to_dtype` and `contiguous`: the same count.

A cap checked only after Candle returns would be too late.

**Intermediates:** stated per family, and kept within a small multiple of the output cap.
- Element-wise and broadcast operations allocate their output only; broadcasting is a view.
- `contiguous` copies once.
- `var` allocates its `(x − mean)²` temporary: ≤ 2 × the input.
- `softmax` and `log_softmax`: ≤ 3 × the input, from exp, sum and divide.
- `sort_last_dim`: the values plus the u32 indices, with a contiguous copy first when needed.
- **The evidence measures them** (the allocation peak) against these statements.
- **Each check has a tiny-allocation refusal control:** an oversized broadcast, a `broadcast_matmul` with expanded operands, a zero-axis reduction, `to_vec2` rows, an `eye`, and an `arange` count.

### 2.2 Ownership, execution and display

**Ownership:** tensors are immutable Rune values. Candle tensors are `Arc`-backed, so a clone is cheap. Every argument is borrowed and stays usable after Ok and after Err.

**Execution:** eager, on the CPU, inside 0129's joined worker (panics become named Errs). **The per-operation overhead of the worker is measured;** if it dominates small operations, that is a finding for a follow-up, not a silent change.

**The display is bounded, and every result kind is visible:**
- a header with the dtype and the full shape (at most 6 dimensions, so bounded);
- **rank 0:** the value;
- **rank 1:** the first 8 values;
- **rank 2:** the 8 × 8 corner, as 0129 did;
- **rank 3 to 6:** the corner of the first 2-D slice, labelled as such;
- **u8 values faithfully:** a mask shows 0 and 1 because that is what it holds, and 255 shows as 255. **u32 and i64 indices** show as integers;
- **NaN and ±inf** shown as such;
- the total stays within 2,048 bytes.

**Readback:**
- `to_vec` returns at most 2²⁴ values; the count is checked before the copy;
- `to_vec2` requires rank 2;
- `to_scalar` requires rank 0.

### 2.3 Dtypes, precision and IEEE policy

**No implicit promotion:**
- Binary, broadcast and comparison operations require equal dtypes, refused by name otherwise, with both dtypes named.
- `where_cond` requires a u8 mask and equal branch dtypes.
- `matmul` requires f32 or f64.
- Math on integer tensors is refused where Candle's result would be meaningless; the unary math table lists the dtypes each operation accepts.

**Constructors are strict:**
- `from_vec` and `full` take Rune numbers.
- **For `f32`:** finite values beyond the f32 range are refused, as 0129's policy; others round to nearest.
- **For integer dtypes:** a value must be integral and within range (u8 0 to 255, u32 0 to 2³² − 1, i64), or it is refused.
  - **A float bound for i64,** with a control at the 2⁶³ boundary: 2⁶³ itself is out of range, since `i64::MAX` isn't representable as an f64.

**`to_dtype` is Candle's conversion, unchanged,** and its semantics are stated rather than filtered: Rust `as` casts, so float to integer saturates and NaN becomes 0. A script needing strictness checks first (with `ge`, `le` and a reduction).

**Low-level math follows IEEE:** `log(0)` = −inf, `sqrt(−1)` = NaN, division by 0 is ±inf or NaN. Tensors may hold non-finite values. **Only the `Dense` conversion keeps its finite-only refusal** (0129); that boundary is where interchange with Polars happens.

### 2.4 Reductions, axes and operands

- **Negative axes** count from the end and are normalized and checked against the rank.
- **`var` is unbiased (n − 1),** as Candle computes it. Over an axis of length 1 it divides by 0, so **an axis shorter than 2 is refused by name**. A population variance is a composition (`sqr`, then `mean`, minus the squared mean), shown in the z-score example.
- **Scalar operands:** comparisons and `clamp` take tensors or numbers (Candle's `TensorOrScalar`). Arithmetic with a number uses Candle's operator impls (`affine`). Every other binary operation takes tensors.

### 2.5 Ranking

- **`arg_sort_last_dim` and `sort_last_dim`** sort along the last dimension. **An upstream defect drives the input rule** (review of the plan, R1): pinned candle-core's `ArgSort::asort` (`src/sort.rs`) ignores the layout's start offset and sorts a prefix of the backing storage.
  - **Codex reproduced it:** `[0,1,2,9,3,6].narrow(0,3,3)` reports itself contiguous, yet the raw sort returns `[9,3,6]`.
  - **Why `contiguous()` doesn't help:** it clones an already-contiguous offset view unchanged.
  - **So every ranking input is copied first into fresh, offset-zero storage,** with `force_contiguous` or an equivalent proven copy, whatever its layout. The copy is counted.
  - **Controls:** contiguous slices with a nonzero offset, transposes, and plain inputs.
  - **The twin** uses the corrected logical input, and the evidence discloses the upstream difference.
- **An empty last axis** is refused by name.
- **NaN:** the order Candle gives NaN is unspecified, so **an input containing NaN is refused by name** (a scan before sorting).
- **Ties:** Candle documents tie order as unspecified. So the **values are compared exactly; the indices are compared as sets within each run of equal values.** When a `topk` cutoff falls inside a run of equal values, a valid result may pick a different subset of the tied indices. The control then checks that the returned indices are distinct and that each one's value equals the tied value, rather than requiring the same subset.
- **`topk(k, asc)`:** k from 1 to the last-axis length; it returns values and indices; both can be read back.

### 2.6 `arange`: proving the loop before running it (review of the plan, R2)

**Why an analytic count isn't enough:** Candle's `arange_step` is a repeated-add loop, so a count computed from the endpoints doesn't make it safe.
- in f32, `start = 16777216, end = 16777220, step = 1` never progresses;
- `i64::MAX − 1` to `i64::MAX` with step 2 overflows at the terminal increment;
- so does u8 `254` to `255` with step 2.

**The binding first runs a bounded preflight of the same loop,** in the selected dtype, with checked arithmetic. It refuses by name:
- non-finite endpoints or steps;
- a zero step, or a step of the wrong sign;
- any increment, the terminal one included, that overflows, or that fails to change the value;
- more than 2²⁴ steps (the bound is reached before anything proportional is allocated).

**Only a proven sequence reaches Candle,** whose own loop then produces it, bit for bit; the twin calls Candle directly with the same inputs. The loop is never silently replaced with `start + i × step`.

**Controls:** each of the three cases above, plus a zero step, a wrong-signed step, and the 2²⁴ bound.

## 3. The examples (`probes/0134`), each with a direct-Rust twin

**E1, semantic search with Candle only** (`:dep candle`, no Polars):
- read 0133's D1 with `fs` and chunk it (0133's rule);
- embed with `TextEncoder`, then `Tensor::from_dense`;
- `matmul` against the query embeddings, then a `topk` over the passages;
- read back the indices with `to_vec`, and keep each document's first hit in Rune;
- print the titles, scores and excerpts.

This is 0133's U1 answer without a DataFrame library.

**E2, z-scores and correlation with Candle only:**
- **The data:** a numeric matrix of per-document statistics from D1 (words, passages, code blocks, table rows, title length), built with `Tensor::from_vec`.
- **Standardize:** per-column `mean_keepdim` and `var_keepdim` (unbiased), then `sqrt`, `broadcast_sub` and `broadcast_div`.
- **Correlate:** a correlation matrix through `t().matmul(…)` and `affine` (1 / (n − 1)).
- **Rank:** the top correlated pairs through `flatten_all`, a strict upper-triangle mask (from `arange` comparisons), `where_cond` and `topk`.
- **The answer:** the pairs, named.

**E3, Polars + Candle:** 0133's U1, rewritten to rank on tensors (`topk`) and display through Polars.

**The answers are judged, by the agent and labelled so, for usefulness;** every number is matched by its twin.

## 4. Controls (tiny direct-Rust differential tests)

**Every family:**
- **The result** against the direct Candle call: bit for bit, the same kernels.
- **The refusals,** each by name, before any work:
  - mixed dtypes, a wrong rank, an out-of-range axis;
  - an oversized output, refused before allocation (shown through the allocation peak);
  - an empty axis for max, min and argmax, `var` over an axis shorter than 2;
  - NaN in ranking, a bad `topk` k;
  - a u8 mask required, out-of-range constructor values, a zero `arange` step.
- **Rank 0 and zero-length axes,** per the §2.1 rules.
- **IEEE results** (`log(0)`, `sqrt(−1)`, `1/0`) shown, and refused only at the `to_dense` boundary.
- **Non-contiguous inputs** to sorting give the contiguous result.
- **Ties:** the set comparison of indices.
- **The display:** every result kind (scalar, vector, matrix, rank 3, mask, indices, NaN), each within 2,048 bytes.
- **Reuse:** every argument, after Ok and Err.

## Gates

- **The examples:** E1 and E2 run in a plain `:dep candle` session, and E3 in `:dep polars candle`, all after push. Their twins match.
- **Suites:** core, Polars' three, Candle's and the project tool's; the Polars freeze is unchanged.
- **Launch:** the Candle launch against 0132, since there are about 100 more registrations.
- **Coverage, reported against the stated denominator:** 97 of 220 core (`count.py`), 3 of 19 nn, 1 derived.

## Kept from 0133, unchanged here

0133 found that embedding long passages took 387 s against PyTorch's 49 s, and that 39% of passages exceed 256 tokens. Both are important findings, but they are **not** grounds to relax 0134's bounds, nor to change the frozen chunking or the model's execution in this record. Their fixes are their own records.

## Stop rules

- If an operation's output shape can't be inferred before the call, leave it out and name it. Never cap after the fact.
- If a direct-Rust twin and the binding disagree, stop and report.

## Out of scope (0135 and later)

- `index_select`, `gather`, `scatter`, `cat` and `stack`.
- `permute`, `broadcast_as`, `repeat` and padding.
- The other 16 nn functions.
- Convolution, pooling and autograd.
- GPU.
