# rnx 0143: Candle interchange and functional scatter, with E6 ticket topic discovery

Status: plan. Agreed with Codex after 0142: the narrower scope (bounded interchange plus functional scatter; the in-place setters deferred), then signal operations in 0144. The user (2026-10-02): Candle API widening, paired with useful analyst examples.

**Amended after Codex's review (R1 to R4):**
- **R1, I/O allocation and opening:** the ZIP preflight now runs before the archive parser builds anything, and every name list and header is bounded.
- **R2, shapes:** `meshgrid` and hole inference are corrected to Candle's real behaviour.
- **R3, `from_iter`:** deferred. Rune 0.14.2 has no public way for native code to advance a script iterator.
- **R4, E6's empty clusters:** an exact, executable rule.

**What an analyst gets:**
- **Interchange with Python:** read the arrays a NumPy or PyTorch notebook saved (`.npy`, `.npz`), and save results it can load bit for bit (`.npy`, `.npz`, `.safetensors`).
- **Index-driven construction** (`scatter`, `slice_scatter`), comparison by operator name, and coordinate grids.
- **E6, a real workflow** using them: discover the topics in 252 support tickets.

## 1. The manifest: exact identities and every exclusion

The denominator stays **220** public `Tensor` methods (0134's mechanical count of candle-core 0.11.0). 137 are bound by 0134 and 0137. This record adds **13** (to 150, 68.2%). The other **70** are excluded below, each with a reason, in `probes/0143/exclusions.tsv`, and `count.py` is extended to require that bound plus excluded equals 220 exactly, with no identity in both.

**J, interchange (8):**

| identity | script |
|---|---|
| `Tensor::read_npy` | `candle::Tensor::read_npy(path)` |
| `Tensor::read_npz` | `candle::Tensor::read_npz(path)`, a vector of `(name, tensor)` in archive order |
| `Tensor::read_npz_by_name` | `candle::Tensor::read_npz_by_name(path, names)`, tensors in the requested order |
| `Tensor::write_npy` | `t.write_npy(path)` |
| `Tensor::write_npz` | `candle::Tensor::write_npz(pairs, path)` |
| `Tensor::save_safetensors` | `t.save_safetensors(name, path)` |
| `Tensor::write_bytes` | `t.write_bytes()`, the contiguous little-endian data as Rune `Bytes` |
| `Tensor::from_slice` | `candle::Tensor::from_slice(values, shape, dtype)`, where `shape` may hold **one hole** (`-1`), inferred from the value count, as Candle's `ShapeWithOneHole` does. That's what `from_vec` lacks |

**K, functional scatter and grids (5):**

| identity | script |
|---|---|
| `Tensor::scatter` | `t.scatter(indexes, source, dim)`, a new tensor |
| `Tensor::slice_scatter` | `t.slice_scatter(src, dim, start)`, a new tensor |
| `Tensor::slice_scatter0` | `t.slice_scatter0(src, start)` |
| `Tensor::cmp` | `t.cmp(rhs, op)`, where `op` is `"eq"`, `"ne"`, `"lt"`, `"le"`, `"gt"` or `"ge"`, and `rhs` is a tensor or a scalar. Output is `u8` |
| `Tensor::meshgrid` | `candle::Tensor::meshgrid(vectors, xy)`, a vector of tensors |

**Excluded (70), each with its reason:**
- **Custom ops and modules (14)** take Rust trait objects, and there's no script contract for them: `apply`, `apply_t`, `apply_op1`/`2`/`3` with their `_arc` and `_no_bwd` forms, and `inplace_op1`/`2`/`3`.
- **Autograd (5):** the adapter evaluates eagerly without gradients. `backward`, `detach`, `track_op`, `sorted_nodes`, `is_variable`.
- **Storage internals (10):** `from_raw_buffer`, `from_storage`, `storage_and_layout`, `strided_blocks`, `strided_index`, `layout`, `id`, `is_fortran_contiguous`, `normalize_axis`, `new`.
- **Device (2):** the adapter is CPU-only. `device`, `to_device`.
- **In-place setters (6), deferred to their own record:** `scatter_set`, `scatter_add_set`, `slice_set`, `const_set`, `zero_set`, `one_set`.
  - **Why:** Candle's `clone` shares writable storage, unlike Polars' copy-on-write. Independence needs `copy()`, and a clone-first rule wouldn't protect aliases.
- **Randomness (4), deferred:** `rand`, `rand_like`, `randn`, `randn_like`.
  - **Why:** Candle 0.11 can't seed its CPU generator (`set_seed` bails; `rand_uniform` uses `rand::rng()`). A seeded constructor, if one is ever added, is counted apart as derived.
- **`from_iter` (1), deferred:** a script iterable can only be consumed lazily by calling its `next`.
  - **The fact:** in Rune 0.14.2, `Iterator::next` and `Value::protocol_next` are crate-private, and the evidence keeps a compile probe showing it.
  - **Why not work around it:** a workaround would need a new VM callback model (iteration runs script code on the VM thread under its budget and re-entry rules), so it's deferred rather than absorbed. `from_vec` and `from_slice` cover materialized values.
- **Signal operations (16): record 0144.** Convolutions, pooling, upsampling, interpolation.
- **`vs_*` and `vd_*` (12):** these **aren't `Tensor` methods**. They're MKL and Accelerate slice FFI wrappers, matched by 0134's `binary_op!` regex. They stay in the denominator to keep it unchanged, and are named as a counting artifact.

## 2. The interchange contracts

**The decoding model, an explicit scope decision:**
- **The problem:** Candle's readers aren't safe on untrusted files.
  - `read_header` allocates the declared header length (up to 4 GiB in format 2) before reading it.
  - `from_reader` allocates `elem_count` elements from the header's unchecked shape before reading any data.
  - `read_npz` bounds neither entries, nor duplicate names, nor decompressed bytes.
  - `from_reader` is crate-private, so no bounded path through Candle exists.
- **The decision: rnx decodes `.npy` and `.npz` itself,** from one bounded in-memory read (the `Mlp::load` precedent, which reads bounded bytes and then decodes a buffer). It builds the tensor with `Tensor::from_vec`. **Candle's own `read_npy`, `read_npz` and `read_npz_by_name` are the parity oracle** in the tests: on every valid file, the shapes, dtypes, names, order and bits must be identical.
- **Nothing is decoded twice,** so there's no window between validating a file and reading it.

**Opening and the one bounded buffer:**
- **The file is opened non-blocking and checked through the same handle** (the behaviour of `rnx::fs::regular`: `O_NONBLOCK` open, then `fstat`). A FIFO, a directory or a special file is refused at once, never blocked on.
- **Then the same handle** is capped by its metadata length (at most `MAX_FILE`, 64 MiB) and read with `take(MAX_FILE + 1)`.
- **The shared helper:** `read_limited`, which `Mlp::load` and `TextEncoder::load` also use, is changed to this, so they gain the FIFO refusal too.
- **The one intentional allocation:** this file buffer, bounded by `MAX_FILE`. The evidence reports it separately from every allocation after it, each of which is preceded by a check against the values below.

**`.npy`, everything checked before any further allocation:**
- **The magic, and versions 1.0 and 2.0:** Candle supports these. Version 3.0 (UTF-8 headers) and others are refused by name.
- **The header length** is at most 64 KiB and must fit inside the file.
- **The header must be a strict Python dict literal:**
  - exactly the keys `descr`, `fortran_order` and `shape`;
  - a duplicate or unknown key is refused;
  - ASCII only;
  - the trailing padding and newline that NumPy writes are accepted.
- **`descr`:** one of the adapter's dtypes: `<f4`, `<f8`, `<i8`, `<u4`, `|u1` (and `=`/`|` forms where the byte order is unambiguous on a little-endian host).
  - **Refused by name:** big-endian `>`; every other kind (`<f2`, bf16, `<i4`, object, structured, strings).
  - **Departure:** Candle accepts more dtypes (f16, bf16, i16, i32). The adapter's tensors don't have them, so they're refused rather than converted.
- **`fortran_order: True`** is refused, as Candle does.
- **`shape`:** at most `MAX_RANK` (6) dimensions, each a decimal integer.
  - **The product is checked** with overflow detection and must be at most `MAX_ELEMS` (2²⁴). Rank 0 (one value) and zero-length dimensions are allowed.
  - **The data length must equal the product times the item size exactly.**
  - **Departure:** Candle reads the count and ignores trailing bytes. rnx refuses trailing bytes as a malformed file.

**`.npz`:**
- **A preflight over the in-memory bytes, before `ZipArchive::new` builds any metadata:**
  - **Locating the end record:** the end-of-central-directory record is found within the last 22 + 65,535 bytes.
  - **Its entry counts** (this disk and the total) are equal and at most 256.
  - **The central directory** is at most 128 KiB, and its offset and size lie inside the buffer.
  - **ZIP64 end records are refused:** a `0xFFFF` count, a `0xFFFFFFFF` size or offset, or a ZIP64 end locator. They're never needed at these sizes.
  - **Walking the directory:** each record has the right signature and lengths inside the directory; at most 256 records exist, matching the count; each name is at most 255 bytes.
  - **Only then** is the archive parsed, from memory, with the `zip` crate at Candle's version (8.6, default features off).
- **NumPy's local headers:** they carry ZIP64 extra fields (`force_zip64`, with `0xFFFFFFFF` in the local size fields; verified on NumPy 2.3.5). These per-entry extras are accepted. **Sizes come from the central directory,** and a local ZIP64 size must agree with it where present.
- **Stored entries only:** `np.savez` writes them, and Candle's zip build can't inflate either. Deflated entries (`np.savez_compressed`) are refused by name, saying "compressed .npz is not supported; save with np.savez".
- **From the central directory, before any entry is read:**
  - at most 256 entries;
  - every name ends `.npy` and is unique after stripping it (a duplicate is refused; Candle would keep both, or the last);
  - names are valid UTF-8 of at most 255 bytes;
  - the declared uncompressed sizes sum (checked) to at most `MAX_FILE`.
- **Each entry** is read with `take(declared + 1)`: an entry longer than declared is refused. It's then decoded by the `.npy` rules above.
- **The cumulative elements** across entries are at most `4 × MAX_ELEMS`, checked before each entry's allocation.
- **`read_npz_by_name`:**
  - **checked before anything is read:** at most 256 requested names, each at most 255 bytes, all unique (a duplicate is refused);
  - a missing name is refused, naming it;
  - unrequested entries are never decoded.

**Writing (`write_npy`, `write_npz`, `save_safetensors`):**
- **Create-new, like `write_parquet_new`:** an existing file is refused, never overwritten.
- **Encoded in memory first:** the whole encoded size (data, every `.npy` header, ZIP local and central records, and the safetensors JSON header) is computed with checked arithmetic and bounded by `MAX_FILE` **before** any buffer is allocated or any data copied. The target is opened only after encoding succeeds, so a refusal or encoding error leaves no file.
- **Partial files:** a failure while writing (a full disk) may leave a partial file, as `write_parquet_new` states. It's removed best-effort and the error says so.
- **The encodings:**
  - `write_npy` and `write_npz` encode exactly as Candle does: version 1.0 headers, stored zip entries named `<name>.npy`;
  - `save_safetensors` uses the `safetensors` crate's in-memory serializer.
  - **Parity control:** for each, the bytes equal Candle's own writer's output on the same tensor.
- **`write_npz` pairs:** at most 256; names unique, non-empty, at most 255 bytes, without `/` or NUL, checked before encoding. **`save_safetensors`:** the same name rules.
- **`write_bytes`** returns at most `MAX_FILE` bytes.

## 3. The functional scatter and grid contracts

- **`scatter`:**
  - **The shapes** follow Candle's own check: same rank, all dimensions but `dim` equal, and `indexes` shaped like `source`.
  - **`indexes` dtype:** `u32`, `i64` or `u8`.
  - **Every index is checked before the call:** it must be in range. **An index equal to the dtype's maximum is refused.** Candle silently skips that sentinel, which a script wouldn't expect.
  - **Duplicate targets:** the later source element in row-major order wins. A control verifies this, and the contract states it.
  - **The result** is a new tensor; nothing is mutated.
- **`slice_scatter` and `slice_scatter0`:** `start + src.dim(dim)` is at most `t.dim(dim)`, checked (and overflow-checked) before the call. The other dimensions must be equal.
- **`cmp`:** the operator name is one of the six (an unknown one is refused by name). A scalar `rhs` is converted to the tensor's dtype under 0134's exact-conversion rules. Shapes must be equal: there's no broadcasting, as Candle's `cmp`.
- **Every shape check of 0134 and 0137 applies,** including the reverse stride products within `i64` and the `AXIS` bound per dimension, even for zero elements.
- **`meshgrid`:**
  - **Inputs:** two to `MAX_RANK`, as Candle requires at least two. Each is 1-D and **non-empty**: Candle builds `repeat` counts from the input lengths, and `repeat` treats 0 as 1 (0137's finding), so a zero-length input would give a wrong shape.
  - **Bounds:** each input length is at most `AXIS`. Every intermediate (the reshaped input and each `repeat`'s output) and every output is checked independently: overflow, at most `MAX_ELEMS` each, and at most `4 × MAX_ELEMS` in total. **All of it is checked before the call.** For example, `zeros([0])` with `zeros([AXIS + 1])` is refused before any `repeat`.
  - **The ordering:** `xy` is Candle's, which for more than two inputs **differs from NumPy's.** Candle reverses the whole input order and then the outputs; NumPy swaps only the first two axes. With two inputs they agree. A control shows both cases, against Candle's own output.
- **`from_slice`:** at most one `-1` hole. With a hole, a known-dimensions product of **zero is refused**, as Candle does. Otherwise the known product must divide the value count, or the call is refused. Element limits are as `from_vec`, and the inferred shape passes the shape checks above.

All of these run on the joined worker, as the existing tensor operations do.

## 4. E6, ticket topic discovery (frozen before running)

**The data:** 0133's D2, the project's 252 support tickets. Each whole ticket is embedded as in 0137's E4: chunked (0136), embedded (MiniLM, CPU), pooled with `segment_mean`, then L2-normalized.

**The algorithm, spherical k-means, frozen here:**
- **k = 6.**
- **Initialization, deterministic farthest-point (no randomness):**
  - the first centroid is the ticket most similar to the normalized mean of all tickets;
  - each next one is the ticket whose maximum similarity to the chosen centroids is lowest;
  - ties go to the lowest ticket index.
- **Assignment:** each ticket to the centroid with the highest dot product, via `argmax`. Ties go to the lowest centroid index. That's Candle's CPU `argmax`, which replaces only on a strictly greater value, verified by a control.
- **Update:** each centroid is the `segment_mean` of its tickets, then L2-normalized.
- **Empty clusters, an exact rule applied after each assignment and before the update:**
  1. For each empty cluster `c`, in increasing order, choose a **donor ticket**:
     - **eligible:** among tickets whose current cluster has **at least two members**, so a donor never empties another cluster;
     - **chosen:** the one with the lowest similarity to its own cluster's current centroid, ties to the lowest ticket index.
  2. The donor moves to `c`, the sizes update, and the next empty cluster is handled with the updated sizes.
  - **It always terminates:** n = 252 > k = 6, so whenever a cluster is empty, some cluster has two or more members.
  - **After the moves, every cluster is non-empty,** so `segment_mean` is always valid. Each move (iteration, cluster, ticket) is reported.
  - **Identical rows** (repeated tickets): ties at every step go to the lowest index. Identical initial centroids are possible and produce empty clusters, which this rule handles.
  - **Convergence** compares the assignments after the moves with the previous iteration's.
- **Normalization refusal:** a vector (embedding, mean or centroid) whose norm is zero or non-finite is refused by name, never divided.
- **Before the real run:** the frozen rule is tested in **both** implementations (the Rune example and the Rust twin) on synthetic inputs that force empty clusters:
  - all rows identical;
  - k equal to the number of distinct rows;
  - two clusters emptied in one iteration;
  - a would-be donor that is a singleton.
  The two must be identical, move for move.
- **Stopping:** when no assignment changes, with at most 50 iterations. The iteration count is reported. Hitting 50 is reported as non-convergence, never hidden.
- **Cluster sizes:** `zeros.index_add(ids, ones, 0)` (already bound).
- **Bounds:** the similarity matrix is 252 × 6. Every intermediate is at most 252 × 384 values, far under `MAX_ELEMS`, and is stated in the example.

**The output** (a Polars table, displayed):
- per cluster: its size;
- its **suggested** name: the nearest of 0139's frozen label descriptions by centroid similarity, with the similarity. It's presented as a suggestion, not a classification, and two clusters may suggest the same label;
- its 3 most central tickets (highest similarity to the centroid, ties to the lowest index), by number and title.

**The interchange:**
- **What's saved:** centroids (6 × 384, `f32`), assignments (252, `u32`) and the embeddings themselves, written with `write_npz` into one archive, plus the centroids alone with `write_npy`.
- **The round trip:** NumPy loads them, and **every array is bit-equal** (an exact gate). Candle's `read_npz` reads the archive back identically.

**The parity gates (three, kept separate):**
- **Bitwise,** against an independent direct-Candle Rust twin (no rnx code) running the same frozen algorithm: identical initial indices, assignments at every iteration, iteration count, reseeds, and final centroid bits.
- **NumPy, against declared tolerances,** run on the same embeddings (loaded from the exported archive):
  - **Assignments:** compared explicitly. They must match except where a ticket's top two similarities differ by less than 1e-6, and every such near-tie is listed.
  - **Centroids:** a maximum absolute difference of at most 1e-5.
  - **Iteration counts:** equal.
  - **There's no promise of whole-k-means bit equality across libraries.**
- **Timings:** the session's embedding, k-means and export times. NumPy's k-means time is reported beside them.

## 5. Controls

**Interchange:**
- **Parity with Candle's own readers:** every dtype, rank 0 to 6, and a zero-length dimension, written by NumPy (`np.save`, `np.savez`) and read by rnx and by Candle's readers, are identical. rnx's writers produce byte-identical files to Candle's writers.
- **Refusals, each before any proportional allocation** (an allocator-peak control as in 0136's `count_alloc`):
  - a header length of 4 GiB in a 100-byte file;
  - a shape whose product overflows;
  - a product over `MAX_ELEMS`;
  - rank 7;
  - big-endian and every unsupported `descr`;
  - `fortran_order: True`;
  - a duplicate header key;
  - trailing and missing data bytes;
  - version 3.0;
  - for `.npz`: a deflated entry, 257 entries, a duplicate name, declared sizes summing past `MAX_FILE`, an entry longer than declared, and a missing requested name.
- **Writers:** an existing target is refused, and the file is unchanged. An over-limit tensor is refused before the file is created. Invalid `.npz` names are refused.

**Scatter and grids:**
- `scatter` with duplicate targets, row-major last-wins shown;
- an out-of-range index and the sentinel, each refused before the call;
- `slice_scatter` bounds, including an overflow;
- `cmp` with each operator, against a scalar and a tensor, and an unknown operator;
- `meshgrid` with `xy` true and false, against Candle's own, and an over-limit output refused before the call.

**`from_slice`:** a hole inferred, two holes refused, a zero known product refused, a non-dividing count refused.

**`from_iter`:** a compile probe showing that the public API can't advance an iterator. It's kept in the evidence and not built into any crate.

**Opening and preflight:**
- a FIFO and a directory, refused at once (no blocking) by every reader, `Mlp::load` and `TextEncoder::load` included;
- a ZIP64 end record, inconsistent entry counts, a central directory past the buffer or over 128 KiB, a record with a bad signature or overrunning lengths;
- **an allocation control:** an archive packed with many tiny entries (more than 256 within 64 MiB) is refused by the preflight, with the allocator peak after the file buffer staying small;
- **NumPy's own `np.savez` files** (with local ZIP64 extras) read correctly.

**`argmax` ties:** the lowest index wins, which E6 relies on.

## Gates

- **The manifest count:** 150 of 220, with bound plus excluded equal to 220 and no overlap.
- **E6:**
  - as a session paste (`pub fn run(args)`) under the runner, BIT-EQUAL to the Rust twin;
  - the NumPy round trip exact;
  - the NumPy k-means within the declared tolerances;
  - the displayed table kept in the evidence.
- **0137's E4 and E5 and 0139's U4** replay unchanged.
- **Suites:** Candle (with and without `test-support`), core, Polars. Clippy and fmt.
- **Launch:** within noise against 0142.
- **`:dep candle` and `:dep polars candle`** after push, with E6 run through them.

## Out of scope

- The in-place setters and Candle's storage-aliasing contract (their own record).
- Randomness.
- Signal operations (0144).
- Compressed `.npz`, other dtypes, Fortran order, and memory-mapped loading.
- `from_iter`, until Rune exposes iteration to native code or a VM callback model is agreed.
- `safetensors` reading beyond what `Mlp::load` already does.
