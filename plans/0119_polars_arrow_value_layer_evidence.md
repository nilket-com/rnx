# rnx 0119 evidence: the Arrow value layer, closed on the proven bridge

`polars_arrow` is admitted at both pins, under its own `polars::arrow` namespace, and every pre-0119 binding is frozen. A script can now take a Series chunk as an owned Arrow array (`polars::arrow::ArrayRef`), inspect it, read its values through checked readers, and give it back as a Series. Stage 3, the concrete generic arrays, is deferred intact to record 0120 by the plan's deterministic rule.

`probes/0119/verify.sh` replays the evidence:
- it checks every digest;
- it rebuilds both frozen lists byte for byte from 0118's surfaces and catalogues;
- it checks every frozen binding in the 0119 surfaces;
- it recomputes the audits at both pins.

## Scoreboard

| | 0118 | 0119 |
|---|---:|---:|
| v2 available, of applicable | 3,988 of 6,441 (61.9%) | **4,068 of 6,421 (63.4%)** |
| v2 value-tested | 2,642 | **2,705 (42.1%)** |
| v2 to 90% | 1,809 | 1,711 |
| 0.55.2 available (0077 scoreboard) | 2,735 | **2,815** |
| 0.55.2 value-tested | 1,816 | **1,875** |
| 0.55.2 unsupported | 1,227 | 1,300 |

**Why the v2 denominator changed.** It moved by exactly 20 rows (6,441 → 6,421). The 20 `polars_arrow` marker entries, such as the `Eq` markers that install Rune equality, are classified as markers now that the crate is an API crate (markers 528 → 548), instead of being counted as missing internal-crate rows.

**Why 0.55.2's unsupported count rose.** Admitting the crate moved 173 of its callables from "internal crate, apart" into the counted operations (4,361 → 4,534). The deferred and refused Arrow rows are now counted with their reasons instead of set apart. It is not a regression: no row that was generated in 0118 is lost.

## Stage 1: the namespace and the freeze

**The namespace (`namespaced_crates = ["polars_arrow"]`).** A namespaced crate's types always sit under `polars::arrow` and never count in a short-name collision, so no existing path can move (self-test `freeze`). Without the rule, core `Field` would move to `polars::core::Field`; a mutation that counts namespaced types in collisions is caught.

Two more consequences, each tested:
- A namespaced crate's type aliases form no alias wrapper (`IdxArr = PrimitiveArray<IdxSize>` renamed types the snapshot families match exactly). That is found by the gate below.
- A namespaced crate's free functions are refused by name, because a Rune `path` names a type, not a module, so they cannot sit under `polars::arrow` in the one generated module (at 0.55.2, `indexes_to_usizes`).

**The freeze (`frozen_bindings`).** A repository-relative list holds every binding 0118 generated: 4,750 at 0.55.2 and 6,311 at v2. Each row is (entry key, binding id, Rune path, catalogue summary), with the summary giving the argument and return shapes. Generation exits 2 if any binding moves or disappears, if one keeps its path but changes its contract, or if a Rune path is generated twice. Protocol bindings, which have no catalogue summary, are checked by their path, which names the protocol.

Both pins print "none moved, no contract changed". Self-test `freeze` checks:
- that survival passes;
- that a moved binding, a changed contract and a duplicate path each refuse.

**The gate earned its place during implementation.** It caught:
- `IdxCa`'s five snapshot bindings moving (the `IdxArr` alias rename);
- my own over-broad refusal of `ChunkedArray::chunks`, which would have removed 0099's frozen snapshot bindings;
- `StructChunked`'s `downcast_*` snapshots disappearing when the first form of the deferral dropped `StructArray`'s internal wrapper.

**The frozen lists' sources.** The 0.55.2 catalogue is 0118's commit. The v2 catalogue was regenerated with the 0118 generator from 0117's pinned inventory; its surface equals 0118's evidence in every entry (only `source`, the inventory file's name, differs), and it is bundled in `evidence/`.

The drift controls in `tests/generated.rs` mutate the release on purpose, so their copies run without the freeze; the freeze guards real releases.

## Stage 2: the owned array bridge

**The value.** `polars::arrow::ArrayRef` is a hand-written support wrapper around `Box<dyn polars_arrow::array::Array>`, which is `ArrayRef` at both pins. The generator maps both identities to it:
- returns are wrapped;
- owned parameters are moved out of the Rune value (0109's rule), and a reuse is Rune's access error ("Cannot read, value is M…", tested).

**The `Array` trait** reaches the value through the deref route. A trait reachable only through a deref route is no longer refused as "no wrapped implementor".

**Ownership, verified per pin before any clone is exposed:**
- `Array::to_boxed` is `Box::new(self.clone())` (polars-arrow `array/mod.rs:629-631` at 0.55.2, `:630-632` at v2).
- `PrimitiveArray` and `BooleanArray` derive `Clone` over `Buffer`/`Bitmap`, and the string view array holds `Buffer`s of views and data buffers.
- `Buffer::clone` copies its `SharedStorage` handle (polars-buffer `buffer.rs:50-56`), whose clone increments a reference count with `Arc` ordering (`storage.rs:467-472` at 0.55.2, `:469-474` at v2).
- So a boxed clone is a new owned trait object sharing reference-counted buffers.

**Checked readers**, which ship in this stage so that it stands alone:
- `values_i64` (an `Int64` primitive array), `values_str` (the string view array Polars uses) and `values_bool`;
- each downcasts through `as_any()` to exactly one type, and a mismatch is a `ConversionError` naming the Arrow dtype;
- each is bounded by the materialize limit before any copy;
- `dtype_name` gives the Arrow dtype.

**Review round 1, finding 1: the readers bound payload bytes too.** The first cut bounded only the element count, so one long string passed. The readers now go through 0102's one-array snapshots (`array_snapshot`, `array_snapshot_str`, `array_snapshot_bool`): one chunk slot, the cells and the UTF-8 bytes are counted with checked steps before any copy, and re-counted while copying.
- **Test `values_str_bounds_payload_bytes`:** under a 12-slot bound, two short-count strings over the byte budget are refused, one long string alone is refused, and the same receiver stays usable and reads once the bound allows.
- **Mutation:** restoring the count-only bound fails that test.

**Receiver guards (`[[receiver_guards]]`).** A listed index argument is converted, checked against the receiver, and refused as `OutOfBounds` before the Polars call. Each row is cited to its pin's source line:
- `Series::to_arrow`'s chunk index (`below_n_chunks`: `chunks().get(chunk_idx).unwrap()`);
- `is_null` and `is_valid` (`below_len`: `assert!(i < self.len())`);
- `sliced` (`range_len`, mirroring Arrow exactly: a zero length is not checked, as `sliced` returns an empty array for it);
- `split_at_boxed` (`at_most_len`).

Self-test `receiver_guards` covers each check's text and the five malformed rows. A mutation that drops the zero-length exemption is caught.

**Review round 1, finding 2: the guards fail closed.** The guards are a closed, required set in the generator (`receiver_guards::REQUIRED`). Whenever the release admits `polars_arrow`, every required path the inventory holds must have exactly its row: the same check, the same parameter(s), cited, and unique. Each guarded parameter must be `usize` on a callable with a receiver. A missing, extra, duplicated, uncited or differently checked row refuses generation (exit 2) before any emission.
- **Self-test controls:** a removed row; a swapped but valid check (`below_len` on the chunk index); a duplicate; an extra; an uncited row; a non-`usize` parameter; a receiver-less callable.
- **Against the real 0.55.2 release:** removing the `to_arrow` row gives "a required receiver guard is missing", and swapping it to `below_len` gives "the required guard is `below_n_chunks`". Both exit 2.

**Refused by name:**
- `slice` (it needs `&mut`);
- `as_any_mut` and `dtype_mut` (inner mutable borrows);
- `validity` and `with_validity` (an unreachable `Bitmap`);
- `ValueSize::get_values_size` (its trait is only under a private module, E0603).

**End-to-end value test** (`tests/arrow_values.rs`, 6/6 at both pins, including the byte-budget test):
- numeric, string and boolean chunks with nulls: `to_arrow`, then `len`/`null_count`/`is_null`/`dtype_name`, then the matching reader, then `from_arrow`, each equal to Rust;
- a wrong reader is a `ConversionError`;
- the all-null chunk carries the `Null` dtype, and a typed reader is only a checked refusal;
- the out-of-range chunk, `is_null`, `sliced` and `split_at_boxed` are `OutOfBounds`, while `sliced(99, 0)` returns the empty array Rust returns;
- the moved array cannot be reused.

**Oracle fixtures come through the bridge.** The core fixture `arrow_array` is `Series [1, null, 3]` → `to_arrow(0, CompatLevel::newest())` on both sides, compared as the Series it converts back to. Guarded arguments are called in range, as index 0 and length 1 (shapes `int0` and `int1`). The `Array` trait methods, `Series::to_arrow`, `into_chunks`, `rechunk_*_to_arrow` and `to_boxed` are all value-tested, and all match.

The 0099–0106 snapshot families are unchanged: their frozen bindings survive, and their tests pass.

## Stage 3: deferred to 0120, by the plan's rule

Stages 1–2 alone add 115 available rows at 0.55.2 and 136 at v2 before the deferral below. The concrete-array batch would take both pins far past the 150-row line, and this record already carries a namespace, a bridge, a guard table and a cross-crate review. So 0119 closes on the proven bridge, and the batch moves to 0120 intact.

**The deferral is exact.** `deferred_type_prefixes` (`polars_arrow::array::`, `polars_arrow::legacy::array::`) keeps each such type at precisely its 0118 treatment:
- it is an internal type, wrapped under `polars::arrow` only where an API signature mentions it, as the snapshot families need;
- its own callables are out of scope;
- it is never a trait receiver, so the `Array` trait reaches only the bridge value.

Without this, `StructArray`, the fixed-size-list `AnonymousBuilder` and `MutableNullArray` would have become concrete-array wrappers in this record. Self-test `freeze` covers the prefix.

## Cross-crate review (Codex gate)

Each moved row outside `polars_arrow` was reviewed before scoring; the reviews are in the audit's `cross_crate` section.
- **Passed:**
  - `DataType::from_arrow_field`/`to_arrow_field`, `Field::to_arrow`, `Field: From<&ArrowField>`;
  - `Column::rechunk_to_arrow` and `DataFrame::rechunk_into_arrow` (they consume an `Arc`-cheap clone), `DataFrame::rechunk_to_arrow`;
  - `Series::from_arrow` (it moves; a dtype mismatch is a `PolarsError`);
  - `Series::from_chunk_and_dtype` (the dtype is checked first, `series/from.rs:41-45`);
  - `Series::into_chunks`;
  - `Series::to_arrow` (guarded);
  - `CategoricalArrayToArrowConverter::build_values_array` and `CategoricalMapping::to_arrow`.
- **Refused by name:**
  - `ListBuilderTrait::inner_array`: `as_box` is `std::mem::take` (`binview/mutable.rs:681-685`), which leaves the builder's offsets pointing past values it no longer has;
  - `SeriesTrait::chunks`, and `ChunkedArray::chunks` at v2 where it is new: a borrowed chunk list cloned element by element with no bound, when the owned path and the snapshots already serve it.

## The audit, per pin

`probes/0119/audit.py` gives every `polars_arrow` row one disposition. Reachability is computed as a fixpoint over generated bindings, using the inventory's receivers.

| pin | available (all script-reachable) | deferred to 0120 | refused, by name | cross-crate moves reviewed | unreviewed |
|---|---:|---:|---:|---:|---:|
| 0.55.2 | 87 (67 generated, 20 adapted) | 227 | 32 | 13 | 0 |
| v2 | 87 | 268 | 32 | 13 | 0 |

The 32 refusals are named causes: serde through an unbounded IPC buffer, outward `From` conversions, `polars_io` buffering types, blanket-implemented traits, `dyn Any` returns, and the rows listed above.

## Oracle

| pin | vs 0118 | added | removed | moved |
|---|---|---|---|---|
| 0.55.2 | `oracle-results.json` | 69 (68 match, 1 both_panic) | 0 | 0 |
| v2 | `evidence/oracle-results-v2.json` | 73 (71 match, 2 both_panic) | 0 | 3 row-order flips under the approved join/unique policies |

## Suites, builds, launch

- **Generator:** 48/48 self-tests (+`freeze`, +`receiver_guards`).
- **Production:** the oracle and all three suites pass, debug last, including the 18 drift controls.
- **v2:** every stage is `ok`; focused tests pass 6/6 (Arrow), 5/5 (I/O), 4/4 and 3/3, and the sink tests pass 9/9.
- **The 0118 carry-over nit** (the `sink_tests` assert message) now says "refused".

Launch was measured against 0118 (`e0c3878`), 60 interleaved launches per set, with a budget of +5 ms:

| set | old median | new median | delta |
|---|---:|---:|---:|
| 1 | 20.58 ms | 20.64 ms | +0.06 |
| 2 | 20.82 ms | 20.51 ms | −0.31 |
| 3 | 21.81 ms | 21.73 ms | −0.08 |

## Replays

`probes/0119/verify.sh` passes, and so do the 0108–0118 replays.
