# rnx 0078: Polars constructors and operators, evidence

Plan: `plans/0078_polars_constructors_operators.md` (b17249c, revised
after Codex's plan review). Production stays on 0.55.2; the baseline is
record 0077 at 585fee9: 1930 generated callables, 2580 bindings, 1898
cases, 1742 verified, 156 setup failures; scoreboard 1939 available
(44%), 1200 value-tested (27%).

The plan's candidate upper bound was 137 operations (109 `From` + 28
operators). The measured result is 81 newly available and 77 newly
value-tested, because 18 of the `From` listings are impls seen twice
(below), 39 are outward conversions listed on their source page, 30 are
on internal crates, 18 are not eligible, and the rest are refused with a
reason.

## Gate 1: names, collisions, sources

`plan_from_names` names every `From<X>` impl on its owner before any
emission: the source's last path segment in snake case (`from_scalar`,
`from_data_type`), `vec_`/`option_` prefixes for containers
(`from_vec_u8`), a by-reference source as its own `_ref` binding
(`from_sort_options_ref`), and distinct sources sharing a segment
crate-qualified (`from_core_field` / `from_arrow_field`). A name an
inherent method already holds makes the impl unsupported with the
collision named (`name taken by inherent from_x: <path>`); nothing is
adapted as provided by another binding.

rustdoc lists `impl From<&X> for Y` on X's page as well as Y's, and the
0072 inventory records both listings under one impl id. Compiling the
first emission showed it: `SortMultipleOptions: From<&SortMultipleOptions>`
does not exist, the impl is `SortOptions: From<&SortMultipleOptions>`.
The generator keeps the listing whose `for` type is the page's type
(`from_impl_is_on_its_owner`). A listing on another page is a duplicate
only when the retained listing is identified by crate and impl id: it
is then adapted with `counterpart` set to that entry's key and, once
every entry exists (`resolve_duplicates`), carries the retained binding
if that entry is generated, or becomes unsupported with the retained
entry's reason if it is not (3 cases). A listing with no retained
counterpart is an outward conversion, `From<Owner> for Target` with the
target a scalar, tuple, arrow or foreign type (`PivotColumnNaming` to
`&'static str`, `Field` to `(PlSmallStr, DataType)`, `IpcCompression`
to arrow's `Compression`), and is unsupported with that reason (39); it
is a real operation this record does not bind. The scoreboard counts
the 18 adapted duplicates by the structured `counterpart` field, apart
from available, markers and unsupported (Codex's review round 1 found
the earlier version adapting all 58 by the page rule alone). After that
correction no by-value/by-reference twin pair
remains among the generated impls of 0.55.2 (every `_ref` binding's
by-value counterpart is either absent or the duplicate listing), so the
twin control is the self-test's synthetic pair.

Integer sources (`i8`, `i16`, `i32`, `u8`, `u16`, `u32`, `u64`,
`i128`/`u128` where the inventory has them) narrow from the script's
`i64` through `support::narrow` (`TryFrom`) and are fallible with kind
`ConversionError`; the two `f32` sources (`Scalar`, `Expr`) keep the
infallible `as f32` cast, rounding to the nearest `f32` and overflowing
to infinity, as the Rust call `Scalar::from(x as f32)` would.

`--self-test` (synthetic inventory, production emission): a twin pair
yields two bindings with distinct names and two oracle cases, each
calling its own UFCS impl (`<Scalar as From<Field>>::from` and
`<Scalar as From<&Field>>::from`); two sources sharing a segment are
crate-qualified; an inherent `from_data_type` makes the impl unsupported
with the collision named; `From<i8>` is fallible and `From<f32>` is not,
the cast emitted as written; an arrow `Field` source is refused with
the type named; no entry is adapted. Duplicate listings: a repeated
impl (one impl id on two pages) is adapted with its counterpart key and
carries the retained binding; an outward impl targeting `&'static str`
or a tuple is unsupported; a repeated impl whose retained listing is
unsupported is unsupported too; nothing is adapted without an identified
counterpart.

### Census (`surface.json` under `conversions`, 247 rows)

| class | generated | adapted (duplicate listing) | unsupported | not eligible | out of scope |
|---|---:|---:|---:|---:|---:|
| `From` | 53 | 18 | 92 | 18 | 30 |
| assignment (`-=` `&=` `\|=` `^=`) | 24 | | | | 5 |
| `Not` | 4 | | 2 | | 1 |

Refusals by reason (the 92 `From` and 2 `Not` unsupported rows, 94):

| reason | count |
|---|---:|
| outward conversion listed on its source page, no retained listing on the target | 39 |
| generic bucket (owner or source generic: `ChunkedArray`, `Logical`, `Selector`/`DataTypeSelector` `Not`) | 41 |
| owner not wrapped (`AnyValue`, `AnyValueBuffer`, byte sources) | 7 |
| duplicate listing whose retained listing is unsupported | 3 |
| conversion source: unwrapped type (`polars_arrow::datatypes::field::Field`) | 1 |
| conversion source: generic type (`UnitVec<u32>`) | 2 |
| conversion source: by-value argument of a non-Clone type (`CloudWriter`) | 1 |
| name taken by an inherent method | 0 |

The 24 assignment bindings are the four operators on `StatisticsFlags`,
`ScanFlags`, `DataTypeSelector`, `Selector`, `TimeUnitSet` and
`OptFlags`; the four `Not` bindings are on the four flag types
(`Selector` and `DataTypeSelector` `Not` are in the generic bucket;
`RowEncodingOptions` and `pf16` are internal crates).

## Gate 2: bindings and oracle

A `From` binding is a synthetic inherent constructor with receiver none
and one parameter `value`, calling `<Owner as From<Source>>::from` by
UFCS; its oracle case has the ordinary constructor shape and, when the
source is a wrapped, clonable type, returns `(result, source)` on both
sides so the source is compared after the call as well (22 cases; a
scalar source is copied by value in Rune). An assignment binding is
`#[rune::function(instance, protocol = SUB_ASSIGN)] fn(this: &mut W,
rhs: &W)` calling the Rust impl on `this.0` with `rhs.0.clone()`, so the
left Rune value is mutated in place and the right operand stays usable;
its case uses the mutating shape in statement form (`a -= __fx[1]`)
with a unit return, so the receiver's whole value is what is compared.
`Not` is the method `not_()` (Rune 0.14.2 has no unary `NOT` protocol
and `not` is a keyword) returning the complement of a clone.

Runner controls (generated harness, all passing):

- `-=`, `&=`, `|=`, `^=` on `StatisticsFlags` with several bits set on
  both operands (`3` and `6`) match the Rust impl on the pair (left
  value after, right value); `^=` against the `|=` result is a
  mismatch (the whole left value is compared); a right operand claimed
  changed is a mismatch (it is compared too); `-=` on selectors that
  differ (`Wildcard`, `Float`) matches.
- `not_()` returns the complement and leaves the receiver; a receiver
  claimed changed is a mismatch.
- an aliased operand, `a |= a` and `let b = a; a |= b`, is refused by
  Rune's dynamic borrow check before the Rust impl runs (`Cannot read,
  value is -X000000`, the shared read of an exclusively held value) and
  the value compared afterwards is unchanged.
- `from_i8` at `-128` and `127` matches; at `128` both sides refuse
  with `ConversionError`; `from_f32(0.1)` matches `Scalar::from(0.1f32)`
  and `from_f32(1e40)` matches `Scalar::from(f32::INFINITY)`; the `f64`
  scalar is a mismatch.

## Gate 3: totals

`probes/0076/reconcile.py` against 585fee9:

| level | 0077 | now | delta |
|---|---:|---:|---:|
| callables generated / adapted / unsupported | 1930 / 309 / 2106 | 2011 / 327 / 2007 | +81 / +18 / −99 |
| bindings: inherent / protocol | 1039 / 605 | 1092 / 633 | +53 / +28 |
| bindings total | 2580 | 2661 | +81 |
| oracle cases / verified | 1898 / 1742 | 1976 / 1820 | +78 / +78 |
| match / both_error / both_panic / row-order | 1624 / 93 / 23 / 2 | 1702 / 93 / 23 / 2 | +78 / 0 / 0 / 0 |
| fixture_failed (counted apart) | 156 | 156 | 0 |

Newly available and newly value-tested, by class:

| class | newly available | newly value-tested |
|---|---:|---:|
| `From` constructors | 53 | 49 (4 have no source fixture: `pf16`, `DslPlan` ×2, `AggExpr`) |
| assignment operators | 24 | 24 |
| `Not` | 4 | 4 |
| total | 81 | 77 |

The 78th new case is `JoinOptionsIR as PartialEq`, whose receiver now
has a fixture through `from_join_options`; the new constructors act as
producers for types that had none.

Scoreboard (`probes/0077/scoreboard.py`):

| | 0077 | now |
|---|---:|---:|
| available to a script | 1939 (44%) | 2020 (46%) |
| value-tested by the oracle | 1200 (27%) | 1278 (29%) |
| remaining, unsupported with a reason | 2106 (48%) | 2007 (46%) |
| markers apart / duplicate listings apart | 300 / 0 | 300 / 18 |

Recipes: a `From` conversion ranks after every inherent constructor in
the recipe derivation, because its argument is a placeholder fixture
(the first regeneration built `LiteralValue` from `Scalar::default()`,
a null scalar, and `extract_i64`/`extract_usize` became same-kind errors
instead of matches; `new_idxsize(2)` is the recipe again). The 15
`GroupsType` cases now build their receiver through `from_groups_idx`
instead of the `Idx` variant, the same value by the impl's definition,
and match as before. No existing case changed status against 585fee9
in the committed run (`LazyFrame::unique` showed its permitted
row-order variation in one run and not in another, as before).

Cold launch (`probes/0073/launch.py`, three 60-run interleaved
measurements against the 0077 binary built from 585fee9,
`probes/0078/launch-results-*`):

| measurement | 0077 median ms | 0078 median ms | delta |
|---|---:|---:|---:|
| 1 | 17.65 | 17.21 | −0.44 |
| 2 | 14.65 | 15.37 | +0.72 |
| 3 | 15.82 | 16.54 | +0.72 |

81 more bindings are inside the host's noise.

Unchanged and passing: drift, accounting with the pair-level
reconciliation, the deref controls, the lib controls, both harness
tests, clippy with and without test-support, all in the documented
release profile. Adjacent 0.54.4: generated 1956, `cargo check` ok
(adapted 316, unsupported 1947). rc2, experimental (`probes/0074/out-experimental`,
`frozen-experimental.json` refrozen): build, oracle and controls pass;
generated 2180, match 1763, both_error 101, both_panic 25,
fixture_failed 153, row-order 7 under the unordered join policy (5 in
the run before the accounting correction; the joins' order varies run
to run at rc2, as record 0074 found).

The first rc2 run of this record failed to compile the hand-written
support module: `materialize_trusted`, added in 0077's review round
after that record's rc2 run, named `TrustedLen` through
`polars_core::utils::arrow`, a re-export 0.55.2 has and rc2 does not
(there it is `polars_core::utils::polars_arrow`). The adapter now
depends on `polars-arrow` directly (already in its dependency graph;
`Cargo.lock`, the adjacent lock and the experimental rc2 lock gained
the edge) and names `polars_arrow::trusted_len::TrustedLen`, valid on
every release the probes build. No generated code changed.

Observation: under the dev profile three cases whose fixtures violate
Polars debug assertions (`ScalarColumn::from_single_value_series`,
`ColumnStats::from_column_literal`, `DataFrame::slice_par`) panic on
both sides where the release profile returns a value on both sides;
approved outcomes either way, and the documented validation is the
release profile, which produced the committed results.

## Codex review round 1 (bd3733c): two corrections

R1: 39 of the 58 listings adapted as duplicates had no retained entry
with the same impl identity; they are outward conversions and are now
unsupported with that reason, a duplicate needs an identified
counterpart (structured, not a reason prefix), and a duplicate whose
counterpart is unsupported is unsupported; three self-test controls
cover the three shapes; census, scoreboard and this evidence are
reconciled above (available and value-tested unchanged: 2020 and 1278;
unsupported 2007; duplicates 18). R2: the self-test's twin assertion
still expected the pre-source-preservation script; it now checks the
prepared-source flow and both callees, and the full self-test passes
after the final generation. Both validation chains (release suites,
clippy both ways, adjacent, rc2) were rerun after the correction;
generated code did not change, so the launch measurements stand.
