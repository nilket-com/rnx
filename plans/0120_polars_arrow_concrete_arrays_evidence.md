# rnx 0120 evidence: the concrete Arrow arrays scripts can reach

The five concrete immutable arrays that scripts provably reach, and that the facade census counts, are bound as their own `polars::arrow` types:
- `PrimitiveArray` at each pin's natives;
- the large-offset string, binary and list arrays;
- `StructArray` (v2).

A script obtains them from `polars::arrow::ArrayRef` by checked typed downcasts, reads them through guarded and bounded methods, and gives them back. Every other deferred array owner keeps its 0119 disposition. Every 0119 binding, including 0119's own new ones, is frozen.

The evidence is replayed by three scripts:
- `probes/0120/verify.sh` checks every digest. It rebuilds both frozen lists byte for byte from 0119's surfaces and catalogues, checks every frozen binding in the 0120 surfaces, and recomputes both audits from the release files.
- `probes/0120/reach.sh` replays the reachability probe at both pins against the committed route tables.
- `probes/0120/mutations.sh` runs the fail-closed controls against the real releases.

## Scoreboard

| | 0119 | 0120 |
|---|---:|---:|
| v2 available, of applicable | 4,068 of 6,421 (63.4%) | **4,115 of 6,421 (64.1%)** |
| v2 value-tested | 2,705 | **2,750 (42.8%)** |
| v2 to 90% | 1,711 | 1,664 |
| 0.55.2 available (0077 scoreboard) | 2,815 | **2,853** |
| 0.55.2 value-tested | 1,875 | **1,913** |
| 0.55.2 unsupported | 1,300 | 1,420 |

**Why 0.55.2's unsupported count rose.** The listed owners' callables were out of scope in 0119 (deferred owners). They are now decided per instantiation, so their refusals are counted with reasons: 122 refused rows on the four owners at this pin. No row generated in 0119 is lost, and the freeze proves it.

**Bindings.**
- On the listed owners: 166 at v2 (46 operations) and 129 at 0.55.2 (37 operations).
- On existing rows that gained receivers now that their concrete returns are wrapped: 198 at v2 across 13 rows, and 105 at 0.55.2 across 9. These are the `Array` trait methods on each concrete array, and 0102–0105's `downcast_*` snapshot families on each chunked type.

## Reachability, and the census boundary

`probes/0120/reach.rs` builds every dtype a script can build, through the generated bindings (the plan lists the routes). It takes each through every generated `ArrayRef` producer at both generated `CompatLevel`s.
- **Identity check:** the same calls in Rust name the concrete array by an exact `as_any().is::<T>()` for every candidate, every primitive native included. There is no dtype inference (review of the plan). An unclassified row fails the probe.
- **Agreement:** the script's Arrow dtype equals Rust's on all 144 rows at 0.55.2 and 160 at v2.

| identity | script route | census |
|---|---|---|
| `PrimitiveArray<i8…u64, f32, f64>` | numeric Series; Date (i32), Datetime, Duration and Time (i64) | yes |
| `PrimitiveArray<i128, u128, pf16>` | v2 casts; Decimal is i128 | yes (v2) |
| `Utf8Array<i64>`, `BinaryArray<i64>` | string and binary Series at `CompatLevel::oldest` | yes |
| `ListArray<i64>` | list Series, both levels | yes |
| `StructArray` | struct Series, both levels | yes at v2; **no inventory row at 0.55.2** |
| `Utf8ViewArray`, `BinaryViewArray`, `BooleanArray`, `NullArray` | the default chunks | no `polars::` path at either pin |

**StructArray at 0.55.2.** It is reachable at 0.55.2, but the 0.55.2 inventory has no row for it; the facade exposes it only at v2. By the plan's stop rule it is dropped from that pin's allowlist and named in the release file. Its tests assert that the downcast is absent at 0.55.2.

**Never produced by any script route:** the i32-offset arrays, fixed-size list, dictionary and map. They stay deferred.

**Available but not reachable.** `CategoricalMapping::to_arrow` and `CategoricalArrayToArrowConverter::build_values_array` remain generated from 0119, but no binding returns their receivers. They are named here and not counted as usable. Every generated binding on the listed owners is reachable, by downcast or statically (audit `not_reachable: 0` at both pins).

## The allowlist (`[[concrete_arrays]]`), fail closed

Each row names an owner, a native (absent for `StructArray`), a Rune name, its downcast, its probe route and a citation. `families/concrete_arrays.rs` validates the table in `World::new` before any emission, exiting 2 on a failure. It refuses:
- an uncited or unrouted row;
- a duplicate identity;
- a reused name or downcast;
- an identity outside the probe's proven set (`PROVEN`);
- an owner not under a deferred prefix;
- an owner missing from the inventory;
- a genericity mismatch.

Self-test `concrete_arrays` covers each refusal.

**Mutation controls at both pins (`mutations.sh`).** An unmutated copy generates. Each of these exits 2 with its named reason:
- a guard row removed;
- the strict range guard swapped for the zero-length exemption;
- an unproven `Utf8Array<i32>` row;
- an unrouted row;
- a duplicated identity.

**Deferral stays exact.** `deferred_type_prefixes` is unchanged. An allowlisted owner, or one of its listed identities, is not deferred. An unlisted instantiation (`Utf8Array<i32>`) stays deferred (self-test). The audit's `deferred` section compares every other deferred row's status with 0119's:
- none changed at either pin (0 of 83 at 0.55.2, 0 of 100 at v2);
- 28 and 39 refusals only reword their detail, now naming the new wrappers, and are listed as refined.

**The owner bound.** Rows listing `T: NativeType` and `O: Offset` are the only trait facts the record adds, and only for the listed natives. Each is asserted at compile time in the generated code (`const _: () = { fn holds<T: polars_arrow::types::NativeType>() {} let _ = holds::<i64>; };`).

## The way in, and ownership

**The downcasts.** `polars::arrow::ArrayRef::as_int64_array()` and its peers borrow the array, downcast through `as_any()` to exactly one type and clone it into the wrapper. A mismatch is a `ConversionError` naming the actual Arrow dtype (tested: "…the array is Utf8View, not Int64Array"). The way back is `boxed()`, then `Series::from_arrow` (tested at both pins).

**What a downcast's clone shares**, from the pinned sources:
- all five array structs `#[derive(Clone)]` over `Buffer`, `OffsetsBuffer`, `Option<Bitmap>` and `Box<dyn Array>` children;
- `Buffer` and `Bitmap` hold `SharedStorage`, whose `clone` increments a reference count with `Arc` ordering (polars-buffer `storage.rs:467-472` at 0.55.2);
- `Box<dyn Array>` clones through `dyn_clone` into the concrete `Clone` (`array/mod.rs:219`, `to_boxed` at `:630-632`).

So a downcast shares every buffer, and copies only the dtype and the child boxes. No deep copy is claimed.

## The read surface, audited per owner

**Refused by rule, not by row (`admit`):**
- `&mut self` methods, including `slice`, `set_validity`, `take_validity`, `get_mut_values` and `set_values`;
- unsafe or `_unchecked` methods;
- methods from `polars_arrow::legacy`, whatever receiver they reach. Two `from_values_iter` collectors had slipped in through their trait, and are deferred to 0121 with the legacy module.
- any `usize` argument of a receiver method that no required guard names.

Self-test `concrete_arrays` covers each rule.

**Guards: the closed `REQUIRED` set grows by eleven rows**, each cited to the pinned line:
- `value(i)`, `below_len`, on the primitive, string, binary and list arrays (`primitive/mod.rs:213`, `utf8/mod.rs:152`, `binary/mod.rs:160`, `list/mod.rs:164`);
- `get(i)`, `below_len`, on the string and binary arrays (`is_null(i)` panics);
- `sliced(offset, length)` on all five, with a new check, **`range_len_all`**.

The inherent `sliced` (`impl_sliced!`, `array/mod.rs:465-471` at v2, `:464-470` at 0.55.2) asserts `offset + length <= len` for every length, zero included. The trait `Array::sliced` that 0119's `range_len` guards is different: it exempts length 0. So `sliced(99, 0)` on a length-3 array panics here, and it is refused (tested: `sliced(4, 0)` refused, `sliced(3, 0)` an empty array).

The audit's `guards` section lists every `usize` argument on a listed owner's receiver method and its guard: 14 at 0.55.2, 16 at v2, none missing.

**Scalar read-back, per native.** The same rule applies to a single value and to each element of a copied vector:

| native | Rune value | rule |
|---|---|---|
| i8, i16, i32, u8, u16, u32 | int | `as i64` |
| i64 | int | unchanged |
| u64, i128, u128 | int | `support::widen`: a value outside i64 is a `ConversionError` |
| f32 | float | `as f64` |
| f64 | float | unchanged |
| pf16 (v2) | float | `f64::from(pf16)`, exact |

**The pf16 mapping is scoped to the listed identities.** Elsewhere, `polars::pf16` stays the opaque wrapper 0119 froze. A pf16 argument is refused by name ("a binary16 argument narrows a script float lossily": `fill_with` and `from_vec` on `Float16Array`).

Controls (`tests/concrete_arrays.rs`, at each pin where the native exists):
- `i64::MAX` reads back; `i64::MAX + 1`, `i64::MIN − 1` and `u128::MAX` are refused;
- a vector holding one such element fails whole, with no partial result;
- for pf16, f32 and f64: 0.0, −0.0 (sign preserved), ±∞, NaN, 65504 and 2⁻²⁴ each equal Rust's value, and a null reads as invalid;
- every native: value plus validity, with a null in the middle.

**Copies.**
- **Values and offsets buffers** (`Buffer<T>`, `OffsetsBuffer<i64>`) are copied like borrowed slices, through 0082's `copy_slice`. It is bounded before the copy, cumulative per binding, converts each element by its scalar rule, and the first failing element fails the call.
- **String and binary values** are the byte buffer, one element per byte, so the bound counts bytes. Test `copies_are_bounded_by_bytes_not_only_elements`: two strings with four payload bytes are refused at a limit of 3, which an element-count bound would admit.
- **Validity** joins 0085's `bitmap_returns`: a bounded `Vec<bool>`.
- **Children** are returned as owned `ArrayRef`s (`ListArray::value`, `StructArray::values`). They share their buffers and are not copied. Reading them goes through their own bounded readers (tested: a struct field's `values_str`).

**Trait methods served on the listed identities:** `StaticArray::full_null` and `ParameterFreeDtypeStaticArray::get_dtype`. The inventory records these traits under the private `static_array` module. A two-row, cited spelling table uses their public re-export (`array/mod.rs:734` at v2, `:732` at 0.55.2). The v2 build failed to compile until it did.

**Support reader.** `ArrayRef::values_binary` covers the binary view array under 0102's budget. It counts as support, not parity. So every script-produced leaf array has a checked reader, and `NullArray` is served by `len` and `null_count`.

## Freeze: extended to 0119's own bindings

The 0119 freeze was built from 0118's surface, so the bindings 0119 itself introduced were not frozen. Implementation found the consequence. When the concrete arrays joined `Array::dtype`, the plain binding id went to the first new receiver, and 0119's `polars::arrow::ArrayRef::dtype` was renamed to `…__on__arrayref`. The path was the same; the id moved.

Two fixes:
- **The lists.** 0120's frozen lists are 0119's full surfaces: 4,838 bindings at 0.55.2 and 6,405 at v2, strict supersets of 0119's lists. They are built with 0119's catalogues. The v2 catalogue was regenerated by the 0119 generator (worktree at 44a362b); its surface is identical to 0119's evidence, and it is bundled.
- **Pinning.** Generation now loads the freeze before emission, and `pin_frozen_ids` keeps each frozen binding's id. A new binding that would take a frozen id gets the receiver-qualified form. Self-test `freeze` covers this.

The 0119 generator run against the new lists refuses, naming exactly the moved `ArrayRef` ids. Both pins now print "none moved, no contract changed".

## Audits (both pins, `evidence/audit-*.json`)

| | 0.55.2 | v2 |
|---|---:|---:|
| listed identities | 13 | 17 |
| owner rows generated / refused | 37 / 122 | 46 / 137 |
| owner bindings | 129 | 166 |
| not reachable | 0 | 0 |
| guarded usize arguments (missing) | 14 (0) | 16 (0) |
| other deferred rows kept / changed | 83 / 0 | 100 / 0 |
| grown rows / added bindings | 9 / 105 | 13 / 198 |
| cross-crate moves / unreviewed | 1 / 0 | 1 / 0 |

**The cross-crate move**, reviewed: `ChunkedArray::get_row_encoded_array` now returns an owned `LargeBinaryArray`, a listed identity whose readers are guarded and bounded.

**The owners' refusals, by cause (v2):**
- 44: the concrete-array rules (mutable receivers, dtype arguments and static lengths, unguarded or pf16 arguments);
- 34: the 0113 protocol families on generic heads;
- 24: unreachable return or argument types, mostly `Bitmap` or `Buffer` arguments (the write side);
- 17: function-level generics;
- 9: foreign returns (`Arc<dyn Array>`, `Either`);
- 4: callbacks;
- the rest: `from_iter` heads and one protocol shape.

## Oracle

Each listed identity has a fixture: a Series chunk through the bridge, downcast exactly as the probe proved. Each is shown as the Series it converts back to. The buffer returns compare as the vectors the bindings copy. The concrete returns and pf16 needed oracle-side dispatch on the full identity: the bare path dropped the native and fell back to `Debug`.

| | 0.55.2 | v2 |
|---|---:|---:|
| new cases: match | 222 | 334 |
| new: both_panic | 0 | 0 |
| new: both_error | 0 | 0 |
| new: fixture_failed | 0 | 12 |
| removed | 0 | 0 |
| mismatches or broken | 0 | 0 |

**No new both_panic or both_error.** The round-1 rule refuses the static constructors and dtype-taking methods whose cases panicked alike on the default `ArrowDataType` fixture.

**fixture_failed (v2, 12)** are 0102–0105's snapshot families on `Int128Chunked`, `UInt128Chunked` and `Float16Chunked`, which gained bindings here. Their typed fixtures fail to build (i64 data unpacked as i128), exactly as they already do in **288** of 0119's v2 cases. The gap predates this record; fixing it would move old cases, so it is left as a follow-up.

**Old cases.** None moved except listed unordered operations flipping between `match` and `row_order_differs`, run to run:
- `LazyFrame::unique` at 0.55.2;
- `full_join`, `left_join` and `upsample` at v2.

A release-mode oracle run also flips four 0.55.2 cases guarded by `debug_assert!`. The committed results come from the debug suite, where they do not move.

## Review round 1 (Codex)

**1. Static constructors bypassed the read-side rules (blocking).** `admit` passed every static method. So `new_null` and `full_null` allocated by a script length with no bound (`Buffer::zeroed`), and `new_empty`, `new_null`, `to` and the child-type helpers panic on a dtype of another physical type. The oracle had recorded them as `both_panic`.

The fix, by rule in `admit`:
- any `ArrowDataType` parameter on a listed owner is refused, because construction and dtype validation are deferred to 0121;
- so is any `usize` parameter of a static method, because it allocates before any bound.

The dtype-free constructors stay (`from_vec`, bounded by the script's own vector, and `default_dtype`); so do the read side and the in-scope trait methods. This also drops two harmless dtype helpers, `ListArray::default_datatype` and `try_get_child`: a simple rule is worth the loss.

Controls:
- self-test `concrete_arrays`: a static length, a dtype argument and `to` refused, `from_vec` admitted;
- `constructors_and_dtype_arguments_are_not_bound` asserts that none of the seven methods is bound on any owner at either pin.

Effect: v2 4,132 → 4,115 and 0.55.2 2,867 → 2,853. The removed rows were untested (both_panic), so value-tested barely moves (2,754 → 2,750).

**2. A failing element's index.** The plan promises the index and value of a failing element. `support::copy_slice` now appends "(element i)" to a conversion error, and `widen` names the value. Test `a_failing_element_names_its_index_and_value`: "…9223372036854775808 does not fit a script integer (element 1)".

This applies to every copied slice, so 0093's `cont_slice` expectation in `tests/checked_readback.rs` now includes "(element 0)". Only the message changed; the kind is the same.

## Tests, suites and launch

- **Generator:** 49 of 49, with self-tests `concrete_arrays`, `freeze` (pinning) and `receiver_guards` (`range_len_all`).
- **`tests/concrete_arrays.rs`:** 11 of 11 at both pins.
- **Production suites:** release default (17), release test-support (220) and debug generated plus test-support (221) all pass.
- **v2 build:** every stage ok. `oracle: cases_failed` is the standing v2 state from the fixture gap above, as in 0119.
- **Launch against 0119 (44a362b),** three rounds of 60 interleaved launches: median deltas +1.6, +0.5 and +1.9 ms, within the +5 ms budget. The machine was noisy; 0119's medians ranged 20.8–22.2 ms.

## Follow-ups (not in this record)

- **Typed fixtures for 128-bit and f16 chunked arrays** (cast before unpacking). This would value-test about 300 existing v2 cases; it moves old cases, so it belongs in its own record.
- **The static constructors and dtype-taking methods** (`new_null`, `new_empty`, `full_null`, `to`, the child-type helpers), with exact dtype validation and a pre-allocation bound, belong with record 0121's write side.
- **Record 0121:** the mutable side (builders, `MutableArray`, `TryExtend`, the collect extensions, `Buffer` and `Bitmap` arguments) and `legacy::array`.
- **The census boundary.** View, Boolean and Null arrays are reachable but outside the facade census. Widening it moves the denominator, which is the user's call.
