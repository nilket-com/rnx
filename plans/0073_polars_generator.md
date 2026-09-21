# rnx 0073: Polars binding generator, stage one

Status: ready for review.

## Problem

Record 0072 measured the Rust Polars API surface and proved, on compiled
and executed samples in two releases, that rules can bind its
mechanical, conversion and option-struct shapes from rustdoc alone. The
adapter still exposes eighteen hand-written entries out of 4345 eligible
callables in the API crates. The goal is full Rust API parity. This
record is its first stage: a generator that binds the three predicted
buckets, an accounting of every inventoried callable, and coverage
measured from what compiled and ran, never from the prediction. The
roughly seventy percent that 0072 predicted sets the scope of this stage;
it is not a coverage claim, and this record does not make one until the
bindings exist.

## Decision

A checked-in generator, `tools/polars-gen`, reads the 0072 inventory of a
locked documentation set and writes `adapters/polars/src/generated/`,
which is committed. The adapter keeps building on stable; regeneration is
an explicit, locked step, and a test fails when regenerating changes the
committed output (drift). Hand-written bindings stay as they are; where a
generated name would collide with one, the hand-written binding wins and
the entry is accounted as adapted.

### Accounting

`adapters/polars/surface.json`, written by the generator, lists every
eligible callable of the inventory for the API crates (`polars_core`,
`polars_plan`, `polars_lazy`, `polars_io`, `polars_ops`, `polars_time`,
`polars_dtype`, `polars_schema`, `polars_error`) with exactly one status:

- `generated`: bound by the generator, with the Rune path
- `adapted`: bound by hand, with the reason (collision, execution
  policy, presentation), and the file
- `unsupported`: not bound, with a reason code from a fixed list
  (callback, generic owner, generic parameter, return shape, foreign
  type, lifetime, unreachable trait, internal by convention, unsafe,
  hidden, field type, variant payload)

A test fails if any eligible callable is unaccounted for or has two
statuses. The internals reachable through the prelude (arrow, utils, row,
parquet, compute, config) are listed with status `out_of_scope` and are
not counted in coverage; that is a stated boundary, carried from 0072.

### Coverage, measured

Three numbers, reported per bucket and in total against the 0072 eligible
denominator, in `plans/0073_polars_generator_evidence.md`:

1. **compiled**: entries whose generated binding is in the committed
   module that builds; this is every `generated` entry, by construction
2. **executed**: entries with a generated oracle test, in the 0072 style
   (same call in Rune and in Rust on shared fixtures, receiver reuse
   asserted, expressions executed), that passed; fixtures cover the
   types the 0072 harness covers, extended where cheap, and entries
   without a fixture are reported as compiled-only, never as executed
3. **predicted**: the 0072 rule count, shown beside the other two and
   never added to them

### Policies

Ownership. `&self` borrows the wrapped value; `self` clones it, so the
script's value remains usable where Rust would have consumed it; `&mut
self` takes the script value mutably and mutates in place; borrowed
returns are cloned out. All wrapped Polars values are cheap to clone
(Arc-backed); the policy is stated in the adapter README with that
justification.

Conversions. Scalars map to Rune `i64`, `f64`, `bool` and `String`;
narrower integers are checked on the way in and an out-of-range value is
an error naming the parameter; `usize` and `IdxSize` the same; `f32`
widens. `&str`, `String`, `PlSmallStr` and `impl AsRef<str>` take a Rune
string. `Option<T>` is a Rune `Option`; `Vec<T>`, `&[T]`, `impl IntoVec<T>`
and `impl IntoIterator<Item = T>` take a Rune vector; tuples are Rune
tuples; a `HashMap` with string keys is a Rune object. `impl Into<T>` takes
whatever `T` takes. Every reachable concrete struct and enum that a bound
signature mentions gets a wrapper type; the Rune path of a wrapper and of
a free function is its shortest public non-prelude path with the crate
prefix mapped under `polars::`, which is how a Rust user spells it, and
the accounting lists each path.

Option structs and enums. A struct with public fields gets a constructor
from `Default` where `Default` exists, a chainable `with_<field>` setter and
a getter for each field whose type is bound; fields of unbound type are
listed. A unit-only enum gets a wrapper with one associated constant per
variant; a data-carrying enum gets one constructor function per variant
whose payload types are bound, and other variants are listed.

Errors. `PolarsResult<T>` becomes a Rune `Result` whose error is a
`polars::Error` value with `kind()` and a display string, so scripts can
match on the kinds Rust matches on. A Polars panic is not caught by
generated code; execution policy below says where panics are contained.

Execution. Calls that run a query or touch I/O go through the adapter's
engine thread as the hand-written `collect` does today, so no Polars work
runs on the session's runtime thread: the list is generated by name
(`collect*`, `fetch*`, `sink*`, `scan*`, `read*`, `write*`, `execute*`) plus
an explicit hand-maintained addition list, and the accounting records
which entries are routed. Everything else calls directly, as Rust does.

Callbacks. Not bound in this stage. The policy for the next stage is
0072's: a Rune function is converted with `into_sync` inside the wrapper,
a capture that is not a constant is refused with an error, and a
callback error surfaces as a Polars compute error where the closure
signature is fallible.

Documentation. Each generated binding carries the first paragraph of its
rustdoc as its catalogue entry, so the session's help shows what Rust's
does.

### Cost

Registering thousands of functions in a Rune context is a startup cost
that rnx has spent five records lowering. Gate 1 measures it before
anything is generated: context construction and `:dep polars` attach with
N stub functions for N in 100, 1000, 3000. The record's budget is stated
there against the 0069 and 0070 baselines; if eager registration breaks
it, registration becomes lazy per type, and the gate says which.

### Regeneration and drift

The generator's input is `probes/0072/out/0.55.2-adapter/result/
inventory.json` produced under the saved lock; the generator's own
dependencies are locked; the drift test regenerates into a temporary
directory and compares byte for byte. The adjacent-release check
generates from the 0.54.4 inventory and type-checks the adapter against
0.54.4 in a scratch crate, reporting entries added, removed and reshaped,
and any generated file that differs. Hand-written bindings and the
presentation suite must pass unchanged.

## Gates

1. Registration cost probe, budget stated, eager or lazy decided.
2. Mechanical bucket generated for the API crates: builds, drift test,
   accounting complete, coverage numbers 1 and 3.
3. Conversion and option-struct buckets, enum constructors, error type,
   execution routing, catalogue docs.
4. Generated oracle tests and coverage number 2; adjacent-release check;
   startup, binary size and compile time measured against the current
   adapter; evidence written.

## Out of scope

Callbacks (next stage), generic owners and parameters, the internals
reachable through the prelude, Python-style ergonomics, targets other
than Linux x86_64. The full feature configuration is inventoried and
accounted but not generated in this stage; the adapter's feature set
stays `lazy`, `csv`, `parquet`.
