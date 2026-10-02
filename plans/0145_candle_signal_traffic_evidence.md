# rnx 0145 evidence: Candle signal operations, with E7 traffic spikes in four language articles

**The result:**
- **16 more Candle `Tensor` identities:** convolutions, pooling and resampling, bringing coverage to **166 of 220 (75.5%)**, with bound + excluded exactly 220.
- **Every size and buffer is checked before Candle runs,** in upstream's own order of arithmetic. That includes the im2col and col2im columns and conv2d's tiles. An upstream conv1d defect on non-contiguous kernels is found, worked around, demonstrated and drafted for upstream.
- **E7 analyses 3¼ years of daily Wikipedia traffic** for the Rust, Python, Go and Zig articles. It's BIT-EQUAL to an independent Rust twin, and NumPy agrees within the declared tolerances.

## Review round 1 (Codex): three census gaps, fixed

All three were reproduced by Codex through a real Rune VM, each a successful call whose peak exceeded the census.
- **R1, bilinear's tables:**
  - **The gap:** each target entry is `(h0, h1, weight)`, a `(usize, usize, f64)` of 24 bytes (`cpu_backend/mod.rs` lines 532 and 547), not the 16 bytes counted. `upsample_bilinear2d(2000000, 1)` peaked at 56.0 MB against 40.0 MB.
  - **The fix:** each table is now counted as one buffer at 24 bytes an entry, **for bilinear only.** Nearest and `interpolate` build no tables, so they're no longer charged for them.
- **R2, the fast 1×1 `conv2d` path:**
  - **The gap:** it builds **one reshaped input per batch item, in parallel** (`conv2d.rs` line 92), and the census counted one. An input of [16, 128, 64, 64] peaked at about 67.5 MB against 36.2 MB.
  - **The fix:** they're now summed over the batch.
- **R3, the direct transposed paths:**
  - **The gap:** `ConvTranspose1D` and `ConvTranspose2D` allocate their own contiguous input copy (`inp_cont`, beside rnx's canonical copy), plus a kernel row of c_in values per output channel at each kernel position, in parallel over the channels.
  - **The fix:** both are now counted (per group for 1-D; the rows summed over channels).
- **Peak controls for each,** using Codex's cases (`signal_alloc.rs`), each a successful call within the corrected formula:
  - bilinear (2,000,000, 1), plus a refusal forced by the height table alone at 4,194,303 (25,165,818 values over the cap);
  - the 1×1 fast path at batch 16;
  - `conv_transpose1d` (padding 499) and `conv_transpose2d` (padding 32) on their direct paths.
- **Every convolution branch and bilinear now has a successful-call peak control:** conv1d; conv2d tiled, full-im2col and 1×1 fast; conv_transpose1d col2im and direct; conv_transpose2d direct; bilinear. Pooling and the nearest resampling forms allocate, upstream and after rnx's canonical input copy, only their output.

## 1. The manifest (`probes/0145/manifest.tsv`, `exclusions.tsv`, `out/count.txt`)

- **Bound:** 166, with family **L** (signal) contributing 16: `conv1d`, `conv1d_with_algo`, `conv2d`, `conv2d_with_algo`, `conv_transpose1d`, `conv_transpose2d`, `avg_pool2d`, `avg_pool2d_with_stride`, `max_pool2d`, `max_pool2d_with_stride`, `upsample_nearest1d`, `upsample_nearest2d`, `upsample_bilinear2d`, `upsample_bilinear2d_with_scale`, `interpolate1d` and `interpolate2d`.
- **Excluded:** 54.
- **Bound + excluded = 220,** with none in both and none in neither.

## 2. The operations (`adapters/candle/src/tensor_ops/signal.rs`)

**Sizes:** every output size is computed by rnx in candle's own order of arithmetic, with each step checked.
- **Convolutions:** `size + 2p − d(k − 1) − 1`, then `/ s + 1`.
- **`conv_transpose1d`:** `(l − 1)·s − 2p` first. A shape where that step underflows is refused by name as a **pinned restriction**, even when its true output is positive (Codex's `[1, 1, 1]` / `[1, 1, 3]` with padding 1).
- **`conv_transpose2d`:** subtracts `2p` last.
- **Resampling scales** are computed with the same floor as candle, but checked first.

**Kernels:** ordinary ones are [out, in/groups, …]. Transposed ones are [in, out per group, …]; `conv_transpose1d`'s output channels are `kernel[1] × groups`, and `conv_transpose2d` has no groups. A zero-sized axis is refused for every form: no path was proven to allocate nothing for one.

**The census:** each CPU path's temporaries are predicted by formula and bounded before the call. Each buffer is at most 2²⁴ values, and a call's buffers together at most 4 × 2²⁴.
- **`conv1d` (im2col):** the column, b × l_out × (c_in/g × k), and per group the result and its transposed copy. The grouped `chunk`/`cat` copies are counted too, with groups ≤ 4,096.
- **`conv2d`,** in upstream's three branches (`conv2d.rs` lines 35–49):
  - **the 1×1 fast path:** reshapes, the output, and the per-batch matmul results;
  - **any other 1×1:** full im2col, with the column, the result and its copy;
  - **larger kernels, tiled:** the NHWC input copy, the flat kernel and the output, plus per 512-pixel tile its column, its matmul result, and its coordinate pairs at their real byte size (two `usize` each). These are summed over every tile as if all were live, since nested Rayon tasks may keep them alive. One tile must also fit the cap.
- **`conv_transpose1d`:** its col2im column, b × l_in × (c_out/g) × k, whenever that path would be taken.
- **Resampling:** bilinear alone builds per-axis `(usize, usize, f64)` tables, 24 bytes an entry, each counted as one buffer whatever the output's count (review round 1).
- **The 1×1 fast path:** one reshaped input per batch item, summed (review round 1).
- **The direct transposed paths:** their own input copy, and per-channel kernel rows (review round 1).
- **The input and kernel copies** are always counted.

**Canonicalization, and the upstream defect:**
- **The defect:** candle-core 0.11.0's CPU `conv1d` copies a non-contiguous kernel into `kernel_c`, then multiplies by the original under an imposed contiguous layout (`cpu_backend/mod.rs`, around line 2785).
- **The workaround:** every input and kernel goes to `force_contiguous()` (fresh, contiguous, offset-zero storage) on the worker before every signal call.
- **The draft:** `plans/0145_upstream_defect_candle_conv1d_kernel_layout.md` (**not filed**), with a self-contained reproduction and the related hazards found by 0143 and 0145.

**Six wide signatures:** the convolutions take six or seven arguments, and Rune 0.14.2's typed functions stop at five. They're registered as **raw shims** following 0127's pattern:
- the argument count is checked first;
- each slot is taken and replaced with an empty value, receiver first;
- tensors and strings are borrowed, never taken;
- `ToReturn`, then `out.store`.

**`_with_algo`:** `algo` is `"none"` or a cuDNN algorithm name. An unknown name is refused, and on this CPU adapter the algorithm has no effect: the controls show the result equals the plain form's.

## 3. Controls (`adapters/candle/tests/signal.rs`, `signal_alloc.rs`)

**Parity with Candle on logical inputs** (made contiguous first), in f32 and f64 bits:
- `conv1d` with padding, stride, dilation and groups, and with `winograd` named;
- `conv2d` through all three branches;
- `conv_transpose1d` on its col2im and direct paths, with groups;
- `conv_transpose2d`.

**Views give the logical result:** transposed, strided and offset inputs and kernels, for every one of the 16 operations, including both pooling forms with stride, both bilinear `align_corners` settings, and scales below and above 1. **The defect is shown directly:** Candle's own `conv1d` on a transposed kernel differs from the logical result, and rnx's equals it.

**Refusals, each named and before the call:**
- a zero stride and a zero dilation;
- groups 0, groups not dividing the channels, and groups over 4,096;
- a channel mismatch;
- a kernel extent past the padded input (the underflow case, by kernel and by dilation);
- a negative padding; mixed dtypes;
- a seventh argument (the shim's count);
- an unknown algorithm;
- an empty axis; integer dtypes; the wrong rank;
- an `output_padding` too large;
- the pinned transposed-1-D restriction; a transposed-2-D shape with no output;
- a pooling kernel of 0 or past the input; a pooling stride of 0;
- a target of 0 or past `AXIS`;
- NaN, infinite and negative scales, scales giving under 1 or over `AXIS`, and an output past the cap.

**Reuse:** the receiver and kernel are the script's after success and after refusal.

**Allocation** (`signal_alloc.rs`, one test):
- **Forced refusals, one temporary at a time, with small inputs and outputs,** each allocating under 256 KiB:
  - conv1d's im2col column (16,781,312);
  - conv2d's full-im2col column (Codex's case, 16,842,816, with a 513 × 513 output inside the cap);
  - conv2d's tiled sum alone (every tile fits, the sum doesn't);
  - conv_transpose1d's col2im column (Codex's case, 67,108,864);
  - an infinite scale.
- **The census as an upper bound:** for a successful call on each path (conv1d, conv2d tiled, conv2d full im2col, conv_transpose1d col2im), the measured peak is within the census's own formula for those shapes.

**Found by the bound check: gemm's one-time slabs.** The first measurement of a tiled conv2d was 9.0 MB against 1.8 MB predicted.
- **The cause** (gemm-common 0.19, `gemm.rs` `L2_SLAB`): gemm keeps a thread-local slab the size of the L2 cache per thread, allocated on each pool thread's first multiply and kept for the life of the process. It isn't a per-call temporary, and is bounded by threads × L2 size.
- **Measured:** **29.6 MB persistent** after warming every pool thread through rnx's own call path (28 threads here), and **0 bytes** more after two further warm-ups.
- **After the warm-up,** every per-call census bound holds. Measured probes put a tile at about 49,000 values for a 36,864-value column with 4 output channels, matching the census's structure.
- **The census itself was also corrected:** it had held the sum over all tiles to the single-buffer cap, where the plan counts it against the cumulative allowance only (one tile must fit the cap).

## 4. E7, traffic spikes (`probes/0145/e7_traffic.rn`)

**D3:**
- daily English Wikipedia user views for the Rust, Python, Go and Zig articles, 2023-07-01 to 2026-09-30;
- 1,188 days, complete;
- fetched before the plan by `fetch_pageviews.py` (SHA-256 `6ebab618…`), with its User-Agent naming the repository.

**Before the real run, the synthetic cases** (`out/synthetic`). The Rune script, the twin (`twin0133 e7-synth`) and NumPy agree with the outcomes frozen in the plan:

| case | outcome |
|---|---|
| one-day | one event, peak day 200, length 1, z 30.9065 |
| two-day | one event, peak day 251, length 2, z 25.2135 |
| weekly | no events; busiest week 0 |
| constant | refused by both: zero MAD |
| flat-spike | refused by both: zero MAD (as the plan states honestly) |

The script and the twin are BIT-EQUAL on every value of every case.

**The run** (a session paste, `out/e7-session.txt`): load 3 ms; signal operations and statistics 12 ms; tables 1 ms. NumPy's method on the same data takes 8.7 ms.

**Spike events** (z ≥ 4, top 5 per article; `out/e7-events.tsv`, all 17, from the trace; the session preview shows 10):

| article | peak day | views | trend | × trend | z | days |
|---|---|---:|---:|---:|---:|---:|
| Rust | 2026-07-28 | 5,735 | 1,762 | 3.25 | 8.68 | 1 |
| Rust | 2026-05-14 | 5,499 | 1,876 | 2.93 | 7.89 | 1 |
| Rust | 2025-12-24 | 6,611 | 2,441 | 2.71 | 7.30 | 3 |
| Rust | 2024-02-28 | 7,279 | 3,321 | 2.19 | 5.71 | 2 |
| Rust | 2024-07-20 | 3,878 | 2,159 | 1.80 | 4.22 | 1 |
| Python | 2025-06-06 | 15,718 | 3,735 | 4.21 | 9.18 | 2 |
| Python | 2026-07-28 | 7,899 | 3,513 | 2.25 | 5.06 | 1 |
| Python | 2024-04-06 | 12,936 | 5,806 | 2.23 | 5.00 | 1 |
| Python | 2026-01-09 | 9,136 | 4,231 | 2.16 | 4.79 | 1 |
| Python | 2024-06-25 | 9,828 | 5,069 | 1.94 | 4.08 | 1 |
| Go | 2026-08-01 | 16,445 | 1,150 | 14.31 | 19.22 | 1 |
| Go | 2025-03-12 | 4,818 | 2,512 | 1.92 | 4.47 | 1 |
| Zig | 2025-12-03 | 2,274 | 668 | 3.40 | 6.94 | 1 |
| Zig | 2023-07-19 | 1,437 | 438 | 3.28 | 6.74 | 2 |
| Zig | 2023-09-11 | 2,007 | 619 | 3.24 | 6.67 | 3 |
| Zig | 2026-06-01 | 2,315 | 841 | 2.75 | 5.75 | 3 |
| Zig | 2026-07-08 | 1,773 | 677 | 2.62 | 5.48 | 1 |

**The weekday pattern** (views against each week's geometric mean):

| article | Mon | Tue | Wed | Thu | Fri | Sat | Sun |
|---|---:|---:|---:|---:|---:|---:|---:|
| Rust | +5.5% | +10.2% | +10.0% | +9.5% | +3.3% | −16.9% | −16.9% |
| Python | +8.9% | +14.4% | +12.2% | +10.4% | +2.9% | −20.2% | −21.1% |
| Go | +8.0% | +11.4% | +10.8% | +9.3% | +3.3% | −16.8% | −20.1% |
| Zig | +3.6% | +7.6% | +8.3% | +7.1% | +2.8% | −13.4% | −13.2% |

**The busiest week by raw views:**

| article | week from | mean daily views |
|---|---|---:|
| Rust | 2024-02-24 | 4,542 |
| Python | 2024-01-20 | 9,841 |
| Go | 2023-07-01 | 4,551 |
| Zig | 2026-05-30 | 1,628 |

**What the tables say, and don't:**
- They're dates, views and ratios. They make no claim about why traffic moved.
- **Go's 2026-08-01, 14.3 times its trend,** is by far the largest event.
- **Rust and Python both spiked on 2026-07-28,** and Zig peaks cluster in mid-2026.
- **Weekends sit 13–21% below the week** for all four articles.
- **Go's busiest week is the first in the data,** consistent with its trend having declined.

**The gates** (`out/e7-gates.txt`):
- **Exact, against the twin** (`compare_e7.py`, each trace validated first): **BIT-EQUAL** — 9,504 trend and z values, 17 events, 28 weekday values and 4 busiest weeks, all in f64 bits; and each synthetic case.
  - **Corruption controls** (`compare_controls.py`, `out/compare-controls.txt`): 11 corruptions of both traces and a one-sided bit flip are all refused, and the unmodified pair passes.
  - **A finding from the controls:** the first validator accepted an event deleted from both traces. It now recomputes the complete event list from each trace's own z.
- **NumPy, against the declared tolerances:**
  - the trend within 0.13% of its tolerance unit (about 1e-15 relative);
  - z within 1.9e-13;
  - events identical, with no near-threshold day;
  - the weekday profile within 6.3e-16;
  - the busiest weeks identical.

## 5. Findings

- **F1, the gemm slabs** (section 3): a one-time, process-lifetime 29.6 MB here. Accounted apart from the per-call census, with the reason stated.
- **F2, Rune limits met in E7:**
  - `f64` has no `exp` or `ln` (E7 uses Candle's);
  - passing a closure to `map` moves it, so it can't be reused (E7 uses a top-level function).
- **The upstream draft** (section 2).

## Gates

- **The manifest count:** 166 of 220.
- **Replays** (`out/replays.txt`, session pastes): E4 and E5 EQUAL; E6 BIT-EQUAL to its twin; U4 BIT-EQUAL.
- **Suites:**

| suite | passed |
|---|---:|
| Candle, default (release) | 79 |
| Candle, `test-support` (release) | 79 |
| Candle `signal` and `interchange`, debug | 16 |
| core, with `server-runtime` | 408 |
| Polars | 43 |

  `cargo fmt` and `git diff --check` are clean.
  - **Clippy:** nothing in `signal.rs`, after two `is_multiple_of` rewrites and an allowed `too_many_arguments` on `transposed2d_out`, whose eight parameters are upstream's own.
- **Launch** (`probes/0145/launch.py`, 0129's method; `rnx-candle` at 0144's `6d2ab47` against this tree; six rounds of 60 interleaved launches): deltas of +0.72, +0.48, +0.28, −0.27, +0.06 and +1.15 ms, a median of **+0.38 ms**, with round-to-round swings of about ±1 ms. That's noisy, and **at most a fraction of a millisecond** for 16 registrations, in line with 0134's roughly +0.3 ms for about 100 (`out/launch-results-1…6.json`).
- **`:dep candle` and `:dep polars candle`, after push** (a clean worktree binary at the pushed `911b44c`, a fresh private cache; `out/dep/`):
  - **`:dep polars candle`:** the full E7 on D3 is BIT-EQUAL to the twin (9,504 trend and z values, 17 events, 28 weekday values, 4 busiest weeks), with each trace validated first. The NumPy gate passes.
  - **`:dep candle`:** E7's tables need Polars, so `candle_only.sh` derives the Candle-only variant (every line before `run` byte for byte). The one-day, two-day and weekly synthetic cases are each BIT-EQUAL to the twin.
