# Implementation plan 0250: owned tree input for json::stringify

Status: Implemented

Paired ADR: ../adr/0250_owned_tree_input_for_json_stringify.md.

1. Pin 9104d9c3. Read the bridge's input and snapshot helpers and the
   witness path, the 0249 alternate-handler and ExecutionRuntime code, the
   walk and serializer in src/json.rs with their tests, and the store and
   session limits.
2. Before editing, send: the representation of the input marker and why it
   cannot be declared as an output; whether the export decision travels by
   type or by reachability, and where; the byte bound and the existing
   limit it derives from, with the size of the largest value legacy
   serializes that the bound would refuse; and the shape of the shared
   view. Wait for the ruling.
3. Implement the export and the shared walk. Keep JSON policy out of the
   engine's exporter.
4. Add the differential controls of the ADR, the negative controls for
   every existing argument kind, the resource refusal, and the Miri tests.
   Regenerate the pinned table.
5. Send the working tree for source review before signing. Then run the
   required stages, preflight and the commit checker, and bring the packet
   with the before and after class totals. The candidate is frozen once
   the packet is sent.

Results

- Reviewed use-after-move decision: access errors propagate before the adapter
  body, including nested moved nodes. Legacy stringify reports an external
  reference serialization Err instead. The parity table records this as an
  Intentional difference under the exact `use-after-move` rule, with full
  outcome/message/origin matching and mutation controls. No unreadable-value
  descriptor is added; legacy behavior is unchanged.
- The export ceiling is Store::limits.bytes, whose default is isize::MAX.
  It is not a practical default session memory limit. Wiring the session
  ceiling to Store bytes is a follow-up, outside this record.

The reviewed source fixes use one-pass child visitors for both representations;
legacy vectors, tuples, and objects borrow once per container. A counted
100,000-element control requires one iterator pass and exactly N steps for
each shape. The old object snapshot already avoided the dormant nth branch;
the unified visitor removes that branch and the per-child reborrow paths.
DepthCut explicitly represents an unread node, and the serializer rejects it.
The validator retains the Option-chain stack-overflow rationale at its entry.

Byte accounting alone reports the owned-input byte limit. Invalid child or
diagnostic metadata reports InvalidCode; a description exceeding the 1024-byte
stack buffer reports its diagnostic-buffer limit. A nominal name that cannot
be copied by the existing 256-byte metadata API reports that limitation,
never the byte-budget error. Macro and JSON-body indentation uses tabs.

Development validation passed: ownership/export controls, allocation failure
and peak accounting, root/nested moved-value rules and negative mutations,
all root library tests, Rune lib/test clippy, and fixed-source/copy guards.
The parity pin changes only stringify among existing host rows: Equal135->136,
host refusals14->13; three new moved-value probes are Intentional difference.
Only the live src/json.rs census fingerprint changes. Legacy JSON differential
controls include a 100,000-element vector, object order, aliases/cycles,
depth/Option chains, nominal/Result/function/type/range refusals, numeric
extremes, non-finite floats, character and string escaping, and a prior closure.

The signed candidate requires the five exact ownership Miri tests and the
ordinary final gates, including fixed-route bands and cross. Terminal receipts
and the signed identity will be reported in the publication packet.

The root manifest edit activates conservative graph coverage. The current
selection includes all three adapter suites and clippy checks, Jupyter's four
checks, project tests/clippy, demos/server compilation, and the Rune/root,
cross, inventory and fixed-route stages. New registrations also name no-std,
examples, benches, Hegel tests and the Polars generator. Unknown packages
still fail closed; each registration has a failure-propagation control.

Open policy follow-up: distinguish root-package-only dependency edits from
workspace-wide edits only under a separately reviewed graph/manifest proof.
No such narrowing is made here. Recipe validation found a preexisting examples
build failure (missing Tokio rt-multi-thread) on both candidate and published
9104d9c3: baseline receipt BASELINE-EXAMPLES-v1/RESULT.json. The authorized
manifest-only prerequisite adds precisely that feature; no examples source or
lockfile changed. PACKAGE-RECIPES-v2/RESULT.json passes examples, benches,
Hegel and the generator (103.78 seconds combined). The no-std existing CI
recipe separately passed. The full final elapsed time and selected-stage list
are external terminal evidence in the packet, bound to the signed candidate.

The first executable final chain stopped at a stale CLI refusal witness after
339.23 seconds: it used json::stringify, now intentionally admitted. The control
now uses time::from_parts, whose borrowed-object input remains unsupported.
The targeted routing CLI suite passed after this test-only repair. The complete
ordinary final is restarted on the amended candidate, without stage reuse.

The expanded final also requires the existing pinned Jupyter client environment;
the launcher now declares it explicitly. No client installation or source
repair was needed. The next stop exposed this record's serde dependency edge
missing in six active downstream locks. Offline resolution changes only the
rnx dependency list to include serde in the three adapter, two demo and HTTP
server locks; versions and checksums are unchanged (LOCK-DIAGNOSTIC-v1).
Historical probe locks are untouched. Full final restarts after this repair.

FINAL-v5 stopped after 2162.04 seconds at Candle's allocation refusal test;
the same test fails on published 9104d9c3 (BASELINE-CANDLE-ALLOC-v1).
Scratch attribution separates the refusal (160 bytes) from destruction of
the last consumed names handle (2097152 bytes). The test now keeps another
handle until after reading the peak, so it measures refusal/copy/export.
The SMALL limit remains 262144 bytes. The repaired real test passes; an
actual premature tensor export in the scratch adapter fails at 1060940 bytes
(ALLOC-MUTATION-v1), proving the limit still rejects the forbidden work.
ALLOC-ATTRIBUTION-v1 records the phase measurements. No adapter product code
changes. Follow-up: investigate the 2 MiB last-handle destruction allocation
in its own record. Restart the entire ordinary final without stage reuse.

FINAL-v6 stopped after 1451.80 seconds at the next Candle allocation test,
count_alloc; alloc and composition controls passed. The published baseline
reproduces the same 2097328-byte failure. Scratch phase attribution reports
176 bytes for refusal and 2097152 for last-handle destruction. The same
test-only keepalive repair retains the original 65536-byte threshold. A
scratch mutation building a per-text table before refusal fails at 1048656
bytes. COUNT-ATTRIBUTION-v1/RESULT.json binds the baseline, phase and negative
control logs. No adapter production code changes; restart the complete final.

FINAL-v7 stopped after 1472.00 seconds at cross_encoder_alloc. A sweep of
the remaining allocation controls on the published baseline identifies just
cross_encoder_alloc and nli_alloc with the same consumption/destruction issue;
the other five controls pass (REMAINING-ALLOC-v2). Both repaired probes
measure 240-byte refusal and 524288-byte destruction for the oversized pair.
The unchanged 262144-byte limits reject an actual premature per-pair table
mutation at 262296 bytes in both tests (PAIRS-ATTRIBUTION-v1). Keepalive
changes remain test-only. The first sweep's linker failures were disk
exhaustion, not test results; disposable scratch Cargo outputs were removed,
with all sources, logs and receipts retained. The complete final restarts.

POLARS-DISK-RECOVERY-v1 exposed a missing producer selection: the adapter
suite's generated tests require the pinned Polars inventories. Stage input
prerequisites are now declared once beside accept.commands. Selection closes
transitively and rejects absent or late producers. Both Polars suite entries
require polars-inventories; production-cli-controls requires its binary build.
These are the generated-input stage pairs found in the current command list;
Cargo-managed build dependencies remain in their existing recipes. Controls
verify closure/order, missing/late producer refusal, and failed producer
preventing the consumer from running. Inventory producer receipts are required
at finalization. No fixture is added and no completed final stages are reused.
The final receipt records the after result; the earlier Polars recovery receipt
retains the missing-input failure. No Miri-covered source changed.
