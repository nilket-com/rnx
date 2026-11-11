# Implementation plan 0246: measured session parity classification

Status: Proposed

Paired ADR: ../adr/0246_session_parity_classification.md.

1. Pin cfdd6a76. Read the legacy session tests in src/session.rs, the
   transcript fixtures and demos, host_policy in src/routing_inventory.rs,
   and the differential table of ADR 0242.
2. Bring a short note before editing: the list of sources with input counts,
   which cannot be driven mechanically and why, where the table lives and
   its row format, and how the legacy tests' inputs are extracted without
   editing those tests.
3. Implement the classifier and the committed table. Reuse the transcript
   runner of ADR 0242 where it fits; do not add a second one.
4. If a Different result or Domain failure row appears, stop and report it
   with the input before pinning.
5. Add the negative controls. Run the required stages, preflight and the
   commit checker. Bring the packet with the summary table inline.

Results are added here by the implementer.
