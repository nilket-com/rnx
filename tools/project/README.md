# rnx-project

Local source packages and compiled native extensions for rnx. This is a separate
Cargo workspace; none of its dependencies enters stock rnx's default graph.

```sh
cargo build --locked --release --manifest-path tools/project/Cargo.toml --bin rnx-project
rnx-project lock --manifest app/rnx.toml --offline
rnx-project build --manifest app/rnx.toml --offline
rnx-project session --manifest app/rnx.toml
rnx-project eval --manifest app/rnx.toml -- '1 + 1'
rnx-project run --manifest app/rnx.toml -- argument1 argument2
rnx-project run --manifest app/rnx.toml --verify -- argument1 argument2
```

`--manifest` is required; there is no upward search. Paths inside a manifest are
relative to that manifest. Only lock resolves the Cargo graph. Build uses
`--locked`, the release profile and the compiler's host target; run never invokes
Cargo or rustc, builds anything, repairs a lock or requires a network connection.
**Launch checks your sources, trusts your build output unless you ask it to verify.**

By default, matching artifact size, modification time, executable-bit state and
Unix device/inode avoid rereading the executable. A mismatch triggers a full hash
against the known digest; matching contents refresh the receipt, different
contents refuse. `run --verify` always hashes the artifact. It is accepted once,
in either order with `--manifest`, before `--`; lock/build do not accept it.
Run passes everything after `--` as script arguments. Direct runner flags before
`--` are not exposed by this first project CLI. Build chatter goes to stderr.

## Open the project's prompt

After the explicit lock and build above, `session` opens the assembled executable's
REPL with its native extensions, including Polars. `eval` takes exactly one source
argument after `--`; quote it in your shell. Neither command runs the manifest's
entry file, builds anything, or imports mapped Rune packages. Module declarations
remain unavailable in eval and sessions. Source dependencies are still checked
for changes even though these modes do not use a source map.

```sh
rnx-project session --manifest app/rnx.toml --no-splash --color=never
rnx-project eval --verify --manifest app/rnx.toml -- '1 + 1'
```

Session and eval accept `--verify` and `--color=auto|always|never` once, before
any `--` boundary, in either order with `--manifest`. Only session accepts
`--no-splash`. Session takes no positional arguments or trailing `--`; eval takes
exactly one source argument, including an empty string. `--offline` belongs only
to lock/build. Run's existing script-argument boundary is unchanged.

All three launch modes share the receipt and source checks described below.
They inherit your working directory: relative CSV and Parquet paths refer to
where you launched the command, not the manifest directory. Session loads your
usual rnx settings; eval does not. The executable owns the terminal, history,
signals and exit status, with no proxy process. Reset clears bindings and retains
extensions. Use a frame's `preview()` for bounded display; bare frames remain
opaque. Opening a session releases the project command lock, and verification
runs once before launch, not again between inputs. Rebuilding a project does not
change an already-running session.

## Add a known adapter

The tool ships a small catalogue of adapter declarations:

```text
$ rnx-project adapters
NAME      PACKAGE       HOOK       PATH BELOW RUNTIME
polars    rnx-polars    plain      adapters/polars
postgres  rnx-postgres  lifecycle  adapters/postgres
```

In an existing application with a `[runtime]` path, add a name, then explicitly
lock, build and open the new executable:

```sh
rnx-project add --manifest app/rnx.toml polars
rnx-project lock --manifest app/rnx.toml --offline
rnx-project build --manifest app/rnx.toml --offline
rnx-project session --manifest app/rnx.toml
```

Use `--offline` only when the needed Cargo dependency sources are already cached;
omit it from lock/build when they need to be fetched. `add` itself never fetches,
builds, invokes Cargo or starts a builder. Both `polars` and `postgres` can be
given in one add command, before or after `--manifest`. Names are case-sensitive.
Custom adapters still use explicit native tables; there is no registry lookup.

The adapter must exist under the project's declared runtime checkout, at the
listed path. Listing a name does not claim that its sources are installed. Add
checks the shipped Cargo layouts and their direct dependency on that runtime;
compilation and startup remain separate checks. A prebuilt executable override
or a source-only manifest cannot gain adapters with add.

For `[runtime] path = "../rnx"`, add writes a normal `[native.polars]` table with
`path = "../rnx/adapters/polars"`, package, builder and hook. Relative paths stay
relative, absolute paths stay absolute. No catalogue selector remains in your
manifest or lock, so future catalogue changes cannot reinterpret it. Moving a
relative source layout still requires relocking its canonical build identity.

Add preserves existing text and appends missing tables in name order. An
equivalent existing declaration is a no-op that does not touch the manifest;
a different declaration under the same name refuses. A closed inline `native`
table may need to be rewritten as explicit tables by you before appending. All
names and the whole candidate are validated before one manifest replacement.
The original and candidate are each limited to 1 MiB, with at most 32 requested
names and the existing limit of 256 declarations per table.

The project command lock excludes other tool writers. Add rechecks bytes and
file identity before replacing the manifest, but cannot prevent an arbitrary
editor from racing the final rename. Before-rename failure leaves the original;
after-rename failure can leave the complete new file and reports that replacement
happened without confirmed durability. Ordinary permission bits are preserved;
other filesystem attributes are not copied. A stale `.rnx/add-manifest.new`
refuses rather than being followed or overwritten; remove it only when no project
command owns it. No lockfile, receipt or cache entry is updated by add: after an
addition, old launch refuses until explicit lock/build. An already running
session keeps its executable and bindings; add does not load into that process.

Listing is platform-independent. Manifest mutation retains the tool's existing
Unix supervision requirement; Linux is exercised and Windows add refuses.

## Files you commit

The generated form writes exactly **rnx.lock** and **rnx.Cargo.lock** beside
rnx.toml. The former is bounded, deterministic, readable JSON; the latter is
Cargo's own lockfile. Source and native inventories are separate, with package
associations separated from shared tree hashes. No generated wrapper, receipt,
source map or executable is written beside the manifest.

Project-local state is under `.rnx/`, which contains its own `.gitignore` with
`*`: command lock, derived source maps and receipt.json. New generated projects
build their assembly, target tree and executable in the shared cache described
below. Legacy format-1 projects keep those outputs under their own `.rnx/`.
An executable override writes only rnx.lock and removes an obsolete generated-form
rnx.Cargo.lock after publication. Its first launch verifies the locked binary and
establishes a private version-2 receipt; no Cargo is needed.

## Shared assemblies and existing projects

An explicit `lock` now writes format 2 for generated projects. `build` reports
`built shared assembly` or `attached shared assembly`, naming its key and artifact.
Two projects with the same installed native dependencies and build context share
that artifact; application scripts and source mounts stay project-specific.
A ready hit skips compilation, but fully hashes the artifact before publishing
the project's receipt. Launch never builds, attaches or repairs a missing cache.

Existing format-1 generated locks remain local. Their version-1/2 receipts retain
the prior migration and metadata-check rules. Launch and build do not upgrade
those locks. Only an explicit relock selects the shared policy, after which an
explicit build is required. The old local binary is never promoted into the
cache: it was compiled at a different location. A valid existing shared entry may
be reused instead of compiling again. Old local files are left in place.
Format compatibility does not waive stale-input checks: updating a fingerprinted
rnx checkout still requires relocking/rebuilding, just as before.

To select a private root before locking:

```sh
export RNX_PROJECT_CACHE="$HOME/.cache/rnx/assemblies"
rnx-project lock --manifest app/rnx.toml --offline
rnx-project build --manifest app/rnx.toml --offline
rnx-project session --manifest app/rnx.toml
```

Without that override, selection uses `$XDG_CACHE_HOME/rnx/assemblies`, then
`$HOME/.cache/rnx/assemblies`; relative paths refuse. A user symlink to the root
is allowed and canonicalized. That canonical root is part of the locked identity.
An explicit different selection requires relock. Managed descendants refuse
symlinks, special files and group/other-writable directories. Cargo's child gets
a private umask so its target directories follow that policy too.

Cargo resolves and builds from the cache, not the application's working directory.
A project-local Cargo config or toolchain file outside that search chain refuses
rather than being silently ignored. Canonical native paths, the locked graph,
allowed external inputs and toolchain selection participate in the shared key.
Moving a native package or the cache creates a different identity even when its
source bytes are identical.

The cache retains entire `entries/<key>/` directories: assembly sources, target
outputs (including files embedded through OUT_DIR), digest-addressed artifacts
and readiness. There is **no automatic eviction**. Disk use grows with distinct
assemblies and includes full Cargo targets, not just the executable. Remove a
whole entry only after all its sessions, kernels and other consumers have stopped;
the tool does not track that condition. Missing entries require an explicit build.
Never delete only a retained auxiliary build-output directory from a live entry.

This is trusted, non-hermetic reuse. Build scripts and proc macros can observe
unrecorded external state; a hit reuses their earlier output rather than rerunning
them. Choose a fresh private root and relock/build when a fresh build is required.
Auxiliary outputs are retained, not recursively authenticated on each launch.
Shared startup and real Polars disk/attachment costs are measured in 0061 gate 5;
this integration does not claim that attachment is instantaneous.

The command lock is an OS advisory file lock, released even if a process dies.
The two public files cannot be replaced with one filesystem rename. Publication
stages and syncs the bytes, replaces Cargo's file first and rnx.lock last; the
JSON lock commits the exact Cargo digest. Cooperating commands serialize. An
interruption between replacements can leave a mismatched pair, which build/run
**refuse**, rather than accepting mixed generations. Run lock explicitly to
recover. A normal publication failure attempts to restore the old JSON first and then
the old Cargo bytes. There is no claim of an atomic snapshot against an editor or git.

Build removes an old receipt before attempting work. A shared build rechecks
project and assembly inputs before attachment and publishes a version-3 receipt
binding the project-lock digest, assembly key, executable digest and metadata
stamp. A local build publishes version 2. The shared ready document is committed
last under a per-key lock, after full artifact verification; a waiting builder
re-inspects readiness rather than treating lock release as success. A failed
project attachment can leave a valid shared entry without a project receipt.

All launch modes recheck lock pairs, declarations, source/native contents and
generated wrapper identity. Shared launches additionally validate the small ready
binding, but do not scan Cargo's target tree. Identical derived maps are compared
and reused; only run publishes or reads them. Missing generated receipts require
build even if a shared entry is already ready. Old local receipts are never
interpreted as shared attachments.

A valid local version-1 receipt is fully checked once and migrated before launch.
Overrides without a usable receipt check their already-locked digest to establish
one. Malformed receipts refuse; launch never makes changed contents trusted by
changing the digest. Receipt refresh is atomic. A harmless touch or identical
replacement costs one full check, then subsequent launches become fast again.

This is trusted local build output, not a content-authentication boundary. The
default can miss a same-size in-place edit if its mtime is restored or the
filesystem cannot distinguish the timestamps. Identity reuse or manipulated
metadata/receipts can also defeat metadata checks. Use `--verify` for a full
content check. Neither mode makes verify-then-execute atomic against concurrent
replacement. Source edits, including same-size/restored-mtime edits, still use
content fingerprints and are refused until the project is refreshed.

In the 0059 per-project measurements on the small Polars application, the old
project launch took about
155 ms, the metadata default 29 ms, explicit `--verify` 95 ms, and generated-direct
11 ms. These are whole CLI runs on one pinned Linux host, not notebook timings
or universal latency bounds. Native input checks remain about 17 ms. Legacy
migration and metadata-mismatch refresh each took about 98 ms on this example;
they are separate first-use costs, not part of the warm default number.

The 0061 shared-cache measurement used two different applications with the same
Polars extension. A fresh entry with an empty Cargo target took 106 seconds to
build on that host, with dependency sources already cached and Cargo offline.
The second application attached to the existing entry without compilation:
566 ms for its first attachment, and a 315 ms median across ten subsequent
pinned measurements. Attachment includes a full executable hash, input checks,
tool-version checks and receipt publication; it is not free.

Everyday shared launches of the tiny pipeline took about 31 ms, versus 11 ms
direct and 97 ms with `--verify`. Eval and first prompt each added about 20 ms
over direct, within the same 25 ms gate. These are single-host measurements,
not latency promises. The complete retained entry occupied **1,570,058,240 bytes
(about 1.46 GiB)** on disk, including its assembly, Cargo target and 107 MB
artifact. Two applications shared that one entry. Distinct assembly keys retain
additional whole entries; there is no automatic eviction. Do not keep only the
executable: trusted build scripts can make it depend on retained build outputs.

## Inputs and build policy

Source trees include regular files recursively, excluding `.git` directories and
only the project's own outputs at the application root. Native path packages use
Git-tracked **working-tree bytes**, so dirty edits count. Missing tracked files,
untracked non-ignored files, submodules, symlinks, special files and non-Unicode
names refuse. Ancestor workspace, config and toolchain candidates are recorded,
including absence. Shared native roots are hashed once with each association kept.

A native rnx path dependency covers the whole tracked repository, including plans
and nested adapters. A plan edit therefore invalidates a project's build identity.
Narrowing that input set would be a separate root decision. A project whose
lockfiles are inside a native package root refuses: hashing a lock into its own
native input digest would be self-referential. Put the project outside that root;
the tool does not invent an exclusion from the native contract. A source root
containing the project similarly refuses unless it is the application entry root
where the explicit output exclusions apply. Moving a project
preserves content hashes where bytes and modes match, but absolute paths in the
map, inventories and generated assembly change: relock and rebuild explicitly.

Configuration includes stop the audit. Build-affecting Cargo config overrides
are refused: only http, net, registry, registries and term tables are allowed.
CARGO_* environment overrides except absolute CARGO_HOME, and RUST* overrides
except RUSTUP_HOME/RUSTUP_TOOLCHAIN, are refused by name without printing values.
Toolchain selection is recorded through actual rustc -Vv and cargo -V output;
build compares it with the lock. Run needs no installed compiler and validates
the provenance of its already-built artifact instead. Arbitrary trusted build
scripts/proc macros can still read ignored files, environment and external inputs.
This is stale-build detection, not hermetic or bit-reproducible compilation.

Manifests are capped at 1 MiB, JSON control files and captured tool output at
16 MiB. Fingerprinting has a shared per-snapshot allowance of 100,000 entries and
512 MiB; compilation retains its separate 8 MiB source allowance. Native Git
inventory output also has a 16 MiB cap. Tree hashes use the record's versioned
SHA-256 framing with sorted paths, content lengths, bytes and executable mode.

## Platform and validation

Linux commands are exercised. On Unix, Cargo owns a separate process group;
SIGINT/SIGTERM kill that group and reap the direct child before returning 130/143.
Run execs the verified executable, preserving its process status and signal
semantics. Trusted programs that deliberately leave the group are not contained.

**Windows project commands currently refuse at startup.** The code type-checks,
but owned Windows subprocess supervision is not implemented; this is stronger
than simply saying execution is unverified. No orphan-prevention claim is made
for Windows. The root project-source loader remains separately type-checked.

```sh
cargo test --locked --manifest-path tools/project/Cargo.toml -- --test-threads=1
cargo test --locked --manifest-path tools/project/Cargo.toml --features test-support -- --test-threads=1
cargo clippy --locked --manifest-path tools/project/Cargo.toml --all-targets --features test-support -- -D warnings
python3 tools/project/scripts/notices.py --check
```

Test-support adds pause points for publication and post-build edits; they are
absent from the ordinary executable. The assembly probe remains a test-only
adapter over private functions, not a second product CLI. Full workflow fixtures
are in rnx-bench/probes/project-workflow. No tool implementation module is public.

## Requesting adapters from a live session

A session opened through this tool retains its project association. Type
`:dep polars postgres` to request catalogue adapters: the tool describes the
change, asks for consent, then adds, locks, builds or attaches, and checks actual
startup before replacement. Already-installed names are a no-op. Use
`:dep --offline polars` to forbid source downloads. This is a restart with binding
loss, not dynamic loading; saved history is reloaded but never replayed.

For a stock session, install a runtime as shown below and locate this tool through
PATH or `RNX_PROJECT_TOOL`. `RNX_DEP_RUNTIME` remains an explicit override. A retained scratch project is created
only after consent. Its successful replacement prints the quoted command to
reopen it. Projects and scratch consumers reuse the same shared assembly when
their native inputs match. The working directory stays the caller's directory.
A private startup probe executes builders before the replacement executes them
again. Failed preparation preserves the old session, but project edits may remain;
cleanup or exec failure after the restart announcement is terminal.

## Retained runtime sources

Install a complete local runtime snapshot before removing its checkout:

```sh
rnx-project runtime install --from /path/to/rnx
rnx-project runtime show
rnx-project runtime select FULL_INSTALLATION_ID
```

The store is `$XDG_DATA_HOME/rnx/runtimes`, or
`$HOME/.local/share/rnx/runtimes`. A symlink to the store is allowed; managed
paths below its canonical root must be private owned directories/regular files.
Installation copies tracked working-tree bytes, including both shipped adapters,
and builds an independent Git index. Dirty tracked edits are supported; untracked
non-ignored files, conflicts, symlinks and escaping Cargo paths are refused.
No source file is rewritten. Git, Rust/Cargo and native build tools remain needed
for assembly. The installed index intentionally has an unborn HEAD: `git status`
shows every file as added. No commit is needed to make it usable.

A stock session's `:dep` uses this selected installation without an environment
variable. `RNX_DEP_RUNTIME`, when present, is an explicit absolute-path override;
an invalid override refuses rather than falling back. Project sessions always use
their declared runtime. Before consent the notice identifies the runtime and its
provenance. Preparation rechecks the selection and the installed source contents
before creating a scratch. Changing the default does not retarget an old scratch.
Discovery does not search the working directory and does not install anything.

An identical reinstall validates and reselects the existing installation without
changing its original provenance. A different snapshot remains beside earlier
ones. Selecting a default does not alter existing projects or scratch sessions.
Canonical native paths are build inputs, so installing elsewhere gives a different
assembly identity and can require a fresh build. This does not make builds hermetic.

A failed copy/index step preserves the default. If entry publication succeeds but
selection fails, the tool reports **installed but not selected**; the retained ID
can be selected later. After the selection rename, a sync failure reports that the
selection may already have changed. Entries are retained whole; removal and automatic
upgrades are not implemented. Metadata records source provenance and the installing
tool's version and digest, but launcher/runtime compatibility is not checked.
