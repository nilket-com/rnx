# rnx 0145: Candle signal operations, with E7 traffic spikes in four language articles

Status: plan, agreed with Codex and the user (0144 the session fix, then this). The 16 signal operations 0143 excluded, paired with an analyst example on real data.

**Amended after Codex's review (R1 to R4):**
- **R1:** a complete allocation census, path by path.
- **R2:** geometry checked in upstream's own order, with each kernel layout stated.
- **R3:** an upstream conv1d defect on non-contiguous kernels, found and worked around.
- **R4:** E7's synthetic cases fixed with their expected outcomes, and its labels and ties made exact.

**What an analyst gets:** convolution, pooling and resampling over time series and grids, in a session. E7 uses them on 3¼ years of daily Wikipedia traffic for the Rust, Python, Go and Zig articles:
- a smooth trend;
- spike days flagged with a robust z-score and grouped into events;
- the weekday pattern;
- each article's busiest week.

All of it is shown as Polars tables.

## 1. The manifest: 16 identities (L, signal)

These are 0143's `signal` exclusions, moved to bound: 150 → **166 of 220 (75.5%)**. `count.py` keeps bound + excluded = 220, now with 54 excluded.

| identity | script |
|---|---|
| `Tensor::conv1d` | `t.conv1d(kernel, padding, stride, dilation, groups)` |
| `Tensor::conv1d_with_algo` | `t.conv1d_with_algo(kernel, padding, stride, dilation, groups, algo)` |
| `Tensor::conv2d` | `t.conv2d(kernel, padding, stride, dilation, groups)` |
| `Tensor::conv2d_with_algo` | `t.conv2d_with_algo(kernel, padding, stride, dilation, groups, algo)` |
| `Tensor::conv_transpose1d` | `t.conv_transpose1d(kernel, padding, output_padding, stride, dilation, groups)` |
| `Tensor::conv_transpose2d` | `t.conv_transpose2d(kernel, padding, output_padding, stride, dilation)` |
| `Tensor::avg_pool2d` | `t.avg_pool2d([kh, kw])` |
| `Tensor::avg_pool2d_with_stride` | `t.avg_pool2d_with_stride([kh, kw], [sh, sw])` |
| `Tensor::max_pool2d` | `t.max_pool2d([kh, kw])` |
| `Tensor::max_pool2d_with_stride` | `t.max_pool2d_with_stride([kh, kw], [sh, sw])` |
| `Tensor::upsample_nearest1d` | `t.upsample_nearest1d(n)` |
| `Tensor::upsample_nearest2d` | `t.upsample_nearest2d(h, w)` |
| `Tensor::upsample_bilinear2d` | `t.upsample_bilinear2d(h, w, align_corners)` |
| `Tensor::upsample_bilinear2d_with_scale` | `t.upsample_bilinear2d_with_scale(sh, sw, align_corners)` |
| `Tensor::interpolate1d` | `t.interpolate1d(n)` |
| `Tensor::interpolate2d` | `t.interpolate2d(h, w)` |

**The `_with_algo` forms:** `algo` is `"none"` or a cuDNN algorithm name, which Candle passes only to CUDA. On this CPU-only adapter it has no effect, which the contract states and a control shows: the result equals the plain form's.

## 2. Contracts (measured hazards first)

Measured in candle-core 0.11.0's source before planning:
- **The output sizes use unchecked `usize` arithmetic:**
  - conv1d and conv2d compute `(l + 2p − d(k−1) − 1) / s + 1`, which underflows when the kernel's extent exceeds the padded input (a panic in debug, a wrapped size in release);
  - the transposed forms compute `(l − 1)·s − 2p + …`, which underflows on an empty input or a large padding.
- **Stride 0 divides by zero** in convolutions and pooling.
- **conv1d on the CPU uses im2col:** a buffer of batch × output length × (input channels ÷ groups × kernel), plus a copy of the kernel and of the result. It can be far larger than either the input or the output.
- **`upsample_bilinear2d_with_scale`** computes `floor(h · scale) as usize`. A NaN scale saturates to 0, and an infinity to `usize::MAX`.

**R1, the allocation census.** Every CPU path's temporaries (pinned source; each predicted by formula, checked before the call, and its peak measured against the formula in the controls):
- **`conv1d`** (im2col; `cpu_backend/mod.rs` `conv1d`):
  - the column buffer, b × l_out × (c_in/g × k);
  - a kernel copy (only on the non-contiguous branch, which R3 avoids);
  - the matmul result and its transposed copy, each b × c_out/g × l_out.
- **Grouped forms:** Candle `chunk`s the input and kernel into g tensors, convolves each, and `cat`s. That's g input views, g results retained until the `cat`, and the `cat`'s copy, so **groups ≤ `PARTS`** (4,096) is checked before any chunk exists.
- **`conv2d`** (`cpu_backend/conv2d.rs`), three branches, chosen as upstream chooses them (lines 35–49). Each is checked before the call, and in review round 2 Codex found two of them missing from the first census:
  - **A 1×1 kernel with stride 1, padding 0 and dilation 1** takes the reshaped fast path: reshaped copies of the input and kernel (at most their own sizes) and the output.
  - **Any other 1×1 kernel takes the full im2col path:**
    - the column, b × out_h × out_w × (c_in/g × kh × kw);
    - the matmul result and its copy, each b × c_out/g × out_h × out_w.
    - **The forcing control:** input [1, 64, 1, 1], kernel [1, 64, 1, 1], padding 256: a 513 × 513 output within the cap, but a column of 16,842,816 values over it, refused before the call.
  - **Every larger kernel takes the tiled im2col path:**
    - a contiguous NHWC copy of the input, b × c_in × h × w;
    - the flattened kernel, c_out × c_in/g × kh × kw;
    - the output, b × c_out × out_h × out_w;
    - **per tile** (512 output pixels): a column tile of (c_in/g × kh × kw) × 512, a coordinate list of 512 pairs, and a **separate matmul result of c_out/g × 512,** all live together with the output (lines 239–247).
    - **The tile bound is conservative and source-backed** (Codex's review): the tile closures call `MatMul` with Rayon parallelism, so nested tasks can keep parent temporaries alive, and no tighter scheduling bound is proven. The census therefore sums every tile's temporaries over all tiles and batches as if all were live, b × ⌈out_h·out_w ÷ 512⌉ × 512 × (k + c_out/g + 2), and bounds that sum within the cumulative allowance. A single tile must also fit the cap.
    - **A control covers the complete tiled census,** with a refusal forced by the tile sum alone.
- **`conv_transpose1d`:**
  - **the col2im fast path** (a contiguous kernel, dilation 1, padding 0, output_padding 0) builds a **column matrix of b × l_in × c_out/g × k values,** which can dwarf the input, kernel and output (Codex's example: input [1, 1, 64], kernel [1, 1024, 1024], a column of 67,108,864);
  - then the output.
  - **The direct path** allocates the output and contiguous copies.
  - **The bound:** the column matrix is checked whenever its path would be taken.
- **`conv_transpose2d`:** the direct path, with the output and contiguous copies of the input and kernel.
- **Pooling:** the output only.
- **Resampling:** the output.
  - **Bilinear** also builds per-axis index and weight tables of target_h and target_w entries, bounded by `AXIS` each, independently of the output's count. A zero-sized batch or channel axis can't hide them, because the targets are bounded whatever the output count.
- **The cumulative allowance:** the sum over a call's temporaries is within `4 × MAX_ELEMS`.

**So every size is computed by rnx with checked arithmetic before the call, and every allocation is bounded:**
- **dtypes:** f32 and f64. Kernel and input must match, with no promotion.
- **Ranks:** 3 for the 1-D forms (batch, channels, length); 4 for the 2-D forms, pooling and 2-D resampling.
- **Convolutions:**
  - `stride` and `dilation` at least 1; `groups` at least 1, dividing both the input and output channels;
  - the kernel's channel count equal to the input channels ÷ groups;
  - a non-empty kernel and input;
  - the dilated kernel extent within the padded input;
  - `output_padding` less than `stride` or `dilation` (PyTorch's rule) for the transposed forms;
  - **R2, upstream's own order of arithmetic,** each step checked:
    - **`conv_transpose1d`** computes `(l_in − 1)·s − 2p + d(k − 1) + op + 1`, subtracting `2p` **before** adding the dilated kernel. So `(l_in − 1)·s < 2p` underflows upstream even where the final size would be positive (input [1, 1, 1], kernel [1, 1, 3], padding 1). That **pinned restriction is refused by name;**
    - **`conv_transpose2d`** subtracts `2p` last, and is refused only when the whole sum is below `2p`;
    - **convolutions** compute `l + 2p − d(k − 1) − 1` in that order.
  - **Kernel layouts:**
    - **an ordinary kernel** is [c_out, c_in/g, …];
    - **a transposed kernel** is [c_in, c_out/g, …], so `conv_transpose1d`'s total output channels are `kernel[1] × groups`;
    - **`conv_transpose2d` has no groups parameter** (groups = 1).
  - **Empty inputs:** a zero batch or channel axis passes with a zero-value output only where Candle's path allocates nothing proportional to it; a zero spatial axis is refused for every convolution, pooling and resampling form. The zero-axis controls cover each.
  - **R3, an upstream defect:**
    - **The defect:** in CPU `conv1d`'s non-contiguous kernel branch, Candle copies the kernel into `kernel_c`, then passes the **original** kernel to `matmul` under an imposed contiguous layout (`cpu_backend/mod.rs` around line 2785). A strided or transposed kernel gives wrong results.
    - **The workaround:** every kernel is canonicalized to fresh, contiguous, offset-zero storage before any signal call (its copy counted in the census). The defect joins rnx's upstream drafts.
    - **The controls:** strided, transposed and nonzero-offset kernels and inputs for every signal operation, against a direct-Candle reference that makes its inputs contiguous first (the logical result), not against the same broken path.
  - **every output size computed checked,** and the output, the im2col buffer (for conv1d and conv2d), the kernel copy and the result copy each within `MAX_ELEMS`, and together within `4 × MAX_ELEMS`.
- **Pooling:** the kernel at least 1 and within the input; the stride at least 1; the output within the cap.
- **Resampling:**
  - target sizes at least 1 and at most `AXIS`, the output within the cap;
  - **scales finite and positive,** with `floor(h · scale)` and `floor(w · scale)` computed in f64, checked to be at least 1, within `AXIS`, and within the cap before the call.
- **Every argument is borrowed;** the receiver and kernel stay the script's.
- **Every call runs on the joined worker,** as the existing tensor operations do.

## 3. E7, traffic spikes (frozen before any analysis)

**D3** (`probes/0145/data/pageviews.tsv`, fetched by `fetch_pageviews.py`, SHA-256 `6ebab618…` committed):
- daily English Wikipedia views (user agents, all access) for `Rust_(programming_language)`, `Python_(programming_language)`, `Go_(programming_language)` and `Zig_(programming_language)`;
- 2023-07-01 to 2026-09-30, 1,188 days, no missing values.
- **It was fetched before this plan, and nothing has been computed from it.**

**The analysis, in f64, one tensor of shape (1, 4, 1,188):**
1. **`x = log(views)`.** A non-positive count is refused by name.
2. **The trend:** a centred 29-day moving average.
   - **How:** a depthwise `conv1d` (groups = 4, a kernel of 29 ones ÷ 29, padding 14), divided by the same convolution of ones, so the edge days are means of the days that exist.
3. **The residual:** `r = x − trend`.
4. **Each article's robust scale:** `mad = 1.4826 × median(|r − median(r)|)`.
   - **The median** of 1,188 values is the mean of the 594th and 595th sorted values (`sort_last_dim`).
   - **`z = (r − median(r)) / mad`.** A zero MAD is refused.
5. **Spike days:** `z ≥ 4` (spikes only; dips aren't flagged).
   - **Events:** consecutive spike days form an event, whose peak is its highest-z day (ties to the earliest).
   - **Reported per article:** up to 5 events, largest peak z first, each with its peak date, views, trend views (`exp(trend)`), ratio, z, and length in days.
6. **The weekday pattern:**
   - **Weekly means:** `avg_pool2d_with_stride` with kernel and stride (1, 7) over the first 1,183 days (169 whole weeks from Saturday 2023-07-01; the last 5 days are dropped, as stated).
   - **Back to daily length:** `upsample_nearest1d` brings the weekly means back to 1,183 days.
   - **Each weekday's mean** of `x − weekly mean` is shown as a percentage above or below the week (`exp(·) − 1`).
7. **The busiest week, by raw views** (R4: a pooled mean of logs would be the highest geometric-mean week, a different thing):
   - `avg_pool2d_with_stride` (1, 7) over the raw views gives each week's mean daily views;
   - `max_pool2d_with_stride` (1, 169) finds the highest;
   - its week is the first one equal to it (ties to the earliest).

**The tables** (Polars, displayed): the events, the weekday pattern (an article by Monday to Sunday), and the busiest weeks.

**Ties, frozen:**
- **Event peaks:** the highest z in an event, ties to the earliest day.
- **Events per article:** ordered by peak z, descending, ties to the earliest peak date.
- **The busiest week:** ties to the earliest.

**Controls before the real run** (R4): synthetic series fixed here, with outcomes computed beforehand by the independent NumPy implementation (`probes/0145/numpy_e7.py synthetic`), run in **both** the Rune example and the twin.
- **The baseline:** `base(n)[t] = 1000·(1 + 0.05·sin(2πt/7) + 0.03·sin(2πt/29.5) + 0.02·sin(2πt/3.3))`. It has nonzero variation, so the MAD is nonzero.

| case | series | expected |
|---|---|---|
| one-day | `base(400)`, day 200 × 5 | one event: peak day 200, length 1 (z ≈ 30.9) |
| two-day | `base(400)`, days 250 and 251 × 4 | one event: peak day 251, length 2 (z ≈ 25.2) |
| weekly | `1000 · w[t mod 7]` for 196 days, `w = [1.0, 1.2, 1.3, 1.25, 1.1, 0.8, 0.7]` | no events; profile `w / geomean(w) − 1`; busiest week 0 (all equal, ties to the earliest) |
| constant | 400 days of 1,000 | refused: zero MAD |
| flat-spike | 400 days of 1,000, day 200 = 5,000 | **refused: zero MAD.** More than half the residuals are zero, so a spike on a flat series can't be scored by this method; stated honestly, not tuned |

**Validity before parity:** each trace must hold only finite values, have the expected shape, and carry the expected dates, articles and identities before any comparison.

## 4. The gates

- **Exact:**
  - **against an independent direct-Candle Rust twin** (`twin0133 e7`, its own implementation): every trend, residual, z, event, weekday and busiest-week value, compared as f64 bits; each trace validated against its expected shape first (0143's comparer lesson), with corruption controls;
  - the synthetic cases, move for move.
- **NumPy, against declared tolerances** (independent):
  - the trend through `np.convolve`, within 1e-12 absolute + 1e-12 relative;
  - z within 1e-9 absolute;
  - the events identical except days whose |z − 4| < 1e-9, each listed;
  - the weekday profile within 1e-12 absolute (values near zero are compared absolutely);
  - the busiest weeks identical.
- **Timings:** the session's load, signal operations and tables; NumPy's beside them.
- **What the result says:** the events are dates and ratios. Any link to news is left to the reader; the tables don't claim causes.

## 5. Controls (`adapters/candle/tests/signal.rs`, `signal_alloc.rs`)

**Parity with Candle:** every operation against Candle's own on small inputs, the same f32/f64 bits:
- conv1d and conv2d with padding, stride, dilation and groups;
- the transposed forms with `output_padding`;
- each pooling form;
- nearest and bilinear resampling, with `align_corners` both ways;
- scales below and above 1;
- the `_with_algo` forms equal to the plain forms.

**Refusals, each before the call:**
- stride 0 and dilation 0;
- groups not dividing the channels;
- a channel mismatch;
- a kernel extent past the padded input (the underflow case);
- an empty input;
- an `output_padding` too large;
- a pooling kernel of 0 or past the input;
- a target of 0;
- NaN, infinite, zero and negative scales;
- a scale whose output passes the cap;
- the wrong rank and integer dtypes.

**Allocation** (one test, allocator peak): an im2col buffer over the cap with a small input and output; a huge `output_padding`/stride product; an infinite scale. Each is refused under 256 KiB.

**Reuse:** the receiver and kernel are the script's after success and after refusal.

## Gates

- **The manifest count:** 166 of 220, with bound + excluded = 220.
- **E7:** as a session paste, BIT-EQUAL to the twin; NumPy within tolerance; the tables in the evidence.
- **Replays:** E4, E5, E6 and U4 unchanged.
- **Suites:** Candle (with and without `test-support`, release and debug for the new tests), core, Polars; clippy and fmt.
- **Launch:** within noise.
- **`:dep candle` and `:dep polars candle`** after push. E7 needs Polars for its tables, so under `:dep candle` the synthetic cases run through a Candle-only variant, as in 0143.

## Out of scope

- CUDA, cuDNN, and the algorithm choice having any effect.
- Gradients.
- 3-D convolution, and pooling beyond what Candle's `Tensor` has.
- Seasonal decomposition beyond the weekday pattern.
- Claims about why spikes happened.
