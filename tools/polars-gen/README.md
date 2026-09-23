# polars-gen

Generates the Polars adapter's Rune bindings from the record 0072
inventory (`probes/0072/out/<release>-adapter/result/inventory.json`).

    cargo run --manifest-path tools/polars-gen/Cargo.toml -- \
        probes/0072/out/0.55.2-adapter-narrow/result/inventory.json adapters/polars \
        --release tools/polars-gen/releases/0.55.2-joins.toml \
        --buckets mechanical,conversion,option_struct,callback,generic_fn

`--release` is required and names a file in `releases/`: the release's
API-crate list, its `[provenance]` (the crates.io `release` or Git `rev`
the inventory must record, or the generator refuses to run; since record
0081 also the documentation configuration `cfg` and the Polars
`features` the inventory must have resolved, so a feature change cannot
be reported against an inventory that predates it), the
operations whose row order is unspecified (by canonical path, with
`args` giving the exact fixture expression per parameter or `*` for one
that cannot affect order, and a citation), and the oracle exclusions. `0.55.2.toml` holds the record 0073 constants,
`0.55.2-joins.toml` adds the join-order policy and is the shipped one,
`0.54.4.toml` serves the adjacent check and `rc2.toml` the experimental
0074 run. `surface.json` records the file's name, source and SHA-256.
`--self-test` runs the generator's own controls: the order policy, and
the wrapper identity rules (equivalent aliases share one wrapper,
same-name distinct types get distinct Rune paths).

Record 0082 admits immutable borrowed slices: a `&[T]` return, an
iterator item `&[T]`, or a callback argument (`&[u8]`, `Option<&[u8]>`)
whose element has a script conversion is copied into an owned Rune
vector by `support::copy_slice` before the binding returns or the
callback is bridged, so the script owns the result and nothing borrows
Polars storage. The copy is bounded by the materialize limit, counted
cumulatively over every slice one binding copies (nested slices, every
item of one iterator), checked before allocation, and refused as a
`MaterializeLimit` error naming the operation; inside a callback the
refusal is the callback's typed failure. A mutable slice, an Arrow
array or bitmap element, and a slice of iterators stay refused with
their reasons in the census.

Record 0084 infers function-level generics from script values. A
generic that appears only in another generic's bound is inferred with
it (`I: IntoIterator<Item = S>, S: AsRef<str>` becomes `Vec<String>`;
`E: AsRef<[IE]>, IE: Into<Expr>` becomes `Vec<Expr>`), and a vector
element that would map to `&str` is carried as an owned `String`. A
bare `Iterator`, `ExactSizeIterator`, `DoubleEndedIterator`,
`TrustedLen` or `PolarsIterator` bound with an item type is lowered
from a script vector: owned items through `Vec::into_iter()`, `&str`,
`&[u8]` and their `Option` forms through a temporary the binding holds
for the whole call. The `generic_fn` bucket token admits `generic`-bucket
callables that carry function generics without opening the bucket to its
other members. `Into<(…)>` tuple conversions, return-only generics,
closure bounds and the release file's `[[refused]]` paths (each with the
source contract a binding would have to validate) stay refused.

Record 0085 maps a validity `Bitmap` return, direct, optional or as
iterator items, to an owned `Vec<bool>` through `support::copy_bits`,
which shares the cumulative materialize bound with 0082's slice copies
(one `SliceBudget` guard spans a whole iterator). The rule applies only to
the canonical paths in the release file's `bitmap_returns` list
(`ChunkedArray::rechunk_validity`, `ChunkedArray::iter_validities`); every
other `Bitmap`, every bitmap input and every Arrow array stays refused.

Record 0086 adds the input side for the paths in the release file's
`[[bitmap_inputs]]` entries, each with a `length` rule (`receiver`,
`values` or `none`) and a source citation: the one `Bitmap` parameter is a
script vector turned into a bitmap by `support::bitmap_from_bools`, which
reserves the bits from the materialize bound, then compares the length with
`this.0.len()` or the `values` vector (both taken in `pre`, before any
receiver borrow) and returns `ShapeMismatch` before Polars is called. The
oracle's mask fixtures are sized by shape (`mask3`, `mask1`). Struct
receivers are excluded by `[[instantiation.exclude]]`.

Record 0087's `[[iterator_returns]]` entries (path, integer `item`,
citation) let a listed callable's concrete iterator return, which rustdoc
records as a `Map` behind an alias such as `ChunkLenIter`, materialize as an
exact-size iterator inside the call: `support::materialize_exact` refuses an
over-bound count before consuming anything, and `support::widen` converts
each item with a range check instead of `as i64`. The oracle frames the
Rust side by the listed return type. Any other `Map`, and every Arrow array
return, stays refused.

Record 0088's `[[cow_returns]]` entries (inventory `key`, `path`,
citation; the key because `to_physical_repr` shares its canonical path
across implementations) admit a `Cow<Self>` or `Cow<Wrapped>` return whose
inner type is a wrapped, `Clone` type: the result is made owned with
`into_owned()` inside the call, and inside the engine closure when the
binding is routed, then wrapped. Any other `Cow`, and a `Cow` of an
unwrapped or Arrow type, stays refused.

Record 0089's `[[free_instantiations]]` entries (inventory `key`, `path`,
public `callee`, the `generic` name, its concrete `types`, citation)
instantiate a generic free function once per listed type: `ChunkedArray<T>`
in its parameters becomes the wrapper that holds that type, the binding is a
static function on the wrapper that borrows its argument and calls the
Polars function by its public path (Rust infers `T`), and a `usize` result
converts through `support::widen`. A listed type no wrapper holds is a named
exception; every other generic free function keeps its refusal.

Record 0090 adds `natives` to those entries: the native scalar of each
listed type, in order, which replaces `T::Native` in the parameters.
Both substitutions (`ChunkedArray<T>` and `T::Native`) happen only where
the spelling stands as a whole token (`replace_token`), so a path or name
that merely contains it is left alone; any generic or `::Native` left in a
parameter afterwards makes that type a named exception. The substituted
scalars then map through the ordinary argument rules (checked narrowing
for narrower integers, `as f32` for `f32`).

Record 0091 lets a `[[free_instantiations]]` entry name a
`guard_param` and a `guard`; the only guard is `below_idx_max`. While that
entry is emitted, the named `usize` parameter is converted and checked in
the binding's `pre` statements (`support::below_idx_max`), so an invalid
value is an `OutOfBounds` error before the call; other parameters, and other
entries, are unaffected, and the scope clears after the instantiation.

Record 0092's `[[method_scalar_generics]]` entries (inventory `key`,
`path`, the function `generic`, owner `types` and their `natives`,
citation) let a family-census method whose only function generic is a
scalar parameter (`lhs_sub<N: Num + NumCast>`) be proved like any other
method: the census no longer marks its pairs unresolved, applicability
decides them, and each proven pair binds `N` to that pair's native type by
position. The entry fails closed (`MethodScalarGeneric::check`): exactly one
function generic, used only as a whole parameter type and never in the
return, and paired `types`/`natives`; a proven pair whose type is not
listed is refused by name.

Record 0093 checks every integer read-back. `World::ret` converts `u64`,
`usize`, `isize`, `i128` and `u128` with `support::widen` (fallible, named by
the method) on every return route, and a callback argument of those types
fails the callback through `support::callback::unwind` instead of wrapping.
Only an exact `[[bounded_readbacks]]` entry (inventory `key`, canonical
`path`, source citation) keeps a plain `usize` read-back, through
`support::bounded_usize`, which a compile-time `usize` width assertion and a
debug assertion guard. A name alone never qualifies. The oracle's Rust side
formats those types with the same check, so a wrapping binding is a
mismatch. `probes/0093/census.py` classifies every read-back as checked or
bounded, with its route.

Record 0094's `[[hash_tokens]]` entries (inventory `key`, canonical
`path`, `direction` `return` or `parameter`, the parameter's name, the
`source` type `u64` or `Option<u64>`, citation) carry a categorical hash as
an exact 16-digit lowercase hex token. A return goes through
`support::hash_token`. A parameter is parsed by `support::hash_from_token`
before the call and passes the bits to Polars unchanged.
`hash_token_scope` checks every matching entry before any binding text is
emitted. A malformed entry refuses the callable. That covers a type
mismatch, a missing or wrong parameter, a missing citation, an unknown
direction, or a return entry that names a parameter. An unlisted key or
path keeps the checked integer rule. The oracle formats a listed return as
hex and passes the Rust side's `2u64` as the token `"0000000000000002"`.
The categorical receiver fixtures live in `support::categorical_fixtures`.

Record 0095 adds `lhs_div` and `lhs_rem` as two more
`[[method_scalar_generics]]` entries on the same ten types and natives. No
generator rule changed. Their zero, null, `MIN / -1` and float behaviour was
probed on every type in debug and release (`probes/0095`).

Record 0096's `[[null_aware_returns]]` entries (inventory `key`,
canonical `path`, the owner `types`, citation) let a family method's exact
`Either<Vec<T::Native>, Vec<Option<T::Native>>>` return bind on the listed
types. `NullAwareReturn::check` fails closed on a missing citation, an
empty list, a nonnumeric type or a duplicate. An unlisted proven pair is
refused by name. So is a pair whose whole substituted return is not exactly
that shape. With the pair's native in scope, `World::ret` accepts the shape
only at the top level and converts both branches with the existing scalar rule, through
`Either`'s inherent `either`. The method emitter first inserts
`support::null_aware_bound(this.0.len(), ..)`, which runs before the call.
Any other `Either` stays a refused foreign type. The oracle frames the
result as the merged vector of options.

Record 0076 added the deref route (trait methods on a type whose `Deref`
target is that trait), the null-series core fixture, the instantiation
of `ChunkedArray` and `Logical` methods on their alias wrappers from an
applicability evaluation over the inventory's recorded impl heads,
bounds, `where` predicates and associated types (three results: proven,
rejected, unresolved; only proven pairs are emitted, and Rust
compilation is the second check), typed source fixtures and a producer
fixture rule, and the structural array comparator (`into_series`,
never `Debug`). The release file's `[instantiation]` table names the
families shipped and the cited exclusions; `surface.json` records the
whole census under `instantiation`. `--self-test` covers the deref
targets and the applicability rules from synthetic inventories.

Record 0077 added iterator returns: the recognizer (`iterator_return`)
and the materialized return mapping with `support::materialize_exact`
and `materialize_unknown` (the bound is `MATERIALIZE_LIMIT`, recorded in
`surface.json` as `materialize_limit`; `polars::set_materialize_limit`
exists under `test-support` for the controls), materialization inside
the routed closure, length-framed vector, option and tuple formatting
on both oracle sides (`[3:a, b, 1:c]`, so equal-length vectors whose
element texts would join alike stay apart), and a census of every
iterator-returning callable and pair under `iterators`. The release
files carry two oracle exclusions for `row_decode_ordered` and
`row_decode_unordered`, whose output on arbitrary bytes differs run to
run, and the `StructChunked` exclusion grew by `iter` and `no_null_iter`
(`no_call_const`, bisected by compilation).

Record 0078 added `From` constructors and the assignment and `Not`
operators. A `From<X>` impl on a wrapped owner becomes a constructor
`Owner::from_<source>(x)` calling `<Owner as From<X>>::from` by UFCS;
names come from `plan_from_names`: the source's last path segment, a
by-reference twin as `_ref` (its own binding and oracle case, never
merged with or provided by the by-value impl), distinct sources sharing
a segment crate-qualified (`from_core_field`, `from_arrow_field`), a
name an inherent method already holds refused as unsupported with the
collision named. An integer source narrows through `TryFrom<i64>` and
is fallible; an `f32` source is an infallible `as f32` cast (rounded,
overflow to infinity). rustdoc lists `impl From<&X> for Y` on X's page
as well as Y's; the listing on the source page is adapted as a
"duplicate listing" only when the retained listing on Y is identified
by impl id (`counterpart` in `surface.json`, the binding carried over
when that entry is generated, unsupported with its reason otherwise);
a listing with no retained counterpart is an outward conversion
(`From<Owner> for &'static str`) and stays unsupported. `SubAssign`, `BitAndAssign`, `BitOrAssign` and
`BitXorAssign` with a `Self` operand on a `Clone` owner bind the Rune
`SUB_ASSIGN`, `BIT_AND_ASSIGN`, `BIT_OR_ASSIGN` and `BIT_XOR_ASSIGN`
protocols, mutating the left Rune value in place and cloning the right
operand out; Rune 0.14.2 has no unary `NOT` protocol, so `Not` is the
method `not_()` returning the complement and leaving the receiver.
`surface.json` records the census under `conversions`; `--self-test`
covers naming and emission from a synthetic inventory (twin pair, crate
qualification, inherent clash, fallible integer vs cast, unmappable
source). The oracle's `From` cases with a wrapped, clonable source
compare the source after the call as well as the result; the assignment
cases use the mutating shape with a unit return.

Record 0080 emits callback bindings from the 0079 census. Invocation,
mutable-argument and sink audits in the release file gate emission; a
missing audit leaves the operation unbound. Each Rune function is installed
as a synchronized function before Polars work, then called through the
guarded bridge under the current callback budget. Callback-taking bindings
and classified sinks are routed; audited metadata methods in
`[[callback_safe]]` remain usable inside callbacks. `[[callback_recipe]]`
entries keyed by closure signature provide paired Rune and Rust closures
for value-changing oracle cases. The generator checks every recipe's `uses`
against the emitted route before making a case. `callbacks.rows` records
the disposition after emission, and each binding records its route reason
and how a re-entry refusal propagates. Vector parameters borrow the Rune
container and its elements through `support::borrow_vec` and
`support::borrow_element`; the caller can use both afterwards.

Writes `adapters/polars/src/generated/` (types, functions, catalogue,
fixtures), `adapters/polars/tests/generated_oracle.rs` and
`adapters/polars/surface.json`, the accounting of every eligible callable
as generated, adapted, unsupported with a reason, or out of scope.
`--check` writes nothing and fails if the committed files differ; the
adapter's `generated_files_do_not_drift` test runs it.

Policies (ownership, conversions, errors, execution routing, keyword
renames) are described in `plans/0073_polars_generator_evidence.md`
and encoded in `src/main.rs`; record 0075 added the fixture recipes
(constructors and data-carrying variants), the alias and internal-crate
wrappers with impl-aware `Clone`/`Debug`/`Default` facts, and the
free-function arity rule, described in
`plans/0075_polars_coverage_two_evidence.md`. The generator reads the
0072 inventory's `alias_target`, `impl_for`, `impl_bounds` and
`implementors` fields when present.

The generated oracle test compares values structurally through
`adapters/polars/src/oracle.rs`: frames, series and columns cell by cell
with dtypes and nulls, errors by kind, panics by message, and a second
call on the same receiver. Each case runs in two stages on both sides:
`setup` builds the fixtures and returns them, and `main(__fx)` (the Rust
oracle's staged body) makes the measured call on those prepared values,
constructing nothing itself; a setup failure on both sides is counted
apart and verifies nothing, on one side it fails the run, and a second
Rust run whose setup fails is a nondeterminism failure. Every outcome that is not an approved match
fails; the negative controls in `oracle.rs` prove that.
