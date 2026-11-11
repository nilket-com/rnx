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
