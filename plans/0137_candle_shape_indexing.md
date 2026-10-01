# rnx 0137: Candle's shape, indexing and composition family, with passage-to-document pooling

Status: plan, the third of the order agreed with Codex after 0133 (0135 throughput, 0136 chunking, then this). Amended after Codex's review:
- per-operation intermediate and metadata preflights;
- the pinned arithmetic and view contracts;
- an honest pooling tolerance;
- an exact, committed manifest counted mechanically.

**What's still missing for analytics:** 0134 mapped Candle's numeric-analysis core, 97 of 220 core `Tensor` identities (44.1%). A script still can't:
- **join** tensors (`cat`, `stack`) or **reorder** their axes (`permute`);
- **tile, pad or window** them (`repeat`, `pad_*`, `unfold`);
- **select rows by index** (`index_select`, `gather`, `embedding`), or **accumulate into groups** (`index_add`, `scatter_add`);
- **pool passages back into one vector per document,** which 0136 left open.

**The measure, exact:** `probes/0137/manifest.tsv` (committed with this plan) lists **40 core identities** (G 23, H 8, I 9) and the derived `segment_mean`.
- **Counting:** `probes/0134/count.py` counts the manifests cumulatively against the source: `count.py CANDLE_CORE_SRC probes/0134/manifest.tsv probes/0137/manifest.tsv`. It now takes several manifests, classifies families A to I, keeps nn and derived apart, and exits 1 on a missing identity or a duplicate across manifests. Both failures were checked.
- **The gated total:** **137 of 220 (62.3%),** with none missing and no duplicates.

## 1. The manifest and each operation's contract

0134's conventions hold:
- *dims* (axes) may be negative, normalized and checked against the rank;
- the same five dtypes;
- borrowed, immutable receivers and arguments;
- named errors;
- the joined worker's panic boundary for value work.

**The caps:** `CAP` = 2^24 values and rank at most 6, as 0134. To them, this record adds:
- **`AXIS` = 2^24:** a bound on any single axis length that an operation turns into allocations (index vectors, reference lists, readback nodes), **independently of the value count.** A zero-value tensor like `zeros([0, 2^40])` therefore can't drive a large allocation.
- **`PARTS` = 4,096:** a bound on a returned or consumed vector of tensors.

**Element indices against axes:**
- **An axis** (a *dim*) may be negative, counted from the end.
- **An element index** (`get`, `get_on_dim`, and the values inside index tensors) is a non-negative integer below the axis length. Negative indices are refused by name, not wrapped, because Candle's indices don't count from the end.

**G, shape and composition** (23):

| script | contract, all checked before Candle |
|---|---|
| `Tensor::cat(ts, dim)` | 1 to `PARTS` tensors, borrowed; one dtype; equal shapes except `dim`; the summed output count at most `CAP` |
| `Tensor::stack(ts, dim)` | as `cat`, with all shapes equal; the output has rank + 1 at most 6 |
| `t.chunk(n, dim)` | n from 1 to `PARTS` (a cap independent of values); Candle returns min(n, dim size) views, and when n exceeds the size it returns `size` views of 1, so the returned count is checked against `PARTS` too |
| `t.permute(dims)` | an exact permutation of 0..rank |
| `t.broadcast_as(shape)`, `t.expand(shape)` | broadcast compatibility; the target's value count at most `CAP`, since a view is still capped (readback would materialize it) |
| `t.broadcast_left(dims)` | the new rank at most 6, and the target count at most `CAP` |
| `t.flatten_from(d)`, `t.flatten_to(d)` | 0134's checked merged product |
| `t.flip(dims)` | distinct axes. Candle allocates an i64 index vector of each flipped axis's length (`flip` is a loop of `index_select`), so each flipped axis is at most `AXIS`, and the copies (one output-sized copy per flipped axis) are within `CAP` each |
| `t.roll(shift, dim)` | Candle computes `shift.rem_euclid(dim_size as i32)` in i32, so the axis is at most `i32::MAX`, refused otherwise even when the tensor has no values. The shift is any i64, including `i64::MIN`: reduced by rnx in i64 (`rem_euclid` on i64) to `0 ≤ s < size`, then passed as that i32. An empty selected axis is refused |
| `t.repeat(multipliers)` | one multiplier per axis (after Candle's left-extension when fewer axes are given, which is refused here: exactly the rank). **A multiplier of 0 is refused,** because Candle treats it as 1, not as an empty axis. Each multiplier is at most `AXIS`, which bounds Candle's `vec![&inp; repeat]` reference list even for a zero-value tensor. Each per-axis intermediate (`cat` of `repeat` copies) and the final count are at most `CAP` |
| `t.pad_with_zeros(dim, left, right)` | left + right at most `AXIS`; the padded count at most `CAP` |
| `t.pad_with_same(dim, left, right)` | as `pad_with_zeros`; Candle builds a reference list of left + right + 1, bounded by `AXIS`; an empty tensor is refused (Candle refuses it too, now by name first) |
| `t.unfold(dim, size, step)` | size from 1 to the axis, step at least 1. **The integer window count** ⌊(axis − size) / step⌋ + 1 must equal Candle's f32 computation `((axis as f32 − size as f32) / step as f32 + 1.0) as usize`, computed the same way by rnx before the call; a disagreement is refused. **The stride product** `stride[dim] × step` is checked. The new view's value count (windows × size × the rest) is at most `CAP`. A zero-value tensor with a large axis and step is a control |
| `Tensor::tril2(n, dtype)`, `Tensor::triu2(n, dtype)` | n² at most `CAP`, checked |
| `t.to_vec0()` | rank 0 only |
| `t.to_vec3()` | rank 3; **the outer and middle list nodes, d0 and d0 × d1, are bounded by `AXIS` independently of the leaf count** (so `[2^20, 2^20, 0]` is refused), and the leaf values are bounded by 0134's readback limit |
| `t.dim(d)`, `t.stride()` | metadata |
| `t.force_contiguous()`, `t.copy()` | a copy within `CAP` |

**H, indexing and grouping** (8):

| script | contract |
|---|---|
| `t.index_select(ids, dim)` | `ids` is a rank-1 u32 or i64 tensor of at most `AXIS` entries. **Every index is checked in bounds before Candle,** in one pass, refused with the first bad position; negative indices are refused. The output count is at most `CAP` |
| `t.gather(ids, dim)` | `ids` has `t`'s rank, and its shape equals `t`'s except at `dim`; indices checked; the output count is at most `CAP` |
| `t.embedding(ids)` | `t` is rank 2 (a table), `ids` is rank 1; indices checked against the table's rows |
| `t.index_add(ids, src, dim)`, `t.scatter_add(ids, src, dim)` | shapes as Candle requires, checked first; indices checked; the result is a new tensor (receivers unchanged). **The summation order is Candle's** (it's stated, and the twin uses the same calls) |
| `t.get(i)` | rank at least 1; i is an element index (above), below the first axis. A rank-0 tensor or an empty first axis is refused |
| `t.get_on_dim(dim, i)` | dim is an axis (it may be negative); i is an element index below that axis's length |
| `t.slice_assign(ranges, src)` | one half-open `[start, end)` per axis, each within the axis and non-empty; `src`'s shape equals the ranges' extents. Candle builds a u8 mask of `src`'s shape and pads both to `t`'s shape, so its intermediates (a mask, the padded `src`, the padded mask, and the output) are each at most `CAP`. The result is new; `t` is unchanged |

**I, composition math** (9):

| script | contract |
|---|---|
| `a.dot(b)` | rank 1 each, equal length, the same float dtype |
| `m.mv(v)` | rank 2 by rank 1, matching inner length |
| `t.cumsum(dim)` | **Candle builds `triu2(n)`,** where n is the axis length, **and multiplies:** at rank 1 an n² matrix and a matmul; above rank 1, a transpose and a `broadcast_matmul` that concretizes `triu2` per batch. So, before the call: n² at most `CAP`; above rank 1, (batch × n²) at most `CAP` for the expanded operand, and the transposed copy within `CAP`. With `CAP` = 2^24, a 1-D axis is at most 4,096. A longer axis is refused by name ("cumsum over an axis of N needs an N×N matrix, above the cap"), not computed another way. Float dtypes only, as Candle's matmul requires |
| `t.norm()` | float; the result is rank 0 |
| `t.log_sum_exp(dims)` | float; IEEE semantics |
| `t.pow(e)`, `t.broadcast_pow(e)` | float, the same dtype; `broadcast_pow` with 0134's broadcast checks |
| `t.elu(alpha)` | float, finite alpha |
| `t.round_to(decimals)` | float; decimals within ±30 (an i32 for Candle) |

**The derived helper (counted apart): `candle::segment_mean(values, segments, n)`,** the passage-to-document mean:
- **Inputs:** `values` is rank 2, (rows × width), in f32 or f64. `segments` is a rank-1 u32 or i64 tensor of length rows, each entry in 0..n. n is from 1 to `AXIS`.
- **The result:** (n × width). Row k is the mean of the rows whose segment is k. **An empty segment is refused by name.**
- **The definition is Candle's own:** `zeros(n, width).index_add(segments, values, 0)`, divided (broadcast) by the counts, which come from `zeros(n).index_add(segments, ones(rows), 0)`.
- **Preflight** before any allocation: the output (n × width), the counts (n), the ones (rows), and the broadcast division's operands (an n × width copy of the counts) are each at most `CAP`.
- **Non-finite inputs** follow IEEE through Candle's calls (a NaN row makes its segment's mean NaN), and are reported, not refused.

**The numerical checks for pooling, stated honestly:**
- **Same-call parity is authoritative:** a Rust twin issuing the same Candle calls is **bit-equal**.
- **The f64 cross-check is a diagnostic, scoped and stated:** a pure-Rust f64 mean in row order, compared as `|rnx − f64| ≤ 1e-6 + 1e-6·|f64|` per value. **It's gated only on stated well-conditioned inputs,** namely unit-norm embedding rows: the E4 workflow's values and a fixture of them.
- **Ill-conditioned inputs are reported honestly, not gated:**
  - a cancellation control, f32 rows `[1e8, 1, −1e8]`: Candle gives 0, while f64 gives 1/3;
  - NaN, ±∞, and f32 overflow in the sum.

**Excluded, with reasons (stated in the manifest's companion note in the evidence):**
- autograd and in-place internals: `backward`, `detach`, `track_op`, `apply_op*`, `inplace_op*`, `*_set`, `vs_*`, `vd_*`;
- convolution, pooling and upsampling: `conv*`, `*_pool2d*`, `interpolate*`, `upsample*`;
- devices and storage: `to_device`, `device`, `storage_and_layout`, `from_storage`, `from_raw_buffer`, `strided_*`, `layout`, `id`, `is_variable`, `is_fortran_contiguous`;
- unseeded randomness: `rand*`, `randn*`;
- file I/O: `read_np*`, `write_*`, `save_safetensors`;
- `meshgrid`;
- `scatter`, `scatter_set`, `slice_scatter*` and `slice_set`: order under duplicate indices, or in-place forms;
- Rust-only constructors: `from_slice`, `from_iter`, `new`;
- internal helpers: `cmp`, `normalize_axis`, `sorted_nodes`.

## 2. Examples and workflows

**E4, candle-only** (`:dep candle`), ticket near-duplicates from pooled passages:
- **The steps:** D2's whole tickets are chunked (0136), the passages embedded, then pooled per ticket with `segment_mean`, renormalized, scored with a matmul, and ranked with `topk`. The output is the pairs above 0.80, the frozen U3 edge.
- **Parity:** bit for bit with a Rust twin on the same partition and calls.
- **The change against 0133's truncated tickets** (edges and components gained or lost) is reported as a finding, not a target.

**E5, a shape and indexing tour:** a readable script through G, H and I (cat, stack, permute, index_select, gather, cumsum, unfold, slice_assign). Its values are checked against a Rust twin.

## 3. Controls

- **Each contract's refusals,** by name and before Candle, including the review's cases:
  - `repeat` with a 0 multiplier, and with a large multiplier on `zeros([0, 1])`;
  - `chunk` with n above `PARTS`;
  - `flip` on an axis above `AXIS` with zero values;
  - `pad_with_same` with left + right above `AXIS`;
  - `to_vec3` of `[2^20, 2^20, 0]`;
  - `roll` on `zeros([0, 2^32])` (the axis above `i32::MAX`), and with a shift of `i64::MIN` on a normal axis (computed correctly);
  - `unfold`: a window count disagreeing with Candle's f32 count, a stride product overflow, and a zero-value large-stride case;
  - `get` on rank 0 and on an empty axis, and negative element indices;
  - `cumsum` above the n² bound;
  - `slice_assign`'s intermediates.
- **Allocation peaks** (0134's integration-test pattern): an oversized `repeat`, `cat`, `unfold`, `cumsum` and `segment_mean` are each refused under a small allocation bound.
- **Value equality** with direct Candle on small fixtures for every identity, including negative axes and edge shapes.
- **Receivers and arguments stay usable** after every refusal.
- **Pooling:** an empty segment, out-of-range and negative ids, a row-count mismatch, n = 0, the cancellation and non-finite reports, and the bit-equal same-call twin.

## Gates

- **The count:** `count.py` reports 137 of 220 cumulatively, none missing, no duplicates.
- **E4 and E5** are bit-equal to their twins; the f64 pooling cross-check is within tolerance on its stated well-conditioned inputs.
- **The earlier replays** (0135's and 0136's) pass unchanged.
- **Suites:** core, Polars, Candle (release, `test-support` and debug), and the project tool, plus clippy and launch.
- **`:dep candle` and `:dep polars candle`** after push.

## Out of scope

The excluded identities above, GPU, training, and new model architectures.
