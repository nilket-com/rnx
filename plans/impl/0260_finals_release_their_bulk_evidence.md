# Implementation plan 0260: a final releases the bulk evidence it owns

Status: Proposed

Paired ADR: ../adr/0260_finals_release_their_bulk_evidence.md.

1. Pin 73a8e332. Read the routing and adapter routing test output
   helpers, the cross controller and its enrollment records, accept.py,
   child_result validation, gate finalization and the ownership record of
   ADR 0253.
2. Before editing, send a note: every bulk root a final writes today and
   who names it; every reader of a bound file and when it runs; what
   resumption reads; the proposed manifest fields and its size on a real
   final; the bound for kept failure diagnostics and their release point;
   whether fixed-route outputs are in scope; anything in the ADR that the
   code makes wrong. If an inventoried item or a band would change, send
   that instead and stop.
3. Implement the committed driver, assignment and ownership of the
   roots, the manifest, the ordered release and the release record, and
   revalidation from the small records.
4. Add the controls of the ADR.
5. Remove the existing stores as the ADR limits it and list entries and
   bytes.
6. Run the required stages, preflight and the commit checker. Bring the
   packet with disk use of the stores and free space before and after,
   the manifest size, the selected stages and the per-stage durations.
   The candidate is frozen once the packet is sent.

Results are added here by the implementer.

## Implementation and development results

The amended scope is implemented by `tools/column/final.py`, also called by
`accept.py`. Ordinary final invocation is
`python3 -B tools/column/final.py --out target/0260/FINAL-v1`; the default selects the complete canonical catalogue. Explicit `--stages`
selects the required closure plus the named stages, labels the result
`selected-stages`, and cannot claim complete-final acceptance. The environment must
supply any installed tool location not on PATH (e.g. RNX_VALGRIND_ROOT when
fixed-route is selected). No temporary final launcher is required.

The final records fresh roots before launch: routing census/alias and adapter
outputs under target/routing-cli, generated projects in an external temporary
root, cross under target/rnx-verify with a private payload child. Tests inside
a final refuse missing root assignments. Standalone output defaults remain.
Unrelated scratch, final stage logs/archives/child receipts, shared build/Miri
caches and all fixed-route retention remain unchanged.

`final_evidence.py` owns directory/parent identities, delayed release and the
compact manifest. Interior symlinks are hashed as link text and unlinked;
root/ancestor links, replaced identities and preexisting roots are refused.
Each root release journals intent before removal and completion after it,
with parent-directory synchronization. An interrupted removal is idempotent.
An interrupted creating root that is absent is resolved as never created;
an empty directory is removed only by non-recursive rmdir with matching
parent identity; a nonempty or replaced parent is refused.

Every selected live-file gate still runs before ACCEPTANCE and MANIFEST are
made durable. Release follows, then RELEASE and public RESULT. Only a pass
from both acceptance and release is public PASS. `--revalidate` validates
these linked small documents and explicitly reports that released payloads
were not rehashed. Legacy resume and recovery helpers are unchanged; there
is no resumption. Failure diagnostics are head/tail excerpts with byte counts
and hashes, capped at 256 KiB per stage (shared across its roots) and 4 MiB serialized
per final. Literal inherited environment values are redacted from snapshots.
`--abandon-diagnostics` explicitly releases a failed final's bounded snapshot.

The registry holds ownership and producer-group records independently of
the final output directory. It visits ownership records, never age/name-selected
data roots. Released ledgers, locks and group records are retired after a
durable release record; interrupted retirement is recoverable.
The next final refuses live locks, drains recorded producer process groups
for an abandoned owner, and releases the recorded roots. Producers wait on a
pipe until their process-group identity is durable; a killed launcher cannot
leave an unrecorded producer doing work. Existing signal supervision drains
children on ordinary interruption; PID reuse is refused. No passed-case or
stage checkpointing was added.

Manifest fields are a projection: candidate/source/policy/selection, declared
command templates, environment names only, tool identities, stage status/exit/
duration, validated receipt and payload digests. Bound artifact digests already
validated by the reader are reused where available; other files are streamed.
The manifest cap is 256 KiB, checked before release. No raw binding/request
objects or process argv are copied into it.

Measured full-tree digest on the real 0259 cross store: 4,379 entries,
798,074,365 logical bytes, 804,741,120 allocated bytes, 1.02 seconds
(`target/0260-boundary/TREE-COST.json`). This is below the one-minute ruling.

The explicitly authorized old target/routing-cli and target/rnx-verify stores
were inventoried and removed after a one-time live-process reference check.
`target/0260-boundary/OLD-STORES-RELEASE.json` lists each entry and its logical
and allocated bytes. Free space increased by 75,812,892,672 bytes (70.61 GiB).
No adapter store, shared cache or other directory was swept.

Development controls cover terminal ordering, pass/failure cleanup, original
failure precedence, release failure, interruption, killed-driver recovery,
live lock refusal, identity/path refusal, interior symlinks, incomplete
creation, interrupted release, missing live artifacts, completed revalidation,
missing release, manifest overflow and environment-value exclusion. The Rust
assignment boundary is exercised in subprocesses with missing, valid and
symlink roots. Routing CLI/project test binaries compile. Python controls also
run under the final's environment to exercise nested process supervision.

Source review corrections passed all 269 Python controls (4.978 seconds).
Signed-implementation final acceptance remains required.
The packet will report actual manifest size, disk deltas and stage durations.


## Source-review corrections (seq30892)

All seven changes applied. Recovery no longer reads/writes the abandoned
final directory; the registry is self-contained, including producer groups.
Controls delete the final directory while a producer is alive, then recover,
drain it and release. Creating-empty/absent and interrupted retirement have
controls. Released ledger/lock pairs do not accumulate.

Failed acceptance retains its original error and separately records manifest
or diagnostic errors; those errors do not skip release. Passing acceptance
still requires a complete manifest before release. SIGTERM/SIGINT are ignored
throughout the terminal section. Controls inject snapshot errors and send both
signals during release. RESULT keeps compact checks with name/exit.

Preflight parity: disk admission is restored before stages. gate.check_selection
(final=True) enforces pinned candidate HEAD and a clean tree before and after
every stage; child.run also binds source/candidate. CARGO_TARGET_DIR override
is refused. accept.py retains fresh-clone semantics and refuses preexisting
native targets; the ordinary final entry deliberately permits existing native
build caches, as prior ordinary final launchers did. No fresh-acceptance check
was dropped. declared_output is retained on acceptance check rows.

check_selection(final=True) checks required coverage, not full catalogue
coverage. The driver now defaults to names=None (all canonical stages); an
explicit subset is visibly scoped and has complete_final=false even if its
checks pass. Remote validation refuses such a subset as a complete final.
The real record final will use the default, including generated-projects and
all adapter routing families.

The manifest now reads actual environment names from child.environment.keys,
not the envelope's keys/sha256 field names. Environment-value sentinels are
checked in both manifest and diagnostics. Stage execution, terminal handling,
and tool identity are named functions with the release order explicit.

The first complete-final invocation stopped at adapter-format on pre-existing
formatting in adapters/polars/tests/column_reuse.rs. Applied rustfmt only;
no test or production semantics changed. The failed final's RELEASE passed,
and all assigned roots were removed while its original failure was retained.
