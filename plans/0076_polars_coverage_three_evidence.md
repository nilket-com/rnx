# rnx 0076: Polars coverage, stage three, evidence

Plan: `plans/0076_polars_coverage_three.md` (96aa314, revised after
Codex's plan review). Gates are appended as they close. Production stays
on 0.55.2; the baseline is record 0075 at adcf9c8: 1813 generated
entries, 1057 oracle cases, all verified.

## Gate 1: accounting at two levels, and the null-series fixture

### Two levels

`surface.json` keeps one entry per inventory callable (callable
coverage, the denominator of every earlier record) and adds, under each
generated entry, `bindings`: one per emitted binding, keyed by an id
made of the callable's sanitized path plus `__on__<receiver>` for every
receiver after the first, with the receiver's canonical type, the route
(`inherent`, `free`, `protocol`, `implementor`, and from gate 2 `deref`),
its disposition (`case …` or the reason it is unverified) and its case
id. Candidate routes that produced no binding are listed under
`exceptions` with route, receiver and reason. The oracle emits one case
per binding; a multi-binding entry's callable-level disposition is
`case (on n of m receivers)` when any binding has a case. The runner's
results are keyed by case id, and so is the rc2 probe's report.

Reconciliation, in the accounting test on the committed surface: every
generated entry has at least one binding; every binding has exactly one
disposition; a binding has a case id exactly when its disposition is a
case; the bindings with a case equal the emitted case count; binding ids
and case ids are unique; the harness's case ids are exactly the
bindings' case ids; an entry's disposition is a case exactly when one of
its bindings is. Runner control: two cases of one path with distinct ids
both survive collection and are judged apart. Applied to the 0075
surface first, the callable counts were unchanged (1813 generated) and
every existing case id was preserved; the ten entries that already had
several implementors (`ListBuilderTrait::*` on five builders,
`IntoColumn::into_column` on two, `IntoSeries::is_series` on two, …) now
get one case per implementor instead of one for the first with a
fixture, which alone took the case count from 1057 to 1068.

### Null-series fixture

`NullChunked` has a core fixture on both sides (`Series::new_null("x",
2).null().unwrap().clone()`, Rune `fx::null_chunked()`) and a comparator
rendering name, dtype and length, the whole logical content of an
all-null array. Runner controls: a null series of another length,
another name and another dtype each fail through the runner.

| SeriesTrait bindings on `NullChunked` | count |
|---|---:|
| shipped before this record, unverified | 51 |
| now verified, match | 42 |
| now verified, both_error (`and_reduce`, `or_reduce`, `xor_reduce`, `quantile_reduce`, `quantiles_reduce`: `InvalidOperation` on both sides) | 5 |
| unverified, no fixture for a parameter (`filter` takes `&BooleanChunked`, `take` takes `&IdxCa`) | 2 |
| unverified, return type has no comparison (`split_at`, `unique_id`) | 2 |

`Series::null` is verified as well (`both_error` on the `Int64` fixture,
which is not a null series). Newly verified shipped bindings after gate
1: 47, of which 42 value matches and 5 error-only.

## Gate 2: `SeriesTrait` on `Series` through `Deref`

The extractor records the associated types every impl binds
(`impl_assoc`); for `Series` that is `Deref::Target = dyn SeriesTrait`.
Re-extraction of the four 0072 configurations changed nothing else
(summaries byte-identical; inventories identical apart from the new
fields). The generator's deref route binds a trait method on a wrapped
type whose `Deref` target is `dyn` that trait, calling
`<dyn Trait>::method(&*recv.0, …)`, the call autoderef makes. Receiver
forms are kept: `&self` through `Deref`; `&mut self` needs `DerefMut`,
which `Series` does not implement in the inventory (`rename` is the one
exception); a consuming `self` cannot move out of the target. Name
collisions keep inherent first: eight `SeriesTrait` names that `Series`
binds inherently are retained and the trait route is listed as "not
separately exposed, inherent binding retained (…)", claiming nothing
about equivalence. Where the inherent binding did not exist (`is_empty`,
`len`, `propagate_nulls`, `trim_lists_to_normalized_offsets` were
unsupported on every implementor because `NullChunked` takes those
names), the deref route now binds them, so callable coverage rose from
1813 to 1817.

| SeriesTrait bindings on `Series` (route `deref`) | count |
|---|---:|
| newly compiled | 46 |
| verified, match | 37 |
| verified, both_error (the five reductions, `InvalidOperation` on `Int64` too) | 5 |
| unverified, no fixture for a parameter | 2 |
| unverified, return type has no comparison | 2 |
| route exceptions: inherent name retained | 8 |
| route exceptions: `&mut self` without `DerefMut` (`rename`) | 1 |
| route exceptions: foreign return (`field`, `Cow<Field>`), mutable reference argument (`find_validity_mismatch`) | 2 |

The 12 `SeriesTrait` methods in the generic bucket (`as_any`, iterators,
`dyn` returns) stay out; 2 remain unsupported on every route.

Controls: generator self-test, a `Deref` whose target is not a trait
object binds nothing and `DerefMut` is recorded apart; adapter test on
the committed surface, every deref binding is on `polars::Series`, the
`&mut self` exception names `DerefMut`, every `SeriesTrait` name that
`Series` binds inherently is retained with the trait route not
separately exposed, and 33 methods bound on both `NullChunked` and
`Series` recorded different approved results under two case ids
(`has_nulls`: true and false; `first`: a null scalar and `Int64(1)`;
`drop_nulls`: an empty null array and the three values).

### After gates 1 and 2

| | baseline | now |
|---|---:|---:|
| callable coverage: generated | 1813 | 1817 |
| bindings | 1813 | 1888 |
| oracle cases, all verified | 1057 | 1158 |
| match / both_error / both_panic / row-order | 1002 / 44 / 11 / 0 | 1088 / 55 / 13 / 2 |

Newly verified shipped bindings: 47 (gate 1) plus 11 from the multi-
implementor entries. Newly compiled: 46 (gate 2), 42 of them verified.
Setup failures: 0. The unique cases flap between match and row-order as
in 0075.

## Gate 3: instantiation facts and applicability

### Facts recorded

The extractor now records, on every inherent method of a generic owner,
the impl block's canonical head (`impl_head`), its parameter bounds
(`impl_bounds`) and its `where` predicates (`impl_where`); on every
trait, each recorded impl with its for-type, blanket flag, bounds,
`where` predicates and bound associated types (`impls`); and on every
impl the associated types it binds (`impl_assoc`). Re-extraction of the
four 0072 configurations left every summary byte-identical and every
inventory identical apart from the new fields.

What the facts show for the two owners on 0.55.2:

| owner | inherent methods | impl heads | notes |
|---|---:|---:|---|
| `ChunkedArray` | 207 | 13 | 110 on `ChunkedArray<T>` (82 with `T: PolarsDataType`, 20 with `T: PolarsNumericType`); specialized heads: `ListType` 34, `StructType` 20, `BooleanType` 18, `BinaryType` 7, `StringType` 6, `BinaryOffsetType` 3, `Int64Type` 3, `UInt32Type` 2, one each on `UInt64`, `Float32`, `Float64`, `Int32` |
| `Logical` | 58 | 6 | 17 on `Logical<T, <T as PolarsCategoricalType>::PolarsPhysical>`; 15 on `Logical<K, T>`, 14 of them `where Self: LogicalType`; specialized heads on the four date-time instantiations (26) |

`LogicalType` has no recorded impl under the adapter's feature set, so
every `Self: LogicalType` predicate is unresolved, not proven.

### Applicability

For each (method, alias identity) candidate the generator unifies the
impl head with the identity (repeated parameters must bind
consistently; a concrete head argument must be equal; a projection in
the head is checked against recorded associated types once its
parameter is bound), then evaluates every bound and `where` predicate
under that substitution with three results, `proven`, `rejected`,
`unresolved`, the candidate proven only when all are. Supported forms:
`P: LocalTrait` and `Self: LocalTrait` by a recorded direct impl (a
generic impl head unifies and its own bounds are evaluated, four levels
deep); `P: Trait<Assoc = X>` by the impl's recorded binding; `Sized`
trivially; `Clone`, `Debug`, `Default`, `PartialEq` and the other core
derivable traits by a derive or a recorded impl on the exact type,
else unresolved; `A = B` by resolved equality. Unresolved: auto traits,
traits outside the inventory, blanket impls, a bound on a parameter the
head does not bind, a projection no impl records, any other form. The
0075 `trait_holds` shortcuts are not used here. Rust compilation
remains the second check of every emitted pair (gate 4).

Controls (`polars-gen --self-test`, synthetic inventory): a specialized
head matches only its alias; a generic head with a local-trait bound
proves by recorded implementors and rejects otherwise; the substituted
signature resolves `T::Native` to `i64` and an unrecorded projection is
an exception; repeated parameters reject different arguments and accept
equal ones; `Self: LogicalType` proves only for the recorded impl; a
bound on a parameter the head leaves open is unresolved; an
associated-type equality proves and rejects by the recorded binding; a
trait outside the inventory is unresolved.

### Census, before any binding is emitted

3544 candidate pairs: 207 `ChunkedArray` methods on 16 alias identities
(`IdxCa` and `UInt32Chunked` count once) plus 58 `Logical` methods on 4.
Gross applicability counts every callable; eligible applicability counts
only callables outside 0072's `unsupported` bucket (the 181 `unsafe`
methods and 3 underscore-internal ones are excluded before any entry is
generated, and their pairs say so). Against the plan's no-bounds upper
bound of 2672 for methods without function-level generics:

| family | aliases | proven | rejected | unresolved |
|---|---|---:|---:|---:|
| numeric | 10 | 736 | 842 | 492 |
| string-binary | 3 | 202 | 287 | 132 |
| list-struct | 2 | 167 | 159 | 88 |
| boolean | 1 | 81 | 82 | 44 |
| logical | 4 | 22 | 54 | 156 |
| **gross** | 20 | **1208** | **1424** | **912** |
| **eligible** | 20 | **1024** | **1268** | **844** |

Rejections: a specialized head that is another alias's (1224), a bound
the alias's argument has no recorded impl for (196, `T:
PolarsNumericType` on non-numeric aliases and the like), four
associated-type mismatches. Unresolved: function-level generics, out of
this record's scope (676); a trait argument outside the supported
grammar (58, see the review round below); `Self: LogicalType` with no
recorded impl (56); traits outside the inventory (`num_traits::Float`,
`rand`'s `Distribution`; 42); projections no impl records
(`PolarsPhysical` on the date-time types under `PolarsCategoricalType`,
`ValueT` under `StaticArray`; 80). Every pair carries its result, its
reason and, after emission, its disposition in `surface.json` under
`instantiation.pairs`; the accounting test requires every proven pair
to have exactly one of `emitted`, `refused`, `excluded`, `not shipped`,
`not eligible`, and the emitted ones to equal the instantiation
bindings.

## Gate 4: instantiation bindings, fixtures, comparator, budget

### Bindings

For every proven, eligible pair whose family the release file ships
(all five; `[instantiation] families` is empty), the generator
substitutes the owner's parameters in the method's signature (`T`,
`T::Native`, `T::Physical`, `Self`; an instantiation an alias wrapper
holds exactly, `ChunkedArray<Int64Type>`, is spelled as that alias
everywhere downstream) and emits the binding on the alias wrapper,
`polars::Int64Chunked::…`, with its own oracle information; a canonical
path that carries several impl blocks (`Logical::strftime` on four
heads) yields one callable per block, so every instantiation id names
its receiver. Compilation is the second check: the first attempt failed
on `StructChunked`, whose `StaticArray` impl marks `iter` and
`value_unchecked` with `no_call_const` (polars-arrow-0.55.2
`array/static_array.rs:52,80`), reached by `get`, `first`, `last`,
`first_null`, `first_non_null` and `last_non_null` on `ChunkedArray<T>`;
those six pairs, bisected by compilation, are excluded by the release
file with that citation.

| proven pairs, 1208 | count |
|---|---:|
| emitted bindings (route `instantiation`) | 650 |
| refused by the mapping rules (`impl` returns 125, foreign types 80, unreachable polars types 118, generic types 41, bare slices 19, trait objects 1) | 368 |
| excluded by the release file | 6 |
| not eligible (0072 `unsupported` bucket: `unsafe`, underscore-internal) | 184 |

Emitted per family: numeric 394, string-binary 107, list-struct 86,
boolean 49, logical 14; 104 callables, 88 distinct methods.

### Fixtures

Typed source fixtures on both sides, one per family and one per
`Logical` alias (`fx::series_bool()`, `series_str`, `series_binary`,
`series_binary_offset`, `series_i8` … `series_u64`, `series_f32`,
`series_f64`, `series_list`, `series_struct`, `series_date`,
`series_datetime`, `series_duration`, `series_time`), each naming the
producer it feeds. The producer rule gives a wrapped type a fixture from
a bound `&self` binding without parameters that returns it, on a
receiver whose typed fixture names the producer, else the receiver's
default fixture; a producer on a typed fixture is tried before a
constructor, because a constructor's placeholder arguments may be
invalid for the type (`rand_bernoulli("x", 2, 1.5)` was chosen for
`BooleanChunked` before that ordering). Every `ChunkedArray` and
`Logical` alias now has a recipe (23 producers, 26 constructors, 11
variants). The Rust side's `Default` fixture now requires the generated
`default_` binding, as the Rune side already did: an impl on the generic
base had given Rust an empty array where Rune took the producer, and
211 mismatches, 169 setup mismatches and 25 oracle panics came from that
asymmetry before the rule was aligned.

Fixtures actually usable, measured: under the adapter's feature set a
series cannot be created from `Int8`, `Int16`, `UInt8` or `UInt16`
("cannot create series from Int8"), and a cast to `BinaryOffset`
stays `Binary`, so every case on `Int8Chunked`, `Int16Chunked`,
`UInt8Chunked`, `UInt16Chunked` (36 each) and `BinaryOffsetChunked` (34,
plus the two `arg_min_max` free functions) fails setup on both sides
and is counted as `fixture_failed`, 180 cases in all, never verified.

### Comparator

Every `ChunkedArray` and `Logical` alias wrapper is compared as the
series it converts to (`into_series`, test-support only), through the
structural comparator, which since the review round renders nested
values structurally too: a list cell as the full representation of its
inner series, recursively; a struct series by its fields' names and
full values; scalars as their exact `AnyValue` text (floats at full
precision). Polars's display formatting is not an equality oracle
anywhere. Runner controls: a changed element at position 20 of a
40-element array, a changed inner element at position 20 of a
40-element list cell, a null moved inside a list cell, a changed struct
field value, a moved null, and `0.1 + 0.2` against `0.3` each fail
through the runner; the same long array and the same struct match.
Unit controls in `oracle.rs` cover the same on the representation and
the classifier, plus a renamed struct field.

### Cases

| instantiation bindings | match | both_error | both_panic | fixture_failed | unverified |
|---|---:|---:|---:|---:|---:|
| numeric (394) | 226 | 5 | 8 | 144 | 11 |
| string-binary (107) | 68 | 2 | 0 | 34 | 3 |
| list-struct (86) | 79 | 4 | 1 | 0 | 2 |
| boolean (49) | 46 | 2 | 0 | 0 | 1 |
| logical (14) | 13 | 1 | 0 | 0 | 0 |

Unverified: a return type with no comparison (16) or a parameter without
a fixture (1).

### Budget

`probes/0073/launch.py`, 60 interleaved launches, three measurements
against the 0075 binary built from adcf9c8 in a worktree, samples in
`probes/0076/launch-results-{1,2,3}.json`, the whole record's bindings
included (46 on `Series`, 650 instantiations, 1 inherent):

| measurement | 0075 median ms | 0076 median ms | delta |
|---|---:|---:|---:|
| 1 | 13.08 | 15.27 | +2.19 |
| 2 | 14.85 | 15.95 | +1.10 |
| 3 | 14.40 | 16.58 | +2.18 |

The median of the three deltas is 2.18 ms, under the 5 ms budget, so
every family ships; the release file's `families` stays empty (all),
which is the recipe. Host load moved the absolute numbers between
measurements, as in 0075.

## Gate 5: totals

`probes/0076/reconcile.py <baseline surface> <surface> <baseline
results> <results>` prints both levels for any two surfaces; against
the 0075 surface at adcf9c8 (whose trait methods already bound several
receivers, 1842 bindings, not the 1813 callables):

| level | baseline (0075, adcf9c8) | now | delta |
|---|---:|---:|---:|
| callables generated / adapted / unsupported / out of scope | 1813 / 309 / 2223 / 710 | 1922 / 309 / 2114 / 710 | +109 / 0 / −109 / 0 |
| bindings: inherent | 1037 | 1038 | +1 (`Column::zip_with_same_type`, its `BooleanChunked` argument now mappable) |
| bindings: free / protocol / implementor | 96 / 605 / 104 | 96 / 605 / 104 | 0 |
| bindings: deref | 0 | 46 | +46 |
| bindings: instantiation | 0 | 650 | +650 |
| bindings: total | 1842 | 2539 | +697 |
| oracle cases | 1057 | 1859 | +802 |
| verified (setup failures excluded) | 1057 | 1679 | |
| match / both_error / both_panic / row-order | 1002 / 44 / 11 / 0 | 1562 / 92 / 23 / 2 | |
| fixture_failed (counted apart) | 0 | 180 | |

Newly verified shipped bindings: 47 (`SeriesTrait` on `NullChunked`)
plus 11 from the multi-implementor entries, and `Series::null`. Newly
compiled bindings: 697 (46 deref, 650 instantiation, 1 inherent); of
the 696 on the new routes, 499 approved (470 value matches, 29 error or
panic only), 178 setup failures under the adapter's feature set, 19
unverified for want of a comparison or a parameter fixture. Callable
coverage rose by 109 (4 through the deref route, 105 generic-bucket
callables instantiated). Remaining generic-bucket exceptions: 1218
`unsupported: bucket` entries, of which the 3544-pair census accounts
for the two owners' 265 methods pair by pair.

Unchanged and passing: drift, accounting with the binding-level and
pair-level reconciliation and the deref controls, the lib controls
single-threaded, clippy with and without test-support, the hand-written
and presentation suites (26 lib controls after the review rounds).
Adjacent 0.54.4 with `0.54.4.toml`: generated 1868, `cargo check` ok. rc2, experimental, with `rc2.toml`: every stage
passes; generated 2092, match 1628, both_error 100, both_panic 25,
fixture_failed 176, row-order 6 under the unordered policy (the unique
and join cases flap between match and row-order, as in 0075);
`probes/0074/frozen-experimental.json` refrozen.

## Codex review round 1 (4fdac13): three corrections

- **R1, nested values.** `series_repr` rendered every cell as its
  `AnyValue` debug text, and a list cell's debug text is the inner
  series' display, which elides middle values: two one-row `List(Int64)`
  series differing at inner index 20 compared equal. The comparator now
  renders nested values structurally (list cells by their inner series,
  recursively; struct series by field name and value; the same for a
  frame's cells), and the controls above prove the changed inner
  element, the moved inner null and the changed struct field each fail.
  Cases whose value category the comparator does not cover are marked
  unverified, never matched.
- **R2, applicability.** `holds` proved every core derivable trait for
  every scalar, so `f64: Eq` was proven, and `split_bound` kept only
  associated-type equalities, so a generic trait argument
  (`PolarsNumericType<unmodeled::Argument>`) was silently erased. Core
  traits on builtins now follow a per-type table (floats have no `Eq`,
  `Ord`, `Hash`; strings are not `Copy`; unit has no `Display`); any
  bound outside the supported grammar (a generic trait argument, a
  parenthesized signature, a core trait with constraints) is unresolved
  before the trait is looked at. Self-test controls: `f64: Eq` and
  `f64: Ord` rejected, `f64: PartialEq` and `i64: Eq` proven, `String:
  Copy` rejected, the unmodelled trait argument and an `Fn` bound
  unresolved. The census was recomputed: 48 pairs lost a false proof
  (proven 1256 to 1208), and the emitted set went from 682 to 650.
- **R3, reconciliation.** The 184 proven pairs without a disposition
  belonged to callables in 0072's `unsupported` bucket (181 `unsafe`, 3
  underscore-internal), excluded before entry generation; every pair now
  records a disposition, the summary separates gross from eligible
  applicability, and the accounting test asserts the partition over
  every proven pair. The baseline binding count is 1842, not the 1813
  callable count; the tables above come from the re-runnable
  reconciliation, which also shows the one extra inherent binding.

## Codex review round 2 (e3ca795): two corrections

- **R1, struct nullness.** The struct branch of `series_repr` rendered
  length, dtype and fields but not the struct's own validity, so a null
  struct equalled a valid struct whose fields are null. The
  representation now carries the parent validity per row
  (`rows=[valid, null, …]`, from `is_null` on the struct series) apart
  from the fields' nulls, at every nesting depth, since a list cell
  renders its inner series through the same function. Controls: unit,
  a valid struct with a null field against a null struct of the same
  dtype (`null_count` 0 and 1) is a Mismatch, a moved outer null is
  seen, and both distinctions hold inside a list cell; runner, a null
  struct against the valid one, the same nested in a list cell, and the
  same struct with a null field matching.
- **R2, unsized `str`.** The builtin table grouped `str` with the owned
  string, and `Sized` was proven for every type. Now `str` is neither
  `Clone`, `Default` nor `Sized` (it keeps `Debug`, `PartialEq`, `Eq`,
  `Ord`, `Hash`, `Display`); a `&T` is `Copy`, `Clone` and `Sized`, a
  `&mut T` is `Sized` but not `Clone`; `Sized` is proven only where
  sizedness is established (references, scalars, unit, the owned string,
  `PlSmallStr`, reachable structs, enums and unions bare or
  instantiated, aliases of those), rejected for `str`, slices and trait
  objects, and unresolved for anything else. Self-test controls cover
  each of those. The census did not change (no pair depended on `str:
  Clone`, `str: Default` or an unestablished `Sized`), so the emitted
  set, the tally and the launch numbers stand.
- Also fixed: the two harness tests share the engine's process-global
  counters, and the zero-call controls could observe the main test's
  engine thread when the tests ran in parallel; the tests now serialize
  on a mutex, so the counter controls hold under either test-thread
  setting.
