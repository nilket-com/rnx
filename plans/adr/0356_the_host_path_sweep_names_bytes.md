# ADR 0356: The host-path sweep names Bytes

Status: Proposed

Base: published 0179440d (ADR 0355).

## Problem

1. The next record adds the Bytes payload to the new engine with
   no way to construct a Bytes value. Its pre-sign run passed
   rune-suite and root-tests, but the cross stage failed: the
   host-path sweep reads the `TypeName` enum from source and
   refuses any kind without a script witness ("no host-path
   witness for heap kind Bytes").
2. The witness belongs in
   `src/verification/host_paths/inputs.rs`, a module compiled
   only under `cfg(all(test, feature = "cross-verification"))`.
   The ADR 0355 structural checker recognizes only `cfg(test)`
   as test-only, so it classifies the witness as production code
   outside the representation-only shape and refuses the whole
   delta.

## Decision

1. Add one arm to the sweep's witness table: `"Bytes" =>
   "b\"probe\""`. No other change.
2. The arm is dormant on this base: the sweep iterates the
   `TypeName` variants that exist, and there is no Bytes variant
   yet.
3. The gate tools and the structural checker do not change. The
   payload record that follows binds its own proof to this
   record's publication as its measured baseline.

## Controls

1. The cross stage passes on this record.
2. With the payload applied on top, cross passes with Bytes in
   the kind matrix and every Bytes substitution refused, none
   unexpected.
3. One complete final.
