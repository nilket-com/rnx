# rnx 0186: pin the current Rune fork on stock execution

The user decided to pin rnx now and keep tuning. This record advances the dependency to
`f98eb825934d4994478a1759cf340e90d8c5f12f` and preserves stock VM execution. It makes no
wall-clock qualification claim: screen 2 was invalid because three A/A confidence
intervals did not fit their precision bands. Fresh/native default routing is separate.

## Implementation

Move the root and project-tool Rune revisions together; let Cargo update all eight
current lockfiles, leaving historical probes alone. Preserve hard tabs and existing
features. Replace friendly method-error text parsing with `missing_instance_function()`:
prove the method using the receiver and method hashes, display `instance_name()`, and
identify Result by its type hash. Remove `type_hash_of`; pass the original VmError through
the runner. Test chained `counts.values().sum()` end to end and retain panic/fallback and
source-origin coverage.

Render diagnostic source tabs at four-column stops, relative to the source line, using
the same helper for the excerpt and caret prefix. Measure the display width of escaped
chunks between tabs (wide and combining characters included), without repeatedly scanning
the full line. Other terminal control escaping remains unchanged. Test mixed tabs/spaces,
wide text and escaped controls.

Absorb stock strict-bool conditions, sorted Object Debug, operator/field/bounds wording,
leading-free diagnostic type paths and streaming println behavior. Regenerate demo and
notebook outputs through the checked tooling. Review every golden change as sorted Debug,
strict bool, wording, type paths or an explicitly explained other change.

## Validation and landing

Run formatting, Clippy, the full root suite, focused capture/delivery checks, notebook
checks and the affected standalone workspace checks. Preserve failed attempts and exact
commands locally. Keep all current crates on one revision; check the diff for unwanted
branding and feature/default-routing changes. Commit this plan and the implementation as
two signed, subject-only commits. Claude reviews the implementation and evidence before
requesting the user's rnx push authorization.

Work in the new rnx-0186 worktree from origin/main927be8b. The user's unpushed main
checkout is untouched. Reuse the existing shared Cargo target. The clean, fully contained
rnx-0185 worktree remains temporarily because frozen Rune replay evidence names its
external corpus path; no worktree removal is part of this implementation.
