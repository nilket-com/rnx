# rnx 0076: Polars coverage, stage three

Status: revised after Codex's plan review (reviews/0076_review_codex.md),
ready for impl.

## Problem

Record 0075 closed with 1813 generated bindings and 1057 verified cases
on Polars 0.55.2. Two pools remain that no rule of the generator reaches,
and Codex named them in this order:

1. **51 shipped bindings nobody can call.** The 74 `SeriesTrait` methods
   have one public implementor in the inventory, `NullChunked` (the
   `SeriesWrap<…>` implementors are private), so the generator bound 51 of
   them on `polars::NullChunked`. That type has no constructor binding
   (`new` is not public), no `Default`, and no recipe, so the 51 are
   compiled and unverified, and a script cannot obtain a receiver for
   them. The route Rust users take, `series.method()` through `Series:
   Deref<Target = dyn SeriesTrait>`, is a `Deref` impl in the generic
   bucket (rule F1) and produces nothing.
2. **265 methods on generic owners, reachable only through aliases.** The
   207 inherent methods of `ChunkedArray<T>` and the 58 of `Logical<K, T>`
   are in the generic bucket under rule O2, "instantiable per alias".
   Record 0075 wrapped the 16 `ChunkedArray` aliases and 4 `Logical`
   aliases as types, so their values now flow through bindings, but no
   method is bound on them. The inventory lists every alias on every
   method (16 on `ChunkedArray::mean`, which needs `T: PolarsNumericType`)
   because the extractor does not record inherent impl heads or their
   `where` predicates; the 2672 (method, alias) pairs it implies are an
   upper bound, not a count.

Both are measured against the same 0.55.2 inventory, with exceptions
counted and no dependency upgrade in the record.

## Decision

Two gains, reported apart: first verify what already ships, then expand
what compiles. Every number is against the 0075 baseline (adcf9c8: 1813
generated, 1057 verified) and the 0.55.2 inventory. The accounting for
one callable bound on several receivers is defined first (section 3),
because both gains produce that.

### 1. A receiver for `SeriesTrait`: the null series, then `Series` itself

Two routes, in order, each measured on its own.

**Core fixture.** `NullChunked` gets a hand-written core fixture on both
sides, the same rule the frame, series, column and lazy-frame fixtures
use since 0073: Rust `Series::new_null("x".into(), 2).null().unwrap().
clone()` (`Series::null` returns `&NullChunked`, `NullChunked: Clone`),
Rune `fx::null_chunked()`, and a comparator rendering name, dtype and
length, which is the whole logical content of an all-null array. That
verifies the 51 shipped bindings through the null implementation of
every method, and gives `Series::null` (generated, "return type has no
comparison") its comparison. Methods whose null implementation panics or
errors verify that behaviour, as `both_panic`/`both_error`, counted
apart; they are not evidence that the binding works on data.

**Deref target.** The extractor records, for every `Deref` impl on a
reachable type, the canonical `Target`; the generator gains one rule: a
trait method of a trait that is the `Deref` target of a wrapped concrete
type (`Series` derefs to `dyn SeriesTrait`) is bound on that type, the
call spelled through the deref (`(&*recv.0).method(args)`), exactly what
Rust's autoderef does for `series.method()`. The route preserves the
receiver form: `&self` methods go through `Deref`; `&mut self` methods
need `DerefMut`, which `Series` does not implement in the inventory, so
they are exceptions with that reason on this route (they remain bound
on `NullChunked`); a consuming `self` cannot move out of a `dyn` target
and is an exception. Name collisions keep the existing order, inherent
first: for the 10 `SeriesTrait` names that `Series` has inherently, the
inherent binding is retained and the trait route is listed as "not
separately exposed, inherent `Series::name` retained", which claims
nothing about the two operations being equivalent. These bindings are
newly compiled, take `fx::series()` as receiver, and verify against the
real data implementation. The `NullChunked` bindings stay; both are
accounted per section 3.

Dispositions to expect and count: the 12 `SeriesTrait` methods in the
generic bucket stay out (`as_any`, iterators, `dyn` returns); the 6
"on every implementor" exceptions are re-evaluated on `Series` and
reported per reason.

### 2. Concrete instantiations of `ChunkedArray` and `Logical`

**Facts first.** The extractor records, for every inherent impl block on
a generic owner, the canonical impl head (`for` type, which may be a
specialized head such as `ChunkedArray<BooleanType>`), the impl's type
parameters with their bounds, and every `where` predicate in canonical
form, including `Self: LogicalType` and associated-type equalities, on
each method of the block as `impl_head` and `impl_where`. For trait
impls on local traits it records the canonical for-type of each impl
(today the implementor list of `LogicalType` is empty because its impls
are on instantiations) and the associated types the impl binds (`impl
PolarsDataType for Int64Type { type Native = i64; … }`) as `assoc_types`.
Re-extraction changes nothing else; the summaries stay byte-identical,
as in 0075.

**Applicability.** For each (method, alias) candidate the generator
unifies the alias's identity (the 0075 expanded target, `ChunkedArray<
BooleanType>`) with the method's impl head, producing a substitution or
a rejection (a specialized head that does not match). Under that
substitution every parameter bound and `where` predicate gets one of
three results, and the candidate's result is proven only when all are
proven:

- proven: `P: LocalTrait` when the recorded implementors of that trait
  contain the substituted type by a direct impl; `Self: LocalTrait`
  likewise for the substituted `Self`; `P: Trait<Assoc = X>` when the
  recorded `assoc_types` of that impl bind `Assoc` to `X`; `P: Clone`,
  `Debug`, `Default`, `PartialEq` by the 0075 derive-or-impl facts; `P:
  Sized` for any concrete type;
- rejected: a direct-impl list that is complete for the trait and does
  not contain the type; a recorded associated type that differs;
- unresolved: a blanket impl, a bound on a private or foreign trait not
  covered above, `Send`/`Sync` and other auto traits, a bound involving
  another parameter that the substitution leaves open, a projection the
  inventory does not resolve, any predicate form outside the list.

Unresolved is a counted exception, never an instantiable pair. The
0075 `trait_holds` function is not the proof engine; its marker-bound
shortcuts are replaced by the list above, and the 0075 alias wrappers
are re-derived through the same evaluation so their `Clone`/`Debug`
facts and this record's rest on one rule. Rust compilation is the second
check of every emitted pair: a proven pair that does not compile fails
the build, and the evidence says so.

**Rule.** For a proven pair on a method without function-level generics,
the generator emits one binding with the owner's parameters substituted:
`T` by the alias's argument, `T::Native` and other projections by the
recorded associated type, `Self` by the alias. A substituted signature
that still contains a generic, an unresolved projection, or a type the
mapping rules refuse is an exception with that reason. Rune paths are the
alias's: `polars::Int64Chunked::mean`. `IdxCa` and `UInt32Chunked` share
one wrapper and one identity, so they get one binding set, accounted
once. Names that collide with an inherent or `SeriesTrait` binding on
the same wrapper follow the order of section 1.

**Scope and cost.** The instantiable pair count is unknown until the
facts are recorded; 2672 is the no-bounds upper bound. Registration cost
is Rune's context build, about 7 ms per 1900 functions on the 0073 host
measurement. This record sets a budget: the whole record, gate 2's
`Series` bindings included, may add at most 5 ms to the median cold
launch against the 0075 binary (adcf9c8), measured the 0073 way,
interleaved, 60 runs at idle, three times, the decision taken on the
median of the three. If the full set exceeds it, families ship in this
order until the budget is met, each family whole or not at all: numeric
(aliases whose argument implements `PolarsNumericType`, 10), boolean,
string and binary, list and struct, then `Logical` (4 aliases). A method
whose impl head is unbounded belongs to every alias's family and ships
with each family that ships. Equivalent aliases count once. The evidence
gives the pair count per family, the count shipped, the launch numbers
of every measurement, and the final shipped set with the recipe that
selected it, so that a later record revisits the budget with numbers.

**Fixtures.** The 0075 recipes give none of the `ChunkedArray` aliases a
fixture, since their constructors are on the generic type. This record
adds two things. Typed source fixtures: hand-written core series fixtures
per family (`fx::series_bool()`, `fx::series_str()`, `fx::series_f64()`,
`fx::series_list()`, and one per `Logical` alias), small and
deterministic, on both sides. A producer rule: a wrapped type gains a
fixture from a bound binding elsewhere that returns it, whose receiver
and arguments have fixtures of the right type (`BooleanChunked` from
`Series::bool` on `fx::series_bool()`, not on the `Int64` fixture, which
would fail setup), chosen deterministically (shortest canonical path,
then alphabetical, receiver fixture by declaration order), derived as a
fixpoint from existing fixtures only so it terminates, and recorded like
the 0075 recipes (`fixture via producer Series::bool on series_bool`).
Producer construction runs in the 0075 staged setup; a producer whose
setup fails on both sides is a counted setup failure, and the evidence
reports the fixtures actually usable per alias, not the producers that
exist.

**Comparison.** Polars's `Debug` for arrays goes through
`format_array!`, which truncates rows and formats values for display,
so `Debug` availability proves nothing about two arrays differing. No
array is counted as value-verified through `Debug`. Every alias wrapper
gets a test-support comparator that converts the array to a `Series`
(`into_series()`, no script-facing API) and renders it through the 0073
structural comparator: name, dtype, length, every element in order,
nulls included. Where the conversion does not compile for an alias, that
alias's cases are reported as unverified with that reason, never as
matches. Mutation and second-call checks stay. Negative controls: a
changed middle element outside `Debug`'s displayed window, a moved null,
and two float values that display alike (`0.1 + 0.2` against `0.3`) must
each fail through the runner.

### 3. Accounting at two levels

One inventory callable may now yield several bindings: a `SeriesTrait`
method on `NullChunked` and on `Series`; a `ChunkedArray` method on
each of 16 aliases. Two levels are defined and both are reported.

- **Callable coverage**, against the unchanged 0.55.2 denominator: one
  entry per inventory callable, whose status is generated when at least
  one binding exists, with the same statuses as before. The 1813 and
  1057 baselines are at this level and stay comparable.
- **Bindings**, keyed by binding identity: callable key, canonical
  receiver instantiation (the 0075 identity, so equivalent aliases share
  one), and route (`inherent`, `deref`, `instantiation`, `implementor`).
  Each binding has exactly one disposition: a case (with a case id
  derived from the binding identity, never from the path alone) or an
  unverified reason. `surface.json` lists bindings under their entry;
  `oracle-results.json` is keyed by case id; `probes/0074/report.py`
  keys its tables by binding identity, not by path.

Reconciliation tests, in the adapter: every candidate receiver or
(method, alias) pair has exactly one applicability result and one
disposition; every emitted binding has exactly one case-or-unverified
disposition; no two cases share an id; no path-keyed collection in the
runner or the reports; a `SeriesTrait` method bound on two receivers
yields two results that survive collection independently and are both
in the tally.

### 4. Exceptions counted, upgrades excluded

No dependency moves. The rc2 experimental run is repeated with the
final generator under its own manifest, as in 0075, and reported apart.
Every entry the new rules touch and do not generate carries a reason,
and the evidence tabulates them against the 0.55.2 inventory.

## Gates

1. **Accounting and null-series fixture.** Section 3's two levels in
   the generator, surface, runner and reports, with the reconciliation
   tests, proven on today's surface first (identical callable counts,
   each existing case id preserved or mapped). Then the core fixture on
   both sides with its comparator; the 51 bindings verified;
   `Series::null` verified; the tally by outcome, `both_panic`/
   `both_error` listed by method. Controls: the comparator distinguishes
   name, dtype and length; a wrong length fails through the runner.
2. **Deref-target rule.** Extractor `Deref` targets (re-extraction
   byte-identical apart from the new field); the rule with receiver-form
   handling and the collision order; bindings on `Series` newly compiled
   and newly verified, counted apart from gate 1; the exceptions
   re-listed by reason. Controls: a `Deref` to a non-trait target binds
   nothing; a `&mut self` method is an exception on a type without
   `DerefMut`; a name the wrapper has inherently is retained and the
   trait route listed as not separately exposed; a `SeriesTrait` method
   that panics on the null series and succeeds on data verifies both,
   under two bindings that both survive collection.
3. **Instantiation facts and applicability.** Extractor impl heads,
   `where` predicates, parameter bounds, local-trait impl for-types and
   `assoc_types`; the applicability evaluation with its three results;
   the pair count per family (proven / rejected / unresolved) against
   the 2672 upper bound, before any binding is emitted. Controls, from
   synthetic inventories: a specialized impl head matches only its
   alias; repeated type parameters (`Logical<T, T>` against `Logical<A,
   B>`) reject; a `Self: LocalTrait` bound proves only for recorded
   implementors; a bound on another parameter left open is unresolved; an
   associated-type equality proves and rejects by the recorded binding;
   an unresolved projection is an exception.
4. **Instantiation bindings, comparator, budget.** Bindings emitted for
   proven pairs; the typed source fixtures and producer fixtures with
   their staged setup; the `into_series` comparator per alias and its
   negative controls; cold launch measured as section 2 says, families
   shipped under the budget, the shipped set recorded. Controls: an
   emitted pair that does not compile fails the build; `IdxCa` and
   `UInt32Chunked` produce one set; a rejected pair produces no binding.
5. **Evidence.** Totals against the 0075 baseline at both levels: newly
   verified shipped bindings (gates 1 and 2), newly compiled (gates 2
   and 4), newly verified new bindings, error/panic-only verifications,
   exceptions by reason, the unchanged suites and controls, the adjacent
   0.54.4 check, the rc2 experimental run, and the README updates.

| number | how it is measured |
|---|---|
| callable coverage | entries generated / verified against the 0.55.2 denominator, comparable with 1813 / 1057 |
| newly verified, shipped bindings | approved cases on bindings that existed before this record, by gate |
| newly compiled bindings | emitted bindings by route, by gate |
| newly verified, new bindings | approved cases on bindings this record emits, setup failures excluded and counted |
| value-verified vs error/panic-only | approved matches on structural comparators, apart from `both_error`/`both_panic` |
| instantiable pairs | proven / rejected / unresolved per family, against the 2672 upper bound |
| shipped under budget | pairs emitted per family, every launch measurement, the selection recipe |
| exceptions | unsupported and unverified bindings by reason, against the 0.55.2 inventory |

## Out of scope

Dependency upgrades; `SeriesWrap` internals; a script-facing `Series`
conversion for arrays; the generic bucket beyond inherent methods of
`ChunkedArray` and `Logical` (trait impls on them, other generic owners,
function-level generics); `DerefMut` routes; the engine-thread test race
noted in 0075.
