# ADR 0276: gate gaps found by recent records

Status: Proposed

Base: published 4d1cf05ede73244621d57caf2fa2d2773d1d60fb (ADR 0275).

## Problem

Records 0274 and 0275 exposed three gaps in the gate tooling.

A required final does not run the root format and lint checks when
root sources change. ADR 0274 changed root sources, passed its
required final and was published with a lint finding; ADR 0275 was
signed with three misformatted files. Both were caught only because
ADR 0275 happened to need a complete final. A cheap check that only
the expensive route runs is found late or not at all.

The failure bundle of ADR 0271 can hold the tail and not the cause.
For a failed test stage it kept the end of the log, which was the
summary after the failing test's own output. For a failed cross stage
it kept a receipt projection with no error, while the cause, a census
identity mismatch naming a file, sat in a separate diagnostics file
that the bundle did not mention.

The pinned parity table is 163,452 bytes under a cap of 163,840. The
next record that adds rows cannot be committed. The table stores, for
every row, the legacy and the execution observation in full, including
when they are equal.

## Decision

1. The stages a required final selects are closed under this rule: a
   change to sources of a package selects that package's format and
   lint stages, as it already selects its tests. The implementer
   states the rule in the selection code's own terms, lists every
   package and path class it now covers, and shows by a table which
   stages recent records would have selected under the old and new
   rule.
2. A failure bundle names the cause. For a stage whose child is a
   test runner, the bundle holds each failing test's own output
   section ahead of any tail. For a stage whose producer writes a
   diagnostics file or a failing-case list, the bundle holds that
   text. The top-level error is the first cause line, not a generic
   sentence, whenever the stage produced one. The bounds, sanitizer
   and release order of ADR 0271 are unchanged.
3. The parity table gets room without losing information and without
   becoming a large file. The implementer measures where the bytes go
   and proposes the representation; a row whose two observations are
   identical need not store them twice, and a class that requires
   equality should not be able to hold two different values. One row
   per line stays. The cap changes only if the measurement shows the
   representation cannot give at least a doubling of row headroom,
   and then to a stated number with its reason. Every row's class and
   observations after the change equal those before, shown by a
   committed comparison, not by inspection.

## Constraints

- No gate becomes easier to pass. No band, policy file or inventory
  changes. Stage selection only grows.
- The parity classifier's decisions do not change; only how rows are
  stored. Mutation controls on pinned pairs keep working on the new
  form.
- The rules live in committed tools and are exercised by their tests.
- No product source changes.

## Controls

- Selection: a root source edit selects root format and lint; the
  same for each other package; a documentation-only edit selects no
  more than today.
- Bundle: an injected failing test in the middle of a long log, and
  an injected cross identity mismatch, each leave the cause readable
  after release and as the top-level error; a mutation that reverts
  to tail-only fails.
- Table: round trip of the whole table between the old and new form
  is exact; a row that claims equality with differing values cannot
  be written; the size before and after and the headroom in rows.

## Out of scope

What the parity table measures, and retention of passing evidence.
