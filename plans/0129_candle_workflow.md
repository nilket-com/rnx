# rnx 0129: Candle, scoped to one cross-adapter workflow

Status: plan, drafted after 0128. The user chose Candle over Torch (`tch-rs` needs a separate C++ libtorch install, which is the second install 0067 rejected). The record is scoped to a single workflow.

**The workflow:** Polars data → a tensor → small-model inference → predictions back into a `DataFrame` → display.
- **Measure:** it computes, matches direct Rust, and displays.
- **Constraint:** CPU only for the first milestone, so GPU setup doesn't obscure the ownership and data-conversion questions.

## What exists (verified)

- **Composition.** A session with `:dep polars candle` runs one executable with one Rune `Context`. Each adapter's module is installed beside the others (`src/extensions.rs:104-150`; the wrapper's `main` is generated in `tools/project/src/generate.rs:49-83`). `:dep` rebuilds and restarts into the combined executable (`src/dep_transition.rs:465-490`).
- **Nothing crosses between adapters except plain Rune values.** Polars' `DataFrame` is a crate-private Rune `Any` newtype (`adapters/polars/src/lib.rs:16-27`) that another adapter can't name or construct. No crate is shared between adapters apart from `rnx`.
- **The adapter contract:** a `build(&mut Module) -> Result<Vec<(path, help)>, String>` function (plain hook), an optional presenter, and a path dependency on `rnx`. The catalogue is a hard-coded list (`tools/project/src/catalogue.rs:51-66`: `name`, `package`, `hook`, `presentation`, `shared_build`).
- **Candle:** `candle-core`/`candle-nn` 0.11.0 on crates.io. **Their default features are empty**, so the plain dependency is CPU only, with no CUDA, Metal or MKL. Neither declares a `rust-version`.

## 1. The hand-off: a neutral dense block in `rnx` (the central decision)

The two adapters must exchange numeric data without depending on each other. Three ways were weighed:

| way | ownership and cost | verdict |
|---|---|---|
| plain Rune values (`Vec<Vec<f64>>`) | Rune 0.14.2 stores a float inline (`Float(f64)`), so this isn't one heap box per number, but every element still passes through the VM's value and conversion path (review of the plan) | the measured **baseline**, not the contract |
| one adapter depends on the other | couples their builds and versions; the Candle adapter would pin Polars' | refused |
| **a neutral dense block owned by `rnx`** | a checked, immutable buffer, shared after it is built | **the proposal** |

**The type: `rnx::interchange::Dense`.** It is a Rune `Any` type that `rnx` itself registers, so both adapters can name it.

| part | definition |
|---|---|
| data | `enum Data { F32(Arc<Vec<f32>>), F64(Arc<Vec<f64>>) }`; the dtype is the variant, so the two can't disagree. It is `Arc<Vec<T>>`, not `Arc<[T]>`: `Arc::new(vec)` moves the `Vec` and allocates only the `Arc`'s header, whereas `Arc<[T]>::from(Vec<T>)` allocates a new slice and copies every element (review of the plan) |
| shape | `rows` × `columns`, both ≥ 1. A zero-length axis is refused at construction |
| names | exactly `columns` names, each 1 to 256 bytes, all distinct. A duplicate name is refused |
| fields | private and immutable; `Dense::new(data, rows, columns, names) -> Result` is the only constructor, and it enforces every invariant, **the finite-value rule included**: a NaN or ±∞ anywhere is refused there, so every `Dense`, built by rnx or by Rust code, is finite (review of the plan) |

**Limits** (constants in `rnx::interchange`, each checked before any proportional allocation):
- at most **16,777,216 values** (2²⁴), which is 64 MiB of `f32` or 128 MiB of `f64`;
- at most **4,096 columns**;
- names at most 256 bytes each.

**The invariant `Dense` proves itself.** `rows.checked_mul(columns)` must equal the buffer's length, and stay within the limit. Candle 0.11.0's `Tensor::from_vec` takes a concrete shape through `ShapeWithOneHole`, whose concrete-shape case ignores the element count (review of the plan). So Candle never sees a buffer whose length the block hasn't checked. The tensor's element count is asserted again after construction.

**Conversion policy at the Polars crossing.** Numeric columns only: integers of every width, `f32` and `f64`. Booleans, strings, dates and nested dtypes are refused by name. A null is refused, with the column and row.

These are **conservative policies, stated as such**, not claims about what a float can represent (review of the plan):
- **Integers: a contiguous exact range.** An integer is admitted when \|v\| ≤ 2⁵³ for `f64`, or \|v\| ≤ 2²⁴ for `f32`, the ranges in which *every* integer is exact. Anything beyond is refused, even a value that happens to be exactly representable (2²⁴ + 2 is exact in `f32` and is still refused). The integer values are inspected **before any float cast**, as integers, so 2⁵³ + 1 cannot round into an admitted value.
- **`f64` to `f32`: a finite-range policy.** A value with \|v\| ≤ `f32::MAX` is admitted and rounded to nearest. Anything above is refused, even though the next `f64` above `f32::MAX` would itself round to `f32::MAX`.
- **`f32` to `f64`:** exact.
- **NaN or ±∞:** refused, with the column and row.

The same policy applies, in reverse, when a tensor is exported to a block: a non-finite prediction is refused, not silently passed on.

**Each crossing, its order of checks, and its copies, counted honestly:**
1. **Polars → block** (`polars::DataFrame::to_dense(columns, dtype)`, hand-written; the frame is borrowed):
   - **Checks, before any cast or allocation:** the column list is 1 to 4,096 distinct, existing numeric columns; `height × columns` is checked against the limit.
   - **Copies:** each column whose dtype differs is cast by Polars into a temporary `Series`, so up to one temporary per column. The values are then copied once into the row-major buffer. That is up to two copies of the data.
2. **Block → tensor** (`candle::Tensor::from_dense(block)`; the block is borrowed): one copy into a `Vec` that Candle's storage takes. The block keeps its buffer.
3. **Tensor → block** (`tensor.to_dense(names)`):
   - **Checks:** the tensor is 2-D, `f32` or `f64`, and within the value limit; the names are 1 to 4,096 distinct, valid names whose count equals the tensor's columns. All are checked before export allocates.
   - **Copies:** `flatten_all` plus `to_vec1` make one `Vec`, which `Arc::new` moves into the block without copying elements.
4. **Block → Polars:** one `Series` per column, copied out of the row-major data.
   - `polars::DataFrame::from_dense(block)` builds a new frame.
   - `df.with_dense(block)` appends the block's columns to an existing frame. The heights must match, and a name already in the frame is Polars' own error. The frame is borrowed, and a new frame is returned.

**Tests:**
- each limit at its boundary, and just past it;
- each policy boundary, tested as a policy: 2⁵³ admitted and 2⁵³ + 1 refused for `f64` (an `i64` source, never cast first); 2²⁴ admitted and 2²⁴ + 1 and 2²⁴ + 2 refused for `f32`; `f32::MAX` admitted and the next `f64` above it refused; NaN, ±∞ and a null refused; a NaN refused by `Dense::new` itself;
- a mismatched shape and length, a duplicate name, a zero axis, overflowing `rows × columns`;
- every argument binding reused after every call.

**Why `rnx` owns it.** It is the only crate both adapters already depend on. The type is deliberately small: a dense numeric matrix, not a general array library. Arrow interop stays the parked option it was (record 0119's note).

## 2. The Candle adapter (`adapters/candle`), hand-written and minimal

**Scope.** This is not a parity effort. The surface is what the workflow needs:
- `candle::Tensor`: `from_dense`, `to_dense`, `shape`, `dtype`, and display;
- `candle::Mlp`: `load(path)` and `forward(tensor)`.

**The model's fixed architecture (review of the plan).** `Linear`, ReLU, `Linear`, read from a local **safetensors** file with **exactly** these four tensors, all `F32`:

| key | shape |
|---|---|
| `fc1.weight` | `[hidden, in]` |
| `fc1.bias` | `[hidden]` |
| `fc2.weight` | `[out, hidden]` |
| `fc2.bias` | `[out]` |

- **Dimensions** are inferred from these shapes and must agree with each other: `in`, `hidden` and `out` are each 1 to 4,096, and the four tensors hold at most 16,777,216 values in all.
- **Refused by name:** any other key, a missing key, a wrong rank, a wrong dtype, or mismatched dimensions.

**Loading, bounded.**
- **The file:** at most **64 MiB**. Its size is checked with `metadata` first, then read with a bounded read of at most limit + 1 bytes, and refused if the read exceeds the limit, all before parsing.
- **Parsing:** `VarBuilder::from_buffered_safetensors` (not the memory-mapped `unsafe` loader) parses the bytes, and the keys and shapes are checked before the `Linear` layers are built.

**Forward, bounded.** The input must be a 2-D `F32` tensor, matching the fixed `F32` model, with columns equal to `in`. A wrong dtype ("Mlp::forward: input must be F32, found F64") and a wrong rank ("…must be 2-D, found rank N") are named errors raised before `forward` runs. Before any allocation, `batch × hidden` and `batch × out` are each checked against the 16,777,216-value limit, so a small input can't produce an oversized hidden activation. The device is `Device::Cpu`, explicitly. No GPU feature is enabled, and there is no device argument in this milestone.

**Execution (review of the plan).** There is no shared engine API in `rnx` today: `engine::run` is private to the Polars adapter and carries its own callback and reentry boundary. So the Candle adapter gets its own **joined worker**, `candle_worker::run(op, closure)`. It runs the closure on a scoped thread and joins it before returning. A panic in Candle is caught and becomes a script error naming the operation. Polars is unchanged.

**Display (review of the plan).** A tensor's display is bounded **before** any output is pushed:
- the dtype and shape, then at most 8 rows × 8 columns of values;
- each value printed whole, as `f32`/`f64` round-trip text;
- `…` markers for omitted rows and columns.

Only that slice is read from the tensor (`narrow`, then `to_vec2` of at most 64 values), so the whole tensor is never formatted and then cut. The text has no user-supplied strings, so nothing needs escaping, and it is at most **2,048 bytes**: a header of at most 128 bytes, plus 64 values of at most 24 bytes each, plus separators and markers. Controls cover the prompt presenter and `println!`, with a large tensor showing the same bounded text.

**Errors:** Candle's own error becomes a script `Err`; a refusal by a check above names the check.

**Catalogue:** `candle` is added as a plain-hook native, with `presentation = true` (the bounded tensor display) and `shared_build = true`, like Polars. That is confirmed with 0069's gate: the executable runs with the build output removed.

## 3. The workflow and its proof (`probes/0129`)

**The data and the model are deterministic and local,** with no download:
- the probe writes a small CSV of numeric features;
- it writes a safetensors file of fixed MLP weights (created by a Rust helper with known values).

**The script:**

```rune
let df = polars::read_csv("features.csv")?;
let x = df.to_dense(["a", "b", "c"], "f32")?;
let model = candle::Mlp::load("mlp.safetensors")?;
let y = model.forward(candle::Tensor::from_dense(x)?)?;
let out = df.with_dense(y.to_dense(["score"])?)?;   // predictions joined as new columns
out
```

**The Rust twin:** the same steps with Polars and Candle directly (cast, row-major copy, `Tensor::from_vec`, the same MLP and weights, then back to a `DataFrame`).

**Acceptance:**
- **It computes:** the script's result equals the twin's. Predictions are compared bit for bit when the same CPU kernels run, otherwise within a stated `f32` tolerance, with the reason recorded.
- **It displays:** the result frame previews at the session prompt and in `println!`, through 0124's preview.
- **Ownership:** the script's `df`, `x` and `y` stay usable after each call. The block is shared, not taken (0125's rule).
- **The refusals:** every case in sections 1 and 2's tests, each an error, as in the twin, and none a panic.
- **The interchange's cost:** the time per crossing is measured against the plain-Rune-values baseline (VM element and conversion overhead) at a few sizes, so the neutral block's value is a number, not a claim. The copies counted above are reported with it.

**Composition proof:** in a project (`rnx project add polars candle`), the combined executable runs the script. It does so with both adapters' modules installed in one `Context`, and with `:dep polars candle` in a session.

## Gates

- **Polars freeze unchanged:** `to_dense`, `from_dense` and `with_dense` are hand-written Polars functions, and no generated binding moves.
- **Suites:** Polars' usual suites, Candle's own tests, the `rnx` core tests for `Dense`, and the probe.
- **Launch:** Polars' launch is measured against 0128, since `rnx` gains the `Dense` type, and Candle's launch is measured on its own.
- **Platforms:** Linux only, as before; Windows stays an open item.

## Stop rules

- If the hand-off can't be expressed without one adapter depending on the other, stop and report.
- If Candle's CPU path pulls in a non-Rust system dependency, stop: that would be the second-install problem.
- If an `unsafe` load path were needed (memory-mapped weights), use the buffered loader instead, or stop.

## Out of scope

- GPU (CUDA, Metal) and BLAS backends.
- Training.
- Model zoos and downloads.
- Torch.
- A general tensor API.
- Arrow interop.
