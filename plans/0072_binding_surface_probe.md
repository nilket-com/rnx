# rnx 0072: binding surface probe

Status: revised after Codex review (reviews/0072_review_codex.md), ready for impl.

## Problem

Record 0058 exposed Polars through a hand-written adapter: four wrapped
types and about eighteen registered entries, each a Rust function with a
Rune signature. Nothing in a linked crate is visible to a script unless
an adapter registers it; there is no reflection and no generator.

The target is now parity with the Rust Polars API. Ninety percent would
be valuable; one hundred percent is the claim worth making. The concern
is the cost of hand-written wrappers: every entry is written, converted,
error-mapped, documented and kept in step with Polars releases by hand.
How large that surface is, and how much of it a generator could bind
without hand work, is unmeasured. This record measures it. It grows the
adapter by nothing.

## Hypothesis

A reusable generator, fed by the crate's own API description, can expose
most of the Rust Polars API faithfully, with a manageable and explicitly
listed set of exceptions.

## Denominator

A parity claim is only checkable against a complete, defined count, so
the denominator is a measurement contract, not a sample.

**Source.** Rustdoc JSON, produced by a pinned nightly with
`-Z unstable-options --output-format json`. `cargo rustdoc` documents
only the selected crate, so the extractor runs it for `polars` and for
every crate the facade re-exports from (`polars-core`, `polars-lazy`,
`polars-plan`, `polars-io`, `polars-ops`, `polars-time`, and any others
found by following `pub use` chains), then resolves paths across the
resulting indexes. The nightly version, rustdoc JSON format version,
target triple, and both Polars releases are recorded in the evidence.

**Reachability.** An item counts when it is reachable by a public path
from the `polars` crate root, including through `pub use` of individual
items, whole modules, and whole crates, and through `prelude`. Items
public inside a dependency but not re-exported by the facade are not
reachable and are listed in the gross inventory only.

**Identity and deduplication.** The canonical identity of an item is its
defining crate and definition id, not the path it was found by. A method
re-exported under three paths counts once. Generic declarations count
once; instantiations are not enumerated. Methods provided by a trait
count once per (trait, method), with the set of reachable implementing
types listed beside them; blanket and default methods are attributed to
the trait, not multiplied across implementors.

**Totals.** Callable operations (inherent methods, trait methods, free
functions, constructors, operator impls) and supporting items (types,
enum variants, public fields, constants, macros) are separate totals.
Constants and macros are counted even though the probe does not propose
binding them.

**Unknown is not excluded.** Any item whose signature the extractor
cannot resolve is reported as unknown, with its path, in its own total.
Exclusions (unsafe, `#[doc(hidden)]`, unstable, target-specific) are
reported in another total with the reason per item. The evidence
reports three numbers and never conflates them: gross public inventory,
exclusions plus unknowns, and eligible denominator. One hundred percent
of the eligible denominator is parity with the eligible surface, and
the evidence says so in those words.

**Feature sets.** Two inventories: the adapter's configuration
(`default-features = false`, features `lazy`, `csv`, `parquet`), and the
crate's full feature set. If the full-feature build fails to document,
the evidence records which features were dropped and labels the result
a documented configuration union, never "complete". Target is
`x86_64-unknown-linux-gnu`; target-specific surface elsewhere is labeled
unmeasured.

**Extraction control.** Before the real run, the extractor is tested on
a small crate written for the purpose that contains: an item re-exported
from a dependency, the same item re-exported twice under different
paths, a whole-crate re-export, inherent methods, trait methods with
default bodies, a blanket impl, a feature-gated item, a doc-hidden
public item, and an unsafe public function. The expected counts are
written down first; the extractor must reproduce them. Only then does it
run on Polars.

## Classification

Every eligible callable is placed by rule into one of:

1. **mechanical**: receiver, scalar and `Expr`/`Series`/frame arguments,
   plain return
2. **mechanical with conversion**: `impl Into<Expr>`, `IntoVec`, `&str`
   and `AsRef<str>`, `Option<T>` of the above; each needs a named
   conversion rule
3. **option struct**: takes a builder or options type; needs a Rune-side
   constructor for that type, then a rule from 1 or 2
4. **callback**: takes a closure (`map`, `apply`, `map_batches`, ...);
   needs a Rune function bridge
5. **generic or trait bound** that no rule reduces
6. **unsupported**, with the reason

The rules are judgments. They are written down as a list, each with the
signature shapes it matches, and the classifier applies that list so it
re-runs identically on every release. Any manual override is recorded
with the entry and the reason. Counts produced by the rules are
reported as **predicted** bindable counts.

## Demonstrated, kept apart from predicted

A successful sample verifies that sample, not its bucket. The evidence
reports predicted counts and demonstrated counts as separate columns and
never adds them.

**Samples** are selected deterministically: the eligible entries in each
of buckets 1 to 4 are sorted by canonical identity, grouped by distinct
signature and ownership shape (`self`, `&self`, `&mut self`, by-value
argument, borrowed argument, `Option`, `Vec`), and the first entry of
each shape is taken until the quota is met: twenty from bucket 1, ten
from each of 2 to 4. Every conversion rule claimed in bucket 2 must
appear in at least one sample. If a bucket has fewer entries than its
quota, all of it is sampled and the evidence says so.

**Proof** for each sample is a generated binding that compiles and
executes in a session against the existing fixtures, compared with a
direct Rust program calling the same Polars operation on the same
input. Compared: the observable value or error, and whether the
receiver was consumed or reused as in Rust. Callback samples must show
invocation from Polars, error propagation back into Rune, and any
`Send`/`Sync` or deferred-execution constraint that was hit, with how it
was handled.

**Failure** of a sample refines or splits the rule that claimed it. The
entries that rule had covered become provisional, not unsupported, and
the split is recorded. A failed sample does not condemn its bucket.

## Maintenance

Rules are frozen after the 0.55.2 run. The extractor and classifier
then run unchanged against the adjacent release on the same track. The
evidence reports, in order: the change in each denominator total; the
classification result under frozen rules; and only then the overrides
and rule changes needed to restore the 0.55.2 samples. This is one
observed upgrade, reported as such, not a bound on future upgrade cost.

## Deliverables

- `probes/0072/`: extraction control crate, extractor, classifier,
  sample generator, Rust oracle programs; one command re-runs it all
- `plans/0072_binding_surface_evidence.md`: pins; the three denominator
  totals per feature set with exclusion and unknown lists; the rule
  list; predicted and demonstrated tables side by side; per-sample
  oracle comparison; bucket 4 and 5 case notes with an estimate each;
  the adjacent-release tables; the answer to the hypothesis
- a recommendation: generator record, scoped adapter, or a mix, with the
  exception list sized

## Out of scope

No adapter growth, no generator productization, no Python-parity
ergonomics, no notebook work, no targets other than Linux x86_64. Parity
means the Rust API; a Rune script that reads like the Rust program is
the point.
