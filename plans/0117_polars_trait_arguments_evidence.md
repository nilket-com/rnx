# rnx 0117 evidence: trait arguments

The generator now models a trait's arguments. Stages A–D and F are implemented at both pins, and stage E (I/O) stopped under its own rule. `probes/0117/verify.sh` replays the evidence:
- it checks every digest;
- it recomputes stage A's inventory comparison at both pins, from 0112's pinned inventories;
- it regenerates stage A's reason drift with the 0116 generator and compares the triples byte for byte;
- it recomputes the five-pool audit byte for byte, from the 0116 and 0117 v2 surfaces.

## Scoreboard

| | 0116 | 0117 |
|---|---:|---:|
| v2 available, of 6,441 applicable | 3,852 (59.8%) | **3,902 (60.6%)** |
| v2 value-tested | 2,609 | **2,638 (41.0%)** |
| v2 to 90% | 1,945 | 1,895 |
| 0.55.2 available (0077 scoreboard) | 2,647 | **2,675** |
| 0.55.2 value-tested | 1,783 | **1,808** |
| 0.55.2 unsupported | 1,315 | 1,287 |

Against the 0116 generator on the same inventories:
- 0.55.2: +28 rows and +373 bindings;
- v2: +50 rows and +591 bindings.

At both pins, every status move is unsupported → generated. No binding is lost.

## Stage A: trait arguments recorded

The extractor records `trait_args` on every trait impl and foreign-impl callable, and `trait_params` on every trait. The 0110 recovery pass now deduplicates by full impl identity: `for_type`, `blanket`, `trait_args`, bounds and where-predicates.

**Control.** `ctrl_dep2`, which is not the facade root, implements `Cmp<Local2>` and `Cmp<&Local2>` for one type, so only the recovery pass can see them. Both impls are recorded, each with its own arguments. A build that restores the old `(for_type, blanket)` key fails on those rows.

**Comparison.** `compare.py`, run old → new at both pins, finds no problems. The only changes are the new fields and 32 recovered impl rows per pin, each listed in `evidence/stageA-compare-*.json`.

**Drift.** With the 0116 generator, regeneration is byte-identical except for the diagnostic-only drift the plan amendment names: 9 reason texts per pin, in `evidence/stageA-reason-drift.json`.

**The drift is replayed, not only hashed (review round 1).** `probes/0117/drift.sh` builds the 0116 generator (`812238f`) in a worktree and runs it with that commit's release files over the old inventories (0112's bundle) and the new ones, at both pins. `drift.py` then requires three things:
- every generated file other than the surface is byte-identical: bindings, types, catalogue, fixtures and oracle cases;
- every surface field other than the entries, and every entry's key and order, is unchanged;
- exactly 9 entries per pin differ, all unsupported, and only in `reason`.

It writes the key/old/new triples, and `verify.sh` compares them byte for byte with the committed file. Two controls fail it: one extra byte in a generated file, and one changed status.

## Stage B: generic-trait receivers, and the kind dispatch

`families/generic_traits.rs` resolves every non-blanket impl of a generic trait per wrapped receiver, following these rules:
- The trait's parameters are bound to that impl's `trait_args`, and `Self::Assoc` to the impl's associated types.
- The head, bounds and where-predicates are proven for that exact receiver.
- Any impl generic left open refuses the impl, unless it is a scalar generic proven for `i64` and `f64` (0113's rule).
- By coherence, duplicate records of one impl are one arm. The inventory already recorded `ChunkedBuilder for BooleanChunkedBuilder` twice before 0117.

**Dispatch.** Several arms on one receiver become one Rune function only under three conditions:
- they differ in exactly one parameter;
- that parameter's script kinds are pairwise disjoint (a wrapped value, a string, an int, a float, a bool, a list);
- their returns agree.

Each arm is emitted by the ordinary `emit_method` as an unregistered helper. The registered function takes the parameter as a `rune::Value` and calls the arm its kind selects; a value of any other kind is a `ConversionError` naming the parameter.

**Refused by name.**
- Overlapping arms, for example `NamedFromOwned::from_vec`, whose `Vec<i8>`, `Vec<i16>`, … arms all take a list.
- Arms that differ in more than one parameter.
- Arms with different returns.
- An arm that fails to emit, which refuses the whole receiver with that arm's reason.

**Refused impls stay visible.** They are now listed as route exceptions beside the proven receivers; before, they were dropped whenever some receiver generated.

**Defect found and fixed (latent since the first stage B cut).** Every binding of a trait method shared the first receiver's oracle information, so the oracle case for `StringChunked::equal` passed a `StructChunked` and would not compile. Each generic-trait binding now carries its own `OracleInfo`. The entry is fallible if any of its bindings is.

**Self-test `generic_traits`.** It covers:
- two disjoint arms dispatched, with a duplicate record folded to one arm;
- natives as arms of their own kinds;
- an impl on another type that is never an arm of this receiver;
- refused by name, with no binding text: overlapping arms, an unproven impl generic, and a trait-argument count mismatch.

Two mutations fail it: dropping the arm dedup, and dropping the overlap check.

## Stage C: trait-argument bounds

`holds` now enumerates every recorded impl whose head matches, deduplicated by identity:
- it unifies the impl's `trait_args`, after substitution, with the bound's resolved arguments;
- it discharges each candidate's bounds, where-predicates and associated-type constraints;
- it proves the bound only if exactly one candidate is proven. Two or more is "ambiguous", with the candidates named. The record order never decides.

This rule applies to every bound. Three situations are unresolved, never rejected:
- an argument or constraint that does not resolve;
- one that names an unbound parameter;
- a trait whose recorded parameter count differs from the bound's.

Core traits with arguments stay unmodelled; `AsRef<[IdxSize]>` is the remaining `take`/`sample` cause.

**Two resolution defects fixed, found by the measurement:**
- A qualified projection nested inside generic arguments (`Option<<T as PolarsNumericType>::Native>`) was never resolved, because only the first `<` was examined. Every `<` is now examined, innermost first.
- A bound's associated-type constraint was compared unresolved, so `MinMaxKernel<Scalar = Int64Type::Native>` was rejected against `Scalar = i64`. Constraints are now resolved before the comparison. rustdoc's bare trait name in a qualified projection is matched by its last path segment, and the value must still be unique.

**Self-test controls** in `applicability`:
- reversed order (the wrong candidate first, the right one second);
- an ambiguous pair;
- no matching impl;
- a projection argument that resolves, and one that does not;
- an unbound parameter;
- a bound with no arguments;
- a nested projection;
- a constraint compared resolved.

Two mutations fail it: "the first head match decides" and "any proven candidate suffices".

## Stage D: bounded blanket impls

A bare-parameter head (`impl<T: B> Trait for T`) binds `T` to the whole type. The receiver then holds only where every bound and where-predicate is proven. An unbounded head, with no bound but `?Sized` and no predicate, is refused by name. The same rule applies inside `holds`. Causes over hundreds of wrappers are grouped, for example "T on 413 wrapped types: trait `num_traits::Num` is outside the inventory".

**Controls.** A bounded blanket impl proves where its bound holds and is rejected where it fails. Unbounded and `?Sized`-only heads are refused, both in `holds` and as a method head. Two mutations fail the controls: treating every blanket impl as bounded, and dropping the bare-head binding.

## Stage E: I/O, stopped

No reader or writer is reachable:
- `MmapBytesReader for Cursor<T>` needs `T: AsRef<[u8]> + Send + Sync`, which is a core trait with an argument plus auto traits.
- `SerWriter<W>` needs `W: core::io::Write` on the Sink.

The inventory records none of these, and the plan names no model for std facts, so the stop rule applies. The 126 rows are recorded with that cause and no `Sink`/`Cursor` surface is emitted. **The planned I/O behaviour tests are stopped, not passed.** A small, cited table of std facts for `Cursor<Vec<u8>>` and `Sink` is the proposed next record.

## Stage F: builders

`PrimitiveChunkedBuilder` is listed over the ten numeric dtypes at both pins. It is reachable through `new` → `ChunkedBuilder::append_value`/`append_null`/`append_option` → `finish`. `CategoricalChunkedBuilder` stays unlisted by name: it has no `ChunkedBuilder` impl, and its own `finish` returns the generic `CategoricalChunked<T>`.

## The audit (v2)

`probes/0117/audit.py` gives every row of the five pools exactly one disposition:

| pool | rows | generated (bindings) | refused | other |
|---|---:|---:|---:|---|
| generic-trait methods | 64 | 27 (427) | 37 | |
| trait-argument bounds | 6 | 2 (40) | 4 | |
| bounded blanket impls | 37 | 20 (20) | 13 | 4 deferred to callbacks (`PolarsObject` has no impl: an extension point) |
| I/O | 126 | 0 | | 126 stopped (stage E) |
| builders | 8 | 1 (10) | 7 (Categorical) | |

**Named refusals among the generic-trait rows:**
- `ChunkAgg` needs `polars_arrow::types::native::NativeType`, which is outside the inventory.
- The `ChunkedCollect*IterExt` family.
- `ChunkSet`/`ChunkFull`/`ChunkFilter`, whose function-level generics cannot be inferred.
- `NamedFrom`, whose `Range` argument is a foreign type.
- `NamedFromOwned`, whose arms overlap.
- `SerReader`/`SerWriter`, which fall under stage E.

**Named refusals among the blanket rows:**
- `LhsNumOps`, which needs `num_traits::Num`.
- `DataFrameOps`, which needs `IntoDf`; both traits are outside the inventory.
- The `Fn(...)` bound syntax in `CrossJoinFilter` and the UDF traits.

## Registration parts: a defect the debug suite found

With about 385 more registrations, the single generated `install` (5,488 calls in one frame) overflowed the 2 MiB test-thread stack in debug builds: 35 test binaries aborted. It passed at 4 MiB. The generator now writes `install` as `#[inline(never)]` parts of 512 registrations, called in order. No debug frame grows with the binding count, and the registration order is unchanged.

## Focused tests (`tests/trait_arguments.rs`)

Each test compares script calls to direct Rust:
- `equal`/`gt` on `StringChunked` against a series and against a string (the two arms), plus an int giving `ConversionError`;
- `shift` on `Int64Chunked`, through stage C's projection proof;
- an `Int64ChunkedBuilder` chain from `new` to `finish`;
- `TemporalMethods::year` on a datetime `Series` (v2 only; the 0.55.2 narrow build has no `polars_time`).

**Planned test substituted.** The plan's `ChunkAgg::sum` test cannot exist, because `ChunkAgg` is refused (see the audit); `shift` replaces it.

**Results:**
- 0.55.2: 4/4, with the temporal test skipped by design.
- v2 adapter: 4/4, plus 0116's 3/3.

## Oracle

| pin | vs 0116 | added | removed | moved |
|---|---|---|---|---|
| 0.55.2 | `oracle-results.json` | 287 (214 match, 71 both_error, 2 both_panic) | 0 | 0 |
| v2 | `evidence/oracle-results-v2.json` | 497 (337 match, 106 both_error, 3 both_panic, 51 fixture_failed) | 0 | 5, all row-order flips under approved unordered policies (the joins, `unique`, `upsample` with `by`) |

The 51 v2 fixture failures are the known v2 fixture gap: `i128`/`f16`/`u128` recipes, Decimal, Extension and Map. A fixture failure means "not value-tested", never a mismatch. There are 0 mismatches. The 2 `oracle_panicked` cases are 0116's.

## Suites, builds, launch

- **Generator:** 45 of 45 self-tests pass (+1: `generic_traits`; `applicability` extended).
- **Production:** the oracle and all three suites pass, debug last.
- **v2:** every stage is `ok`: the build, the oracle controls, the no-default-features build and tests. The oracle stage reports `cases_failed` only for the 2 pre-existing panics, as in 0116.

Launch was measured with `probes/0073/launch.py`, 60 interleaved launches per set, against 0116 (`812238f`), with a budget of +5 ms:

| set | old median | new median | delta |
|---|---:|---:|---:|
| 1 | 20.47 ms | 22.38 ms | +1.91 |
| 2 | 19.08 ms | 21.37 ms | +2.29 |
| 3 | 19.47 ms | 21.98 ms | +2.51 |

## Replays

The 0108–0117 replays pass.
