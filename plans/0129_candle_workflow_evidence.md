# rnx 0129 evidence: Candle, scoped to one cross-adapter workflow

**The measure is met.** The workflow (Polars CSV → a dense block → a Candle tensor → MLP inference on the CPU → predictions joined back into the `DataFrame` → display) computes, matches direct Rust bit for bit, and displays, in a project that composes both adapters into one executable.
- **No adapter depends on another.** Numbers cross through `rnx::interchange::Dense`, a type in the one crate both already depend on.
- **CPU only.** Candle 0.11.0 with its (empty) default features: no CUDA, Metal, MKL or Accelerate.

## 1. The neutral block (`src/interchange.rs`)

**As planned:**
- `Data::{F32, F64}(Arc<Vec<T>>)`;
- private fields, with `Dense::new` as the only constructor, checking shape, names, length and finiteness;
- the limits: 2²⁴ values, 4,096 columns, names of 1 to 256 bytes, distinct, both axes at least 1;
- the scripts' read-only surface: `shape() -> (rows, columns)`, `names()`, `dtype()`, and a bounded `DISPLAY_FMT`.

**`Arc<Vec<T>>`, against the other owned forms:**
- **`Box<[T]>`** alone can't be shared. A clone copies every element, and the block is borrowed by both sides and kept by the script.
- **`Arc<[T]>::from(vec)`** allocates a new slice and copies every element.
- **`Arc<Box<[T]>>`** works, but `into_boxed_slice` reallocates whenever capacity exceeds length.
- **`Arc<Vec<T>>`** moves the `Vec` without touching elements. `Arc::try_unwrap` hands the `Vec` back when the block is the last owner. The cost is one extra pointer hop per access, which is negligible next to the copies below.

**Added in implementation: a prompt presenter for `Dense`.** The session showed a bare block as `<::interchange::Dense>`, because prompt presentation runs registered presenters, not `DISPLAY_FMT` (0068's rule). `Extensions::install_with` now registers rnx's own presenter (`interchange::present`), with the same bounded summary text as `println!`. It applies wherever extensions are installed: the prompt, eval and the notebook worker.
- **Tests:** the unit test checks that the presented text equals the summary, with 12 columns cut to 8 names and escape characters escaped.
- **Count updates:** four presenter-count assertions in `src/present.rs` now count 2 (the extension's and rnx's).
- **Inventory updates:** the three inventory tests (`src/inspect.rs`, `src/complete.rs`, `src/lib.rs`) count the four new `interchange::Dense` entries.

## 2. Polars ⇄ block (`adapters/polars/src/dense.rs`, hand-written)

**The three functions:**
- `df.to_dense(columns, dtype)`: the frame is borrowed, and the column list is read by borrowing, never taken;
- `DataFrame::from_dense(block)`;
- `df.with_dense(block)`.

**Policy boundaries** (`adapters/polars/tests/dense.rs`, 5 tests), each tested as a policy:
- **f64:** an `i64` of 2⁵³ is admitted and 2⁵³ + 1 refused, at both signs, inspected as an integer before any cast.
- **f32:** 2²⁴ is admitted; 2²⁴ + 1 and 2²⁴ + 2 are refused (the latter is representable, and refused by policy).
- **f32 range:** `f32::MAX` is admitted, and the next `f64` above it is refused, though it would round to `f32::MAX`.
- **Refusals by column and row:** NaN, ±∞ and a null; a string or boolean column, by its dtype.
- **The list and dtype are checked first:** an empty list, a duplicate, an unknown column, `"f16"`, and a non-vector.
- **Reuse:** the frame, the block and the column list are reused after every call. `with_dense` refuses a name already in the frame (Polars' own error, prefixed) and a height mismatch.

**Preflight before any proportional allocation** (review round 1, R1):
- **The name list:** `rnx::interchange::names_from`, shared by both adapters, borrows the script's vector. It checks the length first (1 to 4,096), then each borrowed string's byte length (1 to 256), then distinctness. Only a list that passes is cloned, so a refused list costs at most 4,096 borrow guards.
- **The selected columns:** every selected column's dtype (an integer, `f32` or `f64`) and nulls are checked (`preflight`) before the row-major buffer is allocated.
- **The allocation control** (`adapters/polars/tests/dense_alloc.rs`, one test per binary so no parallel test moves the peak) reads the allocator's peak through a new rnx feature, `allocation-peak` (a dev-dependency only, and implied by `test-support`), over 2¹⁸ rows:
  - an accepted one-column export shows its 2 MiB buffer;
  - a string column, an 8 MiB name and a 100,000-entry list are each refused under 256 KiB;
  - the control catches the old order: with `preflight` disabled, the dtype refusal allocated 4,194,505 bytes and the test failed.
- **Boundaries in `tests/dense.rs`:**
  - 4,096 names pass the limit (the check then reaches the duplicates) and 4,097 are refused;
  - a 256-byte name passes the limit (then "not found") and 257 bytes are refused.

**Copies back into Polars** (R2): each column's strided copy is moved into its `Series` by `ChunkedArray::from_vec`, which wraps the `Vec` as an Arrow buffer. Before, `Series::new` on a `Vec` resolved to `from_slice` and copied it a second time.

**The freeze is unchanged.** `git diff 8cc79bf -- adapters/polars/src/generated tools/polars-gen` is empty: no generated binding moves.

## 3. The Candle adapter (`adapters/candle`)

**The `candle` module:**
- `Tensor::from_dense`, `tensor.to_dense(names)`, `shape()` and `dtype()`;
- `Mlp::load(path)`, `mlp.forward(x)` and `mlp.dims()`;
- a session presenter for `Tensor`.

**Loading:**
- **The file is read bounded:** its metadata is checked first (at most 64 MiB), then it is read with `take(limit + 1)`.
- **`candle_core::safetensors::load_buffer` parses the file.** That is the buffered path the plan's stop rule asked for; no memory map, and no `unsafe`.
- **The checks:** the keys must be exactly `fc1.weight`, `fc1.bias`, `fc2.weight` and `fc2.bias`, all F32. The ranks, the agreeing dimensions (≤ 4,096) and the parameter total are checked before the model is built.

**Inference:**
- **Named refusals before the worker:** "input must be F32, found …", "input must be 2-D, found rank N", "input has N columns, the model takes M", and the batch × hidden and batch × output limits.
- **The worker** (`worker.rs`) runs Candle on a scoped thread joined before return. A Candle panic becomes the script `Err` "{op}: Candle panicked".

**Display** (`display.rs`): at most 8 × 8 values, read with `narrow` and then `to_vec2`, so the whole tensor is never formatted. A header `Tensor[f32; RxC]`, `…` markers, at most 2,048 bytes.

**Unit tests:** 7 of 7.
- **The names list on `to_dense`:** 4,097 entries, a 257-byte name, an empty list and a non-vector are each refused; a 256-byte name is accepted; the script's list is reused after each call.
- **Malformed safetensors bytes** (review round 1), each refused as an `Mlp::load` error with no panic: an empty file, 3 bytes, 4 KiB of garbage, a header length past the end, a header that isn't JSON, data cut short of its offsets, and a zero header.

**Allocation control** (`adapters/candle/tests/alloc.rs`): an accepted 512 × 512 export shows its 1 MiB. An 8 MiB name, a 100,000-entry list and a duplicate are each refused under 256 KiB, before any name is copied or the tensor exported.

**Cargo.toml's wording** now distinguishes the build-time C compile from runtime libraries.

**Catalogue:** `candle` is the third entry (`rnx-candle`, plain hook, `presentation = true`, `shared_build = true`). The catalogue test asserts all three; the project tool's suite passes 81 of 81.

**Native code (the second-install stop rule): not triggered.**
- **The dependency chain:** `candle-core` 0.11.0 depends, unconditionally on non-wasm targets, on `tokenizers` (with `onig`), which depends on `onig_sys`.
- **How `onig_sys` builds:** by default its build script compiles the bundled Oniguruma C source statically with `cc`. pkg-config and a system library are consulted only when `RUSTONIG_SYSTEM_LIBONIG` or a dynamic link is requested, and bindgen is off.
- **Install cost:** this needs a C compiler at build time, which rnx already requires for `ring`. There is no system library and no second install.
- **The built executable** links only `libc`, `libm`, `libgcc_s` and the loader (`ldd`), and holds no `onig_` symbols.
- **Recorded as a dependency fact,** because it is a C build inside Candle's CPU path.

## 4. The workflow and its proof (`probes/0129`)

- **`twin0129 write DIR`** writes the inputs: `features.csv` (64 rows: `id`, `a` f64, `b` i64, `c` f64) and `mlp.safetensors` (3 → 8 → 1, fixed weights).
- **`workflow.rn`** is the script, entry of the project.
- **`twin0129 check DIR`** reads `out.parquet` and asserts the columns are `[id, a, b, c, score]` and that every `score` equals the twin's, bit for bit (`f32::to_bits`). The CPU kernels are the same `gemm` code in both, so no tolerance is needed.

**Composition proof (project):** the project directory sits outside the repository, with the cache private (mode 0700). The steps:
- `rnx project add polars candle`, then `lock` and `build`: a cold build of 6 min 4 s, producing one shared assembly;
- `rnx project run -- DIR`, then `twin0129 check DIR`: **"64 predictions equal bit for bit (first 0.765625, last 1.8203125)"**;
- **displays:** the joined frame (a 0124 preview, `score: f32`) and the tensor (`Tensor[f32; 64x1]`, 8 rows, then `…`);
- **reuse:** `df.height()`, `x.shape()` and `model.dims()` after every call.

**0069's gate (`shared_build = true` for Candle):**
- **Build output removed:** with the assembly's 1.8 GB `target/` directory moved away, the artifact still runs, and `twin0129 check` passes bit for bit.
- **Links:** `ldd` shows no non-system libraries.

**Session (`rnx project session`, piped input), final assembly:**
- `x` presents as `Dense[f32; 64 x 3](a, b, c)`;
- `y` presents as `Tensor[f32; 64x1]` with 8 rows, then `…`;
- the joined frame previews;
- **the large control:** a 300 × 12 tensor shows the same 8 × 8 corner with `…` markers, at the prompt and through `println!`;
- `model.forward` with 2 columns, and with the 12-column tensor, returns `Err("Mlp::forward: input has N columns, the model takes 3")`, not a panic;
- `(x.shape(), y.shape(), df.height()?)` evaluates to `((64, 3), [64, 1], 64)`: every binding is still usable.

**Observation (no change made):** a tensor's `shape()` is a vector (any rank), while a block's is a tuple (always 2-D). That is deliberate, but it is a visible difference.

**Timing (whole project run):** 134 ± 8 ms over 15 runs (`hyperfine`), covering verification, CSV read, inference, Parquet write and display.

## 5. The interchange's cost (`probes/0129/cost`, `probes/0129/cost-results.md`)

The probe measures median µs per crossing over 31 repetitions:
- **dense** is the adapters' own code through the VM, with VM call overhead counted against the block;
- **plain** is the same matrix as `Vec<Vec<f64>>` rows through `rune::to_value` and `rune::from_value`. That is the conversion an adapter returning plain values would pay, followed by the same tensor or column.
- **forward** is frame → tensor; **back** is a rows × 1 tensor → a column appended to the frame.

| rows x columns | values | VM call | dense forward | plain forward | dense back | plain back |
|---|---:|---:|---:|---:|---:|---:|
| 64 x 3 | 192 | 0.1 | 1.3 | 14.4 | 1.2 | 10.4 |
| 4096 x 3 | 12288 | 0.1 | 18.7 | 1071.1 | 5.5 | 742.1 |
| 65536 x 3 | 196608 | 0.1 | 408.0 | 16607.0 | 57.9 | 11250.5 |
| 262144 x 3 | 786432 | 0.4 | 2524.3 | 76664.7 | 249.8 | 52523.1 |
| 16384 x 64 | 1048576 | 0.1 | 6018.8 | 26393.7 | 17.6 | 2852.1 |

**The block is 4× to 150× cheaper per crossing.** The plain path pays a Rune vector allocation per row, plus per-element value conversion.

**The weakest case is wide frames** (64 columns, 4×): the column-at-a-time fill writes the row-major buffer at a stride of 64. A blocked transpose would help, but isn't needed for this record.

**The copies, as counted in the plan:**

| crossing | copies |
|---|---|
| Polars → block | at most one cast `Series` per column whose dtype differs, plus one copy into the row-major buffer |
| block → tensor | one copy into the `Vec` Candle's storage takes |
| tensor → block | `flatten_all` + `to_vec1` make one `Vec`, which is moved into the `Arc` |
| block → Polars | one `Series` per column, copied out of the row-major data |

**Not a copy:** `Dense::new`'s finiteness scan is a read.

## 6. Gates

**Suites:**
- **`rnx` core:** 391 of 391 (release), and 436 of 436 with `test-support`.
- **Polars:**
  - release default: 39, including the allocation control;
  - release `test-support`: 262;
  - debug `test-support`: passing apart from one standing flake (next item).
- **Candle:** 8 of 8: 7 unit tests and the allocation control.
- **Project tool:** 81 of 81.
- **A standing flake in debug:** `tests/in_memory_io.rs` `csv_round_trip_with_a_separator` (0118) fails intermittently with "Missing interface environment": 1 of 12 runs at the 0128 base (`65d32e6`) and 3 of 12 at head. It predates this record and touches none of its code; noted for its own follow-up.
- **Standing nondeterminism (not a regression):** each Polars suite run rewrote `oracle-results.json` with the known `LazyFrame::unique` row-order flip, an order-unspecified case, so the file was restored after each run. Separately, the project tool's `quiet::a_flooding_child…` test failed once under load and passed 5 of 5 alone.

**Launch** (`probes/0129/launch.py`, 0127's method):
- **Method:** three rounds of 60 interleaved launches of `pub fn main(_) { }`, default release builds; results in `launch-results-{1,2,3}.json`.
- **Polars against 0128 (`65d32e6`):** median deltas of +0.37, −0.25 and −0.51 ms, so no change.
- **Candle alone:** a median of 5.8 to 6.0 ms.

**Platforms:** Linux only. Windows stays open.

## 7. `:dep polars candle`: done after push

**Driven under a pty from the stock binary** at the pushed `f0d9518`:
- **First session:** `:dep polars candle` fetched the adapters from the remote at the binary's commit (0067's Git source), built, and returned to the prompt in 428 s, silently (0070).
- **A fresh session** attached to the cached assembly in 15.5 s (0069).
- **The workflow at the prompt:**
  - the joined frame previews `score: f32` with 0.765625, 0.640625, …, the twin's values;
  - `x` presents as `Dense[f32; 64 x 3](a, b, c)`;
  - `y` presents as `Tensor[f32; 64x1]`, with 8 rows, then `…`.
- **The earlier attempt** (before push) failed at resolve with "revision … not found", as 0067's design requires.
