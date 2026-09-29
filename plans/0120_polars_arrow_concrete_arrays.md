# rnx 0120: the concrete Arrow arrays scripts can reach

Status: plan. Record 0119 is closed on `origin/main` at `44a362b`; it deferred the concrete arrays here intact. Claude and Codex agreed this scope before the plan: to-codex-054/055 and Codex's two scope critiques.

The starting v2 scoreboard is 4,068 available and 2,705 value-tested of 6,421 applicable callables (63.4%).

## Which owners, and why: the reachability probe

The owners were chosen from exact script reachability, not from the deferred pool's size (Codex).

`probes/0120/reach.rs` builds each dtype a script can build through the generated bindings:
- every `from_iter_option_*` type, plus `new_null`;
- casts to Binary, BinaryOffset, Date, Datetime, Duration, Time, Int128, UInt128, Float16 and Decimal;
- `implode` for lists;
- `StructChunked::from_columns` for structs.

It takes each one through every generated `ArrayRef` producer (`Series::to_arrow`, `Column::rechunk_to_arrow`, `Series::into_chunks`, `ListChunked::get`), at both generated compat levels (`CompatLevel::newest()` and `oldest()`). The same calls in Rust name the concrete array by an exact `as_any().is::<T>()` for every candidate, primitives included. A row with no exact identity fails the probe. The script's Arrow dtype must equal Rust's on every row. `probes/0120/reach.sh` replays both pins against the committed `reach-{0.55.2,v2}.json`.

| concrete identity | script route | in the facade census |
|---|---|---|
| `PrimitiveArray<i8…u64, f32, f64>` | numeric Series, Date (i32), Datetime/Duration/Time (i64) | yes (`IdxArr`) |
| `PrimitiveArray<i128, u128, pf16>`, v2 only | casts; Decimal is i128 | yes |
| `Utf8Array<i64>` | string Series at `CompatLevel::oldest()` | yes (`LargeStringArray`) |
| `BinaryArray<i64>` | binary Series at `oldest()` | yes (`LargeBinaryArray`) |
| `ListArray<i64>` | list Series, both levels | yes (`LargeListArray`) |
| `StructArray` | struct Series, both levels | yes (`StructArray`) |
| `Utf8ViewArray`, `BinaryViewArray`, `BooleanArray`, `NullArray` | the default string, binary, boolean and null chunks | **no**: no `polars::` path at either pin, and no inventory row |

No script route produces `Utf8Array<i32>`, `BinaryArray<i32>`, `ListArray<i32>`, fixed-size list, dictionary or map arrays. `CategoricalMapping::to_arrow` and `CategoricalArrayToArrowConverter::build_values_array` are generated, but no binding returns their receivers. They are named as available but not reachable, and are not counted as usable.

**The census stays fixed.** The parity target is the facade's Rust API, and the view, Boolean and Null arrays are outside it. Widening the extractor past the facade would move the denominator; that would be a separate record, and the user's call.

## Scope

**1. A positive, exact allowlist of owners and instantiations.** A new closed release table (`[[concrete_arrays]]`, cited) names exactly these instantiations, and `deferred_type_prefixes` stays as it is for everything else:
- `PrimitiveArray<T>` for the probe's natives: i8, i16, i32, i64, u8, u16, u32, u64, f32 and f64 at both pins, plus i128, u128 and pf16 at v2;
- `Utf8Array<i64>`, `BinaryArray<i64>`, `ListArray<i64>`, `StructArray`.

Rune names follow the facade's own aliases, under `polars::arrow`: `LargeStringArray`, `LargeBinaryArray`, `LargeListArray`, `StructArray`, and `Int8Array` … `Float64Array` (plus `Int128Array`, `UInt128Array` and `Float16Array` at v2).

The table is validated before emission, fail-closed, in the same way as 0119's `REQUIRED` guard set. Validation refuses:
- an entry that is not in the inventory at that pin;
- a duplicate;
- an uncited row;
- a row outside a deferred prefix;
- an instantiation the probe did not prove.

**Every other array owner and instantiation keeps its 0119 disposition.** The audit proves this row by row: the i32-offset instantiations, `ArrayBuilder`, `MutableArray`, `TryExtend`, the collect extensions and `legacy::array`.

**2. The way in: checked typed downcasts from `ArrayRef`.** There is one per allowlisted instantiation, for example `as_large_string_array` and `as_int64_array`.
- Each borrows the `ArrayRef` and returns an owned concrete wrapper. On a mismatch it returns a `ConversionError` naming the actual Arrow dtype, and it never assumes a type.
- The owned wrapper is a clone of the concrete array. Whether that clone shares or copies buffers is verified per type from the pinned sources and recorded as found. Nothing is described as a deep copy, as in 0119.
- **The way back:** `to_boxed` or `boxed` returns an `ArrayRef`, so `Series::from_arrow` round-trips.

**3. The read surface, audited per owner.** These are the inherent and trait methods that take `&self` or consume an owned value, such as `len`, `value`, `values`, `offsets`, `validity`, `sliced`, `fields`, the `StaticArray` getters and `GenericBinaryArray` values and offsets. The generator admits them under these rules:
- **Index and range methods:** every one is added to the closed `REQUIRED` receiver-guard set, with its exact check and parameters. The audit lists every safe index or range method on the five owners and shows it guarded. A missing, extra or swapped row exits 2, as in 0119.
- **Copies:** every multi-value copy goes through the 0102 snapshot budget, cumulatively. That covers values, offsets, validity, UTF-8 and binary bytes, and child and field arrays. A nested array counts its children's cells and bytes against the same budget. A child array returned as an owned `ArrayRef` is not a copy.
- **Refused by rule, not by row:** `unsafe` methods, `_unchecked` methods, `&mut self` methods and raw-pointer or lifetime-bearing returns. Each is named with its rule.

**Scalar read-back, per native (review of the plan).** Every value that leaves a `PrimitiveArray<T>` becomes a Rune value by one rule per native. The rule is the same for a single value (`value(i)`, `get`) and for each element of a copied vector (`values`, iterator snapshots):

| native | Rune value | rule |
|---|---|---|
| i8, i16, i32, u8, u16, u32 | int | `as i64`, lossless |
| i64 | int | unchanged |
| u64, i128, u128 | int | 0093's checked `support::widen`: a value outside `i64` is a `ConversionError` naming the operation and the value, never a wrap |
| f32 | float | `as f64`, lossless |
| f64 | float | unchanged |
| pf16 (v2) | float | a new scalar mapping, `f64::from(pf16)` (`impl From<pf16> for f64` at both pins); binary16 ⊂ f64, so the conversion is exact |

For pf16 the mapping keeps:
- ±0 with its sign;
- ±∞;
- NaN as NaN (no payload is promised).

A method that takes a pf16 *argument* would narrow a script float lossily, so it is refused by name; this record is read-side only. Nulls stay `None` in every case.

**No partial results.** In a copied vector the conversion is checked per element inside the 0102 snapshot helper. The first out-of-range element fails the whole call with that element's index and value, and no vector is returned.

**Counting.** A native's value readers count as read-side parity only once its boundary controls pass at each pin where that native exists (pf16, i128 and u128 exist at v2 only):
- **Widened integers:** `i64::MAX` reads back, while `i64::MAX + 1`, `i64::MIN − 1` and `u128::MAX` are refused. A vector with one such element in the middle fails whole.
- **pf16:** 0.0, −0.0 (sign checked), +∞, −∞, NaN, 65504 (max), 2⁻²⁴ (smallest subnormal) and null each equal Rust's `f64::from`.
- **f32 and f64:** NaN, ±∞ and −0.0 (sign checked) each equal Rust's value.
- **Every native:** a value-and-validity control with a null in the middle.

Until its controls pass, a native's wrapper may be reachable while its readers are listed as unsupported, named, and not counted.

**4. Support readers for the out-of-census leaves.** `ArrayRef::values_binary` is added, with the same 0102 budget, so that every script-produced leaf array has a checked reader:
- `values_i64`, `values_str` and `values_bool` already exist;
- `NullArray` is served by `len` and `null_count`, and a typed reader on it is a checked refusal.

These count as support, not parity.

## Proof

- **Reachability:** the rows for each owner cite the probe route. The two figures are kept apart: callable availability, and script reachability. Rows that are available but not reachable are named.
- **Deltas at both pins:** operations, bindings, value-tested and reachable, with every status movement listed with its cause.
- **Frozen:** every 0119 frozen binding survives unchanged, with the same path and contract. The frozen lists are extended with the new bindings at close.
- **Guard and copy controls:**
  - guards: removing or swapping a required guard exits 2 at a real release;
  - copies: a nested list or struct whose children exceed the byte budget is refused before any copy, and a count-only mutation of that check is caught.
- **Value tests at both pins.** For each owner, a script builds the Series, takes the chunk (at `oldest()` for the large-offset string and binary arrays), downcasts, reads, is refused on a wrong downcast with the dtype named, and round-trips. Every observation equals Rust's.
- **Oracle:** fixtures for each allowlisted instantiation come from Series chunks through the bridge, and the oracle has no mismatches. Old status movements are reported separately.
- **Suites and launch:** the usual suites run, debug last. Launch is measured against 0119 (`44a362b`) with a budget of +5 ms, because this record adds about 17 wrapper types.

## Stop rules

- A 0119 frozen binding moves or changes its contract: stop.
- An allowlisted owner turns out to be unreachable at a pin: drop it from that pin's allowlist, and name it.
- A method needs a guard that cannot be made total, or a copy that cannot be bounded before it starts: refuse it by name, and continue.
- A clone claim that the pinned sources do not show: refuse that downcast's clone, and stop that owner.
- Anything outside the allowlist changes disposition: stop and report.
