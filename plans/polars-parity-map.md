# Polars parity: remaining-work map

Written 2026-09-22 after record 0077 (300e867, awaiting review), revised
after Codex's reading of the first draft (the ceiling claim withdrawn,
the adapted entries accounted for, dtype expansion separated from
coverage), from
`adapters/polars/surface.json` on Polars 0.55.2 under the adapter's
feature set (`lazy`, `csv`, `parquet`). Every number below is a count of
inventory callables (distinct API operations), not bindings; 4345 is
the eligible denominator the records have used since 0073 (the 710
internal-crate callables reachable through the prelude are counted
apart, as out of scope).

## Scoreboard

Three numbers, from `probes/0077/scoreboard.py` on the committed surface
and oracle results (re-runnable; the definitions are in the script):

| | operations |
|---|---:|
| counted (API crates; 710 internal-crate callables apart) | 4345 |
| available to a script | 1939 (44%) |
| value-tested on at least one receiver, by the oracle | 1200 (27%), plus 9 hand-written equivalents evidenced by the hand-written suites |
| remaining: unsupported operations, each with a reason | 2106 (48%) |
| marker entries with no operation of their own, reported apart | 300 |

Available means generated plus the nine operations a hand-written
binding already provides; value-tested means at least one oracle case
with a value match, error-only and panic-only verifications excluded,
which is why it is lower than the 1288 operations verified on any
outcome. The 300 markers are not missing functionality.

## Where the 4345 stand

| status | callables | share |
|---|---:|---:|
| generated (bound, at least one receiver) | 1930 | 44.4% |
| of which oracle-verified on at least one receiver | 1288 | 29.6% |
| adapted: marker impls (`StructuralPartialEq` 126, `Eq` 99, `Copy` 75) that add no operation of their own, the equality and cloning they mark being bound as the `PartialEq` and `Clone` protocols | 300 | 6.9% |
| adapted: a hand-written binding of the same name (8) or protocol (1) already provides the operation | 9 | 0.2% |
| unsupported, with a reason | 2106 | 48.5% |

## The 2106 unsupported, by the route that would reach them

Expected yield is an estimate of callables a record could bind **under
today's rules**, judged from the reason texts and what the machinery
already does; it is not a measurement, and it is not a bound on what is
possible. "Not routed today" means the map names no rule for the class;
each such class needs its own decision (async execution, a hashing
function, ownership transfer, exposing an internal type), and any of
them may turn out worth a record.

| class | callables | route | expected yield | cost |
|---|---:|---|---:|---|
| `From` impls | 115 | bind as `from_<type>` constructors, mechanical | ~90 | low |
| assignment operators (`+=`, `&=`, …), `Not` | 28 | Rune assign and unary protocols, mechanical | ~25 | low |
| receiver consumes a non-`Clone` type (builders in `polars_plan`) | 76 | move out of the Rune value ("take" semantics, value unusable after) | ~60 | medium, a semantics decision |
| lifetime owners (`AnyValue<'a>` and kin) | 56 | an owned, lifetime-erased wrapper (`into_static`) | ~45 | medium |
| function-level generics with closure parameters (`apply`, `map`, `fold`, …) | ~100 of 164 | callbacks through `SyncFunction`, proven in 0072 | ~70 | medium |
| generic readers and writers (`CsvWriter<W>`, `ParquetReader<R>`, `IpcReader`, …) | ~63 | instantiate on `File` and an in-memory buffer, the 0076 engine | ~45 | medium |
| `T3` returns that are not iterators (borrows into non-polars types, `impl` non-iterator returns) | 171 | clone-out rules per type | ~50 | medium to high |
| remaining `ChunkedArray`/`Logical` methods (0076 census: 330 refused pairs, 676 fn-generic pairs) | 174 | arrow-internal items, closures | ~30 | high |
| `F1` trait impls in the generic bucket (`IntoIterator`, generic `From<T>`, `Deref` on other types) | 325 | case by case | ~40 | high |
| types not wrapped (`no wrapped implementor` 114, `owner not wrapped` 80, `owner has no public path` 28) | 222 | wrappers for reader/writer builders and private-path types where a public path exists | ~60 | medium to high |
| foreign types, uninferable generics, mutable references, arity, misc | ~110 | none general | ~15 | high |
| `Hash` 114, `TrivialClone` 75, `Drop` 5, flags markers 8, `async` 21, `unsafe` and `_`-internal callables (already in the unsupported bucket) | ~410 | not routed today; each class is a decision (a `hash()` function, an async execution model, an unsafe policy), not a given exclusion | ? | decision first |

Sum of the expected yields under today's rules: about 530 callables,
which would take coverage from 44% to about 57% of the counted API at
the cost of eight or nine records like 0074 to 0077. That is a
projection of the listed routes, not a ceiling: the classes marked
"decision first" are not counted in it, and a decision to route any of
them changes the sum.

Two facts about the denominator matter for any claim. The 4345 include
about 410 callables that have no script-level route today (derive
markers, `Hash`, `Drop`, `unsafe`, underscore-internal); whether they
should count is itself a decision. And bindings are not coverage: 2580
bindings serve 1930 operations, and "verified" (1742 cases) counts error
and panic behaviour as well as value matches (1624).

Feature expansion for the narrow integer dtypes is a verification
matter, not a coverage one: the aliases' bindings already compile, and
enabling `dtype-i8` and kin would let their 156 setup-failing cases run.
It adds distinct operations only where a method exists solely on those
types, which the census does not show.

## What the last four records bought, per distinct operation

| record | new operations | what the work mostly was |
|---|---:|---|
| 0074 | 0 | forward probe against rc2 |
| 0075 | +191 | release inputs, fixture recipes, alias and internal-crate wrappers |
| 0076 | +109 | accounting, null fixture, deref route, applicability engine, instantiations |
| 0077 | +8 | bounded materialization, structural nested comparators, framed oracle |

The yield per record is falling as the machinery matures; the classes
above say why: what is left is either mechanical and small (`From`,
operators) or needs a new semantic decision each (moves, callbacks,
lifetimes).

## Options, by measured yield under today's rules

These are the routes in the order of yield against cost; the choice
between them, and whether to continue toward parity or to stop at a
measured point, is the team's, not this map's.

1. A mechanical record: `From` constructors plus assignment and unary
   operators. Validated against the census before any implementation:
   of the 115 `From` impls, 109 have a wrapped owner and a source the
   mapping rules already take (85 wrapped types, 21 scalars, 3
   containers) and 6 do not; all 28 operators (`SubAssign`,
   `BitAndAssign`, `BitOrAssign`, `BitXorAssign` on the flag types and
   selectors with a `Self` right-hand side, and `Not`) are mappable.
   Predicted: up to 137 operations before name collisions, no new
   semantics.
2. Two decision records with a plan Codex reviews first: consuming
   receivers (take semantics, about 60) and closure callbacks (about
   70). Both change what a script can observe.
3. Readers and writers, lifetime-erased values, non-iterator borrows,
   about 140 together, each a record.
4. The classes not routed today (async, hashing, unsafe, internal types,
   about 410): each starts with a decision about whether and how it
   belongs in a script, then a plan.

## Proposed criteria for each record

- Its plan predicts the distinct operations it will bind, checked
  against the census before the impl starts.
- It reports distinct operations bound, cases verified with value
  matches apart from error-only, and remaining exceptions by reason,
  against the same 0.55.2 inventory.
- Cold launch stays within +5 ms per record and +20 ms over the 0073
  baseline in total.
- A parity claim, whenever made, names the denominator and lists the
  exclusions with their reasons: "N of the M operations counted are
  bound; the rest are in `surface.json` with a reason each."
