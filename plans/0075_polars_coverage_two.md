# rnx 0075: Polars coverage, stage two

Status: revised after Codex's plan review (reviews/0075_review_codex.md), ready for impl.

## Problem

Record 0073 generated a third of the Rust Polars API for 0.55.2. Record
0074 ran that generator unchanged against the sources behind Polars 2.0
rc2 and found it held; what it found wanting was the generator's own
inputs and reach, not its rules: the API-crate subset is a constant that
a new upstream crate silently moves entries out of; the oracle's join
fixtures assumed a row order that Polars's documented default never
promised; 543 generated bindings have no fixture because the fixture
rules only know `Default` and unit variants; and 204 eligible entries are
unsupported because a type in their signature has no wrapper, most often
a concrete alias of a generic type such as `BooleanChunked` or `Schema`,
or a concrete type from an internal crate. This record addresses those
four, with production on 0.55.2 throughout.

## Decision

`tools/polars-gen` grows in four ways, each measured against the accepted
0.55.2 baseline (98885e0: 1622 generated, 832 oracle cases) and reported
as newly compiled bindings, newly oracle-verified bindings, and remaining
exceptions counted by reason. Predictions from 0072 are not restated as
coverage.

### 1. Release-specific inputs

The API-crate list and the unordered-operation list leave the generator's
source and become a per-release input file, `tools/polars-gen/releases/
<release>.toml`, named on the command line and recorded in `surface.json`.
For 0.55.2 the file reproduces today's constants, so the baseline is
unchanged by construction and the drift test proves it. For rc2 a second
file adds `polars_defs` and is used only by the 0074 probe, whose run
with it is labeled experimental under a fresh `frozen.json`.

Join ordering is settled on the oracle's side, per concrete case, not by
method name. The release file lists **canonical operations** (full paths,
never the short Rune name, so an unrelated method of the same name is
untouched) together with the **configuration** under which the Rust
contract leaves row order unspecified, and each generated case records
the fixture and options it ran with. Only a case whose recorded options
match a listed unordered configuration compares rows as a multiset, with
duplicates and the schema kept; a case that passes an explicit
`MaintainOrderJoin` other than `None`, and every non-join operation,
stays ordered. Justification is cited per operation from the pinned
0.55.2 sources: `JoinArgs::default()` sets `maintain_order` to
`MaintainOrderJoin::None`, and `inner_join`, `left_join` and `full_join`
build their arguments with `JoinArgs::new`; `cross_join` and
`JoinBuilder::finish` get their own citations or stay ordered. The
classifier and its unit controls do not change. New integrated controls:
permuting rows passes for a justified unordered join case, fails for a
join case whose options request an order, and fails for an unrelated
ordered operation.

Gate 1 makes two claims and proves them apart. First, with the release
file holding exactly today's constants, the generated module, accounting
and case policies are byte for byte unchanged apart from the recorded
release metadata; the drift test proves it. Then the intentional join
policy is applied and its effect reported on its own: which cases changed
policy, and that their outcomes on 0.55.2 are still matches.

### 2. Fixtures beyond `Default` and unit variants

The generator derives a fixture for a wrapped type, in this order and
recorded per type in `surface.json`:

1. a core fixture (the eight of 0073);
2. `Default`, or a unit variant, as today;
3. an associated constructor binding of the type itself, generated in the
   same run, whose parameters all have fixtures: `new` first, then
   `from_*`, then any other in name order; the fixture is the script call
   and the same Rust call;
4. for a data-carrying enum, the first variant in declaration order whose
   payload types all have fixtures;
5. for a trait-owned method, the first wrapped implementor, in canonical
   order, that has a fixture by rules 1 to 4.

Derivation is a fixpoint over the generated set and terminates because
each rule only consumes fixtures already derived. Selection is
deterministic and recorded per type in `surface.json`: the recipe, or
the reason none applies.

Fixture construction is setup, not the measured call, and it must
succeed on both sides before a target binding is credited. Each case
constructs its receiver and arguments first, in the script and in Rust;
if either construction errors or panics the case is recorded as
`fixture_failed` with the recipe named, the target binding is not called,
and the entry's disposition stays "no usable fixture"; it is never a
`both_error` or `both_panic`. A constructor recipe that fails on its own
arguments is not retried with another recipe silently: the failed recipe
is recorded, and the next recipe in the fixed order is tried only if the
release file allows it for that type, so the choice stays deterministic
and visible. The constructor's own case still verifies its error
behaviour. A successful fixture feeds the existing second-call and
mutation checks unchanged. Required control: a case whose setup fails
identically on both sides against a target that counts its invocations
must add zero verified bindings and show zero target calls.

The denominator for this gate is the **0.55.2** census, derived by the
same rule that 0074 used and published in the evidence before any work:
520 fixture-free generated bindings, of which 248 are structs with no
public fields and no `Default`, 130 enums whose every variant carries
data, 80 structs with public fields and no `Default`, and 62 trait-owned
methods. The rc2 census (260/142/78/63) belongs to the experimental run
and is reported there, never subtracted from.

### 3. Wrappers for argument and return types

Two kinds of type gain wrappers when a bound signature in the API crates
mentions them:

- a concrete alias of a generic type, such as `BooleanChunked`,
  `IdxCa`, `Schema` and `SchemaRef`, wrapped under the alias's name with
  the alias's spelling; methods *on* the generic type stay in the generic
  bucket and out of this record;
- a concrete struct or enum from an internal crate (`polars_utils`,
  `polars_io` internals, `polars_arrow`) that is not generic and has no
  lifetime, wrapped under `polars::<crate short name>::<Name>`, with the
  same `Default`, variant and constructor rules as any other wrapper.

Owners in internal crates remain out of scope; this gate binds
arguments and returns, and its measure is the number of API-crate entries
that move from `unsupported: unwrapped type` to generated, with the
compile and oracle numbers kept apart. Identity for a wrapper is the
canonical instantiated type (generic type plus its concrete arguments),
so two aliases of the same instantiation share one wrapper, and two
distinct types with the same short name get distinct Rune paths by a
deterministic rule recorded in `surface.json`. Controls: equivalent
aliases resolve to one wrapper; same-name distinct types resolve to two.

### 4. Evidence

`plans/0075_polars_coverage_two_evidence.md` reports, per gate and in
total against the 0073 baseline:

| number | how it is measured |
|---|---|
| newly compiled | generated entries in the committed module, by gate |
| newly oracle-verified | cases whose target call ran on both sides and had an approved outcome, by gate; setup failures excluded and counted apart |
| remaining exceptions | unsupported, fixture-free and fixture-failed entries by reason, counted, against the 0.55.2 denominators |
| unchanged | drift test, accounting partition, hand-written and presentation suites, controls |
| cost | cold launch with `probes/0073/launch.py`, before and after |
| adjacent release | `probes/0073/adjacent.sh` under its lock, with an explicit 0.54.4 release file |
| rc2, labeled experimental | the 0074 probe with the rc2 release file, under a separate experimental manifest and output location; 0074's accepted `frozen.json` and results are preserved |

Every run validates the release it was given against the source it
documents, and records the release file's contents digest, not only its
name.

## Gates

1. Release files for 0.55.2, 0.54.4 and rc2; drift proves 0.55.2
   unchanged with today's constants; then the join policy applied and
   reported apart, with its controls; rc2 experimental run under a
   separate manifest.
2. Fixture derivation rules 3 to 5 with setup separated from the measured
   call and its zero-call control; the 0.55.2 census before and after.
3. Alias and internal-crate wrappers with the identity controls; the
   unwrapped-type count before and after.
4. Evidence, cost, adjacent release, README.

## Out of scope

Dependency upgrades; the generic bucket (methods on generic owners,
generic parameters, `impl Trait` returns); callbacks; other feature sets;
protocols with no Rune equivalent; targets other than Linux x86_64.
