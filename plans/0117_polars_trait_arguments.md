# rnx 0117: trait arguments, as one family-level record

Status: plan. Record 0116 is closed on `origin/main` at `812238f`. It stopped its I/O family on a missing proof, and agreed that this record comes next.

The starting v2 scoreboard is 3,852 available and 2,609 value-tested of 6,441 applicable callables (59.8%). This record adds one general capability: proving a trait method's receivers when the trait, or an impl's bound, carries type arguments. It then applies that capability to every pool that waits on it. It is one plan and one implementation, not a record per trait.

## The gap, measured on the 0116 v2 surface

**The inventory does not record a trait impl's trait arguments.** `impl<T, Rhs: ToPrimitive> ChunkCompareEq<Rhs> for ChunkedArray<T>` and `impl ChunkCompareEq<&ChunkedArray<T>> for ChunkedArray<T>` are recorded identically. Each record has only `for_type`, bounds, where-predicates and associated types, never the `Rhs`. The generator therefore cannot know a generic trait's arguments per impl, and it refuses:

| pool | callables | cause |
|---|---:|---|
| generic-trait methods | 64 | `proven_trait_receivers` refuses any generic trait wholesale: `ChunkCompareEq`/`Ineq`, `ChunkAgg`, `ChunkSort`, `ChunkSet`, `ChunkQuantile`, `ChunkedBuilder`, `NewChunkedArray`, `ChunkedCollectIterExt`, `SerReader`, `SerWriter`, … |
| trait-argument bounds | 6 (120 instantiation pairs) | `proof.rs` stops at a bound whose trait carries an argument (`ChunkTake`, `ChunkExpandAtIndex`, `ChunkTakeUnchecked`) |
| bounded blanket impls | about 40 | `impl<T: Bound> Trait for T` is recorded with the bare `T` as its type, and no receiver is ever proven: `TemporalMethods` 20, `LhsNumOps`, `PolarsObjectSafe`, `DataFrameOps`, `CrossJoinFilter` |
| I/O readers and writers | 126 | 0116's stopped family: reachable only through `SerReader<R>`/`SerWriter<W>` |
| dtype builders | 12 | `PrimitiveChunkedBuilder` and `CategoricalChunkedBuilder`, unlisted in 0116 because their `append`/`finish` are `ChunkedBuilder<N, T>` methods |

The rest of the 151-row `trait_dispatch` pool is traits with no impl in the inventory: `ExtensionTypeImpl`, `AnonymousScan`, the `*Udf` traits. Those are extension points a script would implement, so they stay with `callbacks`, each with that reason.

## Stages

Each stage is implemented and measured at both pins before the next begins.

**Stage A: record trait arguments.** The extractor (`probes/0072/extract`) records each trait impl's trait arguments, `trait_args`, in canonical form from the rustdoc `trait_` path, next to `for_type`. It does so for both the trait's `impls` and the foreign-impl callables. A generic trait's own type parameters (`Rhs` in `ChunkCompareEq<Rhs>`) are not recorded either, so its supporting record gains `trait_params`, which stage B needs in order to bind `trait_args` by position.

**Deduplication.** The 0110 implementor-recovery pass (`inventory.rs:573`) deduplicates impls by `(for_type, blanket)`. Two impls of one trait for the same type that differ only in trait arguments collide there, so one can already be erased. Deduplication is keyed by the full impl identity: `for_type`, `blanket`, `trait_args`, bounds and where-predicates. Stage A proves that every same-receiver impl with different arguments survives, for example both `ChunkCompareEq` impls on `ChunkedArray<T>`, at both pins.

**Extraction control.** A unit test in the extractor feeds two impls of one trait for the same type that differ only in trait arguments. Both must be recorded, each with its own `trait_args`. A mutation that restores the old `(for_type, blanket)` key must fail it.

The inventories are re-extracted at both pins with the pinned toolchains and features, and bundled with their digests.

**Proof:** a key-by-key comparison of the old and new inventories shows that the only changes are:
- the new fields, `trait_args` and `trait_params`;
- any impl rows recovered by the corrected deduplication, each listed explicitly with its count.

With the 0116 generator, generation from the new inventories is byte-identical at both pins: the generated output, every entry's status and the oracle cases. A recovered row is inert until stage B reads it. If a recovered row changes any old output, stop and report.

**Amendment (stage A, approved by Codex).** One diagnostic-only drift is accepted by name: the recovered impl rows lengthen the implementor list that 0116 writes into the reason text of 9 unsupported entries at each pin. No status, binding, generated line or oracle case moves. The 9 key/old/new triples per pin are in `probes/0117/evidence/stageA-reason-drift.json`.

**Stage B: generic-trait receivers.** For a method of a generic trait, every impl of that trait is a candidate for each wrapped receiver. It is proven with the existing head unification and bound discharge; the trait's parameters are bound to that impl's `trait_args` after substitution; and the method signature is resolved per (receiver, impl).

When one receiver has several proven impls of the same trait that differ only in trait arguments, for example `ChunkCompareEq<&ChunkedArray<T>>` and `ChunkCompareEq<Rhs: ToPrimitive>`, they become the arms of one Rune function, following 0113's operator grouping. This is allowed **only when the arms' script argument domains, after conversion, are disjoint**. The function dispatches on the runtime argument's kind: a wrapped value of the arm's type, `i64` or `f64`.
- Arms whose domains overlap are refused by name, without picking one and without name suffixes.
- A numeric `Rhs` is admitted only at the proven natives, as in 0113.

An unmodelled trait argument, such as one containing a projection or a nested generic, is refused with that argument named.

**Stage C: trait-argument bounds.** `proof.rs` discharges `X: Trait<A>` by **enumerating every recorded impl** of `Trait` whose head unifies with `X` and whose `trait_args` unify with `A` after substitution, with all its bounds and where-predicates discharged. The bound holds only when exactly one impl is proven. None proven is refused as today. Two or more proven is refused as ambiguous, with the candidates named.

Today `holds` returns the *first* exact or generic impl it finds. That becomes the same enumerate-and-require-unique rule for every bound, not only bounds with trait arguments. Controls:
- reversed candidate order, where the first candidate is wrong and the second is right; the proof must find the second;
- two indistinguishable proven candidates, which must be refused as ambiguous;
- a trait-argument bound with no matching impl;
- one with a projection argument.

Because the rule now applies to all bounds, a bound that today silently took the first candidate may turn ambiguous and refused. Any resulting movement of existing entries or oracle cases is reported, never absorbed silently.

**Stage D: bounded blanket impls.** For `impl<T: B> Trait for T`, a wrapped type is a receiver only once **every bound and where-predicate** of that impl is proven for that exact type, with stage C's uniqueness rule applying to each. An unbounded `impl<T> Trait for T` is refused by name: it would claim every type. So is a blanket impl whose bound cannot be proven.

**Stage E: I/O.** It stays in this record **only if stage B actually makes both construction (`SerReader::new`/`SerWriter::new`) and finishing reachable** for a reader or writer. Otherwise the stop rule applies, as in 0116. Where both are reachable, 0116's rule 4 applies unchanged:
- readers are instantiated at `std::io::Cursor<Vec<u8>>` and writers at `support::Sink`, with every `R`/`W` bound proven exactly;
- `SerReader::new`/`finish` and `SerWriter::new`/`finish` bind through stage B;
- there is no file-system instantiation.

The operation-level `Sink` contract is 0116's, in full:
- cumulative checked accounting, committed + staged + new, checked before any copy;
- staged bytes are committed atomically on `Ok` and discarded on any `Err`;
- the private `SinkLimit` marker is found by downcast in `PolarsError::IO` and mapped to `MaterializeLimit`, while other errors keep their kind;
- the handle is reusable after an error;
- `Cursor` input is length-checked before its single owned copy.

So are its direct tests: multi-write at exactly the limit and one past it, the handle's contents and reuse after an error, the discard of a non-limit error, and the `Cursor` bound.

A reader or writer type whose construction and finishing cannot both be reached from a script generates nothing, and is recorded by name. 0116's reachability gate applies.

**Stage F: builders.** `PrimitiveChunkedBuilder` (the ten numeric dtypes) and `CategoricalChunkedBuilder` (8/16/32) are listed again under 0116's `dtype_instantiations`. Its reachability gate must now pass: `new`, then `append_value`/`append_null` through `ChunkedBuilder`, then `finish`. A builder that still has no path is refused, as before.

## Proof

**Audit.** A table of every row in the five pools, with its final disposition: generated with its binding count, refused with its reason, or deferred to its owner. It reconciles to the surfaces at both pins and is replayable, following 0116's pattern.

**Stage A's evidence** is the inventory comparison, showing only the new fields plus the explicitly listed recovered impl rows, and the byte-identical regeneration apart from the amended reason-text drift.

**Controls.** Each stage has generator self-tests that fail closed:
- stage B: a wrong trait argument, two indistinguishable arms, an unproven `Rhs` native, an impl whose head does not match;
- stage C: a trait-argument bound with no matching impl, and one with a projection argument;
- stage D: an unbounded blanket impl, and a blanket impl whose bound fails;
- stage E: every 0116 `Sink` test, plus a reader/writer bound that cannot be proven.

Each stage also has a mutation control that the stage's own proof catches.

**Focused behaviour tests** compare script calls to direct Rust:
- `ChunkCompareEq::equal` against a series and against a scalar (the two arms);
- `ChunkAgg::sum`;
- `TemporalMethods` on a datetime;
- a CSV, Parquet and IPC round trip through `Sink` and `Cursor`, byte-equal to Rust's for the same frame;
- a `PrimitiveChunkedBuilder` chain, from `new` to `finish`.

**Oracle** at both pins, with every old status movement reported separately. The usual suites, debug last. Launch against 0116 within +5 ms: the new I/O and builder types add registrations, which is measured, not assumed.

## Stop rules

- Stop and report before any generator change if stage A changes anything in the inventories other than:
  - the new fields `trait_args` and `trait_params`;
  - the explicitly listed recovered impl rows;

  or if any recovered row changes the 0116 generator's output, statuses or oracle cases.
- Stop a stage whose first proofs need a model beyond what it names: higher-ranked or associated-type trait arguments, or an unbounded blanket impl. Finish the other stages, and keep the stopped rows in the audit with the exact cause.
- If stage B's argument-kind dispatch cannot tell two arms apart for a trait, refuse that trait's overlapping arms by name. Never pick one arbitrarily.
- If an existing oracle case moves outside the approved row-order policies, stop and report.
