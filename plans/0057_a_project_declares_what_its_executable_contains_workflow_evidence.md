# 0057 gate 5: locks, builds and verified runs

Status: Linux implementation submitted for review. Gate 4 is accepted, with no
pending review. Gate 6 remains open. Windows commands explicitly refuse pending
owned subprocess supervision; this is an implementation limitation, not an
executed Windows claim or a waiver of the record's signal-ownership requirement.

## Product surface and files

The ordinary independent workspace now builds rnx-project with lock, build and
run, requiring --manifest with no upward search. Lock/build accept --offline;
run forwards arguments after --. Runner-specific flags before -- are not exposed
(the record allowed, rather than required, that optional surface). Build output
is stderr; run's stdout belongs to the selected executable.

Only rnx.lock and rnx.Cargo.lock appear beside the manifest in the generated form.
An override needs only rnx.lock. Everything else is below .rnx, including an inner
.gitignore containing `*`; the user's outer .gitignore is untouched. The fixture
checks the directory entries and Git's non-ignored untracked listing. The lock is
bounded pretty JSON, with source and native inventories and assembly identity;
the receipt is three fields: format, lock_sha256 and executable_sha256. The
provisional gate-2 packages array is replaced by the real source/native inputs,
keeping package associations separate from shared trees rather than duplicating
inventories. There were no published product locks to migrate.

## Publication and failure policy

Each command holds an OS advisory lock inside .rnx; process death releases it.
Publication stages and syncs bytes on the same filesystem. Cargo.lock is renamed
first, rnx.lock last; the latter commits the exact Cargo digest. There is no
fiction that two filesystem names can be atomically renamed as one operation.
Cooperating readers serialize, and any interrupted mixed pair refuses. Neither
run nor build repairs it. An explicit lock recovers it.

Bounded backups support normal-error rollback. The old JSON commit marker is
restored before old Cargo bytes; if that restoration fails, the tool refuses to
restore old Cargo beneath a potentially new JSON lock. The injected failure after
JSON publication proves both old byte strings restored. Signal interruption after
Cargo publication proves the old JSON is untouched and build/run reject the new
Cargo/old-JSON pair. The test changes a local Cargo package version so the Cargo
bytes really differ; it cannot pass vacuously with identical lockfiles.

Build removes the prior receipt, verifies the lock and input snapshots, checks
compiler/Cargo versions, regenerates the assembly and copies the exact locked
Cargo bytes. Cargo runs --locked in release mode for the compiler's host target,
with output confined to .rnx. Post-build checks compare inputs and the lock before
copying/verifying the executable and atomically publishing the new receipt.
Run reconstructs the local input audit without Cargo metadata, verifies the pair,
inputs, generator identity, receipt and artifact, then execs on Unix. It invokes
neither Cargo nor rustc. Content-addressed artifacts/maps avoid replacing the
file a just-launched program is about to use. Verify/execute remains a trusted
local operation, not an atomic defense against hostile replacement.

## Build environment and ownership

Cargo metadata is resolved only by lock and confirmed with a second locked query.
The preflight audits invocation ancestry and Cargo home before resolution. Only
http/net/registry/registries/term config tables are accepted; build/profile/target,
env, compiler wrappers, source replacements and config includes refuse. CARGO_*
overrides except CARGO_HOME and RUST* overrides except rustup location/selection
are refused by name. Compiler and Cargo version outputs are compared on build;
run verifies the provenance of its existing artifact without needing a compiler.
Trusted build scripts, proc macros and their external/ignored inputs remain the
record's explicit non-hermetic qualification.

Unix tool subprocesses have a process group. SIGINT/SIGTERM retire that group,
wait for the direct child and return 130/143. Captured control output is bounded
to 16 MiB; build chatter is streamed to stderr. A real build-script fixture spawns
a sleeping child, records both PIDs and waits. SIGINT at that observed state ends
the build with no receipt and both PIDs disappear. Deliberately escaped process
groups and OS stalls are not covered. No Windows ownership promise is substituted
with a direct-child-only kill: Windows CLI entry currently refuses.

## Evidence

rnx-bench/probes/project-workflow/check.py passes sixteen black-box groups using
a tiny API-compatible Rust crate in a private Git tree. This tests Cargo workflow,
not Rune semantics. Groups include successful layout, no compiler invocation on
run (failing PATH sentinels), malformed locks/receipts and artifact tampering,
stale content, compilation failure naming the package, concurrent writers,
post-build edits, SIGINT/SIGTERM, mixed-pair refusal/recovery, normal publication
rollback, relocation and executable overrides without Cargo or receipts.

postgres.py then calls the real commands with the real PostgreSQL adapter, plain
extension and mapped Rune dependency. A typed query preserves quotes, SQL-looking
text, backslash and emoji and returns 42. The published receipt identifies the
actual lock and executable. Only the expected four public project files exist;
the private postmaster is observed gone. A copied compilation cache is explicitly
a warm-cache setup, not a first-build measurement. Root native fingerprints
include all tracked repository files, so these locks are snapshots before later
evidence edits, not a promise that the final repository reuses them unchanged.

ordinary.py uses a non-test-support tool and proves the fault/pause variables do
nothing; an existing stock rnx override returns 42. Pause/failure hooks do not
exist in the shipped build. Results and source/binary hashes are in
rnx-bench/results/project-workflow-0057.

Root code, root manifest/lock/notices, kernel, adapter and server are unchanged.
Both tool test configurations pass 27 tests with two ignored integration tests;
formatting, clippy with warnings denied, notices and a full Windows type-check
also pass. The two ignored legacy integrations are not counted as executed here;
the product fixtures above exercise the new workflow separately. Root regression, packaging checks and matched timing remain gate 6;
there is no performance claim from this gate.


One layout restriction is exposed by the whole-native-root contract: a project's
public locks cannot lie inside a native package root, because the lock would
fingerprint itself. Lock refuses this by name before publishing either public
file, with a discriminating nested-project fixture. It does not exclude additional
native files. The real PostgreSQL project is outside its native roots. This
restriction and the explicit Windows refusal are highlighted for review.

The same check applies to a source root containing the manifest/output directory
unless it is the application entry root itself (where the agreed exact exclusions
apply). A manifest below a larger source root, or a mapped dependency containing
the project, otherwise hashes its own generated outputs. A second fixture pins
the early refusal and retention of the old public lock. No broader source-tree
exclusion is introduced.
