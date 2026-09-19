# rnx 0066: removal is an explicit ownership decision

Status: gates 1–3 accepted and pushed, most recently `021f401` (bench `39467e8`).
Gate 4 is complete and ready for review in the
[closing evidence](0066_removal_is_an_explicit_ownership_decision_closing_evidence.md);
acceptance closes this record on Linux with the stated ownership qualifications.
The [ownership evidence](0066_removal_is_an_explicit_ownership_decision_ownership_evidence.md)
retains the namespace stop and bubblewrap resolution, the
[command evidence](0066_removal_is_an_explicit_ownership_decision_commands_evidence.md)
retains the failure matrix and same-key rebuild, and the
[storage evidence](0066_removal_is_an_explicit_ownership_decision_storage_evidence.md)
retains the old-key kernel and interrupted Polars-sized removal.

## Context

The 0065 identity migration leaves old entries reachable under their original
keys. A retained Polars assembly is about 1.5 GB; the reviewer's accumulated cache
reached about 15 GB. An installed runtime is much smaller, about 8.45 MB logical
including Git objects in 0064, but can be the only remaining copy of its source.
Relocking and migrating reclaim neither. Keeping only an assembly's executable
is insufficient: trusted build scripts can make it read retained build outputs.

There is no complete reference inventory. Projects can live anywhere, old tools
still understand their old locks, kernelspecs can name an artifact directly, and
a session or server can outlive the tool that launched it. The cache's
`locks/<key>.lock` coordinates build/attachment through receipt publication. It
is not a lifetime lease. The runtime's `install.lock` serializes writers, not
users of installed source. Ordinary launches take no cache lifetime lease; the
project lock closes on exec. A successful nonblocking writer-lock acquisition
therefore cannot establish that deletion is safe.

A registry introduced now would miss existing projects and older consumers, and
process scans cannot discover future file reads or offline kernelspecs. This
record chooses explicit user quiescence and exact targets, rather than pretending
those gaps are solved. It adds no launch work or lifetime protocol.

## Decisions

### 1. List, inspect, then explicitly remove one full ID

Add these tool commands, with no root CLI or Rune API change:

```text
rnx-project cache list [--root PATH] [--manifest FILE]...
rnx-project runtime list [--root PATH] [--manifest FILE]...
rnx-project cache remove KEY [--root PATH] [--dry-run] [--resume] [--quiescent]
rnx-project runtime remove ID [--root PATH] [--dry-run] [--resume] [--quiescent]
```

There is exactly one target per remove command: a full 64-character lowercase
hexadecimal key/ID, not a prefix, path, pattern or version selector. Its shape does
not declare its hash algorithm. SHA-256-era and BLAKE3-era entries are addressable.
Unknown options, duplicate single-use options, missing values and surplus
arguments refuse before opening storage. Only `--manifest` is repeatable. No `--all`, `--obsolete`, `--force`, age rule or automatic prune.

`remove --dry-run` is read-only and does not require `--quiescent`. Without
`--dry-run`, the command requires the explicit `--quiescent` acknowledgement;
otherwise it refuses before creating anything and prints the inspection command.
There is no interactive prompt, implicit yes or tty-dependent behavior. A dry run
is not a saved authorization: actual removal repeats every applicable check.
`--resume` explicitly addresses a pending removal, as defined in decision 4.

The acknowledgement means: the user has stopped every consumer of this entry,
including older tools, builders, direct executions, sessions, servers and notebook
kernels, and will start none until the command finishes. The user also accepts
that retained project locks, scratch manifests, kernelspecs and scripts may still
name it and will need rebuilding, reconfiguration or another source copy. Stopping
a kernel does not rewrite its kernelspec. Removing a runtime can permanently
remove its only source copy. The notice and README state this plainly.

No command labels an entry "unused" or claims to enumerate all its references.
Versions are descriptive metadata only. Active consumers that ignore the
acknowledgement are not protected by this record, even if a writer lock is free.
In particular, a killed old builder may have orphaned children after its
close-on-exec lock was released. Quiescence includes those children.

### 2. Root selection, read-only inventory and scope

Without `--root`, select the cache using the existing RNX_PROJECT_CACHE,
XDG_CACHE_HOME and HOME precedence, or the runtime store using XDG_DATA_HOME/HOME.
Do not walk upward from the working directory or infer the store from a project
manifest. Only the explicitly requested annotations below read project files.
`--root` is an explicit absolute Unicode store-root override for these maintenance
commands only. It outranks the environment, with no fallback on an invalid value.
It does not change lock/build/run/session/eval or runtime install/select behavior.

Canonicalize the user's chosen root, allowing a symlink that points to a store on
another disk. Below that point, managed control paths must be real, owned,
non-group/other-writable directories or regular files as appropriate, not links.
Check store kind: cache roots have their `locks` directory; runtime roots have
`install.lock`. Opposite-kind or ambiguous control layouts refuse, so passing a
runtime root to `cache remove` cannot bypass selected-runtime protection. A missing
root lists as absent without creating it; a malformed existing root is an error.

A sorted listing reports the canonical store root, full IDs, location (entry or
pending removal), bounded metadata format/provenance when readable, runtime
selection, file count, logical regular-file bytes and estimated allocated bytes.
Corrupt, missing or unknown-version entry documents are labelled unrecognized;
they are not silently called valid or obsolete. List the two locations separately
if the same key has both. Human output escapes control characters and unusual
names. Diagnostic commands shell-quote the actual tool path and include
`--root <canonical-root>` so pasted commands do not silently select another store.

For list and remove **with --dry-run only**, accept repeatable `--manifest FILE`
(up to 64 supplied paths). This is an explicit read-only annotation request, not
reference discovery or a store selector. Read only the named manifest and its
fixed project lock/receipt paths; do not search directories, resolve a dependency
graph, follow project references to other manifests or inspect kernelspecs.
Manifest files retain the existing 1 MiB bound; lock/receipt documents retain the
16 MiB bound, with reads/accounting also charged to the command's memory limit.

Cache annotations use the named project's receipt and its recorded assembly/root
binding in the lock. Mark the matching visible entry `referenced by <manifest>`
and show the recorded path. Runtime annotations use the installed source root
named by that project's lock, not the current default, RNX_DEP_RUNTIME or the
installation's provenance path. Label these as recorded references: no source
freshness, content authentication or executable validation is implied. Support
inspection of the shipped old/current lock and receipt shapes without admitting
old formats into current launch/build paths. Project-local artifacts and overrides
are reported as such, not matched to a shared entry merely by a digest.

Show named references outside the selected store or to missing entries separately.
A reference to entries/ID does not become a reference to removing/ID when that
entry is pending deletion. Missing, malformed, unsupported or observably changing
project documents produce an indeterminate annotation and nonzero inspection
status, never an assertion of no reference. Do not acquire project writer locks,
create .rnx, refresh receipts or repair/migrate documents. Coalesce repeated paths
to the same named manifest for display. Mutating remove refuses --manifest rather
than implying these annotations authorize deletion or protect other projects.

Every annotated report and the README say **only the manifests named were checked**.
An unannotated entry may still have other consumers. An annotation is not a busy
reader, lifetime lease or deletion veto; quiescence and acceptance of broken
retained references remain the separate removal contract.

Control documents use the existing 16 MiB cache and 64 KiB runtime limits.
Filesystem inspection reads metadata, not artifact/source contents: no Git,
Cargo, compiler, adapter builder, source fingerprint, fsck or executable is run.
The existing private SHA-256 authentication path remains migration-only. Removal
of owned corrupt entry data does not require first making it usable again.

Bounds: at most 10,000 named entries in a store listing, 1,000,000 visited nodes
per command, depth 128, 256 MiB of retained traversal/accounting data and 16 MiB of
rendered output. Check allowances before expanding/allocating the next item.
Bounded documents and traversal are cancellable. A bound, permission or topology
failure makes inspection incomplete and nonzero; actual removal refuses before
commit if its preflight cannot complete. Unknown top-level names are reported and
never turned into deletion candidates. Non-Unicode names inside build output can
be handled as OS bytes without opening their contents; they must be escaped in
errors. Managed IDs and control names retain their strict syntax.

Deduplicate inode accounting within each reported entry. Logical size and
`st_blocks * 512` are observations, not guaranteed free-space recovery: hardlinks
outside the entry, reflinks, compression and open descriptors can retain blocks.
Report measured free-space changes only as separate fixture observations. Do not
sum per-entry allocated estimates into a promise about unique physical storage.

Scope is complete `entries/<id>` directories and this record's pending removals.
Never delete project-local `.rnx`, executable overrides, user projects, scratch
projects, kernelspecs, Cargo's global registry/cache, other entries, lockfiles or
runtime selection as a side effect. Resolver scratch and installer `.stage-*`
cleanup keep their existing owners; this is not a sweep of all store contents.

### 3. Writer coordination and selection protection

Actual cache removal acquires the existing `locks/<key>.lock` exclusively and
nonblockingly, retaining it through final deletion/reporting. Actual runtime
removal similarly takes `install.lock`. A busy writer means a named refusal;
removal neither waits for a build to finish nor interrupts it. Lock descriptors
are close-on-exec and remain at their permanent paths after release. Never unlink
a lockfile: a waiter may already have opened that inode. There is no new global
cache lock and no lock ordering across projects or both stores.

Under the lock, re-inspect the target and its ownership. For runtimes, read
`current.json` again. Recognize the bounded format-1 and format-2 selection
shapes without authenticating their source; refuse removal of the selected ID,
including `--resume` for that ID. If selection exists but cannot be interpreted,
refuse rather than treating it as absent. Never unselect automatically or offer a
bypass: select/install a replacement first. Removing the last selected runtime is
outside this first surface. A missing selection permits explicit unselected
removal. Provenance fields such as migrated_from and source_path are history, not
selection and not a discovered lifetime lease.

Dry-run and list never create lockfiles, refresh receipts, repair modes, reclaim
staging directories or acquire mutation ownership. They may report an existing
writer as busy using a non-mutating probe, but cannot reserve a later removal.
Actual removal still checks after taking its lock. Reject a target containing the
maintenance process's current executable or working directory.

These locks protect cooperative writers, including the old tools' same lock
paths. They do not cover ordinary launch or readers. No process-registry,
/proc-based absence claim, kernelspec scan or new shared lock on everyday launch
is introduced. The quiescence acknowledgement is a real contract, not a synonym
for a successful lock operation.

### 4. Hide the whole entry, then finish deletion explicitly

Reserve an owned `removing/` directory next to `entries/` in each store. Existing
readers look only under entries; old installer staging cleanup does not own this
new directory. A removal's commitment boundary is a no-replace rename from
`entries/<id>` to `removing/<id>`, on the same filesystem. Do not move an executable
separately or leave ready.json discoverable over a half-deleted target.

Before rename, validate control directories, selection, the full bounded target
walk and target identity. Keep directory handles; use relative no-follow
operations below them, and refuse filesystem/mount crossings, including a
same-device bind mount. The Linux implementation may use the already-pinned libc
for the necessary directory/rename operations. Gate 1 must demonstrate the
boundary; a host that cannot enforce it refuses before commitment. Do not replace
it with a canonicalize-then-recursively-follow-path fallback.

Inside the target, regular files and directories must be owned by the user.
Symlinks in build output are leaf objects: count/unlink the link without reading
or traversing its referent. Symlinks at the store, entries, removing or selected
entry control boundary refuse (except the allowed initial root spelling).
Special files and mounted subtrees refuse during preflight, without blocking or
opening them. Read-only regular files can be unlinked from writable owned parents;
removal must not chmod/truncate a shared file or alter a hardlink's other names.

Recheck the source entry identity immediately before rename and the destination
identity afterwards. A changed identity stops deletion. Do not overwrite an
existing pending directory or visible entry. Sync both parent directories after
rename before deleting any child. Once renamed, no rollback is promised: a cleanup
error or signal leaves pending state and prints its exact resume command. Sync
failures distinguish "rename may already have committed" from a precommit refusal.
The rename itself changes all original build-output paths, so it already requires
quiescence even if no file has yet been unlinked.

Delete using bounded directory-relative, no-follow traversal. Recheck boundaries
rather than resolving a link into another tree. Sync modified directories and the
pending parent before reporting success. SIGINT/SIGTERM use the tool's existing
signal-status convention and stop at checked operation boundaries; SIGKILL can
leave pending work. No detached janitor or automatic resume on launch, build,
install, select, list or dry-run. The trusted-store/quiescence assumption still
applies; this is not a sandbox against a same-user adversary moving open
directories elsewhere during deletion.

`remove ID --resume --quiescent` operates only on `removing/ID`, taking the same
writer lock and rerunning the applicable checks on its remaining contents. It
must never fall back to deleting `entries/ID`. Without --resume, an existing
pending removal refuses with the recovery command. Without a pending removal,
--resume refuses even if a visible entry has since been rebuilt. With both
locations present, resume deletes only the pending one and preserves the new
visible entry byte-for-byte. Runtime selection protection still applies.

No removal manifest with authenticated content is needed: the checked store,
strict ID and explicit location are the deletion authority. Partially removed
control documents need not decode to resume. Unknown future entry-document
versions are deletable as explicitly selected owned data, never interpretable as
current launch identities. Existing lock, receipt, ready, identity, installation
and selection versions do not change. Maintenance commands neither generate nor
resolve assembly identities. The generator and fingerprint formats stay fixed;
a tracked source edit still changes its input identity in the ordinary way.

Precommit failures preserve the entry and selection; created infrastructure such
as a missing per-key lock or removing directory may remain. Postcommit failures
report pending state and cannot claim reclaimed space or a clean removal. Exit
zero requires the selected location gone and required directory syncs successful.
No-op absence is a named refusal, so a mistyped or wrong-location command does not
pretend to have removed anything.

### 5. End-user outcomes and failure recovery

The README shows list, dry-run and remove together, with exact full IDs copied
from listing and --manifest examples identifying the user's working assembly
and locked runtime. It states that only the named manifests are covered. It explains stopping all consumers, preserving needed source copies,
selected-runtime refusal and resuming an interruption. Old and new IDs receive the
same deliberate treatment. No wording implies relock, successful migration or a
new generator made an old entry unreferenced.

Deleting an assembly invalidates consumers that name its original path. Current
project run/eval/session still never build: they refuse a missing entry with the
existing build guidance. Explicit build may reconstruct it when locked sources,
toolchain and configuration are still available; no universal reconstruction or
bit-reproducibility promise is added. A kernelspec naming a deleted executable
must be repointed/reinstalled by its owner. Removing an installed source can make
both input verification and rebuilding impossible until a suitable source copy
is restored and the project explicitly relocked. No project or kernelspec is
rewritten by removal.

## Gates and stop points

1. **Ownership and filesystem prototype.** Isolated tool copy, actual old/new
   cache/runtime layouts, directory handles and real rename/unlink operations.
   Prove that old live sessions and an old-key kernel can read retained output
   while no writer lock is held; list/dry-run leave them working. Stop them before
   actual removal. Prove busy build/install refusal, no inherited writer lock,
   selected old/new runtime refusal, and rename-before-delete visibility. Exercise
   internal symlinks to outside sentinels, control-path symlinks, file replacements,
   special files, hardlinks, read-only files, nested mounts and bind mounts. A mount
   fixture may use an isolated user/mount namespace; if unavailable, record that
   gap and stop for review rather than asserting the boundary untested. Root
   symlink spelling must work. No unexplained escape, identity difference or
   reference claim passes. This gate precedes a product deletion primitive.
2. **Commands and interruption matrix.** Exact CLI/root precedence, full-ID
   refusals, sorted bounded listing and zero-write dry-run. Cover annotations for
   several explicitly named old/current projects, relative paths, duplicates,
   shared/local/override forms, different stores, missing entries, pending versus
   visible entries and missing/corrupt/changed documents. Assert no project writes,
   no search or inferred completeness, and refusal on mutating --manifest use.
   Corrupt/missing/unknown entry documents remain inspectable/removable as owned data; invalid selection
   and malformed control roots refuse. Acquire the same old/new writer locks;
   kill or interrupt the remover before rename, after rename, after each parent
   sync and during deletion. Preserve all precommit bytes, name postcommit state,
   resume only pending data and retain lock inodes. Include removal failure from
   permissions and sync errors. After a killed remover, rebuild the same cache key
   with an old tool, then prove resume preserves that new visible entry. No
   implicit resume, selected-runtime bypass or compiler/Git invocation.
3. **Real storage journey.** Private stores only. Create authentic SHA-256 and
   BLAKE3 Polars/combined assemblies and runtime installations, including a real
   old-key kernel as the live retained-output consumer, using its old-key
   kernelspec again. Migrate first, then run list and annotated dry-run while the
   kernel remains alive; execute cells that read its retained output before and
   after inspection. Explicitly stop and reap it before removing the chosen old
   entries/runtimes. The new installed default must still serve stock :dep, with
   Polars and a typed PostgreSQL query. Kept entries, project locks/receipts,
   kernelspecs and selection stay unchanged. A deliberately removed current-format
   assembly refuses ordinary launch without compiling, and explicit build recovers
   when its retained inputs permit. Remove one corrupted unselected runtime too;
   no authentication/repair is needed to delete its owned data. Demonstrate pending
   removal recovery in a retained Polars-sized tree, not only tiny fixtures.
4. **Costs, regression and documentation.** Report actual inspected node counts,
   logical/allocated estimates, listing/dry-run/removal/resume wall times and
   observed free-space changes separately on real entries; no exact byte-recovery
   promise or universal threshold. Confirm no per-launch work, identity-schema/generator
   changes or dependency additions. Reuse the matched zero-to-three launch roster
   against 0065 with both repeats, retained outputs and all six first-adapter cells
   reported; do not reopen the accepted noise qualification as a new absolute
   promise. Tool fmt, strict clippy and suites in both configurations, notices,
   publication/migration/interactive/installed :dep regressions; root suites
   serially, notices, fresh default selfcheck and normalized graph. Root source,
   Cargo files, API, kernel, adapters and server remain unchanged. Linux execution
   only, with other platforms refusing maintenance before writes. README includes
   real reclamation and retained-reference consequences before acceptance. The gate-2
   review also carries the recurring loaded-host flake in
   `capability_refuses_bad_replies_and_retires_timed_out_child`: give oversized
   reply validation deterministic coverage without changing the bounded production
   handshake, while retaining the real child-timeout and retirement check.

## Guardrails and later work

No automatic eviction, global project registry, claim to find all references,
lifetime lease migration, default-runtime clearing, removal of local projects or
scratch state, automatic kernelspec repair, cross-root bulk transaction, hash
migration, Git-variable whitelist, path-depth optimization or launcher/runtime
compatibility rule. No user stores are deleted by the fixtures or by writing this
draft. A cache generation is never a deletion predicate. If the ownership or
filesystem gate cannot be demonstrated, preserve that measured stop for review;
do not substitute a version sweep to meet a disk-reclamation target.

Plan plus implementation is the default history shape. A measured stop or changed
design earns its own reproducible checkpoint; review corrections fold into their
unpublished checkpoint. After this record, the Git-variable whitelist and path
depth remain the smaller launch follow-ups, followed by the already-visible
launcher/runtime skew decision.
