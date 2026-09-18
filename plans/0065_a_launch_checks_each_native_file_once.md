# rnx 0065: a launch checks each native file once

Status: accepted for implementation after the 2026-09-18 draft review.
The removal record is explicitly next after this record closes; gate 3 includes
a kernelspec referencing an old-key artifact among surviving consumers.
Gate 1's reader/encoding checks are accepted on Linux; see
[the encoding evidence](0065_a_launch_checks_each_native_file_once_encoding_evidence.md).
Gate 2's current-format workflows and matched format-only timing are accepted on Linux; see [the workflow evidence](0065_a_launch_checks_each_native_file_once_workflow_evidence.md).
Gate 3 is accepted on Linux; see
[the migration evidence](0065_a_launch_checks_each_native_file_once_migration_evidence.md).
Gate 4's isolated equivalence candidate passes on Linux, ready for review before
product reuse; see [the nested evidence](0065_a_launch_checks_each_native_file_once_nested_evidence.md).
Gates 5–6 remain open. Baseline: rnx `7cd3205`, rnx-bench `921ffe3`.
0064 is closed on Linux. This record addresses native inventory after runtime
installation; it does not introduce a persistent source-verification cache.

## Context

Every project run, eval and session checks its sources before executing. This is
an end-user launch cost. At zero adapters it still fingerprints the entire runtime
Git-tracked tree. A declared adapter under that runtime is inventoried separately,
so its files and Git repository are visited again.

The accepted inventory probe (`rnx-bench c2af5de`,
`results/native-inventory-0065/evidence.md`) measures overhead over each matching
direct artifact of 17.5, 22.2, 26.2 and 30.2 ms for zero through three adapters.
The third is a complete renamed PostgreSQL adapter, not a stub. The fixture's
constant runtime contains 443 files and 6,988,177 bytes, including that copy at
all counts. The descriptive slope is about 4.2 ms per adapter for this roster.
Most incremental cost is repeated Git work; the runtime floor is mostly reading
and hashing. The ancestor audit is already shared and costs under a millisecond.

Git topology matters. Each root in that fixture runs three Git commands; its
submodule check starts a fourth process because the fixture is nested inside
another working tree. Those counts are not universal for installations outside
a checkout. Deeper canonical paths add about 0.8–0.9 ms at two adapters; a shallow
path of the same length adds about 0.15–0.22 ms. This supports, but does not wholly
attribute, the earlier installed-runtime difference. Depth optimization is deferred.

The accepted hash probe (`rnx-bench 921ffe3`,
`results/hash-choice-0065/evidence.md`) resolves the algorithm choice before a
format migration. The same 443-file reader costs about 10.6 ms with existing
SHA-256 framing, 6.9 ms with one SHA-256 content digest per file, and 5.5 ms with
BLAKE3. Including Git, the runtime fingerprint is about 14.7 / 10.8 / 9.7 ms.
The 107,567,832-byte artifact's already-single-pass SHA-256 costs about 66 ms;
single-core BLAKE3 costs about 35 ms. These are reader measurements, not projected
whole-product launch promises. Four-core BLAKE3 costs about 16 ms only with a
1 MiB buffer; parallelizing existing 16 KiB reads is slower than one core.

Existing identities are durable user state. Locks, receipts, ready documents and
installed-runtime metadata name SHA-256 explicitly. Installation IDs derive from
tree digests, and validation recomputes them. Assembly keys incorporate native
tree identities. Changing either tree construction or algorithm changes those
keys; relocking does not reclaim old entries of roughly 1.5 GB each. Older tools,
projects, direct launches, sessions and kernels can still use such entries.

## Decisions

### 1. Single-core BLAKE3 and one content digest per file

Pin `blake3 = "=1.8.7"` in the project tool's own workspace, using its ordinary
single-threaded streaming API with default SIMD dispatch. Do not enable Rayon,
spawn hashing threads, mmap files or enlarge the existing 16 KiB read buffer in
this record. Record the resolved graph and update the tool's notices. Root,
adapter, server and kernel dependency graphs are unchanged. SHA-256 remains only
where the migration verifier below requires it; externally defined Git object
identities and Cargo registry checksums are not changed.

File content identity is the ordinary 32-byte BLAKE3 digest. The tree preimage is:

- domain bytes `rnx-tree-v2\0`;
- for each file, sorted by the existing normalized UTF-8 relative-name ordering:
  unsigned 64-bit big-endian name-byte length, name bytes, one executable byte
  (0 or 1), unsigned 64-bit big-endian content length, then the raw 32-byte file
  digest.

BLAKE3 of that preimage is the tree digest. No content hex string or raw content
is fed to the tree hasher. Empty trees, zero-byte files, ordering and field
boundaries have fixed test vectors. Duplicate names refuse. Preserve the existing
path/name rules; do not replace them with filesystem enumeration order.

Keep the same bounded no-follow/nonblocking reader, component checks, metadata
checks and final detection-byte behavior. No increased allowances or skipped
regular-file checks buy the speedup. Each separately opened canonical path is
its own input; hard links at different paths are not coalesced by inode.

Use BLAKE3 for current tool-owned file, document, wrapper, Cargo-lock-content,
artifact, installation and assembly identities. This changes the hash of Cargo's
lockfile as a blob, not Cargo's own contents or checksum algorithm. Derive runtime
IDs from BLAKE3 of `rnx-installed-runtime-v2\n<tree-blake3-hex>\n`. Assembly keys
are BLAKE3 of the canonical versioned identity document as below.

### 2. Publish one explicit family of new formats

New digests are lowercase 64-character hexadecimal. Rename serialized fields
according to their actual meaning: `blake3`, `manifest_blake3`, `main_blake3`,
`cargo_lock_blake3`, `lock_blake3`, `executable_blake3`, `tree_blake3` and
`tool_blake3` as applicable. Do not place BLAKE3 bytes in a `sha256` field or
recognize an algorithm from digest length. Strict decoding rejects mixed schemas,
unknown fields/versions, malformed digests and invalid canonical identities.

| Document | Existing version(s) | New version |
|---|---|---|
| Project lock | 1 local/override, 2 shared | 3 shared/override |
| Project receipt | 1–3 | 4; assembly key required only for shared |
| Assembly identity / generator | 1 / 1 | 2 / 2 |
| Cache ready document | 1 | 2 |
| Runtime installation | 1 | 2 |
| Runtime default selection | 1 | 2 |

The user-authored manifest stays format 1. The loader source-map handoff stays
format 1: it has paths/mounts, not fingerprint digests. The generated Rust wrapper
and public execution/extension interfaces do not change. Do not bump a protocol
merely because it transports an opaque digest; prove the existing private :dep
carrier can retain that interpretation at gate 3. Stop for review if it cannot.

All newly locked generated projects use shared assembly, as since 0061. No new
local-generated format is introduced. Executable overrides use lock 3/receipt 4
and the same 0059 stamp policy. Whole-input BLAKE3 verification runs at build and
attachment, on a stamp mismatch, and under `--verify`; a match refreshes the stamp
against the previously recorded digest, never adopts an unexpected artifact.
The documented same-size/restored-mtime artifact miss remains exactly as in 0059.
Source content is still read on every invocation, including without `--verify`.

### 3. Explicit project and runtime migration

The new tool recognizes old format envelopes to give recovery instructions; it
does not build, attach or launch from old locks/receipts. Refuse before artifact
execution or receipt refresh and name the manifest plus the exact `lock` and
`build` commands. Existing files are unchanged by that refusal. Unknown/corrupt
formats remain errors, not presumed migration candidates. Explicit lock publishes
the new pair through the existing atomic-pair protocol. Explicit build creates or
attaches a new-key entry and publishes receipt 4. Never promote an old artifact
by hashing it under the new algorithm: its assembly/build path is an observable
input. Old local artifacts, shared entries and receipts are retained until their
normal explicit replacement or separate removal. Overrides retain their declared
executable path but must be explicitly relocked and built/verified too.

An old selected runtime gives a shell-quoted recovery command:

```text
rnx-project runtime install --from '<canonical-store>/entries/<old-id>/source'
```

`runtime show`, `runtime select <old-id>` and stock :dep discovery name that old
installation and the command. No selection, source, project or session mutation
occurs merely on discovery. A :dep preparation refusal keeps the old session and
its started operations under 0063. A version-1 selection is recognized to reach
this message; it is not silently relabelled version 2.

The existing own-installation path exception needs an explicit migration branch:
it currently also requires the source path's ID to equal the newly computed ID,
which cannot hold after this change. Only `runtime install --from` on that exact
owned retained source path may enter the branch, under the installation writer
lock and existing storage bounds. No substring/prefix or arbitrary nested path
qualifies.

Use a private migration-only legacy validator for installation format 1 and tree
format 1. It retains the bounded SHA-256 verifier needed to compare the old
recorded source identity, file/byte counts and ID, plus Git administration/fsck,
layout and permission checks. It is not a launch fallback or an inventory cache.
Without that comparison a changed retained source could be silently imported as
an authentic old installation. Gate 3 tests this distinction explicitly.

After old content validates, inventory with v2, copy into the new ID, build an
independent Git index, revalidate source/copy and publish/select through 0064's
existing ordering. Old source bytes and installation documents are never rewritten
or deleted; validated Git permission repair retains 0064's existing rule. Preserve original
source provenance as labelled provenance, record the new installing tool digest
and time, and record `migrated_from` with old format and ID outside the new identity.
A same-new-ID reinstall validates/reselects without rewriting its first provenance.
Before publication any failure preserves selection; after publication retain the
existing installed-but-unselected and selection-may-have-changed distinctions.
Migration reads the old source again before publication to detect observed edits.

Old runtime source remains an ordinary explicit path dependency for projects that
already name it. Relocking such a project does not consult or retarget the installed
default; it uses the declared source path and a new assembly identity. Migrating
the default likewise does not retarget old scratches. Document this alongside the
fresh stock-session route. New source installation is not a runtime code upgrade:
launcher-versus-source revision compatibility remains unchecked as in 0064.

### 4. Retain old entries; generator version is not a liveness proof

No automatic deletion or `cache prune` removal command ships in this record.
A superseded generator means the **new tool** cannot attach that entry. It does
not mean an old tool/project no longer references it, or that a running session,
kernel or direct executable has stopped using its retained build outputs.
0061's build lock is not a lifetime lease and is not held through every launch.
Scanning /proc for an executable would not prove absence of future references or
auxiliary OUT_DIR reads either.

State the cost plainly in migration messages and the README: a new key can build
another approximately 1.5 GB Polars assembly; old entries stay on disk, and old
runtime entries also stay. Relocking/reinstalling reclaims no space. Keep the
existing whole-entry manual-removal guidance only after the user has stopped all
consumers; do not call a version-based dry run an unreferenced-entry report.
A removal record must decide lifetime/reference handling or explicit user-owned
quiescence and consequences. Its scope can be narrow, but this performance record
will not claim safe deletion by version alone.

### 5. Reuse enclosing native observations within one inventory call

After the format-only checkpoint is measured, optimize nested native roots in a
separate equivalence gate. An ordinary shipped adapter is inside the runtime's
tracked tree. Derive its relative path set and file digests from that enclosing
observation, then frame its own root-relative tree. Preserve all package/root
associations, canonical paths and separately represented trees in the lock and
identity. Different relative names still produce different tree identities.
Do not flatten roots, conflate diamonds or infer Cargo packages from directories.

Reuse only when the enclosing and nested roots demonstrably use the same Git
working tree and scoped tracked/untracked policy. Check intervening repository
boundaries, including .git files/directories and linked worktrees. A nested or
external repository, a submodule, unusual routing context, missing proof or a
boundary change takes the established independent per-root path and its refusals;
never silently treat an independent repository as a parent-index subtree. Begin
conservatively with fallback for inherited Git routing/configuration overrides
whose equivalence has not been established. No global Git-result cache is added.

The equivalence prototype fixes the exact eligibility checks before product use.
Exercise untracked and ignored files, staged additions/deletions/conflicts, missing
tracked files, executable bits, symlinked components and a nested Git repository
inside an adapter. Parent and child entry sets must match their independently
scoped Git observations for eligible quiescent inputs. All parent refusals still
apply; an empty child root cannot become a successful empty native inventory.

A reused file is a result of a bounded validated read in **this** invocation.
Retain the current 100,000-entry/512 MiB logical inventory accounting: every
represented file in every represented root is charged, even when its bytes were
physically read once. Charge a derived child before constructing it, preserving
sticky refusal and bounded document sizes. Audit files keep their current separate
read/accounting rules. Reuse must not make a formerly over-limit lock acceptable.

There is no atomic filesystem snapshot today, and read-once reuse changes the
observation schedule. State the guarantee precisely: equality for quiescent inputs,
full content reads anew on every invocation, and refusal for changes observed by
its validation checks. It is not identical detection of every concurrent edit at
the time the old implementation would have done a second read. Recheck repository
boundary/index identity and relevant file metadata before reusing observations;
observed changes refuse rather than mix snapshots. A same-size edit with restored
metadata after the single validated read may be missed until the next invocation;
the old later reread could have observed it. Gate 4 records that timed counterexample
explicitly, alongside the existing post-final-read limitation, for review rather
than calling the observation windows identical. Persistent metadata caching remains
excluded, and a pre-launch source edit with restored mtime must still be detected
on the next launch in both default and --verify modes.

Stop for review if quiescent coverage, bounds, topology checks or mutation handling
cannot be demonstrated. Do not weaken the input set, run Git less by assumption,
or keep stale records across calls just to hit a latency target.

### 6. Measure changes separately and gate the product slope

Record three uninstrumented product checkpoints on the same host and input roster:
accepted baseline; BLAKE3/v2 framing and migration without nested reuse; then nested
reuse. Each project has its own directly launched artifact control. Repeat real
run, eval and spawn-to-first-prompt measurements at zero through three adapters,
with runtime files/bytes reported separately. Use the complete renamed adapter
fixture, preserving its constant runtime floor. Do not use a tiny third stub.

Keep one pinned core/one Polars thread, two interleaved repeats with at least 30
observations per cell, output validation and all samples retained. Report direct,
project, difference, successive increments and descriptive fitted slope. Keep
clocked attribution separate from headline timings. Measure full --verify,
attachment and runtime migration separately; do not hide them in warmups.
Include same-worktree, external-root and nested-repository shapes and shallow/deep
controls, distinguishing full measured rosters from synthetic unit cases.

For the ordinary same-worktree zero-to-three roster, require both repeats to show
at least a 3 ms reduction in the zero-adapter overhead versus the matched baseline
and less than 1 ms added overhead per shipped/copy adapter after nested reuse.
Every count must improve over its matched baseline. These are measured product
gates, not a universal fixed-25-ms promise. External/fallback topologies retain
full coverage and report their own slopes. If a gate misses, keep the result and
return to review; neither discard samples nor silently substitute weaker checks.

## Gates and stop points

1. **Encoding and algorithm contract.** Independent vectors for file/tree inputs,
   framing ambiguities, unusual names, mode/order/size/content changes and limits.
   Replay fingerprint refusals against both the legacy baseline and v2 reader,
   comparing semantic inventory fields rather than equal digests. Validate the
   BLAKE3 dependency graph/notices and all current document versions. No production
   parallel hashing. Source-map format and root interfaces remain byte-identical.
2. **Current-format workflow and first timing checkpoint.** Real shared generated
   and override projects through lock/build/run/eval/session, two-consumer hits
   with compilation trapped, full verify, stamp hit/miss/refresh, tampering and
   restored-mtime source edits, publication failures and cancellation. Reject old,
   unknown and mixed formats before execution. Measure the format-only product
   cost against baseline before introducing nested reuse.
3. **Migration and installed-runtime journeys.** Start from genuine 7cd3205 locks,
   receipts, ready entries, current selection and installations, not new structs
   with their version numbers edited. Exercise exact recovery commands including
   spaces/apostrophes; record old files/digests unchanged on refusal. Migrate from
   retained source with the fixture checkout renamed away. Corrupt old source,
   blob, metadata or ID refuses without publication or selection; permission drift
   follows 0064's validate-before-repair boundary. Inject copy/Git/publication and
   selection failures. Prove new default stock :dep Polars and typed PostgreSQL,
   old scratch independence, and zero compilation on a second new-key attachment.
   Assert old assemblies/runtimes remain and measure incremental disk use. Old
   sessions with retained-output reads and a kernelspec referencing an old-key
   artifact survive the new tool's migration; launch that kernel after migration
   and execute a cell, rather than only inspecting its JSON. The :dep
   protocol/builder/startup/commit boundaries keep the 0063 behavior.
4. **Nested-root equivalence before enabling reuse.** Independent per-root oracle
   versus candidate for all decision-5 topologies and refusals, inherited Git
   routing/config, boundary replacement, untracked/ignored/staged changes, limits
   and logical duplicate charging. Test-only counters prove one content read for
   eligible overlapping files and the intended Git-call reduction; fallbacks prove
   independent reads. Force edits at read/reuse boundaries, including the stated
   restored-metadata timing divergence. No result survives into a second inventory
   call. Return to review on a stop; do not integrate an unproved fast path.
5. **Final everyday costs and real use.** Rerun decision 6's roster and topology
   matrix with the uninstrumented tool. Show runtime floor and marginal adapter
   cost, full verification and attachment separately. Run the Polars/typed-query
   combined project and session error-recovery journey against a private cluster,
   including installed-source discovery and a second consumer. No process remains.
6. **Regression and documentation.** Tool fmt, strict clippy and suites in both
   feature configurations, notices, adapted project/publication/verification/
   interactive/installation/transition fixtures with adaptations disclosed. Root
   suites serially, notices, fresh default selfcheck and normalized default graph;
   root Cargo/source/API, kernel, adapters and server stay unchanged. README shows
   upgrade commands, retained-disk consequences, new format boundaries, single-core
   policy and concurrent-observation limits. Linux execution only; Windows and
   separately measured type-check results must be labelled honestly.

## Guardrails and later work

No source metadata cache, weaker default source checks between invocations,
parallel --verify, cache eviction, runtime uninstall, dynamic native loading,
registry, package solver, native-tree relocation equivalence, root loader change,
context reuse or installed-runtime version-compatibility promise. No claim that
hashing content once makes a concurrently edited checkout an atomic snapshot.

The measured stops and format-only/final checkpoints earn commits because their
sources must remain reproducible. Incidental fixes fold into their checkpoint
before push. Do not rewrite accepted baselines or describe predicted combined
savings as observations. Next review is gate 2's current-format workflows and timing evidence. The record immediately
after 0065 closes must decide removal of retained assemblies/runtimes, so the disk
cost has an explicit next decision rather than an open-ended deferral. It must
account for old tools, project references, sessions and kernels before deletion.
