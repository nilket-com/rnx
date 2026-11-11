# Implementation plan 0241: Restore executable identities after cleanup

Status: Accepted

Paired ADR: 0241_history_identity_migration.md.

1. Anchor-transition approval and cutover/re-clone are complete.
   Verify remote tips,
   HEAD trees and the accepted package. Commit the accepted signed plan pair
   locally as the first record on new history; no separate bootstrap tool.
   Nothing is published until the whole pair passes ordinary restored gates.
2. Trace executable identity readers and required-stage/test execution.
   Disposition each field/tool as live lookup, unchanged imported/external
   identity, original provenance/citation, or untouched later-deletion
   candidate. Bring the concrete boundary before production edits.
3. Import only the 342 + 281 rewritten old/new pairs and compact provenance
   metadata. Check exact projection; structurally prove unchanged identities
   omitted from the map. Test wrong repository, ambiguous prefix, duplicate,
   missing/corrupt pair, unknown identity and self-record exclusion.
4. Restore live metadata, hygiene, build and census lookup consumers. Retire
   exact purged exemptions with path/blob proof; preserve surviving hashes,
   source/registry witness values and normal policy constants.
5. Under the explicit user approval, use the existing anchor_transition mechanism
   with the mapped anchor and signed 0241 plan/ADR declarations. Run one ordinary
   mapped-anchor build/capture on the existing cache-miss path. No bridge code,
   comparison whitelist, new envelope, --import-anchor or cache-function edits.
   Preserve old proof outside clones; compare binary bytes and every recorded
   kernel/mode/size/Ir exactly. Report binary differences; STOP on any record
   difference. INITIAL_ANCHOR changes only if its branch is reached by a
   current required check/test. Preserve limits, matrix and source inventory.
6. Add the thin read-only old/new resolver with optional mapped-path blob
   check and an existing archive-tool pointer. No archive reader or historical
   document edits. Keep historical-only tools unchanged; list later-deletion
   candidates in the packet. No replayer bridges.
7. Run focused controls and normal preflight, then required gates from actual
   changed paths, including cross where required. No interim PASS for the
   initially stale checkers. Only the approved ordinary anchor recapture is
   planned; stop for review if its records differ or additional captures
   become necessary.
8. Sign exactly one implementation, bind final receipts to it, prove runtime
   source/lock scope unchanged and bring the publication packet. Bench gets a
   separately reviewed follower only if a live executable consumer needs it.

Negative controls: unknown/ambiguous/misnamespaced identity; incomplete map;
unchanged identity not in the preserved closure; attempted translation of
blob/source/binary/receipt digest; retired exemption reintroduced; mapped
anchor used to reset cumulative reference; unequal fixed-loop inventory;
changed policy or nonidentical measured record passed as a pure rename; changed/purged path
claimed identical. Positive controls preserve unchanged imported history,
resolve old and new identities, and keep exact original evidence hashes.

Results/publication pending. No runtime optimization, dependency update,
new measurement claim, history edit or force push belongs in this record.
