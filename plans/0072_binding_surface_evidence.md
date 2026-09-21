# rnx 0072: binding surface probe, evidence

Plan: `plans/0072_binding_surface_probe.md` (cab624c). Tool: `probes/0072/`,
one command `probes/0072/run.sh`, which fails on any invalid step. Every
table below is printed by `probes/0072/report.py` from the tool's output;
the narrative says what the numbers do and do not establish. This is the
third version, after Codex's two implementation reviews (a40315b,
3256cbe): callbacks are bridged with `Function::into_sync`, the full
configuration is every declared feature, types are classified by
identity, receiver reuse is asserted, the callback capture control
carries a runtime scalar, every replay runs `--locked` against a saved
lock that must survive byte for byte, and the runner proves it can fail
twice over.

## Answer to the hypothesis

The hypothesis was: a reusable generator, fed by the crate's own API
description, can expose most of the Rust Polars API faithfully, with a
manageable and explicitly listed set of exceptions.

**What is predicted.** In the API crates under the adapter's feature set,
4345 callables are eligible, of which 1168 are `#[derive]`d protocol impls
(Clone, Debug, PartialEq, Hash, Default) inventoried on the same terms as
hand-written ones. The rules place 3021 (69.5%) in the three generator
buckets. Counting only hand-written operations, 3177 are eligible and
1962 (61.8%) are in those buckets. Under every declared feature the
figures are 5964 eligible, 69.6% in the generator buckets, or 65.4%
without derived impls. These are rule outputs, supported by the samples
below, not proof that the surface is bound.

**What is demonstrated.** 51 samples selected deterministically across
signature shapes from the four non-generic buckets: 50 compiled, executed
in Rune and produced the same observable value as a direct Rust call on
the same fixtures, with the receiver still usable afterwards; the one
remaining sample, `Expr::floor_div`, is feature gated (`round_series`) and
both the oracle and the binding fail the same way. All ten callback
samples, including `Expr::apply` and `Expr::map` whose closures must be
`Send + Sync`, executed through `Function::into_sync` inside the wrapper.
For each callback sample three controls held: a Rune panic inside the
closure came back as an error; a closure capturing a runtime scalar, whose
value selects the only branch that yields the oracle's result, produced
that result; a closure capturing a native value was refused with a
returned error, not a panic. The earlier conclusion that a Rune-owning thread and
channel are required was wrong: they remain one design for unrestricted
captures, and nothing here shows they are needed.

**What remains.** In the API crates, 46 callbacks and 1278 generic
entries (29.4%). The generic causes are tabulated below; the largest are
generic owners reached through concrete aliases (301, the `ChunkedArray`
family), return types the rules do not bind (294), and generic or
associated-type parameters (287). Which of these further rules would
absorb, and how many entries would remain for hand work, is not measured
here; any number for it is an estimate, and this evidence gives none.

**One hundred percent** of the eligible denominator is parity with the
eligible surface, not unqualified parity with Rust Polars: exclusions,
trait methods whose traits are not path-reachable, constants and macros,
and generic declarations counted once are listed below.

## Pins

| configuration | features declared | resolved | declared but not resolved | crates documented | failed to document | toolchain | rustdoc | format |
|---|---:|---:|---|---:|---|---|---|---:|
| 0.54.4-adapter | 162 | 9 | 153 (the adapter set is intentionally narrow) | 24 | none | nightly-2026-09-20 | 1.100.0-nightly | 61 |
| 0.54.4-full | 162 | 162 | none | 26 | none | nightly-2026-09-20 | 1.100.0-nightly | 61 |
| 0.55.2-adapter | 162 | 9 | 153 (the adapter set is intentionally narrow) | 24 | none | nightly-2026-09-20 | 1.100.0-nightly | 61 |
| 0.55.2-full | 162 | 162 | none | 26 | none | nightly-2026-09-20 | 1.100.0-nightly | 61 |

Target `x86_64-unknown-linux-gnu`; other targets unmeasured. The adapter
rows resolve nine features on purpose, the adapter's own set; the full
rows resolve all 162 declared features, with no omission and no crate
failing to document. Locks are kept in `probes/0072/inventory/locks/` (documentation) and
`probes/0072/samples/locks/` (harness, per release); every cargo call on a
replay runs `--locked` against the saved lock, the lock must be byte for
byte unchanged afterwards or the step fails, and a lock is created only
for a configuration that has none. The control crate is documented with
the same pinned nightly under its committed lock. Output directories are
cleared before regeneration. Verified on this machine: replaying the
0.55.2 adapter documentation and both sample suites left every saved lock
unchanged.

## Extraction control

`probes/0072/control/`: a facade crate over two dependencies containing a
single-item re-export, the same item re-exported under a second path, a
glob re-export of a module, a whole-crate re-export, a partially
re-exported second dependency, inherent methods, a trait with a required
and a provided method, a blanket impl, a feature-gated function, a
`#[doc(hidden)]` function, an `unsafe` function, a constant, a macro, an
enum, and, for classification identity, two structs named `Opts` in
different crates (one with a public field, one without), a generic struct
with a concrete alias, a `HashMap` whose value type decides, an `Into`
bound, an `Into` bound with an extra trait, and a derived `Clone`.
`expected.json`, including the expected bucket for fourteen entries, was
written before the extractor ran; `check.py` compares. Result:
`CONTROL PASS` on every run, with and without the feature.

## Denominators

Identity is the defining crate and rustdoc id, so an item reached by
three paths counts once. Generic declarations count once. Trait methods
count once per (trait, method) with implementors listed beside them.
Constants, statics, macros, type aliases, fields and variants are
supporting items, not callables. Parameter and return types are resolved
by the same identity, following aliases with their arguments; a type the
documentation set cannot resolve makes the entry unknown, which is
reported and not eligible. A polars crate missing from the set would be
unknown, never treated as an external crate.

| configuration | gross public callables | reachable, deduplicated | excluded | unknown re-exports | unknown types (U1) | eligible | of which derived impls | trait unreachable |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 0.54.4-adapter | 16094 | 6201 | 1286 | 0 | 0 | **4915** | 1497 | 67 |
| 0.54.4-full | 18334 | 7935 | 1336 | 0 | 0 | **6599** | 2047 | 79 |
| 0.55.2-adapter | 16261 | 6349 | 1294 | 0 | 0 | **5055** | 1556 | 70 |
| 0.55.2-full | 18507 | 8091 | 1342 | 0 | 0 | **6749** | 2110 | 82 |

Excluded means unsafe (X1), `#[doc(hidden)]` on every path (X2), raw
pointer or C-variadic (X3) and names beginning with an underscore (X5);
the rule table gives the counts. Trait methods whose trait is not
reachable by a public path are eligible but classified generic (X4).
Derived impls are counted among callables and flagged; the column gives
how many.

## Predicted buckets

Rule outputs. The rules are judgments, applied by a program so the result
reproduces; they are listed with hit counts at the end.

| configuration | mechanical | conversion | option struct | callback | generic |
|---|---:|---:|---:|---:|---:|
| 0.54.4-adapter | 2042 (41.5%) | 754 (15.3%) | 467 (9.5%) | 47 (1.0%) | 1605 (32.7%) |
| 0.54.4-full | 2626 (39.8%) | 1009 (15.3%) | 791 (12.0%) | 56 (0.8%) | 2117 (32.1%) |
| 0.55.2-adapter | 2100 (41.5%) | 791 (15.6%) | 482 (9.5%) | 47 (0.9%) | 1635 (32.3%) |
| 0.55.2-full | 2685 (39.8%) | 1049 (15.5%) | 808 (12.0%) | 56 (0.8%) | 2151 (31.9%) |

### By defining crate, 0.55.2 adapter configuration

| crate | eligible | mechanical | conversion | option struct | callback | generic | excluded |
|---|---:|---:|---:|---:|---:|---:|---:|
| polars_arrow | 346 | 68 | 17 | 6 | 1 | 254 | 41 |
| polars_compute | 44 | 30 | 6 | 0 | 0 | 8 | 376 |
| polars_config | 13 | 10 | 1 | 0 | 0 | 2 | 0 |
| polars_core | 1980 | 677 | 307 | 143 | 24 | 829 | 149 |
| polars_dtype | 52 | 29 | 16 | 0 | 0 | 7 | 2 |
| polars_error | 34 | 13 | 1 | 1 | 2 | 17 | 2 |
| polars_io | 720 | 322 | 126 | 50 | 2 | 220 | 2 |
| polars_lazy | 189 | 67 | 39 | 65 | 3 | 15 | 3 |
| polars_ops | 193 | 100 | 43 | 39 | 0 | 11 | 11 |
| polars_parquet | 36 | 16 | 5 | 1 | 0 | 14 | 0 |
| polars_parquet_format | 2 | 0 | 0 | 0 | 0 | 2 | 0 |
| polars_plan | 1113 | 622 | 186 | 175 | 15 | 115 | 0 |
| polars_row | 54 | 43 | 2 | 0 | 0 | 9 | 0 |
| polars_schema | 64 | 0 | 0 | 0 | 0 | 64 | 0 |
| polars_utils | 215 | 103 | 42 | 2 | 0 | 68 | 708 |

The prelude reaches arrow, utils and row internals. They are part of the
reachable public surface and are reported, but the answer above uses the
API-crate subset; the subset is a judgment, and both figures are given.

### API crates versus internals

| configuration | subset | eligible | mechanical | conversion | option struct | callback | generic |
|---|---|---:|---:|---:|---:|---:|---:|
| 0.54.4-adapter | API crates | 4219 | 1773 (42.0%) | 682 (16.2%) | 458 (10.9%) | 46 (1.1%) | 1260 (29.9%) |
| 0.54.4-adapter | internals (arrow, utils, row, parquet, compute, config) | 696 | 269 (38.6%) | 72 (10.3%) | 9 (1.3%) | 1 (0.1%) | 345 (49.6%) |
| 0.54.4-full | API crates | 5828 | 2328 (39.9%) | 929 (15.9%) | 778 (13.3%) | 55 (0.9%) | 1738 (29.8%) |
| 0.54.4-full | internals (arrow, utils, row, parquet, compute, config) | 771 | 298 (38.7%) | 80 (10.4%) | 13 (1.7%) | 1 (0.1%) | 379 (49.2%) |
| 0.55.2-adapter | API crates | 4345 | 1830 (42.1%) | 718 (16.5%) | 473 (10.9%) | 46 (1.1%) | 1278 (29.4%) |
| 0.55.2-adapter | internals (arrow, utils, row, parquet, compute, config) | 710 | 270 (38.0%) | 73 (10.3%) | 9 (1.3%) | 1 (0.1%) | 357 (50.3%) |
| 0.55.2-full | API crates | 5964 | 2386 (40.0%) | 968 (16.2%) | 795 (13.3%) | 55 (0.9%) | 1760 (29.5%) |
| 0.55.2-full | internals (arrow, utils, row, parquet, compute, config) | 785 | 299 (38.1%) | 81 (10.3%) | 13 (1.7%) | 1 (0.1%) | 391 (49.8%) |

### What makes an entry generic

| cause | entries |
|---|---:|
| owner has type parameters (O2, alias-instantiable) | 301 |
| return | 294 |
| a parameter (P7 generic/dyn/assoc) | 287 |
| owner has type parameters (O1) | 230 |
| a parameter (P8 foreign type) | 50 |
| a parameter (P9 unreachable polars type) | 43 |
| a parameter (L1 lifetime type) | 32 |
| protocol | 19 |
| trait scope | 11 |
| receiver | 8 |
| foreign trait | 3 |

Most common foreign parameter types (P8): `&mut Formatter` 16, `PlSeedableRandomStateQuality` 5, `Error` 4, `NaiveDateTime` 3, `Bytes` 2, `&PlIndexSet<PlSmallStr>` 2, `&NaiveTime` 1, `RangeFrom<usize>` 1, `RangeTo<usize>` 1, `RangeToInclusive<usize>` 1

## Demonstrated

Samples: eligible entries in the API crates, sorted by canonical path,
grouped by bucket and signature shape, shapes visited in order of
population, one entry per shape per round until the quota (20, 10, 10,
10). An entry whose types have no fixture is skipped with the reason
recorded and the next entry of that shape taken. Every claimed conversion
rule appears in at least one conversion sample. Fixtures: a three-column
frame, its lazy form, `col("x")`, an `i64` series and column, `Int64`, the
frame's schema, a field and a group-by; scalars are fixed literals; option
structs come from `Default` with generated boolean setters.

Each sample is a generated Rune binding on a wrapper type, a Rune script
that calls it twice on the same receiver and formats both results, and a
Rust oracle making the same call on the same fixtures with the same
formatting. Expressions are executed, not printed: the oracle and the
binding both select the expression against the fixture frame and
collect, so deferred callbacks run. A sample passes only when the first
result matches the oracle and the second call on the same receiver
matches it too (or the sample has no receiver); a second result that
differs is `receiver_reuse_failed`. Callback samples add three scripts: a
closure that panics, a closure capturing a runtime scalar (`let n =
s::runtime_one()`, a native call the compiler cannot fold) that decides
the result, and a closure capturing a native value. The runner exits
nonzero unless every sample passed or was feature gated and every
callback control had its required outcome. `run.sh` first proves that
twice: it injects a wrong oracle into one sample and a broken second
result into another, and requires the runner to fail on each.

| bucket | executed_match | feature_gated | total |
|---|---:|---:|---:|
| mechanical | 19 | 1 | 20 |
| conversion | 11 | 0 | 11 |
| option_struct | 10 | 0 | 10 |
| callback | 10 | 0 | 10 |

Runner: cargo test exit 0; identity {'gen.py': '8bc31c7e7ab30837', 'harness/src/lib.rs': 'f51d62872b95bf72', 'inventory': '../out/0.55.2-adapter/result/inventory.json', 'inventory_sha256': '81fdce51b4c4a0a7'}

| callback control | outcome | samples |
|---|---|---:|
| error_propagation | error_propagated | 10 |
| const_capture | scalar_capture_carried | 10 |
| native_capture | native_capture_refused | 10 |

Receiver semantics observed: methods taking `self` are bound by cloning
the wrapped value, so the Rune value stays usable where Rust would consume
it; `&mut self` mutates the Rune value in place; borrowed returns are
cloned out. Callback semantics: a Rune panic surfaces as a Polars
`ComputeError` where the closure signature is fallible and as an unwinding
panic where it is not, so a non-fallible closure signature cannot carry a
script error except by unwinding.

Skipped before generation, by reason (limits of this harness's fixture
set and generator, not of the rules):

| reason | entries |
|---|---:|
| owner type has no fixture | 779 |
| no fixture for a parameter or return type | 79 |
| foreign or derived trait impl kind not generated (Debug, Clone, Hash, From, Iterator, ...) | 34 |
| type carries a lifetime (AnyValue) | 19 |
| closure shape not generated (arity 0, non-wrapped argument, generic return) | 14 |
| enum without a unit variant for a fixture | 9 |
| option struct has no Default | 4 |
| other | 4 |
| generic bound not generated | 2 |
| owner option struct has no Default | 2 |

## Maintenance: one observed upgrade

Rules and generator were frozen after the 0.55.2 run and applied unchanged
to 0.54.4, the adjacent release on the same track; `run.sh` regenerates,
compiles and executes both.

Runner: cargo test exit 0; identity {'gen.py': '8409cce0a1e6d4ad', 'harness/src/lib.rs': 'f51d62872b95bf72', 'inventory': '../out/0.54.4-adapter/result/inventory.json', 'inventory_sha256': 'e90035b0235ad30c'}

| bucket | executed_match | feature_gated | total |
|---|---:|---:|---:|
| mechanical | 19 | 1 | 20 |
| conversion | 11 | 0 | 11 |
| option_struct | 10 | 0 | 10 |
| callback | 10 | 0 | 10 |

Same entry and shape selected in both releases: 48 of 51; the selection is deterministic per inventory, so entries added or reshaped between releases move the sample set.
Harness fixed part (fixtures, runner, oracle formatting): built unchanged against 0.54.4.

| measure | count |
|---|---:|
| callables only in 0.54.4 (removed by 0.55.2) | 67 |
| callables only in 0.55.2 (added) | 214 |
| same path, signature shape changed | 16 |
| same path, predicted bucket changed | 13 |
| same path, unchanged | 5810 |

No override or rule change was needed. This is one observed upgrade
between two minor releases, not a bound on upgrade cost.

## Rule table with hits

| rule | shape | hits |
|---|---|---:|
| F1 | foreign trait impl (operators, From/Into, Display, Iterator, ...) classified by its first method's signature; Iterator and Deref impls -> generic | 1539 |
| F2 | protocol impls (Display, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default) -> mechanical (Rune protocol) | 935 |
| F3 | From/TryFrom impls classified by their source argument -> conversion at least | 218 |
| L1 | type carries a lifetime parameter (AnyValue<'a>) -> generic | 38 |
| O1 | owner type has type parameters and no concrete reachable alias -> generic | 675 |
| O2 | owner type has type parameters but is reached through concrete aliases (Int64Chunked = ChunkedArray<Int64Type>) -> generic, instantiable per alias | 489 |
| P1 | scalar: bool, integers, floats, usize/isize, char, String, &str, PlSmallStr -> mechanical | 914 |
| P2 | reachable polars struct with no public fields, or enum with only unit variants, by value or reference -> mechanical (wrapped value); a generic type used with concrete arguments is one such value per instantiation | 657 |
| P3 | Option<T>, Vec<T>, &[T], (T, U), Arc<T>, Box<T>, Cow<T>, &T of a mechanical T -> conversion | 322 |
| P4 | impl Into<T>, impl AsRef<str>, impl IntoVec<T>, impl IntoIterator<Item = T>, or a generic bounded so, with T mechanical or conversion -> conversion | 58 |
| P5 | reachable polars struct with public fields, or enum with data-carrying variants -> option struct | 749 |
| P6 | closure: impl Fn/FnMut/FnOnce, generic bounded by Fn*, Box<dyn Fn*>, Arc<dyn ..Udf..> or a named *Udf type -> callback | 170 |
| P7 | generic parameter or impl Trait with any other bound, dyn Trait, associated type projection -> generic | 787 |
| P8 | foreign type from an undocumented crate (chrono, arrow2, bytes, ...) that is not a std scalar or container -> generic | 76 |
| P9 | polars type public in its crate but not reachable from the root -> generic | 140 |
| R1 | receiver none, self, &self or &mut self -> mechanical | 5405 |
| R2 | receiver of another shape (Box<Self>, Pin<..>) -> generic | 9 |
| T1 | return Self, unit, scalar, wrapped value, PolarsResult<T>/Result<T, PolarsError>/Option<T> of those -> mechanical | 2515 |
| T2 | return &T or &mut T of a wrapped value (cloned on the way out), or a container of mechanical returns -> conversion | 1068 |
| T3 | return impl Trait, generic T, iterator, reference with non-static lifetime into a non-polars type -> generic | 1116 |
| X1 | unsafe fn -> unsupported | 200 |
| X2 | #[doc(hidden)] on every path -> unsupported | 1087 |
| X3 | raw pointer, extern type or C-variadic anywhere -> unsupported | 10 |
| X4 | trait method whose trait is not reachable by public path -> generic (needs the trait in scope) | 70 |
| X5 | name starts with `_` (public by necessity, internal by convention) -> unsupported | 58 |

## Recommendation

Run the generator record with the three predicted buckets on the API
crates as its scope, and publish the exception list from
`inventory.json` with the adapter so parity is always a number against a
stated denominator. Bridge callbacks with `Function::into_sync` and
report refusal as an error; a Rune-owning thread is a later option for
unrestricted captures, if wanted. Alias instantiation and return-type
rules are the first candidates for widening, and their yield should be
measured the same way before it is claimed.

## Per-sample results, 0.55.2

| id | bucket | entry | shape | status | notes |
|---|---|---|---|---|---|
| s000 | mechanical | `abort::register_polars_abort_mechanism` | `none -> ()` | executed_match |  |
| s001 | mechanical | `DataType::contains_categoricals` | `&self -> scalar` | executed_match |  |
| s002 | mechanical | `cmp::PartialEq` | `protocol PartialEq` | executed_match | PartialEq via Rune protocol PARTIAL_EQ |
| s003 | mechanical | `Column::is_sorted_flag` | `&self -> T` | executed_match |  |
| s004 | mechanical | `SortMultipleOptions::with_order_reversed` | `self -> Self` | executed_match | receiver consumed in Rust; wrapper clones so the Rune value stays usable |
| s005 | mechanical | `SortMultipleOptions::with_maintain_order` | `self scalar -> Self` | executed_match | receiver consumed in Rust; wrapper clones so the Rune value stays usable |
| s006 | mechanical | `DataType::cat_physical` | `&self -> PolarsResult<T>` | executed_match |  |
| s007 | mechanical | `Expr::floor_div` | `self Self -> Self` | feature_gated | activate 'round_series' feature |
| s008 | mechanical | `SortMultipleOptions::new` | `none -> Self` | executed_match |  |
| s009 | mechanical | `fmt::Display` | `protocol Display` | executed_match | Display via Rune protocol DISPLAY_FMT |
| s010 | mechanical | `Column::shrink_to_fit` | `&mut self -> ()` | executed_match | &mut self: mutates the Rune value in place |
| s011 | mechanical | `Column::into_frame` | `self -> T` | executed_match | receiver consumed in Rust; wrapper clones so the Rune value stays usable |
| s012 | mechanical | `CsvParseOptions::with_encoding` | `self T -> Self` | executed_match | receiver consumed in Rust; wrapper clones so the Rune value stays usable |
| s013 | mechanical | `DataFrame::empty_with_height` | `none scalar -> Self` | executed_match |  |
| s014 | mechanical | `Column::clear` | `&self -> Self` | executed_match |  |
| s015 | mechanical | `concurrency::get_request_budget` | `none -> scalar` | executed_match |  |
| s016 | mechanical | `JoinArgs::new` | `none T -> Self` | executed_match |  |
| s017 | mechanical | `Column::n_unique` | `&self -> PolarsResult<scalar>` | executed_match |  |
| s018 | mechanical | `LazyFrame::describe_optimized_plan` | `&self -> PolarsResult<string>` | executed_match |  |
| s019 | mechanical | `Column::try_add_owned` | `self Self -> PolarsResult<Self>` | executed_match | receiver consumed in Rust; wrapper clones so the Rune value stays usable |
| s020 | conversion | `DataType::to_physical` | `&self -> T` | executed_match |  |
| s021 | conversion | `DataType::inner_dtype` | `&self -> Option<&T>` | executed_match | borrowed return cloned |
| s022 | conversion | `DataType::implode` | `self -> T` | executed_match | receiver consumed in Rust; wrapper clones so the Rune value stays usable |
| s023 | conversion | `DataType::numeric_to_unsigned_bit_repr` | `&self -> Option<T>` | executed_match |  |
| s024 | conversion | `Column::first_non_null` | `&self -> Option<scalar>` | executed_match |  |
| s025 | conversion | `Column::unique` | `&self -> PolarsResult<T>` | executed_match |  |
| s026 | conversion | `CsvParseOptions::with_quote_char` | `self Option<scalar> -> Self` | executed_match | receiver consumed in Rust; wrapper clones so the Rune value stays usable |
| s027 | conversion | `Field::name` | `&self -> &string` | executed_match | borrowed return cloned |
| s028 | conversion | `functions::first` | `none -> T` | executed_match |  |
| s029 | conversion | `DataType::try_into_inner_dtype` | `self -> PolarsResult<T>` | executed_match | receiver consumed in Rust; wrapper clones so the Rune value stays usable |
| s030 | option_struct | `CloudType::from_cloud_scheme` | `none T -> Self` | executed_match |  |
| s031 | option_struct | `LazyFrame::fill_nan` | `self G -> T` | executed_match | receiver consumed in Rust; wrapper clones so the Rune value stays usable |
| s032 | option_struct | `Field::with_dtype` | `self T -> Self` | executed_match | receiver consumed in Rust; wrapper clones so the Rune value stays usable |
| s033 | option_struct | `LazyFrame::shift` | `self G -> Self` | executed_match | receiver consumed in Rust; wrapper clones so the Rune value stays usable |
| s034 | option_struct | `LazyFrame::with_column` | `self T -> T` | executed_match | receiver consumed in Rust; wrapper clones so the Rune value stays usable |
| s035 | option_struct | `Column::cast` | `&self &T -> PolarsResult<T>` | executed_match |  |
| s036 | option_struct | `bitwise::count_ones` | `none &T -> PolarsResult<T>` | executed_match |  |
| s037 | option_struct | `bit_repr::reinterpret` | `none &T &T -> PolarsResult<T>` | executed_match |  |
| s038 | option_struct | `Series::fill_null` | `&self T -> PolarsResult<T>` | executed_match |  |
| s039 | option_struct | `DataType::contains_dtype_recursive` | `&self &T -> scalar` | executed_match |  |
| s040 | callback | `Expr::map_expr` | `self G -> Self` | executed_match | callback FnMut(Expr) -> p::Expr; receiver consumed in Rust; wrapper clones so the Rune value stays usable; closure converted with Function::into_sync inside the wrapper; captures other than constants are refused with an error; error: error_propagated; const: constant_capture_allowed; native: native_capture_refused |
| s041 | callback | `Expr::agg_with_fmt_str` | `self G G impl -> Self` | executed_match | callback Fn(Column) -> p::PolarsResult<p::Column>; closure must be Send + Sync: a rune::runtime::Function is neither in Rune 0.14.2; callback Fn(Schema, Field) -> p::PolarsResult<p::Field>; closure must be Send + Sync: a rune::runtime::Function is neither in Rune 0.14.2; receiver consumed in Rust; wrapper clones so the Rune value stays usable; closure converted with Function::into_sync inside the wrapper; captures other than constants are refused with an error; error: error_propagated; const: constant_capture_allowed; native: native_capture_refused |
| s042 | callback | `Column::apply_broadcasting_binary_elementwise` | `&self &Self closure -> PolarsResult<T>` | executed_match | callback Fn(Series, Series) -> p::Series; closure converted with Function::into_sync inside the wrapper; captures other than constants are refused with an error; error: error_propagated; const: constant_capture_allowed; native: native_capture_refused |
| s043 | callback | `Column::try_apply_unary_elementwise` | `&self closure -> PolarsResult<T>` | executed_match | callback Fn(Series) -> p::PolarsResult<p::Series>; closure converted with Function::into_sync inside the wrapper; captures other than constants are refused with an error; error: error_propagated; const: constant_capture_allowed; native: native_capture_refused |
| s044 | callback | `DataFrame::try_apply_columns` | `&self closure -> PolarsResult<Vec<T>>` | executed_match | callback Fn(Column) -> p::PolarsResult<p::Column>; closure must be Send + Sync: a rune::runtime::Function is neither in Rune 0.14.2; closure converted with Function::into_sync inside the wrapper; captures other than constants are refused with an error; error: error_propagated; const: constant_capture_allowed; native: native_capture_refused |
| s045 | callback | `Column::apply_unary_elementwise` | `&self closure -> T` | executed_match | callback Fn(Series) -> p::Series; closure converted with Function::into_sync inside the wrapper; captures other than constants are refused with an error; error: error_propagated; const: constant_capture_allowed; native: native_capture_refused |
| s046 | callback | `DataFrame::apply_columns` | `&self closure -> Vec<T>` | executed_match | callback Fn(Column) -> p::Column; closure must be Send + Sync: a rune::runtime::Function is neither in Rune 0.14.2; closure converted with Function::into_sync inside the wrapper; captures other than constants are refused with an error; error: error_propagated; const: constant_capture_allowed; native: native_capture_refused |
| s047 | callback | `Expr::try_map_expr` | `self G -> PolarsResult<Self>` | executed_match | callback FnMut(Expr) -> p::PolarsResult<p::Expr>; receiver consumed in Rust; wrapper clones so the Rune value stays usable; closure converted with Function::into_sync inside the wrapper; captures other than constants are refused with an error; error: error_propagated; const: constant_capture_allowed; native: native_capture_refused |
| s048 | callback | `Expr::apply` | `self G G -> Self` | executed_match | callback Fn(Column) -> p::PolarsResult<p::Column>; closure must be Send + Sync: a rune::runtime::Function is neither in Rune 0.14.2; callback Fn(Schema, Field) -> p::PolarsResult<p::Field>; closure must be Send + Sync: a rune::runtime::Function is neither in Rune 0.14.2; receiver consumed in Rust; wrapper clones so the Rune value stays usable; closure converted with Function::into_sync inside the wrapper; captures other than constants are refused with an error; error: error_propagated; const: constant_capture_allowed; native: native_capture_refused |
| s049 | callback | `Expr::apply_with_fmt_str` | `self G G impl -> Self` | executed_match | callback Fn(Column) -> p::PolarsResult<p::Column>; closure must be Send + Sync: a rune::runtime::Function is neither in Rune 0.14.2; callback Fn(Schema, Field) -> p::PolarsResult<p::Field>; closure must be Send + Sync: a rune::runtime::Function is neither in Rune 0.14.2; receiver consumed in Rust; wrapper clones so the Rune value stays usable; closure converted with Function::into_sync inside the wrapper; captures other than constants are refused with an error; error: error_propagated; const: constant_capture_allowed; native: native_capture_refused |
| s050 | conversion | `SortMultipleOptions::with_nulls_last_multi` | `self impl -> Self` | executed_match | receiver consumed in Rust; wrapper clones so the Rune value stays usable |
