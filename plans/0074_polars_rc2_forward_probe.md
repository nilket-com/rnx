# rnx 0074: forward-upgrade probe against Polars 2.0 rc2

Status: ready for review.

## Problem

Record 0073 generated a third of the Rust Polars API for the pinned
crate 0.55.2 and measured one retrospective upgrade, 0.54.4 to 0.55.2,
under frozen rules. The claim the generator was built to earn is that it
keeps up with upstream. Polars announced Python 2.0.0rc2 on 2026-09-21,
tag `py-2.0.0-rc.2`, commit `da47b7405f0e2d188e71cce7c2d588c695f4098d`.
The Rust workspace on that commit is versioned 0.55.1 and nothing newer
than 0.55.2 is on crates.io, so the Rust changes behind 2.0 are only
reachable as a git source. This record runs the accepted 0073 generator,
unchanged, against that commit and reports what happens. It changes no
production dependency.

## Decision

A probe under `probes/0074/`, one command, producing
`plans/0074_polars_rc2_forward_probe_evidence.md`. Nothing in
`adapters/polars`, `tools/polars-gen` or any Cargo manifest outside the
probe changes; the adapter stays on 0.55.2 and the generator stays at
98885e0.

### Pins and locks

- Source: `git = "https://github.com/pola-rs/polars"`, `rev =
  "da47b7405f0e2d188e71cce7c2d588c695f4098d"`, spelled in full everywhere
  it appears. The tag name is recorded but the rev is what is pinned.
- Every cargo invocation in the probe runs `--locked` against a lock
  committed under `probes/0074/locks/`, created once on the first run and
  required unchanged on replay, as `probes/0072/inventory/doc.sh` and
  `probes/0073/adjacent.sh` already do. The 0.55.2 side is the existing
  locked documentation set and adapter.
- Toolchain: the same pinned nightly, `nightly-2026-09-20`, for rustdoc
  JSON; stable for everything else.

### Identical definitions

- Feature set: the adapter's, `default-features = false` with `lazy`, `csv`,
  `parquet`, on both sides. If a feature was renamed or removed at rc2, the
  probe records the resolved feature list of both sides and the difference
  is an API change, not a silent substitution.
- Inventory: the 0072 extractor at its committed revision, same crate
  selection rule, same rules, same API-crate subset, same exclusions.
  Denominators on both sides are reported with the same three numbers
  (gross, exclusions plus unknowns, eligible), and `unknown` is reported
  if any crate at rc2 fails to document.
- Generator: `tools/polars-gen` at 98885e0, same buckets, same
  hand-written wrapper mapping, same fixtures.

### What is measured, kept apart

1. **API delta**, inventory to inventory, keyed by canonical path and
   signature: entries added, removed, reshaped, and whose predicted bucket
   changed; wrapper types added or removed; entries whose accounting
   status changed under the unchanged generator.
2. **Compilation**: the generated module built into a scratch copy of the
   adapter pinned to the rc2 git source (the adapter's direct polars-*
   dependencies at the same rev); every error classified by the entry that
   produced it and the generator rule involved, as `probes/0073` does with
   `--check`. A failing build is reported as such and the probe continues
   to the report; it does not exit early.
3. **Oracle differences**: if the scratch adapter builds, the generated
   oracle test runs against rc2 and the outcomes are compared case by case
   with the 0.55.2 run: cases that stayed approved, cases that changed
   outcome, cases that no longer exist, with the classifier unchanged
   and its controls run first. If it does not build, this section says so
   and reports nothing else.
4. **Excluded cases**: entries the probe could not judge and why: no
   fixture, excluded oracle, out of scope, or a type that has no public
   path at rc2.

The unchanged run is reported first and in full before anything is fixed.

### Proposed fixes, separately

For each failure class, the generator or rule change that would address
it, as a proposal with an estimate of entries affected. Any experimental
rerun with such a change applied is labeled experimental, keeps its own
locks, and never replaces the unchanged run's numbers.

## Conclusion the evidence must reach

A recommendation for the next coverage record drawn from what rc2 did to
the surface: which rules held, which broke, and which shared conversions
or internal argument types would unlock the most entries per rule. It
must not assume the remaining two thirds are the same shape as the third
already generated.

## Out of scope

Bumping the adapter's Polars version, changing the generator, extending
coverage, callbacks, other feature sets, other targets.
