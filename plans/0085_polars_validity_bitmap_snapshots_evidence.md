# 0085 validity bitmap snapshots: evidence

Plan `7d2ed51`. `ChunkedArray::rechunk_validity` and `ChunkedArray::iter_validities` return owned `Vec<bool>` snapshots of the validity bitmap. All 32 proven pairs emitted and all 32 new oracle cases match Rust. No Arrow bitmap or borrow crosses the binding boundary.

## Gate 1: freeze and audit

`probes/0085/pairs.json` holds the 32 rows (key, method, alias, substituted signature, disposition before and after, oracle outcome): `polars_core:3495` `rechunk_validity` and `polars_core:3676` `iter_validities`, each proven on the 16 aliases (the ten numeric families including `IdxCa`, `BooleanChunked`, `StringChunked`, `BinaryChunked`, `BinaryOffsetChunked`, `ListChunked` and `StructChunked`) and refused before this record at return mapping (`unreachable polars type`). All 32 were candidates: every substituted return is `Option<Bitmap>` or an `ExactSizeIterator` of `Option<&Bitmap>`, every alias has a default fixture, and neither method mutates its receiver. Source contracts, polars-core 0.55.2: `iter_validities` maps each chunk to `arr.validity()` (`chunked_array/mod.rs:411-418`); `rechunk_validity` clones the single chunk's validity, returns `None` when there are no nulls or the array is empty, and otherwise concatenates the chunks' masks, filling unmasked chunks with set bits (`chunked_array/ops/chunkops.rs:199-216`). `Column::rechunk_validity`, `SeriesTrait::rechunk_validity` and every bitmap input stay out of scope.

## Gate 2: generator and ownership

The release file lists the two canonical paths under `bitmap_returns` (placed above the first table header so TOML reads it at top level). The return mapping admits `polars_arrow::bitmap::immutable::Bitmap` only while the emitter handles a listed path, through a scoped flag on `World` set and cleared by both `emit_callable` and the method emitter, which instantiated pairs reach directly. The bitmap becomes `support::copy_bits(&__r, "<operation>")?` returning `Vec<bool>`; `Option` and the iterator arm compose as before, so `None` stays `None`. `copy_bits` iterates the logical bits in order after reserving them from the same cumulative bound as 0082's `copy_slice`; the reservation now lives in one `reserve` helper checked as `n > limit - used` before any allocation. An iterator whose items copy bits runs under one `SliceBudget` guard, so every chunk of one call counts against one bound, inside the engine-thread closure when routed; the guard's `Drop` restores it on success, error and unwind.

Synthetic controls (`bitmap self-test`): a listed direct `Bitmap`, `Option<Bitmap>` (`None => None`), and an iterator of `Option<&Bitmap>` (guard plus materialize plus copy); an unlisted `Option<Bitmap>` return, a bitmap input and a `PrimitiveArray` return stay refused; the flag is false after emission. Support unit tests: bits in order, the second bitmap over the bound refused with `3 validity bits with 3 already copied`, an empty bitmap is an empty vector, and the guard restored after a panic inside a guarded call. Compilation with all features and without generated bindings is the second applicability check; no other callable changed status.

Counts, 0084 to 0085: generated operations 2,090 to 2,092; unsupported 1,942 to 1,940; bindings +32; refused proven pairs 287 to 255; oracle cases 2,073 to 2,105; adapted, out of scope and wrappers unchanged.

## Gate 3: behaviour and oracle

`tests/validity_bitmaps.rs`: a single nullable chunk gives `Some` of `101`; a two-chunk array with a masked and an unmasked chunk gives per-chunk `[Some(101), None]` and a rechunked `101111`; an all-valid array gives `None`; the receiver keeps its length. Bounds: a limit of 3 copies a 3-bit mask and a limit of 2 refuses it with `rechunk_validity: 3 validity bits with 0 already copied, more than the bound of 2`; two masked chunks under a limit of 5 refuse the second with `3 already copied` and succeed at 6; the receiver answers normally after each refusal.

Oracle: 2,105 cases, all verified; the 32 new cases all match (default fixtures, so most report `None` masks; the behaviour suite covers masked chunks). Among old cases only `LazyFrame::unique_generic` changed, row_order_differs to match, the known permutation. Tally 1,989 match, 98 both_error, 17 both_panic, 1 row_order_differs. Scoreboard, 0084 to 0085: available 2,101 to 2,103, value-tested 1,328 to 1,330, unsupported 1,942 to 1,940.

## Gate 4: cost

Generator self-test, drift and accounting, both adapter suites with fail-fast disabled and the full oracle pass. The 0084 release binary was retained (SHA-256 `597d7e13fa0ea40aa7ecf045abdd0a1673d06ceb9073d05c6e5ba58d1e5489e9`); the 0085 binary is `a77ea4cf7bfb200468a8407c903b527b3014fc5b10b929e5a6eec259e476831c`. Cold launch, three interleaved 60-run sets, budget +5 ms:

| Set | 0084 median | 0085 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 18.44 ms | 17.63 ms | −0.81 ms | `probes/0085/launch-results-1.json` |
| 2 | 16.98 ms | 16.53 ms | −0.45 ms | `probes/0085/launch-results-2.json` |
| 3 | 18.94 ms | 17.95 ms | −0.99 ms | `probes/0085/launch-results-3.json` |

## Remaining

255 refused proven pairs remain in the Arrow pool. Bitmap inputs (`set_validity`, `with_validity`, `with_validities`, `from_vec_validity`, `from_bitmap`, `with_outer_validity`) need a length and mutation contract; `Column` and `SeriesTrait` `rechunk_validity` need their own audit. The 0083 table's decisions are unchanged.
