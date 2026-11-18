# ADR 0296: Fixed-route policy, scoring and anchor cache in Rust

Status: Proposed

Base: published 96a809fe09fed4f9a19487854f4d4de5c3784c92 (ADR 0295).

## Problem

`fixed-route` is the final's last Python stage program. Its measurement
policy lives in two helpers. `tools/column/drift_band.py` validates the
policy, checks the anchor's approved lineage, conditions on unchanged
fixed source, and scores the per-record and cumulative bands.
`tools/column/anchor_cache.py` keeps the one owned anchor bundle under a
lock, with a crash-recoverable ownership journal. Both are
measurement-sensitive, so they move first and alone, before the
orchestrator (ADR 0297).

## Decision

1. `tools/final/src/fixed_route.rs` ports both, with the same semantics,
   refusal messages and shapes:
   - the policy validation, with the kernel list of
     `old_path_programs.KERNELS`;
   - the anchor transition and its approval: a full anchor commit, the
     baseline policy, a structured approval whose plan commit is an
     ancestor, a `plan: adr` subject, signature-verified, and declaring
     the transition;
   - the source condition: every inventory item's canonical text, from
     the given guard program, unchanged between baseline and candidate;
   - the case index, the band comparison (level and growth, W and T, in
     per mille, integer arithmetic), the measurement recipe and the score;
   - the anchor cache: the same root, the same `OWNERS.json` (schema 1) and
     `ANCHOR.json` (schema 2) shapes, the exclusive lock held by the
     consumer, recovery of every non-current or incomplete entry when the
     lock is taken, identity-checked removal, lookup (hit, absent, key
     differs, verification failed), verification and publication through
     pending, publishing and complete states.
   A bundle or journal written by either implementation is valid for the
   other, so the switch in 0297 needs no cache reset.
2. Nothing runs the port yet: the Python orchestrator keeps its helpers
   until 0297, which ports it onto these and retires `drift_band.py`,
   `anchor_cache.py` and their tests. This record changes no stage.

## Constraints

- The same integer band arithmetic and limits; no tolerance changes.
- No new crate, no new Python file.

## Controls

- Witnesses from the published Python at 96a809fe with controlled
  values: every case of `test_drift_band.py` and `test_anchor_cache.py`
  plus boundary cases:
  - the band at exact limits and one past;
  - every refusal of validation, index and score;
  - transitions bootstrap, unchanged, linked and skipping;
  - approval over the repository's own signed history (the live policy's
    reviewed transition) and planted refusals;
  - the source condition over planted sources with a planted guard;
  - the cache's journal and results through publish, hit, miss, tamper,
    corruption, interrupted publication, root replacement and links.
- Interchangeability: the Rust cache's journal and bundle equal the
  Python witness's, modulo identifiers and paths.
- A mutation per refusal family fails its control.
- The required final.
