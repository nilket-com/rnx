# Implementation plan 0245: session error frames in input coordinates

Status: Proposed

Paired ADR: ../adr/0245_session_frames_in_input_coordinates.md.

1. Pin fa6ad468. Read the failure path in src/session/domain_session.rs, the
   Generated source maps in src/session.rs, CrossFailure in the domain API,
   and failure() in src/worker.rs.
2. Implement the frame renderer and, only if needed, the primary-diagnostic
   accessor. State in the results the omission rule for wrapper frames and
   the frame bound, with the reason for the number.
3. Update the transcript and worker tables and add the `<memory>` guard.
4. If tools/column is touched, also make miri_exact accept --package for
   root rnx tests. Otherwise leave it.
5. Run the required stages, preflight and the commit checker. Bring the
   packet to review with the changed rows inline.

Results are added here by the implementer.

## Implementation results

CrossFailure::primary_message exposes only the primary diagnostic while retaining
its existing borrowed-program lifetime. message() remains unchanged. Session
frames are rendered from the structured frame's own program/source/revision and
that revision's Generated map, before the failure borrow is dropped.

Omission rule: a frame in a known source with a retained map but outside every
user segment is generated wrapper text and is omitted. Missing locations,
source metadata, maps or retained input text instead say `unavailable source`.
User frames keep captured order. The renderer bounds output to 32 visible frames
and 16 KiB including a truncation marker: enough caller context without allowing
an unbounded recursive stack or panic payload to dominate an interactive reply.
The worker's existing tighter transport bounds still apply. No new protocol field.

Four Wording transcript rows now pin mapped input coordinates, including older
numbering labels. Controls cover three-input chains, old closures after function
redefinition and renumber, reset, frame-count/UTF-8 byte cuts, wrapper-only omission
and unavailable sources. The worker comparison pins mapped frames against the
same legacy failure; transcript and worker tables reject generated memory labels.
Legacy presentation and the fixed operation loop are untouched. No new unsafe or
ownership operations; Miri is not extended for this presentation-only change.

Follow-ups: measured parity classification is still the recommended next record
0246. Generic miri_exact root-package harness discovery remains a tooling follow-up;
this record does not edit tools/column.

An unrelated engine-crate addition moved an inlining boundary in the fixed route
by 1.2% on sieve growth; follow-up is to pin that boundary. The bounded correction
marks the new accessor and its Display::fmt inline, without editing collection
or heap helpers; the normal performance rules still apply.

The reviewed identity-parser fix preserves hashes inside symbols and strips only
whitespace-prefixed address comments outside symbols. Direct closure branches,
RIP comments containing closure names and refusal of unresolved indirect branches
have controls. On the fa6ad468 baseline binary, all 203,575 instruction rows the
old resolver accepted produce identical corrected rows (diagnostic comparison
retains each symbol's original bounds and disables only whole-symbol byte-total
checking to compare individual rows). No previously resolved row changed.

Prepared Miri harness discovery now takes the selected package (default rune),
validates its name and uses its Cargo crate spelling. Root rnx and incremental
path rejection controls pass; the retained real rnx harness from 0244 is found.
No Miri semantics, inventory or reuse-key relaxation was made.

Executor identity is informational in the fixed-route gate; acceptance is the
source condition plus instruction-count bands. Reviewed unsupported instruction
forms produce FAIL with an explicit reason, first symbol/instruction and affected
symbol count, without an error key. Kernel identity remains a hard failure for
those forms; tooling errors, incomplete disassembly and unexpected exceptions
remain fatal. Follow-up: either give identity a job (such as capture reuse for
identical routines) or remove the observation; neither is part of this record.

The final reviewed observation rule treats unsupported binary content as identity
not established (FAIL without error), not as permission to compare only a subset.
Replay on the retained FINAL-v3 pair completed without a tool error:
- Unreviewed instruction/target forms: 571 distinct symbol names.
- Duplicate symbol names with no reviewed pairing rule: 16 distinct names.
All disassembled bytes must be covered before classifying instruction forms;
ELF/tool failures and internal invariant failures remain fatal. Kernel identity
still fails hard. Source condition plus the unchanged bands decide acceptance.
