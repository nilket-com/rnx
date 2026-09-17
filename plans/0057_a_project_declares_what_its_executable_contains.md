# rnx 0057: a project declares what its executable contains

Status: proposed 2026-09-16, for review before implementation. Step five of
extensibility, first local-package contract. The accepted source/native prototype
is rnx-bench `1da057c`, with evidence in `results/package-boundary/` and review
in `reviews/0057_package_prototype_review_claude.md`. The prototype is pushed.
This draft makes the decisions it exposed; it does not call the prototype a
package implementation. Context reuse is outside this record.

## Context

0050 supplies a bounded multi-file loader. 0051 supplies executable assembly,
0053 operation ownership, and 0056 a distinct embedding entry for servers. The
PostgreSQL adapter is a real native dependency. Cargo already resolves its Rust
graph; rnx needs declarations that produce the wrapper, not another Rust solver.

The nine-case source probe maps a module's first item component to an external
directory and resolves remaining components below it. Nested and inline modules
work, an alias can change without changing package sources, and faults retain
their physical source. But `crate::` still means the consumer. Referencing a
name without declaring its module never invokes the loader. A nested dependency
map was not implemented by that probe and is an implementation gate here.

The native probe generates identical Cargo.toml and main.rs from reordered
inputs, then runs both builder kinds through run, eval, a session and a notebook
restart. Changing a local adapter changes the binary without changing Cargo.lock.
Generated source hashes and Cargo.lock alone cannot identify path dependency
contents. The existing rnx-pg also works as a hash-checked executable override.

Two qualifications to the review recommendations matter. `super::` is ordinary
Rune ancestry: at a package root it reaches the consumer's enclosing module,
not that package. And the adapter notices script hashes license texts, not a
tracked source tree. Source fingerprinting is new work specified below.

## Decisions

### 1. Explicit local projects, with a separate build tool

Add `tools/project/` as an independent workspace and lockfile, producing
`rnx-project`. Its TOML parser, hashing and Cargo orchestration do not enter
stock rnx's default graph. It has its own notices and packaging checks.
The root change is an opt-in `project-sources` feature for the bounded loader
and a versioned CLI handoff described in decision 6. No default discovery,
background build, environment search path or home configuration is added.

First scope: local Rune source packages and local Rust adapter crates, assembled
into an rnx-compatible CLI/worker executable. Rust registry dependencies inside
those adapters remain Cargo's responsibility. No Rune registry, Git downloader,
version solver, publish command, automatic dependency installation, or server
route declaration language. Source packages cannot themselves declare native
extensions in this version; the application declares all native registrations.

The commands are explicit:

    rnx-project lock --manifest app/rnx.toml
    rnx-project build --manifest app/rnx.toml
    rnx-project run --manifest app/rnx.toml -- argument1 argument2

`lock` is the only command allowed to create or update the project lock or
resolve a changed Cargo graph. `build` requires an existing matching lock and
uses Cargo's locked mode. `run` requires a verified build receipt; it neither
builds nor repairs a stale lock. Failures name the mismatch and the command to
run. `--offline` on lock/build is passed to Cargo; dependency fetching is otherwise
explicitly part of those commands. Run needs neither network nor Cargo.

No upward search: --manifest is required. Paths are relative to the manifest
that declares them, not the caller's working directory. Script arguments after
`--` arrive untouched. Run may accept explicit --budget, --debug-source and
--color before `--`, forwarding the existing options to their existing positions.
Build chatter goes to stderr; run stdout is solely the selected executable's.
Exit and signal forwarding must preserve the child's status and not orphan it.

### 2. A declaration describes assembly, with an executable escape hatch

Proposed version-one application syntax:

```toml
format = 1
[application]
entry = "main.rn"
[runtime]
path = "../rnx"
[sources.words]
path = "../words"
[native.postgres]
path = "../rnx/adapters/postgres"
package = "rnx-postgres"
builder = "build"
hook = "lifecycle"
```

`runtime.path` names the local rnx Cargo package, whose exact Rune pin is inherited.
A native table key is its registered Rune crate name, not its Cargo package name.
`hook` is `plain` or `lifecycle`, selecting with or with_lifecycle. Builder is a
Rust identifier path relative to the generated Cargo dependency alias; validate
its components, never interpolate arbitrary Rust expressions. A builder retains
0051's trusted-adapter obligations, including correctly prefixed help. Changing
its declared name does not rewrite an adapter's internal help strings.

A source package's rnx.toml instead has `[source] root = "."` and optional
`sources` entries. Its entry is root/mod.rn. There is no application, runtime,
native table or executable in a source manifest. Every manifest path and
source root is recorded; an alias edge points to a package directory containing
that manifest, whose root is resolved relative to it. No package's own display name
participates in lookup. The importing edge's alias is its name at that location.

Alternatively replace `[runtime]` and all `[native.*]` with:

```toml
[executable]
path = "../build/rnx-pg"
```

The override must implement the rnx CLI. It is not inspected to infer extensions,
is not rebuilt and cannot be combined with native declarations. Its hash records
identity, not a description of its contents. A server executable with a different
CLI is not silently treated as rnx. The 0056 server is a future manifest consumer;
this first tool does not generate its routes, pools or application factory.

Unknown fields, conflicting forms, missing required fields, duplicate keys,
invalid identifiers and duplicate/reserved native names refuse before generation.
Top-level source aliases must not collide with native names or batteries. Source
aliases at every level must be valid Rune module identifiers. Inline modules
still behave as Rune defines them; a mapped alias supplies a file only when a
file-module declaration reaches the loader. Unused source entries are allowed.

### 3. Package ownership follows explicit item-prefix mounts

The application declares `mod words;`. Its manifest maps `words` to the source
package root; `words::nested` resolves root/nested/mod.rn before root/nested.rn.
Each source dependency edge mounts its target under its parent's logical prefix.
If words declares a dependency `codec`, its mount is `words::codec`. That package
must still contain `mod codec;` at its root to load it. A declaration inside
words::nested has path words::nested::codec and is an ordinary local module,
not another spelling of words' dependency. Use self/super paths to refer to the
root-declared module. No imports or declarations are synthesized.

Expand these mounts recursively before compilation. Candidate lookup uses the
longest component-wise mounted prefix, then appends the unmatched components to
that mount's root. At a mount itself, use root/mod.rn. Local modules below an
unmapped prefix retain 0050's rule. A declared mount wins over a same-spelled
local candidate, without a fallback if its target is missing. Explain this in
the README and gate it, so adding a local file cannot silently replace a package.

These remain modules of one Rune crate. `crate::` means the application; `self`
means the current module; `super` means its parent, including crossing a package
boundary at its root. This is not namespace isolation. A consumer normally uses
its declared alias but may access public descendants under Rune's visibility
rules. There is no opaque package boundary or rewriting of absolute references.

A diamond is mounted twice, for example left::shared and right::shared, even if
both edges name one physical directory. These are distinct Rune item/type paths,
not one unified package instance. The two copies are charged separately when
read by the compiler. Detect cycles on the current ancestry of canonical manifest
paths; sharing outside that ancestry is allowed. There is no version selection.
The graph is bounded to 64 distinct manifests, 256 expanded mounts and depth 16;
exceeding a bound names the importing edge. Canonicalization is for graph identity,
not a claim that two aliases have the same Rune type identity.

### 4. Lock inputs, generated assembly and local contents separately

Write versioned JSON `rnx.lock` beside the application manifest. It records the
normalized declarations and source edges, package identities and file inventories,
SHA-256 tree digests, generated Cargo.toml/main.rs hashes, exact generated
Cargo.lock bytes' hash, selected target, profile and features, and compiler/Cargo
version output. Store generated Cargo.lock beside it as `rnx.Cargo.lock`.
Wrapper-level hashes belong once to the executable assembly, not redundantly to
every native package. All reachable local Cargo path packages, including rnx and
transitive path dependencies, are inventoried using Cargo metadata's resolution.
Registry/git package identity remains Cargo.lock's, with its existing checksums
and revisions. This tool does not invent replacement identities for that graph.

For native path packages, enumerate Git-tracked files beneath the package root
and hash their current working-tree bytes, not HEAD's blobs: dirty edits count.
Refuse a missing tracked file, submodule, symlink, special file, non-Unicode name,
or an untracked non-ignored file in that root. This first native path form requires
a Git working tree; an export without Git is a later input form. Include file
paths and executable mode. Hash shared roots once but retain every Cargo package
association. Ancestor workspace manifests, Cargo configuration and toolchain files
that govern a path package must be recorded too; an unaccounted ancestor input is
a stop condition, not an ignored dependency. Record selected Cargo features.

For Rune package roots and the application's entry directory, recursively hash
all regular files, including their manifest when inside the root. Exclude only
`.git` directories and the tool's own exact output names at the application root:
`.rnx`, rnx.lock and rnx.Cargo.lock. Refuse symlinks, special files and non-Unicode
names in these project trees. Plain 0050 file runs continue following symlinks;
this restriction is the explicitly opted-in project identity contract. Source
manifests outside their source root are hashed separately.

Tree encoding is versioned: sorted UTF-8 relative paths with `/` separators,
a big-endian u64 path-byte length, path bytes, one executable-mode byte,
a big-endian u64 content length, then exactly that many file bytes, under the
domain prefix `rnx-tree-v1\0`. A size change during a read refuses the snapshot.
On Unix the mode byte says whether any executable bit is set; elsewhere it is
zero, with platform recorded in the lock. No mtime identity.
Bound traversal before growth: 100,000 entries and 512 MiB read across all
fingerprinted roots, 1 MiB per manifest and 16 MiB per lock or handoff file.
Streaming hashes have bounded buffers. These are fingerprinting allowances;
compilation still has the separate, unchanged 8 MiB source allowance.

This is input identity and stale-build detection, not hermetic execution. Trusted
Rust build scripts, proc macros and adapters may read ignored files, external
paths, the environment or network; Cargo itself may consult machine configuration.
Record effective build flags and target and refuse unsupported overrides rather
than claiming they were hashed. Pre/post checks detect ordinary edits but cannot
prove an atomic snapshot against a concurrent editor. Do not advertise bitwise
reproducible artifacts from these hashes alone. No lock authenticates its author.

### 5. Generate deterministically; never make run rebuild

Sort native registration names, assign stable Cargo dependency aliases and
emit the small main_with wrapper. Both plain and lifecycle builders are supported.
Enable project-sources on the runtime dependency; retain normal allocation
accounting. The generator does not enable server-runtime or rewrite the shared
Rune pin. Emit into private staging below the application .rnx directory. Cargo
owns dependency resolution and build failures, including mismatched Rune types.

Lock resolves Cargo only after validating the local source graph and declaration
shape. Write new lock artifacts atomically from staged files after success; a
failure must not leave a new project lock paired with an old Cargo.lock. Existing
output is retained until both are ready, with readers rejecting mismatched pairs.
Build copies the exact locked Cargo file and uses --locked, recording provenance,
features, generated inputs and the final executable hash in a separate receipt.
Cargo output is not a claimed successful build until the expected artifact and
all post-build identities match. Failed or interrupted builds publish no receipt.

Run checks the lock, local input digests, receipt and binary hash before launch.
An override needs its locked executable hash instead of a generated build receipt.
For the override, lock needs no Cargo and records no invented generated inputs.
Its replacement is an explicit lock update. Hash checking followed by execution
is not atomic against a hostile replacement; these are trusted local projects,
not a sandbox or code-signing mechanism. No automatic retry of the program.

### 6. An explicit, bounded handoff to the existing runner

Under project-sources only, add an early `project-source-version` command that
returns protocol version 1 without building a context or invoking an extension.
Add `run --source-map PATH ENTRY ...` before the entry path. The generated tool
writes a version-one JSON mount table containing the exact entry and expanded
logical prefixes with absolute roots, then passes that option. This file is an
explicit loader input, not an environment variable or a manifest search path.
A malformed, oversized, duplicate or inconsistent mount table refuses before
compilation; the entry must match the table. Treat paths as paths, never shell
text. There is no ambient map inherited by another context.

The project tool owns lock verification; the runner validates handoff structure
and bounds, not Cargo identities. A direct caller supplying a source map is making
an explicit filesystem selection, just as with an entry path. The map is not an
authority token. No map or new feature means precisely today's loader behavior.
All mapped reads go through 0050's same Loader file/read path, shared allowance,
source snapshots, UTF-8 checks and source-accurate diagnostics. No probe-style
Source::from_path shortcut. Missing mapped modules name the physical candidate
and declaring source; no fallback to another package or working directory.
Handoff fields are `format` (1), `entry` (absolute path), and `mounts` (an array
of objects with `prefix`, an array of identifier components, and `root`, an
absolute path). Refuse unknown fields and non-absolute paths. The table carries
no code, budget override or executable selection.

An override with source dependencies must answer the bounded version handshake;
unknown or incompatible support refuses naming the executable. An older rnx-pg
with no mapped sources remains usable through its ordinary run command. Bound
the handshake to one second and 4 KiB stdout with stderr capped, retire its child
on failure, and do not execute any script while checking support. The generated
executable itself remains usable for native eval/session/worker operation, but
those entry points receive no source map and gain no module loading. Config stays
file-free. Program::compile and the server package also retain their existing
entry-root behavior; mapped server compilation needs its own public API decision.

## Gates, in order

1. Extend the accepted source cases through the real bounded loader. Add a
   transitive chain and diamond, colliding local candidate, missing dependency,
   cycles and graph limits, alias rename, root-level super, crate/self behavior,
   explicit declaration absence and an inline declaration. Demonstrate longest
   prefix selection and distinct types for two aliases of one source. Runtime
   and compile faults, method recovery and returned field names use the right
   loaded source. Prove the 8 MiB allowance spans all copies, with sticky refusal.
2. Round-trip and reject manifests, locks and handoffs. Gate every size/graph
   boundary and malformed field, alias and builder; reordered declarations produce
   byte-identical generated files. No Rust-expression injection through builder
   text. Wrong feature/version and unsupported override fail before script entry.
3. Fingerprint edits, additions, deletions, mode changes, dirty tracked files,
   ignored/native external-input qualifications, untracked native files, symlinks,
   special files, non-Unicode names and shared Cargo path dependencies. A change
   outside the directly declared adapter but inside a transitive path package
   invalidates the build. Reproduce the accepted unchanged-Cargo.lock mutation.
   Audit ancestor Cargo inputs; stop for review if the stated inventory cannot
   be derived without misrepresenting what it covers.
4. Assemble the real PostgreSQL adapter plus a plain fixture from declarations.
   On a private cluster run a parameterized query in an application that also
   imports a mapped Rune dependency. Test script arguments, statuses, Ctrl-C and
   absence of children after interruption. Reuse the native session and notebook
   restart gates without granting either source loading. Test the existing rnx-pg
   override, tamper refusal and a source-capable override. Never touch user data.
5. Exercise lock/build/run failures and interruptions, stale/tampered receipts,
   concurrent edits detected by post-checks, no Cargo invocation during run, and
   no receipt after a failed build. Copy the project into an equivalent directory
   layout and state which identities stay stable; do not turn that into a binary
   reproducibility claim. Native Cargo errors retain the relevant package names.
6. Root suites in existing configurations plus project-sources, tool tests,
   formatting, clippy, notices and packaged-manifest checks. Existing plain run
   diagnostics and non-project CLI outputs remain byte-identical. Default graph
   contains no tool dependencies. Matched stock startup versus 07f6709; separately
   measure first build, warm build and project run hashing overhead. Report
   Windows type checks separately from execution. No root regression is excused
   as an expected cost of opting into projects.

## Guardrails, risks and forward

Do not implement a registry, dependency version unification, source rewriting,
shared handler context cache, dynamic Rust ABI or new VM ownership model. Native
adapters retain their trust and lifecycle contracts. A diamond duplicates types
as well as code; make this visible before suggesting it solves version conflicts.

Review the plan before code. If mount integration cannot reuse the bounded loader,
if input inventory overclaims build reproducibility, or if the explicit handoff
leaks into a session/config context, stop and revise the record. The accepted
probe proves feasibility of the pieces, not their product integration.

The first local workflow intentionally leaves mapped server compilation, native
registry declarations, source export identity without Git for Rust packages,
package publication and framework ergonomics for later consumers. It supplies a
manifest and lock grounded in the adapter, without declaring all packaging done.
