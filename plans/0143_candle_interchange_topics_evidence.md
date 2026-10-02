# rnx 0143 evidence: Candle interchange and functional scatter, with E6 ticket topic discovery

**The result:**
- **13 more Candle `Tensor` identities:** 150 of 220 (68.2%). The other 70 are each excluded with a reason, and the count proves bound plus excluded equals 220, with no overlap.
- **rnx now reads and writes NumPy's files** (`.npy`, `.npz`) and safetensors, with its own bounded decoder. Its writers are byte-identical to Candle's.
- **E6 discovers topics in the project's 252 support tickets.** It's bit-equal to an independent Rust twin, round-trips exactly through NumPy, and NumPy's own k-means agrees within 6e-8.

## Review round 1 (Codex), four findings, fixed

- **R1, the scripts' names were taken:**
  - **The flaw:** `read_npz_by_name` and `write_npz` read names with `from_value`, which moves the script's strings and tuples. A later `n.len()` failed with "Cannot read".
  - **The fix:** names and pairs are now borrowed (`borrow_string_ref`, `borrow_tuple_ref`), their rules checked on the borrowed text before any copy.
  - **Bounded diagnostics:** an over-long name is reported by its length, never quoted.
  - **Controls:**
    - `names_and_pairs_stay_the_scripts_after_success_and_refusal`: the script reads every string, vector, tuple and tensor after a success and after a refusal on a later item;
    - the allocation test adds a 10 MiB name to both: refused from its length, allocating under 256 KiB, with a diagnostic under 200 bytes.
- **R2, the safetensors bound ignored JSON escaping:**
  - **The flaw:** 255 bytes of U+0001 escape to 1,530 in the header, so `zeros([16776983], "f32")` with that name produced 67,109,540 bytes, 676 past 64 MiB.
  - **The fix:** the bound now uses the name's exact escaped length (`serde_json`), plus the fixed keys, the shape's digits, the offsets, padding and the length prefix. A final check refuses an encoding over the limit before anything is written.
  - **Control:** `the_safetensors_bound_counts_json_escaping`: that case is refused and no file exists, while the same tensor with a plain 255-byte name is written within 64 MiB.
- **R3, `slice_scatter`'s transposed shapes were unchecked:**
  - **The flaw:** Candle moves axis `d` to the front of both sides, and those transposed shapes have their own stride products. `zeros([8589934592, 0, 8589934592])` was accepted in release and panicked inside Candle in debug.
  - **The fix:** both transposed shapes are checked before the call.
  - **Control:** `slice_scatter_checks_the_transposed_shapes`: refused by name, in release and debug, with the receiver unchanged afterwards.
- **R4, the exact E6 gate compared agreement only:**
  - **The flaw:** two empty traces passed, and so did deleting the same lines from both.
  - **The fix:** `compare_e6.py` now validates each trace on its own first:
    - the expected N, K and D;
    - the required sections in order, and exact counts and widths;
    - finite f32 values;
    - every move present in its iteration's assignments;
    - every iteration's clusters non-empty;
    - convergence meaning two equal last iterations (otherwise exactly 50);
    - the final assignments equal to the last iteration's.
  - **The plan's gate in full:** "assignments at every iteration" is now recorded by both producers (one `iter` line per iteration) and compared. **Before this fix, only the final assignments were compared,** so the earlier evidence's claim was narrower than the plan's.
  - **Controls** (`compare_controls.py`, `out/compare-controls.txt`): 12 corruptions applied to both real traces, and one bit flipped in one, are each refused. The unmodified pair passes.

## 1. The manifest (`probes/0143/manifest.tsv`, `exclusions.tsv`, `out/count.txt`)

`count.py` (extended) over 0134's, 0137's and this record's manifests, with the exclusions:
- **bound:** 150 (A 21, B 7, C 47, D 18, E 2, F 2, G 23, H 8, I 9, **J 8, K 5**);
- **excluded:** 70 — custom ops 14, autograd 5, internals 10, device 2, in-place setters 6, randomness 4, iterator 1, signal operations 16, and 12 artifacts;
- **bound + excluded = 220,** none in both and none in neither.

**Two exclusions resolved during the record:**
- **`from_iter` is deferred.** Its compile probe (`out/from-iter-probe.txt`) fails with `E0624: method next is private` and `method protocol_next is private` on Rune 0.14.2.
- **The 12 `vs_*`/`vd_*` entries aren't `Tensor` methods.** They're MKL and Accelerate slice FFI wrappers that 0134's `binary_op!` regex matched. They're kept, so the denominator stays 220.

## 2. The interchange (`adapters/candle/src/tensor_ops/interchange.rs`)

**Reading, the decoding decision:**
- **The reader:** one bounded, non-blocking read of a regular file (`read_limited`), then rnx's own strict decoder. Candle's readers allocate from unchecked header values, and their `from_reader` is crate-private.
- **Every header value is checked before any allocation beyond the file buffer:**
  - the version, and a header length of at most 64 KiB inside the file;
  - the dict's exact keys, each exactly once;
  - the dtype (`<f4`, `<f8`, `<i8`, `<u4`, `|u1`), and C order;
  - the rank, at most 6, with checked count and stride products;
  - the data length, exact.
- **The `.npz` preflight walks the layout over the bytes before the zip crate builds anything:**
  - the end record is exactly the last 22 bytes, so a comment or trailing data is refused;
  - a ZIP64 end record is refused;
  - at most 256 entries, the counts consistent, a central directory of at most 128 KiB ending at the end record;
  - every record and local header has its signature, consistent lengths and sizes, and lies in order;
  - stored entries only;
  - names unique `.npy`.
- **NumPy's local ZIP64 extras are accepted:** `np.savez` writes `0xFFFFFFFF` local sizes, verified on NumPy 2.3.5. Their sizes must agree with the central directory.
- **One directory:** the zip crate must then see the same entries at the same data offsets and sizes, and it checks each entry's CRC.

**Writing:** create-new; the whole encoding bounded before any buffer exists; byte-identical to Candle's `write_npy`, `write_npz`, `save_safetensors` and `write_bytes`.

**`read_limited`,** shared with `Mlp::load` and `TextEncoder::load`, now opens non-blocking and checks the same handle. A FIFO or directory is refused at once, never blocked on.

## 3. Controls (`adapters/candle/tests/interchange.rs`, `interchange_alloc.rs`)

**Parity with Candle's own readers:** NumPy-written fixtures (`probes/0143/fixtures.py`, `tests/data/0143`) are read by rnx and by Candle's readers identically. That's all 35 per-dtype files (five dtypes; rank 0 to 6, a zero-length axis), a transposed source, and `np.savez`'s archive (archive order, and by name).

**Writers:**
- byte-identical files for all five dtypes, a zero-element tensor and a non-contiguous view;
- an existing target refused and left unchanged;
- an over-limit tensor refused before the file exists;
- bad, duplicate, over-long and too many names, refused.

**Refusals by name:**
- **`.npy`:** magic; version 3.0; a 4 GiB header; a header past the file; an overflowing shape; one over the cap; rank 7; a duplicate, unknown or missing key; trailing and missing data; a negative dimension; f16, i32, big-endian and Fortran order.
- **`.npz`:** compressed (`np.savez_compressed`); a duplicate name; a comment; a forged end record inside a comment (refused, since no directory ends at it); a non-`.npy` entry; 257 entries; trailing data; disagreeing counts; a directory size past the end record; a bad record signature; a ZIP64 end locator; declared sizes past 64 MiB; disagreeing local sizes; a missing or duplicated requested name.

**Allocation** (one test, allocator peak): beyond the file buffer, each refusal allocated under 256 KiB:
- a 4 GiB declared header;
- an overflowing shape, and one of 1e9 elements;
- 20,000 tiny entries (refused by the preflight before the zip crate builds metadata);
- a valid archive whose entry declares 1e9 elements.

**Opening:** a FIFO given to `read_npy`, `read_npz`, `Mlp::load` and `TextEncoder::load` (its `config.json` a FIFO), and a directory, are each refused within the 10 s watchdog, never blocked on.

**Scatter, grids, comparison:**
- `scatter` is functional (the input unchanged), and duplicate targets show the later row-major source winning;
- **refused before the call:** an out-of-range index, the u32 sentinel `4294967295` (Candle skips it silently), a negative i64, f32 indexes and a shape mismatch;
- `slice_scatter`'s bounds, including a start of `i64::MAX`;
- `cmp`'s six operators against a scalar and a tensor, and an unknown operator refused;
- `meshgrid` with two and three inputs, `xy` both ways, equal to Candle's own. Candle's three-input `xy` gives shape `[4, 3, 2]`, not NumPy's;
- **`meshgrid` refused:** one input; a zero-length input; a 5,000 × 5,000 grid;
- **`from_slice`:** the hole inferred; two holes, a zero known product and a non-dividing count refused.

**`argmax` ties** go to the lowest index, which E6 relies on.

## 4. E6, ticket topic discovery (`probes/0143/e6_topics.rn`)

**Frozen in the plan:** k = 6, farthest-point initialization, the donor rule, at most 50 iterations.

**The forced-empty cases first** (`probes/0143/synthetic`, `out/synthetic`): the Rune script and the twin (`twin0133 e6-synth`, its own Rust implementation) are **identical, move for move and iteration for iteration** (the strict comparer, each trace validated against its case's N, K and D):

| case | moves | what it shows |
|---|---:|---|
| identical | 4 | five identical rows: two clusters emptied in one iteration, every iteration |
| distinct | 0 | k equal to the distinct rows |
| two-empty | 4 | two clusters refilled from one donor cluster in one iteration |
| singleton | 2 | the lowest-index candidate is a singleton and is skipped |
| zero-row | — | refused by both: "a row has norm 0.0; it cannot be normalized" |

**The run** (a session paste under runner0134, `out/e6-session.txt`):

```
DataFrame: 6 rows × 5 columns
"cluster": i64 | "size": i64 | "suggested": string | "similarity": f64 | "most central tickets": string
0 | 106 | "bug" | 0.6261411905288696 | "#560 [Regression] Installing a module with a crate name panics. | #810 Any::down"…[truncated]
1 | 1 | "bug" | 0.08915893733501434 | "#266 License question "
2 | 23 | "feature" | 0.7752770185470581 | "#486 Proper way to call Rust from Rune Scripts | #444 Project questions  | #692 "…[truncated]
3 | 82 | "feature" | 0.5138209462165833 | "#23 feature: runtime traits to represent protocols | #771 Before releasing 0.14 "…[truncated]
4 | 39 | "bug" | 0.4113244414329529 | "#435 Allow variable names starting with an underscore `_` | #325 Incorrect error"…[truncated]
5 | 1 | "feature" | 0.09818197786808014 | "#559 Is there a reason LitStr does not implement Peek?"

252 tickets, 458 passages; embed 5930 ms, k-means 43 ms (23 iterations, converged true, 0 moves), export 4 ms; read back exact: true
WORKFLOW OK e6
```

**The three gates, kept separate** (`out/e6-gates.txt`):
- **Exact, against the twin** (`twin0133 e6`): **BIT-EQUAL,** each trace validated first. The initial picks `204,57,87,163,141,136`; **every iteration's 252 assignments, for all 23 iterations** (converged); 0 moves; the 252 final assignments; and all 2,304 centroid values in f32 bits.
  - **Formatting:** 4 values differ only in notation (Rune writes small negative floats without an exponent, Rust's `{:?}` with one), so the comparer checks bits.
- **The NumPy round trip, exact:** `np.load` reads the archive (`<f4` 252 × 384, `<f4` 6 × 384, `<u4` 252). The centroids and assignments equal the twin's bits, and `centroids.npy` equals the archive's. Inside the session, Candle's own `read_npz` and `read_npy` read the files back identical (`read back exact: true`).
- **NumPy's own k-means, within the declared tolerances:**
  - the same initial picks, 23 iterations, 0 assignments differing, and no near-ties;
  - the largest centroid difference is 5.96e-8 (bound 1e-5).

**Timings:**
- **The session:** embedding 5.9 s, k-means 43 ms, export 4 ms; 8.2 s from the call.
- **NumPy's k-means:** 18 to 25 ms. The script's donor and size loops run in Rune.

**What the result says (presented, not tuned):**
- **Farthest-point initialization picked two outliers**, #266 ("License question") and #559, which stayed singleton clusters. So k = 6 found **four substantive topics**: 106, 82, 39 and 23 tickets.
- **The suggested names are coarse:** 0139's five label descriptions, nearest by centroid. They fit some clusters well (similarity 0.78 for the "Rust from Rune" cluster, named feature) and others weakly (0.41).
- **Better options for a later record:** an outlier-resistant initialization, or names from the central tickets themselves. They need a new frozen rule, not a change after seeing this result.

## 5. Findings

- **F1, the session submits a pasted function early when a macro call spans lines.**
  - **The minimal reproduction:** `pub fn run(args) {` / `println!("{} and {}",` / `1, 2);` … fails, with the continuation parsed as a new input.
  - **What's affected:** sessions only. Multi-line function calls paste correctly, and `run` and `eval` are unaffected.
  - **The likely cause:** `session::completeness` uses the first parse error's span, and an unterminated macro call's error isn't at the input's end.
  - **For a follow-up** (like 0134's trailing-comment fix). E6 keeps its macro calls on one line.
- **F2, Rune limits met while writing E6** (worked around, reported):
  - `s[x] += 1` is "Unsupported binary expression" (compound assignment to an index);
  - `Vec` has no `contains`. 0140's diagnostic named it clearly: "no method `contains` on `::std::vec::Vec`".

## Gates

- **The replays:** E4 and E5 EQUAL to their twins, and U4 BIT-EQUAL (`out/replays.txt`).
- **Suites:**

| suite | passed |
|---|---:|
| Candle, default | 74 |
| Candle, `test-support` | 74 |
| core, with `server-runtime` | 408 |
| Polars, default | 43 |

  `cargo fmt`, clippy on the touched files (two redundant closures and a late initialization, fixed), and `git diff --check`: all clean.

- **Launch** (`probes/0143/launch.py`, 0129's method; `rnx-candle` at 0142's `6c5f690` against this tree; three rounds of 60 interleaved launches): deltas of −0.09, +0.43 and +0.12 ms, a median of +0.12 ms. **Within noise** for 13 more registrations (`out/launch-results-1…3.json`).
- **`:dep candle` and `:dep polars candle`, after push** (a clean worktree binary at the pushed `e674a2c`, a fresh private `RNX_PROJECT_CACHE`; `out/dep/`):
  - **`:dep polars candle`, the full E6:** BIT-EQUAL to the twin, with each trace validated first: all 23 iterations of 252 assignments, the final assignments, and 2,304 centroid values. The NumPy round trip is exact, and NumPy's k-means is within tolerance (5.96e-8). The same table displays; embedding takes 5.9 s.
  - **`:dep candle`:**
    - **Why not E6 itself:** its `run` builds the table with Polars, so it can't compile without Polars (the first attempt's "Missing item polars::DataFrame::new", which also showed the cache must be a private directory).
    - **What ran instead:** `candle_only.sh` derives a Candle-only variant mechanically, with every line before `run` byte for byte (the k-means, the trace, the synthetic entry) and a one-line `run` that dispatches to it.
    - **The result:** all four synthetic cases are BIT-EQUAL to the twin under `:dep candle`.
