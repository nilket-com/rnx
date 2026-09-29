# rnx 0116 evidence: generic methods, as one audited batch

All 793 rows of the 0113 batch table's `generic_methods` rule are decided. `probes/0116/verify.sh` replays the evidence:
- it checks every digest;
- it recomputes the v2 census, the batch table and the 793-row final audit byte for byte;
- it replays the partition from its pinned inputs: the 0115 generator, the 0112 inventory bundle, and the `v2.toml` of that commit.

## Scoreboard

| | 0113 | 0116 |
|---|---:|---:|
| v2 available, of 6,441 applicable | 3,761 (58.4%) | **3,852 (59.8%)** |
| v2 value-tested | 2,513 | **2,609 (40.5%)** |
| v2 to 90% | 2,036 | 1,945 |
| 0.55.2 available (0077 scoreboard) | 2,585 | **2,647** |
| 0.55.2 value-tested | 1,727 | **1,783** |
| 0.55.2 unsupported | 1,377 | 1,315 |

The full v2 denominator is unchanged: 8,389 callables, with the same exclusions, 528 markers and 23 duplicates.

## The 793 rows

`probes/0116/final.py` gives every row exactly one final disposition. 94 rows generate, with 209 bindings.

| family | generated | other disposition |
|---|---:|---|
| `alias_owner` (58) | 39 | 19 refused by name (see rule 2) |
| `already` (35) | 30 | 5 refused as inner mutable borrows (stage 1) |
| `dtype_owner` (26) | 4 | 22 refused by name (rule 3) |
| `time_zone` (24) | 21 | 3 refused for other named causes: an array return, arity 9, a lifetime iterator |
| `generic_owner_other` (8) | 0 | 7 refused by name, 1 deferred to callbacks |
| `io_generic` (126) | 0 | **stopped: trait-argument modelling** (rule 4) |
| `chunked_pairs` (109) | 0 | census only (rule 5) |
| deferred (366) | 0 | trait_dispatch 149, value_grammar 86, internal_crates 94, callbacks 21, async 16 |
| named refusals (41) | 0 | trait-object values 18, unbounded generic inputs 9, hasher state 5, the file-system boundary 3, receiver shapes 2, facade names 2, `never` 1, a static borrow 1 |

## Rule 1: the bucket gate, isolated first (with Codex's `&mut` correction)

The `generic` token now joins the production and v2 bucket sets. It was measured alone at both pins, and the stage-1 snapshots are in `evidence/stage1/`.

**Stop-rule report.** Lifting the gate exposed a latent fail-open. The `&mut`-return rule reduced *any* `&mut T` return to unit, not only the `&mut Self` chains its comment names. `SeriesTrait::as_any_mut -> &mut dyn Any` was then emitted, and it did not compile, because the value is not `Send`.

Codex decided on the correction, as an approved exception to the no-move rule:
- A `&mut` return is a chain only for a `&mut self` method returning `&mut Self` or `&mut` the owner, directly or inside a `Result`.
- Every other mutable borrow is refused as "inner mutable borrow". This includes `&mut dyn`, `into_materialized_series` (whose receiver mutation cannot be proven from the signature) and `any_value_mut`.

The four pre-existing bindings that moved from generated to unsupported are deliberate. Each only discarded an inner borrow, and was counted available though it did nothing in a script:
- `Logical::physical_mut`;
- `GroupsIdx::first_mut`;
- `GroupsType::idx_mut`;
- `IcebergPathProvider::file_part_prefix_mut`.

Their six oracle cases are removed.

Stage 1 at 0.55.2: +23 generated, −4 corrected, and no other status move.

The self-test `mut_return` checks the chain rule case by case:
- chains: `&mut Self` directly and inside a `Result`, and `&mut` the owner;
- refused before any text: an inner wrapped value, a `Vec`, `Result<&mut String>`, a `dyn`, and a `&self` method returning `&mut Self`.

Two existing tests were updated by design:
- 0082's slice test now expects the named refusal for `&mut [T]`.
- `deref_route_controls` now judges `&mut self` `SeriesTrait` methods by their inventory receiver. `rename`'s witness is now kept behind the newly generated inherent `Series::rename`, and `as_any_mut` is refused earlier. Its invariant is unchanged: no `&mut self` method gets a deref binding.

## Rule 2: alias owners

The census covers every generic owner with alias wrappers. At both pins this adds 39 generated rows, all moves from unsupported, with no other movement: `Schema` 36, the binary/string builders 2, and `FileProviderFunction` 1. Each other alias pair has a named disposition: function-level generics outside the census (12), a head rejected by name, a mapping refusal, or the callback audit. `ArrowSchema` is declared in `polars_arrow`, which is not an API crate, so it has no wrapper.

The focused test `schema_alias_methods_match_rust` checks `with_capacity`, `insert`, `len`, `contains` and `index_of` against Rust.

## Rule 3: dtype owners, and reachability

A release `dtype_instantiations` table creates synthetic alias wrappers, each with a type record copied from its owner. Following review, a listing must be **reachable**: generation refuses a listed instantiation that has no generated constructor, or no method returning a result.

Only `ListPrimitiveChunkedBuilder` over the ten numeric dtypes is listed, at both pins. A script reaches it through `new`, then `append_slice`/`append_series`, then `finish` via `ListBuilderTrait`. The focused test `list_builder_chain_matches_rust` compares the finished list to Rust's.

Not listed, each with its reason:
- `PrimitiveChunkedBuilder`: only `new`; `append`/`finish` are methods of the generic trait `ChunkedBuilder<N, T>`, which needs trait-argument modelling;
- `CategoricalChunkedBuilder`: no `finish`, because `CategoricalChunked<T>` is itself generic;
- `DatetimeInfer`: no script-reachable constructor;
- `ObjectArray`/`ObjectChunkedBuilder`: `T: PolarsObject` has no script type;
- `NoNull`/`SpecialEq`: newtypes over any `T`.

The self-test `dtype_owners` checks:
- that each wrapper is named and spelled as listed;
- that four malformed listings are refused by name (a count mismatch, an unknown type, no citation, a non-generic owner);
- reachability, where a constructor alone or a result alone is not a path.

End to end, re-adding the `PrimitiveChunkedBuilder` listing makes generation exit 2 with "no script path from construction to a result".

## Rule 4: I/O, stopped

Readers and writers are constructed and finished only through `SerReader<R>::new(R)`/`finish` and `SerWriter<W>::new(W)`/`finish(&mut DataFrame)`. Both are generic traits whose argument is the instantiated source or sink, so proving their receivers needs trait-argument modelling. That is the plan's stop condition ("a new trait model"), and it is the same missing proof that blocks `ChunkedBuilder` and part of the trait-dispatch pool.

As Codex decided, the 126 rows are recorded with that exact cause, and **no `Sink`/`Cursor` surface is emitted**. Binding the inherent builder methods would count unusable callables. The planned I/O behaviour tests (the multi-write limit, reuse, the `Cursor` bound) are **stopped, not passed**.

Trait-argument modelling is the next family-level record. The `Sink` contract in the 0116 plan stands as that record's I/O design.

## Rule 5: the chunked census

For the 109 rows, `final.json` records every pair's cause. The leading causes, by pairs:

| cause | pairs |
|---|---:|
| function-level generics outside the 0076 census | 345 |
| head argument mismatch | 248 |
| head mismatch | 120 |
| unmodelled trait argument | 120 |
| no recorded impl | 116 |
| an unresolved associated item | 95 |

The rest are mapping refusals: unreachable types, `Cow`/`Box<dyn>` returns, generic returns. No row is generated here beyond what existing exact rules admit. A later record must name its proof, keys and falsifying controls first. The unmodelled trait arguments again point to trait-argument modelling.

## Rule 6: time zones

`chrono_tz::Tz` crosses as its IANA name: `Tz`, `&Tz`, `Option<&Tz>` and `Option<Tz>` are parsed inline, and an unknown name is a `ConversionError` naming the parameter. The support module cannot name `chrono_tz` at 0.55.2, which is why the parse is inline. A return is `tz.name()`. 21 of the 24 rows generate at v2; the 0.55.2 narrow inventory has none.

**A defect the v2 build found and fixed.** `Option<Tz>` by value first went through the generic `Option` rewrite, which renames its element variable *textually* and so corrupted `Error::conversion`. It now has its own branch, and the self-test covers it.

The v2 scratch manifest gains a direct `chrono-tz = "=0.10.4"` edge. `probes/0108/locks/adapter.lock` gains exactly that one edge, resolved offline against the pinned lock.

The self-test `time_zone` covers all four parameter forms, a refused `&mut Tz`, and the return. The focused test `time_zone_is_its_iana_name` round-trips `Europe/Paris` and refuses `Nowhere/Nope` with `ConversionError`. It passes in the v2 adapter; at 0.55.2 it skips by design, because that surface has no time-zone callables.

## Oracle

| pin | vs base | added | removed | moved |
|---|---|---|---|---|
| 0.55.2 | vs 0115 | 121 (112 match, 7 both_error, 2 both_panic) | 6 (the corrected accessors) | 1: `unique_generic`, match → row_order_differs, under its approved unordered policy |
| v2 | vs 0113 | 187 (154 match, 10 both_error, 2 both_panic, 21 fixture_failed) | 6 | 3: row_order_differs → match, under the join/unique policies |

The 21 v2 fixture failures are the known v2 gap (`f16`/`i128`/`u128` recipes) plus new owners with no fixture yet (`MapChunked`, `ExtensionChunked`, `Decimal`). A fixture failure means "not value-tested", never a mismatch. There are 0 mismatches. Since stage 1: +88 cases, no new fixture failures, and only row-order flips.

## Suites, builds, launch

- **Generator.** 44 of 44 self-tests pass (+3: `mut_return`, `dtype_owners`, `time_zone`).
- **Production suites.** All three pass, debug last. The focused tests pass 3 of 3.
- **v2.** The build, the oracle controls, the oracle and the no-default stage all pass, and the focused tests pass 3 of 3 in the v2 adapter.

Launch, measured with `probes/0073/launch.py`, 60 interleaved launches per set, old = 0115 (`6cea3a9`). Budget +5 ms.

| set | old median | new median | delta |
|---|---:|---:|---:|
| 1 | 20.84 ms | 21.24 ms | +0.40 |
| 2 | 20.14 ms | 21.32 ms | +1.18 |
| 3 | 20.37 ms | 20.41 ms | +0.04 |

## Replays

The 0108–0113 and 0116 replays pass. The 0114 and 0115 replays now run as their own commit's copy, in a worktree at `32d1b40` and `6cea3a9` respectively. A replay proves its record's generator, and later records legitimately change the generator, the release files and the probes those replays read. Both pass.

## Review round 1 (Codex)

**1. The dtype listing's `generic` was never checked.** Changing a listing's `T` to `Bogus` admitted the same wrappers. `World` now records each owner's inherent impl heads from the inventory, and `synthetic_wrappers` requires:
- at least one generic head (a head with a parameter argument; specialized concrete heads instantiate nothing and are skipped);
- that every generic head has exactly one parameter, the listed one.

Otherwise the listing is refused by name. Controls in `dtype_owners`: a `Bogus` generic is refused ("generic `Bogus` is not the owner's one parameter"), and a recorded two-parameter head is refused ("a recorded head has <T, U>"). Mutation check: dropping the parameter-name comparison fails the self-test.

**2. The `&mut` chain check ignored type arguments.** It accepted `path == owner`, so for an owner `Foo<T>` a `&mut Foo<Other>` return counted as a chain. A chain is now only `Self`, or a return structurally equal to the instantiated receiver (`inner.render() == owner.render()`). Controls in `mut_return`: `&mut Series<i64>` and `PolarsResult<&mut Series<i64>>` on a `Series` receiver are refused as inner mutable borrows, with no binding text. Mutation check: restoring `|| path == owner` fails the self-test.

**Effect.** Neither fix changes generated output. At 0.55.2, `--check` shows no drift. At v2, `functions.rs`, `types.rs`, `fixtures.rs`, `generated_oracle.rs` and `surface.json` are byte-identical to the validated build. So the suites, the oracle results, the evidence bundle and the launch figures stand. There are 44 generator self-tests, all passing.
