# rnx 0061: projects share an assembled executable

Status: plan accepted 2026-09-17 at 3c27365. Gate 1 is accepted and pushed
at rnx b96a94a / rnx-bench 0147c24, recorded in `_context_evidence.md`. Gate 2
is accepted and pushed at rnx 3cc0b51 / rnx-bench 3f23e43, recorded in
`_identity_evidence.md`. Gate 3 is accepted and pushed at rnx 814d20f / rnx-bench 4c8256f, recorded in
`_publication_evidence.md`. Gate 4 command integration is ready for Linux review
in `_commands_evidence.md`; gates 5–6 remain open. The assembly-identity probe
is accepted and pushed at rnx-bench 26af790. This is the shared-cache decision
before an adapter catalogue or :dep workflow.

## Context

0057 builds an executable per project. 0059 makes launch verification cheap enough
for everyday use, and 0060 opens that executable's session. Two applications
using the same installed native extensions still perform separate Cargo builds
and store separate build directories. The next dependency workflow should be able
to reuse an assembly without requiring users to find or copy an executable.

The accepted probe separates three cases. Different applications with identical
absolute native paths already have identical generated manifest/main/Cargo lock;
the real two-project Polars control confirms it. Relative path spelling adds
noise that canonicalization can remove. Moving a native tree is different:
CARGO_MANIFEST_DIR can change output without changing any source byte. Even
unchanged native paths can observe the project's build directory through OUT_DIR.
Thus a normalized key alone cannot promise equivalence to fresh per-project builds.

This record changes where a generated assembly is built. A cache entry owns the
build location and lifetime. Projects consume that assembly; their Rune scripts
and mapped source packages do not participate in native compilation. Reuse means
reuse of that specified build, not reproduction of a build in the user's project.
The executable digest identifies the output. An assembly key identifies the
recorded inputs and location used to find that output before compilation.

## Decisions

### 1. Explicit build, shared by default for newly locked generated projects

Keep the commands lock, build, run, session and eval. A new generated-project lock
selects the shared assembly policy. Build verifies the locked inputs, attaches a
valid ready entry on a hit, or builds it on a miss. It reports either outcome on
stderr, naming the key. No new dependency resolver, registry, daemon or auto-build
is added. Run/session/eval never resolve, compile or populate a cache entry.

Executable overrides retain their existing policy and do not enter this cache.
Different projects with the same assembly key attach the same executable at its
cache path, without copying or hard-linking it into each .rnx/artifacts directory.
Project receipts, derived maps and command locks remain private to each project.
The two public lock files remain beside the manifest; no other project-tree
outputs are introduced outside .rnx.

### 2. Per-user root and a stable entry location

On Linux select RNX_PROJECT_CACHE when present, requiring an absolute Unicode
path; otherwise use an absolute XDG_CACHE_HOME plus rnx/assemblies, or
HOME/.cache/rnx/assemblies with an absolute HOME. Refuse unavailable/relative
locations rather than falling back to the working directory. Create the tool's
root and private descendants with mode 0700. Existing tool-owned directories must
be owned by the effective user and not writable by group/others. Allow symlinks in the user-selected path to the root, then canonicalize it;
refuse symlinks and special files in tool-managed paths below that canonical
root using checked opens;
this remains a trusted user-owned directory, not a hostile-editor sandbox.

Canonicalize the root and record it in the generated lock. It is part of the
assembly identity: moving the cache is not a transparent relocation. Selection
happens at lock; build/launch use that locked root. An explicit RNX_PROJECT_CACHE
that names a different root refuses with a relock instruction. Do not put
the cache inside any fingerprinted native or source root, where outputs could
invalidate their own inputs. Tests always select a private root; they never use
the user's real cache. No machine-wide or cross-user sharing in this record.

Layout under that root is versioned: locks/<key>.lock and entries/<key>/, with
assembly/, target/, artifacts/<digest> and ready.json inside the entry. The
assembly and target paths are chosen before Cargo build and never renamed after
compilation. Artifact publication may rename a temporary file within artifacts;
that does not relocate the directory observed by build scripts. Retain assembly,
Cargo outputs and other entry files as a unit, even after a project detaches.

No automatic pruning, LRU limit, eviction command or forced rebuild of a ready
entry is introduced. Disk use grows with distinct assemblies and is reported in
the README. Manual removal is a whole-entry operation only after all users of
that entry, including running sessions and kernels, have stopped. The tool cannot
prove that condition yet. External cache deletion invalidates referring projects;
launch names the missing entry and asks for build, never silently reconstructs it.
Interrupted unpublished builds can be cleaned/retried under the entry lock because
no project receipt may refer to them. Live-entry tracking belongs to later work.

### 3. Cargo has one recorded build context

For new shared assemblies, Cargo's working directory and manifest live under the
cache root, not under the consuming project. Lock performs metadata resolution
in a private temporary workspace under that root; no compilation occurs there.
Build uses the final entry's assembly directory. Both generated manifests have
identical bytes and canonical absolute native dependency paths. Temporary names
and the final key directory do not enter the key recursively.

Audit the actual Cargo search locations before resolving or building: cache-root
ancestors, Cargo home, native package ancestors/workspace redirects, and explicitly
recorded absent candidates. Generated workspace files are accounted through their
known bytes, not recursively treated as external inputs. Managed intermediate
directories may contain only the tool's known layout; unexpected Cargo config,
toolchain or workspace files there refuse. Gate 1 must demonstrate that resolution
and final build see the same allowed external configuration without a circular key.

Project-local Cargo config/toolchain files which would have selected a different
context under 0057 must not be silently ignored. At lock, inspect project ancestors;
if an existing config/toolchain file is outside the selected cache search chain,
refuse naming it and explain where shared builds obtain configuration. This also
applies to a newly introduced such file before build or launch. Project source
fingerprints still record project files but are excluded from the assembly key.
Native workspace manifests and redirects retain the existing inventory rules.

Keep the existing allowed Cargo configuration tables and build-affecting environment
refusals. Record canonical Cargo home, effective RUSTUP_HOME/RUSTUP_TOOLCHAIN
selection (including absence), and the observed cargo -V and rustc -Vv under the
chosen context. Resolve the host target there. Build confirms toolchain identity;
a cache hit may query versions but invokes no Cargo build/metadata, build script,
linker or compiler compilation. Launch requires neither Cargo nor rustc.

This is still not hermetic. Trusted build scripts/proc macros can observe arbitrary
environment, external files, time and native tools. A hit reuses the recorded build
rather than rerunning those observations. The README must say so, including how to
start a fresh private cache root when a user deliberately needs a fresh build.
Do not claim source hashes or version strings identify every possible build input.

### 4. Versioned assembly identity, separate from project identity

Encode a bounded, deterministic, versioned identity document using a fixed schema
and sorted collections; SHA-256 of those canonical bytes is the assembly key.
Paths are structured canonical paths, not substring replacements as used in the
controlled probe. Preserve package associations and dependency roles explicitly.
Include:

- identity/generator policy version and canonical cache root;
- canonical generated Cargo manifest and exact main bytes, including registration
  names, builder paths, ordering and plain/lifecycle hook selection;
- exact locked Cargo graph digest, host target, release profile, selected features
  and effective toolchain selection/version observations;
- canonical path and working-tree fingerprint for every resolved local native
  package, including rnx, retaining shared-tree associations rather than conflating
  two packages with equal contents;
- allowed external-input contents and absent candidates at their canonical paths,
  plus the recorded Cargo/build context from decision 3.

Exclude application entry paths/text, Rune source packages and mounts, project
locations/lock hashes, receipt stamps, cache-ready timestamps and executable hashes.
Relative spellings of the same native roots converge. Different canonical native
roots remain different even with identical contents. A binding name, feature,
builder or native edit changes the key even when Cargo.lock does not change.
Reuse 0057's bounded inventory and read rules: no faster/weaker hasher or narrowed
native input set. Retain the existing 100000-entry/512 MiB fingerprint allowance
and 16 MiB control-document cap. Stop if all effective context inputs cannot be
accounted for without embedding project location or a recursive final key.

### 5. Publication, concurrent builds and failures

Use a per-key advisory lock outside the entry directory, with close-on-exec file
descriptors. Acquire project lock before entry lock everywhere. Builders for one
key wait interruptibly; unrelated keys do not share a global build mutex. A waiting
builder revalidates its project and assembly inputs after acquiring the lock.
Reuse 0057's owned Cargo process group and interruption/reaping contract.

For a miss, prepare the final stable entry path, copy the exact locked Cargo bytes,
and build with --locked --release and its entry-owned target directory. Offline
remains an explicit lock/build option. Compare native/context snapshots before
and after build; also recheck the requesting project's lock pair and source inputs
before attaching it. A changed native/context snapshot publishes no ready entry.
A changed project cannot acquire a receipt even if another project could reuse
that assembly; preserving such a ready entry is allowed only after all assembly
checks passed independently.

Publish the bounded ready document last using temporary-file, fsync and rename.
It commits the identity document/digest, executable digest and fixed artifact
location, with validated formats and no arbitrary launch path. Fully hash and
verify the installed artifact before publication. A directory without ready is
not a hit. Normal failure or signal publishes no project receipt or partial ready
acceptance. Keep failed-entry diagnostics; retry may clean an unpublished entry
under the same lock. Readiness is an application-level publication rule, not a
claim that every Cargo output has been recursively fsynced against power loss.

On a hit, validate ready against the independently recomputed locked identity,
then fully hash the artifact against ready's digest before attaching a fresh project
receipt. Never trust a digest newly computed from a corrupt artifact. A malformed,
wrong-key or corrupted ready entry refuses with its location; do not overwrite a
published entry which another process might be using. A cache hit skips compilation,
not content validation. A cold entry and a warm Cargo target are not the same kind
of hit, and evidence must label them separately.

### 6. Lock/receipt migration and everyday launch

Introduce generated-lock format 2 with explicit shared-cache binding
and assembly identity; continue reading 0057's version-1 locks. Explicit lock
upgrades generated projects, atomically publishing the Cargo/project pair under
the existing mismatch/rollback rules. It never silently promotes a per-project
artifact into the shared cache: that binary was built in a different location.
Legacy generated build/launch stays supported on its original local path until
explicit relock. Overrides retain their existing lock semantics. Strictly reject
unknown versions/fields and invalid digests as before.

Shared project receipts use version 3 with the project-lock digest, assembly
key, expected executable digest and 0059 stamp. Derive cache root/key/path from
the validated public lock and ready document; a receipt cannot choose another
executable. A shared lock with no receipt asks for build even when a ready entry
exists. Build is the explicit attachment command. Legacy v1/v2 receipt behaviour
for local generated/override projects remains unchanged; do not reinterpret an
old receipt as a shared one.

All launch modes retain the same full project input checks and metadata-default
artifact policy. Matching stamps avoid binary reads; changed metadata and --verify
hash against the already committed digest. Receipt refresh is project-local.
Validate the small ready/identity binding on launch, but do not scan the target
or retained build directory. Runtime-accessed auxiliary build outputs are retained,
not recursively authenticated on each launch; state that scope. No launch waits
for compilation or updates shared readiness. Missing/corrupt bindings refuse.
Run alone creates its source map, and session/eval retain 0060's scope and exec.

## Gates and stop points

1. Context/key prototype first through the real generator/inventory and Cargo:
   resolve then build in final location under a private cache. Prove identical
   allowed config/toolchain selection, no recursive identity, and candidate-file
   creation refusing at the relevant boundary. Reuse the accepted OUT_DIR fixture:
   two consumers observe the same cache path and its retained file after attachment.
   CARGO_MANIFEST_DIR still names the original native root. Stop for review if
   context equivalence or stable ownership cannot be demonstrated.
2. Identity matrix: different app scripts/mounts share, relative aliases of the
   same paths share, relocated native trees miss, and native bytes, registration
   names/hooks, actual features, toolchain selection, target/profile declarations,
   Cargo graph and external inputs invalidate as specified. Separate actual build
   observations from synthetic key-unit cases; do not claim a cross-target build
   without executing it. Native-tree input coverage stays unchanged.
3. Two concurrent projects, one key: exactly one Cargo build, both attach the same
   artifact and read their own sources. Two keys can progress independently.
   Interrupt builder and waiter separately; observe children reaped, no false
   ready/receipt, and a retry completing. Inject failures before ready publication,
   after ready before receipt, during attachment and while sources/locks change.
   Refuse malformed/unknown/wrong-key readiness, symlinks and artifact corruption.
   Cache missing after attachment refuses launch without compiling. No ready entry
   is silently repaired or overwritten. Use compiler-invocation counters/traps
   with positive controls, not elapsed time as proof of a cache hit.
4. Legacy local locks/receipts and executable overrides retain their behaviour.
   Relock requires a new shared build/attachment and never adopts a local artifact.
   Full source checks, metadata misses, --verify, receipt publication failures and
   run/session/eval parity continue to pass the 0059/0060 fixtures. Shared launches
   work without Cargo/rustc; bad project or cache binding executes no script.
5. Real two-project Polars journey: different Rune scripts, same native roots,
   explicit lock/build for each, one shared artifact, eval/session usable in both,
   frames and error recovery as in 0060. Measure separately first shared compilation
   with a cold target, ready-entry attachment, default launch, --verify and direct
   artifact. Downloads, if any, are separate setup, never hidden as compilation.
   A ready hit has zero compilation; it need not be called instantaneous. Retain
   two interleaved repeats of twenty ordinary launch observations per mode with
   pinned CPU/thread settings. Default overhead must remain within 25 ms of direct
   on the existing host; preserve misses and return to review rather than weaken
   source checks. Record retained directory size and cache-hit validation cost.
6. Tool suites, formatting, strict Clippy and notices; existing project workflow,
   fingerprint, verification and interactive regressions. Root API/source/default
   graph, adapters, kernel and server stay unchanged. No new dependency without
   review. Linux execution only; Windows commands retain their explicit refusal,
   with type-check results labelled separately. README includes lock/build/session,
   selection of the private root, changed Cargo context, retained outputs, recovery
   and the stated non-hermetic reuse policy.

## Guardrails and later work

No catalogue, download of prebuilt native code, :dep, live-session replacement,
source import changes, dynamic loading, global GC, cross-user cache, native-tree
relocation equivalence, source-verification cache or reproducible-build claim.
The version handshake proves support, not extension startup. A later transition
may probe an eval, but its arbitrary trusted builder needs real process deadline
and cleanup bounds; its measured millisecond cost is not itself a bound. Settings
are session-only, so even eval success is not a complete session-readiness proof.
Nothing here transfers bindings or makes a later exec atomic against failure.

The next review point is this draft, then gate 1's context/ownership evidence
before general cache implementation. Preserve the accepted probe and meaningful
stops as measured checkpoints; fold incidental review corrections before push.
