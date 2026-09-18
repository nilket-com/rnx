# rnx 0064: a runtime installation outlives its checkout

Status: accepted for implementation after the 2026-09-18 draft review, with
F1 on runtime provenance and unchecked launcher/runtime skew folded in below.
Baseline: rnx `1ecc35c` / rnx-bench `9aa378d`.
0063 is accepted and closed on Linux. This record removes its source-checkout
environment setup from ordinary stock `:dep` use. Native-inventory optimization
follows separately, measured against the number of declared native packages.

## Context

A stock executable cannot reconstruct the source tree that built it. Cargo needs
that tree to assemble extensions through 0051; a prebuilt rnx binary alone is
not a dependency SDK. Today 0063 therefore asks the user to export
`RNX_DEP_RUNTIME=/absolute/path/to/rnx` before creating a scratch project.

Three existing contracts constrain installation:

- 0057 fingerprints Git's tracked path set and working-tree bytes, not Cargo's
  package archive or a commit hash. Its native inventory refuses untracked
  non-ignored files, unmerged entries, symlinks and submodules. The root tracked
  set includes adapters, tool, kernel, server, plans and documentation.
- 0061 includes canonical native paths and the cache-owned build location in
  assembly identity. Equal source bytes installed elsewhere have a different
  assembly key. Build scripts and manifest-directory constants can observe that
  relocation; sharing the checkout's artifact would assert an unproved identity.
- 0063 makes project ownership explicit, validates a tool-authored description
  before consent, and preserves the old session through preparation failures.
  Installation discovery must not introduce working-directory search or change
  the commitment boundary.

The intended journey, with rnx and rnx-project already available on PATH, is:

```sh
rnx-project runtime install --from /path/to/rnx
rnx
# At the prompt:
# :dep polars
```

The initial source may be removed after installation. This is a local source
installation, not a downloader, binary installer or package registry. Rust/Cargo,
Git and required native build tools remain prerequisites for assembly. The
installer does not build Polars in advance or promise a Python-wheel experience.

## Decisions

### 1. Install the tracked source snapshot, including shipped adapters

`rnx-project runtime install --from PATH` accepts one explicit runtime root;
relative input resolves against this command's working directory, then the root
is canonicalized. Require the root to be the top of a Git working tree and to
contain the supported rnx Cargo layout. A linked-worktree source is allowed;
its external Git administration is read for inventory, never copied or linked.

Copy every tracked regular file below that root, using working-tree bytes and
executable bits. Do not use `cargo package`, infer an include list, export HEAD,
or silently omit plans, notices, examples or independent workspaces. Dirty
tracked edits are allowed and identified by their content. Untracked non-ignored
files, missing tracked files, conflicts, submodules, symlinks, special files and
unsupported names refuse under the existing fingerprint rules. Ignored build
outputs, ignored reviews and the source's `.git` administration do not travel.

The shipped `adapters/polars` and `adapters/postgres` travel at their existing
relative paths, with their manifests, locks, notices and sources. Validate both
against 0062's catalogue layout. Do not rewrite any copied manifest or source to
make installation succeed. Scope is these shipped layouts, not arbitrary Cargo
workspace relocation. Statically check copied Cargo manifests for path/workspace
references escaping the snapshot, including target/build/dev dependencies,
patch/replace tables and explicit package/target source paths; unsupported indirection refuses by file and field. An
external workspace or path dependency is not silently borrowed from the source
machine. Gate 1 must prove the supported layouts against real Cargo.

Use the versioned 0057 file/tree encoding and its 100,000-entry, 512 MiB logical
source allowance. Enforce reads against the allowance, with the existing single
detection-byte rule, rather than trusting metadata. Preserve byte and executable
identity with private installed modes (0600 or 0700); do not preserve set-id or
other unrelated permission bits. Fingerprint the original before and after the
copy and the destination independently. Different path sets, bytes or executable
bits refuse before activation. This detects observed edits, not an atomic snapshot
of a hostile or concurrently edited filesystem.

### 2. An independent Git index, not a new inventory mode

Create fresh Git administration inside the installed source. It must contain an
independent index and the objects needed for its tracked entries, with no remote,
alternate object store, worktree link, absolute core.worktree setting, source
hooks or copied source configuration. No original history is required and no
synthetic commit is needed merely to enumerate the index. The installed working
bytes and indexed paths/modes must reproduce the original tree digest.

Populate this index from the validated file inventory without running attribute
clean filters, line-ending conversion or hooks. Installer-owned Git commands use
an isolated Git configuration/template context and do not inherit Git routing
variables that could redirect the destination. Gate 1 selects and records the
exact commands, proves literal unusual paths and executable modes, and proves
that configuration/attributes cannot change the copied snapshot. Stop if an
independent faithful index cannot be demonstrated without changing inventory
semantics. Do not copy the source's `.git` as a workaround.

Git remains required by the project tool's existing native inventory: lock,
build and verified run/eval/session still invoke it. Plain direct rnx sessions
and direct artifacts do not acquire that requirement merely because a runtime
is installed. Missing Git must produce an actionable prerequisite error; stock
`:dep` keeps its old prompt. A non-Git installed inventory and elimination of
Git on everyday launches are not smuggled into this record.

### 3. Retained installations and explicit default selection

Use an absolute `$XDG_DATA_HOME/rnx/runtimes`, or when XDG_DATA_HOME is unset,
`$HOME/.local/share/rnx/runtimes`. Empty, relative or non-Unicode configured roots
refuse. Canonicalize the user's selected root; a user symlink to another disk is
allowed. Below that canonical root, managed paths must be owned directories or
regular files with private write permissions, without symlinks or special files.
Do not place this store inside the source checkout, an independently managed
native root, a scratch project or the assembly cache, or nest those stores
inside it. Its own installed source roots are the deliberate exception.

The layout is:

```text
runtimes/
  install.lock
  current.json
  entries/<installation-id>/
    installation.json
    source/                 # complete tracked snapshot, independent .git
```

An installation ID is a versioned hash of the snapshot encoding/version and its
content digest. It identifies installed source content, not an assembly or a
release version; rnx's `0.0.0` cannot distinguish these snapshots. Metadata is
outside the fingerprinted source root. A bounded, strict version-1 installation
document records the ID, tree digest, file/byte counts and supported layout.
Record provenance alongside that identity: the original canonical source path,
source commit when available, a dirty-tracked/uncommitted-snapshot marker where
applicable, and installation time in UTC. Also record the installing project
tool's version string and executable SHA-256 digest. The version string alone
is not sufficient while the tool reports `0.0.0`. These fields diagnose which
source and installer produced the installation; they do not certify compatibility
with a later launcher or tool. They are not inputs to the installation ID, and
no later operation reads the original source path. No copied file depends on it.

The bounded, strict version-1 current document names one full installation ID;
it is not a free-form path or a symlink. Cap each metadata document at 64 KiB;
reject unknown versions/fields, malformed IDs and traversal spellings.
`runtime install` selects the completed installation as the default and prints
its ID, source path, source size and prerequisites. Reinstalling identical
content fully validates the retained entry and selects it, without rewriting it
or replacing its original installation provenance with that of the latest caller.
A corrupt retained entry refuses; do not overwrite it under the same identity.

`rnx-project runtime show` prints the selected ID and canonical source path,
and separately labels any RNX_DEP_RUNTIME override. It does no resolution,
compilation or mutation. `rnx-project runtime select ID` validates a retained
entry fully and changes the default atomically. IDs are full, not ambiguous
prefixes. Installing/selecting never rewrites existing project manifests, locks,
receipts, scratch associations or shared assemblies. Old installations remain
available. Uninstall, eviction and automatic upgrades are deferred because
existing projects can refer to them by absolute path.

### 4. Publish completely, then select

Serialize writers with one owned advisory installation lock outside entries;
waiters honor interruption and re-examine state after acquiring it. Stage under
the same canonical store, with exclusive names. Never write into the source.
Bound retained logical source plus generated Git administration to 2 GiB and
400,000 files; these are new installer bounds, not larger source allowances.
All Git subprocess output is bounded and its children are owned, cancelled and
reaped through the tool's subprocess mechanism. No Git child inherits the
installation lock across exec. Check cancellation between bounded copy steps.

Sync copied files and generated Git administration, validate the installed
snapshot, write/sync installation metadata and publish the whole entry by rename,
then sync its parent. Revalidate at its final location: Git administration must
not depend on the staging name. Only after the entry is complete may a synced
temporary current document replace current.json, followed by a directory sync.
Readers see the old default or a complete new one. A directory without valid
installation metadata is never a discovered runtime.

On ordinary failure, remove owned staging data and preserve the old selection.
If entry publication succeeded but selection failed, retain the completed entry
and report that it is installed but not selected. After selection's rename, a
sync failure must report that selection may already have changed; do not claim
rollback. Interrupted leftovers are inspected under the writer lock on retry;
never delete published entries or another writer's live work. This is local
trusted storage, not protection against an owner deliberately racing the tool.

### 5. Tool-owned discovery, with an explicit override

For an unassociated stock session, the tool resolves scratch runtime selection
in this order:

1. RNX_DEP_RUNTIME, when present, is the explicit absolute-path override. Invalid,
   missing or unusable override targets refuse; do not silently fall back.
2. Otherwise use the current installation in the standard data store above.
3. Otherwise refuse with the exact runtime-install command form and the existing
   project-session alternative. No files are created just to discover absence.

An associated session still uses its project's declared runtime, ignoring both
the override and installed default. A custom unassociated executable still
refuses additions as in 0063. Never search the current directory, parents, a
nearby Cargo.toml, the executable's location or Git remotes for a runtime.
Discovery happens only when requested; ordinary cells, run/eval, worker, help and
version gain no store lookup. The root REPL does not become an installer or keep
an independent runtime catalogue; the project tool owns this decision.

Describe reads bounded metadata and layout. Before consent its notice names the
runtime that will replace the current session, not just the proposed manifest:

> Runtime: installation <id> (installed from <source path>, <commit and/or dirty
> tracked snapshot>, <installation time>)

For explicit selection it instead prints `Runtime: override <canonical path>`.
The installed notice also identifies the canonical installed source root. Render
all provenance through the existing terminal-safe output path. This visibility
is not a compatibility check or a promise that the replacement has all features
of the current launcher. Describe opens no writer lock,
updates no selection and builds nothing. Carry that exact selection through
consent using the existing tool description/revalidation path. Prepare rechecks
the default/override and, for an installation, fully validates its recorded source
identity before creating a scratch or authoring declarations. A changed selection
requires a fresh description and consent, not a silently different runtime. Do
not trust installation metadata as proof of unchanged source bytes.

The scratch manifest stores the installed source's canonical absolute path as
an ordinary runtime declaration. Later default changes do not retarget it.
Existing 0062 adapter authoring, 0061 assembly, 0059 verification, 0063 startup
probe and commitment semantics apply unchanged. No manifest/lock/receipt/cache-key
format change or public Rune API is justified here. A protocol change, if gate 1
finds one necessary, must be versioned and fail before mutation with older tools;
do not silently change the existing frame contract.

### 6. Installation is relocation, not assembly equivalence

An installed tree must have the same source tree digest as its source snapshot.
Its canonical runtime/adapter paths differ, so its assembly key differs even
when generated code and Cargo graph otherwise agree. Record both comparisons.
Do not promote a checkout-built artifact or rewrite an existing lock to claim
cross-location reuse. The first installed-runtime assembly may compile again.

Two scratch/project consumers of the same installed runtime and native recipe
must share the installation's assembly through 0061, with a compiler trap proving
the second attachment. Moving the installation store changes canonical paths;
old absolute project declarations can break and require explicit repair/relock.
An original parent Cargo configuration, toolchain file or native build-script
input is not made hermetic by copying source. Retain 0061's audited configuration
policy, record the installed build context and refuse unsupported dependencies.
Source equality is not a promise of identical binaries or build behavior.

## Gates and stop points

1. **Independent source/index prototype before product implementation.** Copy a
   fixture checkout using the real fingerprinter, including shipped adapter
   layouts. Prove source/destination digest equality and Git index independence,
   including a linked-worktree input, dirty tracked bytes, literal path spellings,
   executable bits and hostile Git filter/routing configuration controls. Rename
   the original fixture checkout so its former path does not exist. Do not touch
   the user's working checkout. Remove RNX_DEP_RUNTIME and exercise stock :dep
   through prototype installed discovery, the real project tool and real serving
   entry. Require a cold private assembly cache: no previously built artifact
   may hide an original-path dependency. Build and execute real Polars and a typed
   PostgreSQL query on a private cluster. Inspect generated manifests, native
   inventory and Cargo metadata for any surviving path back to the source.
   Stop and revise if source closure/index/discovery cannot be established, a
   public extension API is needed, or inventory/identity coverage must change.
2. **Installation and publication failure matrix.** Boundaries and overflows,
   missing/dirty/unmerged files, escaped Cargo paths, symlinks/FIFOs, invalid
   metadata, source changes during copying, corrupt same-ID entries, failed Git,
   concurrent installers/selectors, interrupted builder and waiter, and every
   entry/selection publication boundary. Assert old selection before commitment,
   honest outcomes after rename, no partial discovered entry and no subprocess
   leftovers. A symlinked user store works; managed symlinks do not.
3. **Discovery and ownership matrix.** No override/default, valid override,
   invalid override with a valid default, default selection change across consent,
   corrupted installed content, deleted installation, conflicting cwd projects,
   and an associated project overriding neither way. Missing Git is an actionable
   error; the stock session and a started tracked future survive. Decline and
   failed preparation leave source/default/session ownership unchanged, with
   0063's honest qualification for already-published project files. Test multiple
   retained installations and old scratches reopening after default selection.
4. **End-user journeys from installed sources.** Build the launcher, project tool
   and installed runtime from one exact source snapshot for these journeys;
   record that snapshot identity and the launcher/tool binary digests. This is
   not evidence for mixed-revision compatibility. Check the installation's
   provenance and installing-tool identity, and the runtime notice before consent,
   including the explicit override form. Real stock :dep polars with no
   RNX_DEP_RUNTIME, frame retained/transformed across inputs and a catchable error,
   mixed postgres addition, typed private-cluster query, history/binding-loss and
   scratch reopen. Original fixture source path remains absent throughout. Second
   consumer attaches with compilation trapped. Record equal source digests,
   different checkout/installation assembly keys and equal installed-consumer
   keys. No source-index or symlink back to the unavailable checkout is allowed.
5. **Costs and documentation.** Measure installation copy/index/publication time
   and retained source/Git bytes, first installed build separately from ready
   attachment, discovery/describe cost, and matched default/direct launches for
   both one-native and two-native installed projects. Compare each with its
   checkout-shaped control, not a fixed adapter-independent latency promise.
   This record must not add installation rescans to ordinary project launches or
   cells. Explain Git/Cargo/toolchain requirements, the one-time source install,
   default/override precedence, retained disk growth and relocation's new build.
6. **Regression.** Root suites serially, tool tests/clippy/notices, default graph
   and public API unchanged; 0063 preparation/startup/commitment, project/cache
   verification and real adapter replays. Linux installation/transition execution
   first; Windows type-check and explicit unsupported mutation behavior without
   changing ordinary sessions. Archive exact measured source, not hashes alone.

## Guardrails and next work

Do not turn installation into native dependency resolution, cross-location cache
reuse, a prebuilt plugin format or a release downloader. No source narrowing or
non-Git fingerprint mode here. No automatic migration of existing projects and
no eviction while they can retain paths into an installation.

Launcher-versus-installed-runtime skew is unchecked here. Upgrading the stock
launcher and tool does not upgrade an installed source snapshot: `:dep` can
replace a newer launcher with an executable assembled from older installed code,
or the reverse, and features may differ. The notice makes the selected runtime
visible and metadata preserves its installer identity; neither proves semantic
compatibility. Existing protocol/startup checks remain in force but are not a
version compatibility policy. A later compatibility decision owns any matching,
warning or refusal policy for such mixed revisions.

The next record addresses native inventory's cost as adapter count grows. That
record owns any caching or reduced checks and their coverage implications.
Binary/source distribution without an initial local checkout, if desired, needs
its own provenance and delivery decisions; this record must not advertise it as
already solved. Dynamic loading, mapped notebook sources and value migration
remain separate from installing a buildable runtime.
