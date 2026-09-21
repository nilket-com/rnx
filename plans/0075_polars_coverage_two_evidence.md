# rnx 0075: Polars coverage, stage two, evidence

Plan: `plans/0075_polars_coverage_two.md` (708e650). Gates are appended
as they close. Production stays on 0.55.2 throughout; the baseline is
record 0073 at 98885e0: 1622 generated entries, 832 oracle cases.

## Gate 1: release-specific inputs and the join-order policy

### Release files

The API-crate list, the unordered-operation list and the excluded-oracle
list left the generator's source. They live in
`tools/polars-gen/releases/<name>.toml`, named on the command line with
`--release`, and `surface.json` records the file's name, source string
and SHA-256. `polars-gen --self-test` covers the policy rule. Files:

| file | purpose |
|---|---|
| `0.55.2.toml` | today's constants, exactly, for the baseline claim |
| `0.55.2-joins.toml` | 0.55.2 with the join-order policy; the shipped one |
| `0.54.4.toml` | the adjacent-release check |
| `rc2.toml` | the 0074 probe's experimental run; adds `polars_defs` |

### Claim one: the baseline is unchanged

Regenerated from the 0.55.2 inventory with `0.55.2.toml`, the generated
module (`types.rs`, `functions.rs`, `catalogue.rs`, `fixtures.rs`,
`mod.rs`) is byte for byte the committed 98885e0 module. `surface.json`
gained six lines, the release metadata, and nothing else. The generated
oracle test changed only in that each case now carries its recorded
policy string and the runner gained the join-order controls below; the
832 cases are identical in id, script, receiver and order policy, checked
by parsing both versions. The drift test, accounting test and oracle test
pass on that state; the tally is the baseline's: 801 match, 2 row-order
under the unique policy, 19 both-error, 10 both-panic.

### Claim two: the join policy, applied and reported apart

Order policy is per case: a case compares rows as a multiset only when
its canonical operation is listed in the release file and its recorded
arguments are the listed configuration. The rule is in one place,
`Release::policy`: an argument that passes an explicit `MaintainOrderJoin`
other than `None` keeps the case ordered whatever the list says, and a
method elsewhere with the same short name is not matched, because the
list holds canonical paths. Citations from the pinned 0.55.2 sources are
in `0.55.2-joins.toml`: `MaintainOrderJoin::None` is `#[default]`
(polars-ops `join/args.rs:124`), `JoinArgs::default()` takes it (`:153`),
and `inner_join`, `left_join`, `full_join` build with `JoinArgs::new`
(polars-lazy `frame/mod.rs:1256, 1281, 1306`). `cross_join` and
`JoinBuilder::finish` are not listed: they have no case on 0.55.2 and no
citation was made for them.

Regenerating with `0.55.2-joins.toml` changed the policy of exactly four
cases and nothing else; their scripts are unchanged and all four still
match on 0.55.2:

| case | policy |
|---|---|
| `LazyFrame::inner_join` | unordered: default arguments |
| `LazyFrame::left_join` | unordered: default arguments |
| `LazyFrame::full_join` | unordered: default arguments |
| `LazyFrame::join` | unordered: `JoinArgs::default()` |

The classifier and its unit controls did not change. Integrated controls
added to the generated runner, all passing: permuting rows passes for a
justified unordered join case, fails for the same operation when the
script passes `MaintainOrderJoin::Left` through `JoinArgs`, and fails for
an unrelated ordered operation.

The shipped state is `0.55.2-joins.toml`; the drift test names it.

## Gate 2: fixture recipes, setup apart from the measured call

### Rules

Two recipes were added to the fixture rules of record 0073 (a core
fixture, `Default`, or a unit variant), evaluated as a fixpoint until no
type gains one:

- **constructor**: a generated binding on the type named `new` or
  `from_*` (then any other associated function returning `Self`, in
  alphabetical order) whose every argument has a fixture; the first that
  qualifies is the recipe.
- **variant**: a data-carrying enum variant whose every payload has a
  fixture; the first in declaration order.

A fallible constructor is unwrapped in the fixture with a panic whose
message starts `fixture:`; both sides carry the same prefix. The
classifier reads it first: a case whose setup fails on either side is
`fixture_failed`, never a match and never verified, and the runner's
`bad` count excludes it only from the failure count, not from the
report. Trait methods look up their implementors in the inventory and
take the first with a fixture as the receiver; on 0.55.2 that unlocked
none of the 43 `SeriesTrait` methods (its implementors are the private
`SeriesWrap` types) and the count stays in the census below.

`surface.json` records every derived recipe with its Rune and Rust text
and every type that has none with the reason. Case dispositions name the
recipes a script actually contains, `case (fixture via constructor new
(fallible))`; a constructor's own case is not "via" itself.

### Controls

- Unit (`oracle.rs`, 23 lib controls pass): a `fixture:` panic on both
  sides is `FixtureFailed`, not approved, not counted as verified.
- Integrated (generated runner): a control case whose setup panics with
  `fixture:` on both sides against `polars::engine_counts()` adds zero
  verified cases and starts zero engine threads.
- The drift test regenerates with `0.55.2-joins.toml` and compares
  byte for byte; the accounting test reconciles case dispositions with
  emitted cases by prefix.

### 0.55.2, before and after

Generated entries: 1622 before, 1622 after. **No binding was newly
compiled by this gate**; it changes what the oracle can exercise.

| fixture-free generated bindings (0074 rule) | before | after |
|---|---:|---:|
| struct, no public fields, no `Default` | 248 | 109 |
| enum, no unit variant | 130 | 72 |
| struct, public fields, no `Default` | 80 | 76 |
| trait-owned method | 62 | 56 |
| **total** | **520** | **313** |

Recipes derived: 36 (26 constructor, 10 variant). Types still without a
recipe: 96, of which 48 have no public fields, no `Default` and no
constructor binding; 28 have public fields and no constructor binding;
19 are enums with neither a unit variant nor a variant whose payload has
fixtures; 1 has constructors but none with fixtures for all arguments.

Oracle cases: 832 before, 972 after. The 832 old cases are unchanged in
id, script and policy. Of the 140 new cases, 118 use a constructor
recipe, 21 a variant recipe, and 1 (`GroupsType as Default`) became
emittable because its type now has a fixture but its script does not use
it. Largest receivers that gained cases: `StatisticsFlags` (29),
`TimeUnitSet` (22), `ScalarColumn` (21), `ScanFlags` (21), `GroupsType`
(15), `LiteralValue` (13), `ColumnStats` (10).

Tally after (all 972 verified, 0 fixture_failed, 0 failures):

| outcome | before | after |
|---|---:|---:|
| match | 801 | 940 |
| row_order_differs (under the unique policy) | 2 | 2 |
| both_error | 19 | 19 |
| both_panic | 10 | 11 |

The `unique` case flaps between match and row-order across runs; it is
listed unordered and counts as a match either way.

## Gate 3: wrappers for alias and internal-crate types

### Inventory input

The record 0072 extractor now records three facts it did not before,
each absent (`null` or empty) in older inventories and read with a
default by the generator: for a type alias, `alias_target`, the aliased
type rendered canonically with every non-generic alias inside it
expanded; for a foreign trait impl, `impl_for` and `impl_bounds`, the
impl's `for` type and its generic parameters' bounds, canonical; for a
trait, `implementors`, the types with a direct impl. Re-extracting the
four 0072 configurations changed nothing else: every `summary.json` is
byte-identical and every `inventory.json` is identical once the new
fields are removed. `IdxCa` and `UInt32Chunked` both render as
`ChunkedArray<UInt32Type>`; `SchemaRef` as `Arc<Schema<DataType, ()>>`;
`CatSize` as `u32`.

### Rules

- **Alias**: a concrete, unhidden alias in an API crate whose expanded
  target is an instantiation of a reachable struct or enum, or an `Arc`
  of one, is wrapped under the alias's name. Identity is the expanded
  target: aliases with one identity share one wrapper struct, one Rune
  type, and the representative name is the alphabetically first alias
  name, the others recorded in `surface.json` under `shared_with`.
  `IdxCa` and `UInt32Chunked` are the one such pair on 0.55.2:
  `polars::IdxCa`. An alias of a scalar, `Vec` or reference is not
  wrapped; in argument and return position it stands for its target, so
  `CatSize` maps as `u32` and `GroupsSlice` as its vector.
- **Internal type**: a concrete, lifetime-free struct or enum of a crate
  outside the release's API list that a bound API signature mentions is
  wrapped under `polars::<crate short name>::<Name>`, spelled through the
  `polars` facade's re-exports, since the adapter does not depend on
  those crates. Eight on 0.55.2: `polars::arrow::{ArrowDataType,
  ReshapeDimension, TimeUnit}`, `polars::compute::QuantileMethod`,
  `polars::config::Engine`, `polars::row::RowEncodingOptions`,
  `polars::utils::{CloudScheme, PlRefPath}`. `polars_arrow::Field`
  stays unwrapped: its only facade path is the ambiguous
  `polars::prelude::ArrowField` (3 entries).
- **Trait facts for an instantiation**: `Clone`, `Debug` and `Default`
  for `Base<Args>` hold when an impl on `Base` unifies with it and the
  bounds on the unified parameters hold: a local trait by its recorded
  implementors, one of the three by recursion, marker bounds trivially.
  So `BooleanChunked` has `Debug` (an impl for `ChunkedArray<BooleanType>`
  exists), `Int64Chunked` has it through `T: PolarsNumericType`, and
  `StringChunked`, `ListChunked` and `StructChunked` do not under the
  adapter's feature set, where the rustdoc JSON holds exactly two
  `Debug` impls on `ChunkedArray`. The Rune side of a `Default` fixture
  now also requires the generated `default_` binding; an impl on the
  generic base gives no binding, and the 21 alias types in that state
  are listed as such under `fixtures.none`.
- **Arity**: Rune's `Function` trait covers free functions of at most
  five parameters (rune 0.14.2 `function/macros.rs`, every reference
  permutation); instance functions go to fifteen. Four free functions
  newly reachable through the wrappers exceed it and are unsupported
  with the reason `arity`: `init_builders`, `count_rows`, `read_chunk`,
  `slice_broadcast_list`.

Controls, in `polars-gen --self-test`: from a synthetic inventory,
equivalent aliases resolve to one wrapper struct and one Rune path with
the alphabetically first name; a different instantiation is a different
wrapper; two same-name distinct types in two crates resolve to distinct
Rune paths (`polars::core::Field`, `polars::plan::Field`); a scalar alias
is not wrapped. The generator also refuses to emit two wrappers with one
Rune path.

### 0.55.2, before and after

| measure | after gate 2 | after gate 3 |
|---|---:|---:|
| wrapper structs (paths served) | 237 (237) | 268 (269) |
| `unsupported: unwrapped type` entries | 213 | 3 |
| generated entries | 1622 | 1813 |
| oracle cases | 972 | 1057 |

Newly compiled: 191 entries (182 from `unwrapped type`, 9 from `on every
implementor`). Of them 85 became oracle cases, 45 have a return type
with no comparison, 38 lack a fixture for a parameter and 23 for the
receiver. Tally after, all 1057 verified, 0 fixture_failed, 0 failures (the two
`unique` cases flap between match and row-order between runs, both
approved under their policy; this run had them matching):

| outcome | after gate 2 | after gate 3 |
|---|---:|---:|
| match | 940 | 1002 |
| row_order_differs (unique policy) | 2 | 0 |
| both_error | 19 | 44 |
| both_panic | 11 | 11 |

The 25 new both-error cases are `Series::{bool,f32,…,u64,idx}` and
`Column::{…}` on the fixture's `Int64` series (`SchemaMismatch` on both
sides, 22) and three `ComputeError`s; they verify the error path only.
The census of fixture-free generated bindings grows with the generated
set: 313 after gate 2, 336 after gate 3 (trait-owned 56 to 64, the eight
being `SeriesTrait` methods newly compiled through the alias wrappers).

## Gate 4: totals, cost, adjacent release, rc2

### Against the 0073 baseline (98885e0)

| number | baseline | this record |
|---|---:|---:|
| generated entries | 1622 | 1813 |
| adapted | 309 | 309 |
| unsupported | 2414 | 2223 |
| out of scope | 710 | 710 |
| wrapper structs | 237 | 268 |
| oracle cases, all verified | 832 | 1057 |
| match (plus 0 to 2 `unique` row-order flaps) | 801 | 1002 |
| both_error / both_panic | 19 / 10 | 44 / 11 |
| setup failures | 0 | 0 |
| fixture-free generated bindings (census) | 520 | 336 |
| unwrapped-type exceptions | 213 | 3 |

Newly compiled: 191 (gate 3). Newly oracle-verified cases: 225 (140
from gate 2 recipes, 85 from gate 3 wrappers), of which 199 match and 26
verify an error or panic. Remaining exceptions by reason, unsupported:
bucket 1324, trait impl not mapped to a protocol 358, no wrapped
implementor 112, owner not wrapped 80, receiver consumes a non-Clone
type 76, lifetime owner 56, foreign type 50, owner has no public path
28, generic type 22, async 21, uninferable generic parameter 20, on
every implementor 17, arity 4, unwrapped type 3, the rest under 20 each.
Generated but not verified (756, with the 1057 cases the 1813
generated entries): 336 without a receiver fixture, 84 with a return
type that has no comparison, 18 with a receiver type that has none, 62
without a parameter fixture, 238 `Clone`/`Debug` protocols with no
script-level trigger, 16 operator results not wrapped, 2 excluded as
nondeterministic.

Unchanged: the drift test, the accounting partition, the 23 lib
controls (single-threaded, as the README's validation runs them; in
parallel the engine counter test races the file tests, a pre-existing
flake unrelated to this record: `engine.rs` and `files.rs` are
untouched), the hand-written and presentation suites.

### Cost

`probes/0073/launch.py`, 60 interleaved launches at idle, samples in
`probes/0075/launch-results.json`:

| binary | median ms | min ms | p90 ms |
|---|---:|---:|---:|
| 0073 baseline (`target/0073-after-target`) | 14.74 | 11.16 | 19.48 |
| this record | 15.64 | 11.18 | 22.95 |

Two earlier 40-run measurements gave median differences of 0.08 and
1.25 ms with equal minimums. 191 more functions and 31 more types cost
about 1 ms of median on this host, inside the run-to-run spread; the
absolute numbers are higher than 0073's session, as that evidence
warned.

### Adjacent release

`probes/0073/adjacent.sh` with `0.54.4.toml`: generated 1759, adapted
298, unsupported 2162, out of scope 696, 255 wrapper structs;
`cargo check` passes under the committed lock; the three module files
differ from 0.55.2's as before.

### rc2, labeled experimental

The 0074 probe with `rc2.toml` (`PROBE_EXPERIMENTAL=1`, separate output,
locks and manifest; 0074's accepted `frozen.json` and `out/` untouched).
With `polars_defs` in the release's API list the eligible set at rc2 is
4682 against 4345 on 0.55.2 (4118 identical, 31 reshaped, 196 removed,
533 added). Every stage passes: build under `--locked`, runner controls
including the join-order controls, oracle. Accounting at rc2: generated
1986, adapted 365, unsupported 2331, out of scope 710. Tally at rc2:
match 1079, row_order_differs 5, both_error 52, both_panic 13,
fixture_failed 1 (three runs: 1077/7, 1078/6, 1079/5, the joins and the
unique cases flapping). The row-order cases are joins (`full_join`,
`join` this run; all four in others), `LazyFrame::unique`,
`LazyFrame::unique_generic` and `Expr::unique`, all under the unordered
policy the rc2 file lists; the one setup failure is
`Fractions as PartialEq`, whose `new(2)` fixture is refused at rc2
("fractions must be between 0.0 and 1.0"), counted apart. `Expr::unique_stable` panics on both
sides at rc2 (`activate 'is_first_distinct'`), a feature-gate change.
The frozen manifest `probes/0074/frozen-experimental.json` records the
inputs of this run; the accepted 0074 `frozen.json` no longer matches the
tree because the extractor and generator changed, which is what that
check is for.

## Codex review round 1 (28e34de): three corrections

### R1: setup is an execution stage whose values the measured call uses

Every generated case now has two stages on each side, and the measured
call constructs nothing itself. The script's `pub fn setup()` builds
every fixture the call needs (receiver, arguments, operands) and returns
them as a vector; `pub fn main(__fx)` receives that vector and makes the
measured call on the prepared values (a two-call case gets a second,
separately prepared argument set for its second call and keeps the same
receiver). The Rust oracle function builds the same fixtures under
`catch_unwind` and returns `Staged::SetupFailed` if that fails; only
then does it run the call with those values and return `Staged::Ran`.
Each of the two Rust runs has its own staged setup. The runner compiles
the script first (a script that does not compile is `Broken` before any
setup), runs the script's setup and the first Rust run, and only when
both setups succeed passes the prepared values into `main`, runs the
oracle again, and classifies. `classify` takes the two `Setup` results
structurally: both failed is `FixtureFailed` (counted apart, never
verified, not a run failure); one failed is `SetupMismatch`, a failure. A
second Rust run whose setup fails is a disagreement with the first
(`Nondeterministic`), never a setup skip. No message text is inspected.
A constructor under test (`X::new`, `X as Default`) is never preflighted:
its case's setup holds only the arguments, or nothing.

Codex's round-2 reproduction (a fixture that succeeds once and panics
on reconstruction, approved as `BothPanic` with zero target calls) is no
longer expressible: there is no reconstruction, and a fixture that fails
in setup on both sides is `FixtureFailed` with zero target calls.

Controls, all passing:

- Unit (`oracle.rs`, 24 lib controls): both setups failed is
  `FixtureFailed`; one failed is `SetupMismatch` and not approved; a
  target panic saying `fixture:` on both sides is `BothPanic`, on one
  side `BindingPanicked`; a compile error with a failed or successful
  Rust setup is `Broken`; a wrong value with a failed second Rust run is
  not `FixtureFailed` and not approved.
- Integrated (generated runner), zero-call controls checked against
  `polars::engine_counts()` before and after: an untagged setup panic on
  both sides; a constructor returning an error unwrapped in setup on both
  sides; a first fixture that succeeds followed by a later fixture that
  fails, on both sides. Each is `FixtureFailed` with the engine count
  unchanged. One-sided: Rune setup failing alone and Rust setup failing
  alone are `SetupMismatch`; a non-compiling script with a failing Rust
  setup is `Broken`; a wrong value whose second Rust run panics with
  `fixture:` wording is `Nondeterministic`; a target panic that mentions
  `fixture:` is `OraclePanicked`; a first Rust run whose setup succeeds
  followed by a second run whose setup fails is `Nondeterministic`.

The 1057 cases regenerate in this shape and give the same tally (1000
match plus the unique flaps, 44 both_error, 11 both_panic, 0
fixture_failed, 0 setup_mismatch). At rc2 the one setup failure,
`Fractions as PartialEq`, is a structural `FixtureFailed`: its `new(2)`
is refused in both setup stages.

### R2: the release policy must belong to the inventory

The extractor copies the documentation run's provenance from `pins.json`
into `inventory.json` as `provenance` (`release` for a crates.io release,
`rev` for Git sources, plus `cfg`); the four 0072 configurations were
re-extracted and their summaries are byte-identical. Every release file
has a `[provenance]` table naming `release` or `rev`, and the generator
checks it against the inventory before generating anything: a missing
inventory provenance, a release file without provenance, or a mismatch
in either direction exits with `refusing to generate: …`. Controls:
`--self-test` covers match, other release, Git inventory against a
release-pinned file, no provenance on either side, and revision
mismatch; the adapter test `a_release_file_for_another_inventory_is_refused`
runs the committed generator on the 0.55.2 inventory with `rc2.toml` and
with `0.54.4.toml` and requires the refusal (this is Codex's
reproduction, kept as a test). The experimental rc2 run passes the check
through the `rev` the 0074 documentation run recorded.

### R3: the join policy matches the case's configuration exactly

Each `[[unordered]]` entry now records `args`, one entry per parameter of
the operation: the exact Rune fixture expression the case must pass, or
`*` for a parameter that cannot affect row order. `Release::policy` is
unordered only when the case's parameter names are exactly the recorded
ones and every non-`*` expression is equal; anything else is ordered
with a reason naming the parameter. `LazyFrame::join` requires
`args = "polars::JoinArgs::default_()"`; the three convenience joins and
the `unique` family list only `*` parameters, since they have no order
option. `--self-test` covers: the recorded recipe (unordered), an
explicit `Left` (ordered), a chained `None` then `Left` (ordered),
`unknown_nondefault_fixture()` (ordered), a missing or extra parameter
(ordered), an unlisted operation, and a same-named method elsewhere.

The integrated join controls no longer hardcode their policy: the
generator computes each control's decision through `Release::policy` on
the control's parameters, substitutes it into the runner, asserts at
generation time that the explicit-order control (j2) and the
unrecognized-configuration control (j4, `with_maintain_order(None)`,
semantically default but not the recorded recipe) are ordered, and
records the reason strings in the runner. The runner then checks that
permuting rows passes exactly when the policy said unordered (j1),
fails for j2, j4 and an unrelated ordered operation (j3), with the
duplicate-row and schema controls retained. Under `0.55.2.toml`, which
lists no joins, j1 is ordered and its expectation flips accordingly.
