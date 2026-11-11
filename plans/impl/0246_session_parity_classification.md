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

## Results

The reviewed literal-only extractor covers 100 of 125 eval/shown source sites
(80%). Computed loops, private-state tests and generated arguments are counted
and named, not interpreted. Unknown state-building events block subsequent
observations on that session until reset; the ledger reports those rows separately.
Fourteen of 29 source sequences are classified end-to-end. The 25-event session
and seven-row worker differential fixtures remain referenced with their existing
labels; none becomes a policy decision in this record.

The reproducible table is `tests/fixtures/session_parity.json`. It includes
121 literal-session observations, 63 host registrations, 13 stock notebook cells,
and the two requested constant probes. Totals: 116 Equal, 21 Wording, 27 host
refusals, one admission refusal, 16 admitted but not executed, and 18 not classified.
Twenty-five adapter notebook cells and terminal-driven sources are separately
named exclusions. Notebook cells compare stdout, values and diagnostics in their
own working directory. Host witnesses first prove legacy compilation; admitted
clock, filesystem and stdin calls are not executed. No network fixtures or adapter
builds are required.

Work groups: 22 synchronous host enrollments, five async registrations, one
vector-constant admission gap, and two diagnostic improvements. String constants
are accepted; vector constants are refused with an incorrect async description.
The string/int constant-redeclaration diagnostic loses expected/actual type
information. It is marked `domain_worse`, not discarded as harmless wording.
The two other reviewed wording rules are appended frames and equivalent missing-
method descriptions. Exact observed strings remain pinned. No Different result
or Domain-only failure occurred in the classified corpus.

The ordinary `session::parity::pinned_classification` test reproduces the table.
Controls verify literal extraction/order and the coverage floor, that removing
one real host witness's typed enrollment changes only that row, and that moving
a class fails the pin. Existing legacy tests are unchanged. All implementation
code is test-only; no engine, session admission, default, or runtime behavior is
changed. Required-stage acceptance is recorded in the publication packet.
