# rnx 0077: Polars iterator returns

Status: revised after Codex's plan review (reviews/0077_review_codex.md),
ready for impl.

## Problem

Record 0076 left 368 proven (method, alias) pairs that the mapping rules
refuse; the largest class, 125 pairs on 15 methods, returns an
`impl Iterator`. The same shape exists at callable level: 39 eligible
callables of the API crates return an iterator, all of them in 0072's
generic bucket under rule T3 (a return that is `impl Trait` or a
non-static borrow), 25 on generic owners (`ChunkedArray` 16, `Schema` 7,
`Logical` 1, `BatchedWriter` 1) and 14 on concrete ones (`DataFrame` 4,
`Container` 2, `GroupsIdx`, `StatisticsFlags`, `SchemaExt`,
`SchemaNamesAndDtypes`, `ScanFlags`, `PartitionedSinkOptionsIR`,
`TimeUnitSet`, `OptFlags`, one each). None
has function-level generics. Their items, counted from the inventory:
`Option<Series>` 3, `Option<&str>` 3, `Series` 1, `&Series` 1,
`DataFrame` 1, `T::Native` and `&[T::Native]`, `T::Physical` and
`Option<T::Physical>`, `usize`, `(&PlSmallStr, &DataType)`, and, not
mappable, arrow-internal `ArrayBox`, `RecordBatch`, `Bitmap`,
`PrimitiveArray`, `BinaryViewArray` and `bitflags::Iter`. Eighteen of
the 39 declare a length (`ExactSizeIterator`, `TrustedLen`,
`PolarsIterator`); 21 do not.

The 180 setup failures of 0076 are a separate matter. A probe under
today's feature set (`probes/0077/dtype-probe`, source, command and
`result.txt` archived; `polars =0.55.2`, `lazy`, `csv`, `parquet`) shows that no route builds an `Int8`, `Int16`,
`UInt8` or `UInt16` series: `cast` errors ("cannot create series from
Int8"), `full_null` and `new_empty` panic ("not implemented for dtype
Int8"), and `from_any_values_and_dtype` refuses. Those four need the
`dtype-i8`, `dtype-i16`, `dtype-u8`, `dtype-u16` features, a cost
decision this record does not take. `BinaryOffset`, by contrast, is
built by `from_any_values_and_dtype` with `AnyValue::Binary` (and by
`full_null`), while `cast` keeps `Binary`; that is a fixture defect in
0076, fixed here.

## Decision

An iterator return becomes a Rune vector, materialized inside the
binding under an explicit bound, with the consumption semantics stated
in the catalogue. Every number is reported against the 0076 baseline
(a7434f8: 1922 generated callables, 2539 bindings, 1859 cases, 1679
verified) with newly compiled bindings and newly verified cases apart.

### 1. The materialization rule

A return type `impl B<Item = X>` where `B` names `Iterator`,
`DoubleEndedIterator`, `ExactSizeIterator`, `PolarsIterator` or
`TrustedLen` (with any lifetime and auto-trait additions), possibly
inside `Option` or `PolarsResult`, maps when `X` maps as a vector
element under the existing rules for returns: a scalar, a string, an
`Option` of those, a wrapped type (by value or by reference, cloned out
under the 0073 policy), or a tuple of those. The binding drives the
Rust iterator to exhaustion inside the call and returns `Vec<X'>`; it
never hands a lazy iterator to the script. An item that does not map
leaves the callable unsupported with the reason `iterator item`, so
the arrow-internal items stay counted.

**Bound.** Materialization is bounded by an item count, `materialize_limit`
(L = 1 048 576), recorded in `surface.json`; it is not a byte, allocation
or time bound. The contract is inclusive: exactly L items succeed; L + 1
refuse. For an iterator whose length is known, and only
`ExactSizeIterator` (`len()`) or `TrustedLen` (an exact `size_hint`
upper bound by the trait's contract) count as known, the length is
compared with L before the first item is taken, and a longer iterator
refuses without a `next` call. For any other iterator no `size_hint` is
trusted as a length: items are taken one at a time, at most L are
retained and converted, and detecting excess takes one more `next`,
whose item is discarded without conversion; a `Some` there refuses.
The count guard stays in place after a preflight too, and no capacity
is ever reserved from an unchecked hint. A refusal is a `polars::Error`
of kind `MaterializeLimit` naming the method and L; no prefix is
returned. One helper implements the rule, parameterized by the bound,
so the low-bound controls exercise the production code path; the
default-feature path is checked at the production bound as well.

**Consumption.** A `&self` iterator borrows the receiver; the binding
materializes and the Rune value stays usable, which the second-call
check verifies. A consuming `self` iterator follows the existing clone
policy. A `&mut self` iterator is not mapped (reason `mutable iterator
receiver`), since the receiver's state after partial consumption is not
something a materializing binding can express. Iterator *arguments*
(`impl IntoIterator`) are out of this record.

**Engine routing.** Today's emitted shape routes only the callee
through `engine::run` and converts the result outside it; an iterator
is lazy, so that shape would either return a borrow of an engine-local
receiver or drive `next` on the caller's thread. For a routed binding
(the existing policy decides which: data-type owners and the named
prefixes; a scalar iterator on a plan type is not routed) the iterator
is created, driven, bound-checked and its borrowed elements detached
into owned values inside the routed closure, while the owner it borrows
stays alive there; only owned, transferable results leave the closure,
and any Rune-only conversion follows outside. An unrouted binding does
the same on the caller's thread. On refusal or a conversion failure the
iterator and its owner are dropped inside the closure and the receiver
remains usable.

### 2. Where the rule applies

The callables are in 0072's generic bucket by rule T3, so, as 0076 did
for instantiations, the generator handles them from that bucket
without changing the extractor's classification: an eligible callable
whose generics are only the T3 return is emitted through the normal
route (inherent, deref, instantiation), and `surface.json` records under
`iterators` every candidate with its disposition: mapped, item not
mappable (with the item), mutable receiver, or refused by another rule.
For the generic owners the 0076 census already carries the pairs; the
mapping rule turns proven pairs refused for `impl return` into bindings
or into `iterator item` refusals.

### 3. Oracle

The existing vector formatting joins element text with a comma and a
space and is not injective: `["a, b", "c"]` and `["a", "b, c"]` render
alike, and options and tuples compose the same way. Materialized
results are therefore compared through a structured representation:
a `Repr::Seq` of element representations, recursively, that keeps the
vector's length and order, an option as `None` or `Some(repr)`, a tuple
as a fixed-arity sequence, a string with its boundaries (length-framed
text), and a series or frame element through the accepted structural
comparator. Both sides produce the same structure and the classifier
compares structures, never a joined string. A case whose element type
has no comparator is unverified with that reason; two sides sharing a
lossy formatter never certify a binding.

### 4. The `BinaryOffset` fixture

The typed source fixture `series_binary_offset` is built with
`from_any_values_and_dtype` instead of `cast`, on both sides; the
`BinaryOffsetChunked` cases (34) and the two `arg_min_max` free
functions then verify instead of failing setup. The four narrow integer
aliases keep their setup failures, counted apart, with the probe's
finding recorded; enabling their dtypes is a separate record.

## Gates

1. **Census.** Every iterator-returning callable and pair with its item
   type and disposition under the rule, before any emission; the
   `materialize_limit` recorded. Controls: the return-type recognizer
   accepts each bound spelling the inventory shows and rejects
   `impl Trait` returns that are not iterators (`impl Display`).
2. **Rule and controls.** The mapping, the bound helper and the routed
   materialization in the generator and `support.rs`. Controls through
   the generated runner, with the helper at a low bound: empty, L − 1,
   L (succeeds), L + 1 (refuses, `MaterializeLimit`, no prefix
   returned, exactly L + 1 `next` calls and L conversions counted),
   an unbounded unknown-length iterator (refuses at L + 1 `next`
   calls), a non-exact `size_hint` that overstates (not treated as a
   length: the items are driven and counted), a known length over the
   limit (refuses with zero `next` calls); receiver reuse after success
   and after refusal; the production bound on the default-feature path.
   Routing controls: a routed borrowed iterator whose `next` runs on the
   engine thread (thread-identity check) under an entered Tokio
   runtime; a consuming-`self` iterator; cleanup and receiver reuse
   after overflow and after a conversion failure. Oracle controls: the
   equal-length string collision `["a, b", "c"]` against `["a", "b, c"]`
   is a mismatch; a changed element, reordered elements, `Some` against
   `None`, and a changed cell in a nested series or frame element each
   fail; an item that does not map leaves the callable unsupported with
   the item named.
3. **Emission.** Bindings on concrete owners, then instantiations,
   compiled as the second check; newly compiled and newly verified
   counted apart, refusals by item.
4. **BinaryOffset.** The fixture built by `from_any_values_and_dtype`;
   the 36 cases verified or their outcomes listed; the four narrow
   integer aliases' failures unchanged and the probe recorded.
5. **Evidence.** Totals against a7434f8 from `probes/0076/reconcile.py`,
   cold launch before and after (three 60-run measurements), the
   adjacent 0.54.4 check and the rc2 experimental run, README updates.

| number | how it is measured |
|---|---|
| iterator candidates | callables and pairs with an iterator return, by owner and item |
| newly compiled | bindings by route whose return is materialized |
| newly verified | approved cases on those bindings, setup failures apart |
| refused | by item type, mutable receiver, other rule |
| bound controls | empty, L − 1, L, L + 1, unknown-length overflow, non-exact hint, known-over-limit; `next` and conversion counts; receiver reuse |
| routing controls | routed `next` on the engine thread under a Tokio runtime; cleanup on refusal |
| oracle controls | structural sequence comparison: the string collision, changed and reordered elements, option distinction, nested cell |
| BinaryOffset | cases verified after the fixture fix, against 36 failures |
| cost | launch medians before and after |

## Out of scope

Feature expansion for `Int8`, `Int16`, `UInt8`, `UInt16` (a cost
decision of its own); lazy iterator handles in Rune; iterator
arguments; function-level generics; the other 0076 refusal classes
(unreachable polars types, foreign types, bare slices).
