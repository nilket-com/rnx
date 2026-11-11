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

## Execution and disposition inventory

| Consumer | Required/current execution and disposition |
| --- | --- |
| ci/commit_subjects.py | Preflight resolves cutoff, import and follower commits; translate five rewritten fields. Imported Rune bounds/reference unchanged. |
| ci/hygiene_history.json | Preflight reads every baseline blob; translate baseline, retire 25 exact purged path/blob exemptions, keep the other blob IDs. |
| tools/column/clippy.py | Current host/clippy stage archives BASE; translate that lookup only. |
| tools/verify/fixtures/0224_registry_refresh.json | Live refresh_census command enforces before/after revision pair; translate both, preserve all source and registry hashes. |
| tools/refresh_census.py | Resolves explicitly supplied current revisions; no implicit normalization or source changes needed. |
| tools/column/fixed_route_policy.json | Live cumulative anchor: existing approved transition to mapped 0238; all numerical policy and inventory unchanged. |
| drift_band.py INITIAL_ANCHOR | Only bootstrap lineage control reaches no-policy branch; no Git lookup there. Keep original constant to recognize original policy lineage. |
| fixed_route_gate.py | Existing cache miss and ordinary capture; no code changes. |
| gate_policy/required_stages/paired_build | Resolve current candidate, parent and signed plan dynamically; no historical constant translation. |
| census_manifest.json baseline_rnx | Provenance, not an executable Git lookup; unchanged. Live source/registry fingerprints unchanged. |
| rune EXTERNAL-EXPORT and measure qualifications | External corpus provenance / unchanged Rune graph; unchanged. |
| cross_program_gate.py BASELINE | Historical CLI default; required fixed-route gate imports only stage(), which executes caller-supplied commands. Default not executed: deletion candidate, unchanged. |
| f64_plan_capture, f64_output_capture, boolean_capture, portable_vectors | Historical capture entry points not in required stage selection; deletion candidates, unchanged. |
| migration_rules, preparation_rule, score_0214 | Controller tests exercise synthetic records and pair comparisons; no Git lookup executed. Historical replay entry points are deletion candidates, unchanged. |
| historical *_pins.json, 0215_constants, fixed_route_0237/0238 | Historical evidence comparison inputs; no current required historical build lookup. Deletion candidates, unchanged. |
| both ci/data_archive.json and archive readers | Deliberately read original archived objects; preserve original revisions and checksums. |
| bench current_matrix.py and historical renderers | No executable change required by this record; historical build/provenance remains original. current_matrix replay is a later-deletion candidate. |
| plans, README citations, external dependency locks | Historical citations or third-party identities; unchanged. Thin resolver added separately. |

Map projection and unchanged-closure proof: exactly 342 rnx and 281 bench
rewrites; 2,197 rnx unchanged ancestors and no unchanged bench commits. The
623-pair source configuration excludes this record. Source hashes and retained
blob exemptions are not translated. No benchmark follower is required.

## Anchor outcome and accepted residual

The first capture matched 18/32 archived records. Native-on differences were
predominantly libc, up to 0.88 per mille; one rnx count was -8. This failed
the exact comparison and remains retained as a failure. The authorized
path-matched diagnostic matched 30/32 records. Both awfy_permute native-on
sizes differ by -4 Ir, entirely in Cranelift's PassTimes timing-token drop;
all dynamic/other counts, fixtures and outputs match. The same timer had
varied between the two archived old-anchor captures.

On 2026-11-10 the user answered "accept" to this exact residual, accepting
the path-matched capture as a pure rename. This supplements the initial
exact-equality condition; it does not erase either failed exact comparison.
The selected anchor binary SHA256 is
`dbeb63ddcef644be98136ff1442ce594067a51696836f2b773aa20beca041dbc`.
The path-matched cache entry is recorded in the final packet. The first,
non-matching capture is not the accepted reference.

Binary inequality is expected because RNX_ENGINE_IDENTITY embeds the checkout
HEAD, even when building the same exported anchor revision. Matching both
build and run path lengths removed the earlier libc deltas; this experiment
does not distinguish which path was responsible. No runtime source changed.

Backlog: normalize fixed-route build/capture path lengths and include their
shape in the measurement recipe. Observed sensitivity is up to about 0.9 per
mille against 5/10 per mille bands. This is outside 0241; no cache/reuse code
or threshold is changed here.

Final ordinary gates pending.
