# rnx 0062: an adapter name becomes a declaration

Status: plan accepted 2026-09-17 with the path-spelling decision below, following
accepted 0061 at rnx d4f10b9 / rnx-bench 6576a88. Gate 1 was accepted at
075e95e / a649ac4. Gates 2–6 pass and are ready for Linux batch review in
`_implementation_evidence.md`; the catalogue commands are implemented with no
format or dependency changes. This is the catalogue step before a live session
can request a dependency.

## Context

0061 makes two applications reuse one assembly when their native inputs match.
The user still has to know a native package's path, Rust package name, builder
symbol and registration hook. Polars and PostgreSQL already have those facts in
the shipped examples, but there is no supported way to ask the project tool for
them by name.

The existing format-1 project manifest expresses each native dependency as an
explicit path/package/builder/hook table. The wrapper generator translates those
tables into 0051 registration calls; 0061 locks their concrete inputs and shares
the resulting build. Keep that representation. A catalogue is an authoring aid,
not another dependency resolver consulted on every launch.

The selected small surface is a listing command and an explicit manifest-editing
command. It does not yet replace a session, discover a runtime installation,
fetch an adapter, or create a scratch project. Those decisions belong to the
subsequent :dep workflow. This record is useful on its own: an existing project
can add Polars without copying the adapter's four implementation fields.

## Decisions

### 1. A shipped table, with two exact names

Keep a private catalogue in the independent project-tool package. Its initial
entries are:

| Name and Rune namespace | Location below runtime root | Cargo package | Builder | Hook |
| --- | --- | --- | --- | --- |
| polars | adapters/polars | rnx-polars | build | plain |
| postgres | adapters/postgres | rnx-postgres | build | lifecycle |

Names are exact, lowercase and case-sensitive. No synonyms, fuzzy selection,
arbitrary URLs, registry fallback or implicit version selection. Unknown names
refuse and list the available names. The catalogue records adapter assembly
facts, not a second copy of driver versions or licences. The adapter manifests,
Cargo lock and existing notices remain authoritative for those.

`rnx-project adapters` lists the entries deterministically, with namespace,
package, hook and runtime-relative path. The README shows its output once so
both names are discoverable. It reads no project, invokes no Cargo,
creates no cache and runs no builder. Listing a known entry does not claim its
source is installed or that its builder will compile. Reject extra arguments.

The catalogue is code shipped with the tool; there is no user catalogue format,
plugin directory, environment override or public Rust API in this record.
Custom adapters continue to use explicit native tables.

### 2. Resolve against the project's declared runtime

Add `rnx-project add --manifest app/rnx.toml polars`, with one or more distinct
catalogue names and the manifest option permitted before or after them. Reject
duplicate/unknown options, duplicate names, empty selection and more than 32
names before touching the manifest. No --force, --offline or --verify on add.

Require an existing valid application manifest with a runtime path. Source-only
manifests and executable overrides refuse: an opaque prebuilt binary cannot gain
an extension through a declaration. Do not infer a runtime from the tool's own
location, its compile-time source path, PATH, a registry or a current session.

Resolve the runtime from the manifest directory, canonicalize it, and locate the
catalogued adapter below that root. Check bounded regular Cargo manifests for
the expected runtime and adapter package names and the adapter's direct path
dependency on that same canonical runtime. This is a check of the two shipped
layouts, not a general Cargo manifest resolver: inherited/unsupported forms
refuse with a named explanation rather than guessing. Missing adapter sources
refuse naming the expected location; nothing is downloaded or installed.

These are structural checks only. No Rust symbol inspection, compilation,
source-map handshake or eval is performed. Builder signatures and the pinned Rune
compatibility are checked by the ordinary build. Actual extension startup remains
unproven until an execution; a later session transition needs its own bounded
eval probe and must also account for session settings.

### 3. Materialize ordinary native tables, without new lock semantics

Add each selected name as its matching native namespace. Preserve the declared
runtime path's form: a relative runtime produces a relative adapter path, and an
absolute runtime produces an absolute adapter path. Append the catalogue's
runtime-relative suffix to the declared runtime path rather than replacing that
spelling with its canonical location. Do not simplify parent components across
symlinks. Resolve and check that the written path reaches the validated adapter;
canonical paths remain the basis for structural checks and assembly identity,
not a reason to put machine-specific paths into a relative manifest.

For a runtime declared as `../../../rnx`, write the ordinary format-1 table:

```toml
[native.polars]
path = "../../../rnx/adapters/polars"
package = "rnx-polars"
builder = "build"
hook = "plain"
```

This keeps a relative source layout portable when moved or cloned together.
Existing locks/cache identities still contain canonical locations: relocation
requires explicit relock/build under 0061. An absolute runtime declaration
already chooses a machine-specific location; add preserves that choice.

There is no `catalogue = "polars"` selector left in the manifest or lock. Once
written, the declaration means exactly what a manually written equivalent means.
Changing or removing a catalogue entry in a future tool cannot reinterpret an
existing project's lock, wrapper, assembly key or receipt. Add no manifest, lock,
receipt, generator or assembly-identity format version for this convenience.

If the namespace already has the same canonical path and identical package,
builder and hook, report it as already present and leave its bytes alone. A
different existing declaration refuses; never replace, rename or merge it.
Resolve and validate every requested entry before editing any of them. Thus a
multi-name request with one collision makes no partial addition.

Preserve all existing manifest bytes, comments, order and formatting. Append
only absent tables after the existing content, sorted among the additions,
without reordering existing tables. Use correctly serialized TOML strings and a
separating newline. Parse and validate the complete candidate
with the existing parser before publication. Existing inline-table or dotted-key
layouts can make an append illegal: refuse that layout with guidance to use an
explicit table, rather than reformatting the file or silently changing meaning.
The 1 MiB manifest bound applies to the candidate as well as the original; the
existing declaration count, identifier and reserved-name checks remain.

### 4. One manifest write, no hidden lock or build

Use the existing project advisory command lock so add cannot overlap another
project command. A busy project refuses as today. The declared manifest resolves
to its canonical file as with existing commands; a symlink spelling of that
file is not replaced by a new unrelated manifest at the link path.

Read through a bounded regular-file path and retain the original bytes and file
identity. Before publishing, recheck both; refuse an intervening edit or file
replacement. This coordinates tool writers, not arbitrary editors. There is no
claim of an atomic compare-and-swap against an editor racing the final rename.

Write the validated candidate to a private temporary regular file under .rnx,
preserve the manifest's ordinary permission bits, sync it, rename it onto the
canonical manifest, then sync the parent directory. Do not reuse a symlink or
special file as the temporary destination. Remove an unpublished temporary on
ordinary failure. Failure before rename leaves the original intact. An interrupt
or error after rename may leave the complete new manifest; report that boundary
honestly, including uncertain durability after a directory-sync error. Never
leave a truncated manifest. The no-op path does not rewrite or touch it.

Do not alter the entry, sources, either public lockfile, receipt, source map or
shared entry. The only auxiliary changes allowed are the existing .rnx command
lock/ignore file and bounded temporary publication state. An actual addition
leaves the project needing explicit lock/build; old launch must refuse stale
declarations rather than quietly using a binary without the requested adapter.

Successful output names additions/no-ops and prints the concrete next commands:

```sh
rnx-project add --manifest app/rnx.toml polars
rnx-project lock --manifest app/rnx.toml --offline
rnx-project build --manifest app/rnx.toml --offline
rnx-project session --manifest app/rnx.toml
```

The README explains that --offline is appropriate only with the required Cargo
sources already available. Add itself never performs network or build work.
Adding a declaration does not modify or transfer an already running session's
bindings. The user launches a newly assembled session explicitly.

## Gates and stop points

1. Prototype the authoring path before product mutation. Through the real parser
   and generator, resolve both shipped entries and compare resulting native
   declarations, wrapper and assembly identity with explicitly written controls.
   Include relative and absolute runtime paths, alternate relative spellings,
   symlink/parent components without unsafe lexical simplification, a relocated
   relative layout, paths requiring TOML escaping, comments, missing final
   newline, unsorted existing native tables and append-incompatible
   inline/dotted forms. Stop if compatibility requires a manifest-format change,
   a general Cargo resolver or rewriting unrelated user text.
2. Listing and refusal matrix: deterministic names, no filesystem writes or tool
   processes for listing; unknown/duplicate/oversized requests; missing/mismatched
   runtime and adapter metadata; source/override projects; namespace conflicts;
   regular-file and size checks. Trap Cargo/compiler/network-capable helper
   execution, with positive controls. No adapter builder is called by add.
3. Publication: multi-name all-or-nothing validation, repeated add as a byte/inode/
   mtime-preserving no-op, preservation of comments and permissions, concurrent
   project lock, an editor changing bytes or replacing the manifest before the
   recheck, malformed/special temporary files, injected failures before/after
   rename, and signals around publication. Existing lock pair/receipt/artifact
   bytes remain unchanged. State the post-rename outcome, not an impossible
   universal rollback claim.
4. Real Polars journey: a project adds polars by name, explicit lock/build produces
   a working session, then retains a frame across a catchable error and previews
   the result. Match the generated table bytes to a hand-written control for the
   same runtime spelling. A second project with the equivalent manually written
   declaration has the same shared identity and attaches with compilation trapped. Show that
   an older built project refuses launch after add until explicit refresh. Do not
   claim a catalogue name dynamically loads into a running process.
5. Real PostgreSQL declaration: add postgres chooses the lifecycle hook; assemble
   and execute a parameterized query on a private throwaway cluster, then verify
   cleanup. Test polars plus postgres in one manifest at least through wrapper
   generation/registration; label whether that combined artifact was executed.
6. Tool suites, formatting, strict Clippy and notices; existing lock/build/run/
   session/eval and shared-cache fixtures. Root source, public API, default graph,
   adapters, kernel and server stay unchanged. No new dependency without review.
   Linux mutation execution only; Windows type-check and command availability
   are recorded separately. Preserve 0061's launch checks and timing contract;
   listing/add must not enter the launch path or change assembly identity for an
   equivalent existing explicit manifest.

## Risks and later work

The source checkout is still required. This table makes installed adapters easy
to name; it is not source distribution, installation, dependency solving or a
promise that every rnx checkout supports the same adapters. A future catalogue
update is ordinary reviewed tool code, while existing declarations remain fixed.

The next record can use these names to prepare a project and build/attach before
replacing a session. It must choose project/scratch ownership, compatibility and
startup deadlines, and make binding loss explicit. Successful build or version
handshake does not establish successful startup. Eviction/live-entry tracking,
native relocation sharing and Windows execution remain separate work.

Keep meaningful measured checkpoints; fold incidental review edits before push.
The next review point is the gates 2–6 implementation and evidence as one batch.
