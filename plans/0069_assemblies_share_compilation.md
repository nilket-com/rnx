# rnx 0069: assemblies share compilation

Status: gate 4 accepted on nano 2026-09-20; closes on Linux when the user's
slim journey confirms the second wait. Gate 4 measured a second assembly over
the same natives at 5 s against 115 s, Polars + PostgreSQL at 40 s against
116 s, no startup change, and half the storage; see
[the costs and regression evidence](0069_costs_and_regression_evidence.md).
Originally: accepted after three drafts. Gate 1 accepted, see
[the wrapper identity evidence](0069_wrapper_identity_evidence.md) (note its
numbering correction: generator 4, Git identity format 4). Gate 2 ported it
and added the shared directory; see
[the shared directory evidence](0069_shared_directory_evidence.md). Gate 3
ran the lifetime sequences and wrote the catalogue declarations; see
[the lifetime evidence](0069_lifetime_evidence.md). Gate 4's nano evidence is
accepted; the slim journey closes the record.
Originally: second draft after review. Built on the accepted probe, [the shared
compilation probe evidence](0069_shared_compilation_probe_evidence.md)
(rnx `4799d52`, bench `8997902`). 0068 is closed on Linux. The first draft
claimed retained-output preservation by construction of source identity; the
review reproduced a same-revision rewrite through a build script's declared
environment input. The second draft made an executable scan the contract;
the review reproduced a reader that finds its retained file through runtime
configuration, so both executables pass the scan and the older one still
changes. This third draft makes sharing an explicit declared contract
(decision 3), decided at lock time so the identity never changes after a
build (decision 5); the scan remains as a refusal where a declaration is
detectably false. The locking and removal design stands from the second
draft.

## Problem and user journey

On slim the user installed rnx, ran `:dep polars` and waited about 100
seconds while 304 crates compiled — 152 of them by name the same crates that
had just been compiled for the launcher. The same wait returns whenever an
assembly changes in any way: adding PostgreSQL beside Polars, or relocking
after a declaration gains `presentation = true` (0068 gate 1 measured 116 s
with a warm cache in the same directory). Today every assembly compiles the
whole world into its own Cargo target directory, and the cache shares only
finished executables.

The probe established what can and cannot be shared, with the tool's exact
build command and Cargo's stable `build.build-dir`:

- A second wrapper over the same natives: 111 s today; 0.6 s and one
  compiled unit with a shared directory and a discriminated wrapper name.
- Adding PostgreSQL to a Polars assembly: 111 s today; 37–38 s shared.
- The launcher's own build output: not worth retaining (≤1 s of a Polars
  build; its crates are the small ones).
- Two hazards. Without a distinct wrapper identity, Cargo served a *wrong
  executable* for a different wrapper (mtime freshness on a workspace
  package with the same name, version and dependencies). And a later build
  can rewrite a retained build-script output (`OUT_DIR`) that an existing
  executable reads at runtime, changing that executable's behaviour without
  touching it; deleting the directory breaks such executables outright.

The intended result: after the first Polars build on a machine, the next
assembly over the same natives is ready in about a second, a new native's
graph in tens of seconds, and no existing executable changes behaviour or
breaks because another assembly was built or removed.

## Current boundaries, checked in source

The generated wrapper is always `rnx-project-app` (`generate.rs:86`); the
assembly identity (format 2, generator 2, `cache_identity.rs`) hashes the
context, the generated manifest and `main`, the lock digest and the native
inventory, and the executable is read from `<entry>/target/release/` after
`cargo build --locked --release --target-dir <entry>/target`
(`cache_entry.rs:318`). Resolution starts from the project's previous
`rnx.Cargo.lock` when one exists (`git_sources.rs:331`) and from nothing
otherwise. The tool refuses every user `CARGO_*` and `RUST*` override except
`CARGO_HOME`, `RUSTUP_HOME` and `RUSTUP_TOOLCHAIN` (`cache_entry.rs:169`,
`workflow.rs:596`), and 0067 admits only `target.<triple>.{linker,
rustflags}` from Cargo configuration. 0061 made the build directory an
identity input because build scripts can observe it and executables can read
retained output from it, and kept every entry's directory in place for that
reason. 0066 removes whole entries and nothing else.

Cargo's `build.build-dir` separates intermediate build output from final
artifacts and is stable since 1.91 (slim 1.95, nano 1.98). Cargo documents
that a build script may re-run and reuse the same `OUT_DIR`: when its
package changes (a path package's content, l4 in the probe), and also for a
version- or revision-addressed package whenever a declared external input
changes (`rerun-if-env-changed`, `rerun-if-changed` on a system file), which
the review reproduced at one pinned Git revision. No source identity makes
retained output immutable. 0061 decision 8 explicitly supports executables
that read such retained output at runtime, and its fixture does; that
support is kept. The manifest readers admit path and Git declarations only;
registry packages are transitive dependencies here.

## Decisions

### 1. One shared build directory per cache root and build context

The tool owns a build directory under the cache root, `<root>/build/<key>`,
created with the entry directories' ownership and permission policy. The key
is the full BLAKE3 of a canonical preimage with a domain tag
(`rnx-build-key-1`): the `rustc -vV` and `cargo --version` strings, host and
target triples, profile, the sorted feature set, the canonical cache root and
Cargo home, and the admitted Cargo configuration values (0067's
`target.<triple>.linker` and `rustflags`). Everything else — package
identities, features per unit, dependency graphs — is left to Cargo's own
fingerprints inside the directory, which is what they are for. A different
toolchain, target, profile, feature set or admitted configuration is a
different directory.

One eligibility rule, applied at lock time and nowhere else: an assembly
builds in the shared directory when its runtime is a Git-source declaration,
every native is a Git-source declaration, and every native carries
`shared_build = true` (decision 3) — the stock `:dep` world, where the
catalogue writes it. A path native with `shared_build = true` makes the
assembly private (the declaration is accepted and recorded, and the path
disqualifies, as it would without it); an assembly with a Git runtime and no
natives is shared, since only the runtime's graph is in it. It builds with `--config build.build-dir=<that directory>` while keeping its
own `--target-dir` for the final artifact, publication and receipts exactly
as today. The user's `CARGO_*` overrides stay refused: the setting is passed
by the tool on its own command line, never read from the environment or
from user configuration, and the 0067 configuration audit is unchanged.
Every other assembly — a path runtime, any path native, any native without
the declaration — keeps a private directory as today; a later record may
offer sharing for path sources with an explicit rule.

### 2. A production discriminator for the wrapper package

The wrapper package is named `rnx-app-<digest>` where the digest is the
full BLAKE3 (64 hex) of a framed preimage with a domain tag
(`rnx-wrapper-name-1`, then the length-prefixed canonical generated manifest
carrying a fixed placeholder name, then the length-prefixed generated
`main`) — everything Cargo's freshness could otherwise miss, computed before
the name exists, so the assembly key (which hashes the named manifest) is
not an input to it. Equal wrapper
content yields the same name, which is correct: it is the same unit. The
executable is named the same way inside the target directory; the published
artifact name and the receipt are unchanged. This is a generator change:
generator 3. Existing entries built by generator 2 remain valid for the
locks that name them; relocking produces a new key and a new entry, once.
The probe's 16-hex digest is not adopted.

### 3. Sharing is a declared contract, refused where a declaration is detectably false

The shared directory is mutable: Cargo may rewrite an `OUT_DIR` in it
whenever a build script re-runs, and no scan of an executable can find every
reader of retained output — a reader can learn the path at runtime from
configuration. So sharing is a contract the adapter author makes for their
graph, in the declaration: `shared_build = true` states that nothing in the
native's dependency graph reads retained build output at runtime, so its
executables neither change when the shared directory is rebuilt into nor
break when it is removed. Absent or false means private, which is today.
The declaration is optional and serialized only when true, so existing
declarations, locks and envelopes keep their bytes; old readers refuse the
field by name as they refuse `presentation` (0068). The runtime's own
dependency graph is vouched for by the record, not by a declaration: gate 3
shows that a runtime-only shared executable holds no reference and survives
the rewrite and removal sequence, and that evidence is what makes the
zero-native case eligible; a runtime revision that changed that would need
its own record. The catalogue writes the declaration for `polars` and for
`postgres` only after gate 3 has shown, separately, that a Polars
executable, a PostgreSQL executable and the combined executable hold no
reference to the shared directory on this toolchain and survive the same
sequence; it leaves existing declarations alone.

Trust follows the existing line: a build script is trusted Rust, and so is
this statement about the graph it builds. The tool enforces what it can
detect: after a shared build, before publication, it scans the executable
for the shared directory's canonical path, and if the path occurs it
*refuses* — the assembly is not published, the ready document is not
written, and the message names the natives whose declarations must be
dropped. A refusal is not a fallback: nothing about the assembly's identity
changes, and the next lock without the declaration is a different, private
assembly. What the scan cannot see is, by definition of the declaration, the
author's breach, and the record says so. 0061 decision 8 is kept for every
assembly without the declaration: readers of retained output work from a
private directory with the 0061 guarantees, declaring nothing; what they
never get is sharing.

### 4. Resolution starts from the runtime's lock

When a project has no previous `rnx.Cargo.lock`, resolution starts from the
selected runtime's `Cargo.lock` (the Git checkout's, or the path checkout's)
as Cargo's preference for shared crates, so assemblies under one root and
runtime revision resolve alike and share units. The seed is a preference,
not a constraint: natives that require other versions get them, and a
divergent resolution costs its own compilation, which the tool reports as
today. The lock digest already identifies the result; no new identity input.

### 5. Coordination, removal, identity and scope

Cargo's build lock serializes builds inside the directory; it does not
coordinate the tool renaming or deleting the directory. The tool owns a
coordination lock at a stable path outside the removable directory,
`<root>/build/<key>.lock`. A builder holds it shared from before its Cargo
invocation until its executable is published or publication is refused;
a waiter holds nothing on it (it waits on the entry, as in 0061).
Lock order is project lock, entry lock, build-directory lock, then Cargo.
Removal holds it exclusively and refuses while any holder exists, as 0066's
busy-writer refusal does; `--quiescent` remains the user's acknowledgement
for consumers the tool cannot see, not a substitute for that refusal.

Removal of the shared directory carries 0066 forward: guarded preflight
under the cache root with mount and symlink refusals, `--dry-run` with
pending inspection, rename to `<key>.removing-<nonce>` before any deletion,
the sync and error boundary, and `--resume` that continues only renamed
directories. A rebuild at the same key after an interrupted removal creates a
fresh `<key>`, which resume never touches. `rnx cache list` shows the
directory with its key, size and the number of *local entries whose ready
document names it* — a count of what the tool recorded, not a discovery of
every consumer. Removing an entry never touches the shared directory.

The build kind — `shared` with the directory key, or `private` — is decided
at lock time from the declarations and recorded in the identity context:
identity format 3. The lock, the assembly key, the ready document and the
receipt therefore describe one outcome, fixed before any build; the only
post-build event is a refusal, which publishes nothing, and an interrupted
build or refusal leaves the entry unpublished as today, to be retried or
relocked. Attachment finds an entry by the key that already names its build
kind. Old format-2, generator-2 locks keep decoding and keep launching their
entries; when such a lock must rebuild a missing entry, the rebuild uses the
generator-2 wrapper name and the private build policy it was recorded with,
so the identity it names is the identity it gets.

Out of scope: retaining `cargo install` output; the repeated preparation of
a scratch project on every stock session (its own record, which needs this
one's identity rules); Windows; sharing for path sources; registry native
declarations.

## Gates

### Gate 1 — the wrapper name and the identity, isolated

Prototype the discriminator, the generator bump, the build key, the
`shared_build` declaration and the identity-context field in isolated
source with a recoverable patch. Vectors: equal wrapper content yields equal
names across projects; a one-call difference yields a different name; the
framing separates manifest from `main` unambiguously; the build key changes
with each named input and with nothing else; `shared_build` omitted, false,
true, of the wrong type, and beside an unknown field, for path and Git
declarations, plain and lifecycle, with old-reader refusal by name; the
build kind derived at lock time is shared only when the runtime is Git and
every native declares; identity documents for generator 2 and 3 and formats
2 and 3 decode, refuse, and canonicalize as stated; an existing generator-2 lock
still launches its entry and rebuilds a missing one with the generator-2
name and a private directory. Rerun the probe's wrong-binary control and its
discriminated series against the prototype's real generated wrappers.

### Gate 2 — the shared directory in the product

Port gate 1. Declared assemblies build into the shared directory under the
coordination lock; every other assembly builds privately; the executable
scan runs on every shared build and refuses publication when it finds the
path, naming the declarations. Prove the cost matrix on the product with the stock binary: first
Polars, second wrapper over the same natives, Polars then PostgreSQL, a
runtime revision bump, and the relock after `presentation = true`; every
executable's behaviour verified through the worker as in the probe; which
shipped graphs trip the scan; two concurrent builders on one directory and a
builder racing a removal (refused); the seed lock's effect on resolution.
The user's `CARGO_*` refusal and the configuration audit unchanged, with
tests.

### Gate 3 — lifetime and removal

The retained-output consumer from 0061 as a Git native without the
declaration: it builds privately, and a later shared build of another
assembly, a build-script re-run triggered by a declared environment input at
the same revision (the review's first counterexample), the l4 build-script
change at a new revision, and removal of the shared directory all leave it
reading its own value. The same native with a false `shared_build = true`:
the scan finds the path and the tool refuses publication, naming it, and no
entry or ready document exists afterwards. The review's second
counterexample — a reader that learns its path from runtime configuration —
with a false declaration: the scan cannot see it, the record's expected
result is that the declaration is the breach, and the gate records the
observed rewrite as the documented consequence, not as a product defect. A
runtime-only executable, a Polars executable, a PostgreSQL executable and
the combined executable from declared shared builds each hold no reference
and run unchanged after the same sequence (a PostgreSQL query against the
0055 fixture cluster, not only a symbol check), which is the evidence the
zero-native rule and both catalogue declarations rest on; either declaration
is withheld if its executable fails this. `rnx cache list` shows the shared
directory with its scoped count; entry removal leaves it; `remove
build-<key>` refuses while a builder holds the lock, refuses without
`--quiescent`, removes with it through rename-then-delete; a removal killed
mid-way followed by a rebuild at the same key leaves a fresh directory that
`--resume` never touches while it finishes the renamed one. The path-source
case keeps its private directory and the 0061 retained-output fixture passes
unchanged.

### Gate 4 — costs, regression and the journey

Interleaved cost samples on the same machine and toolchain for the matrix in
gate 2, before and after; the stock 5% startup gate; the three root
configurations, tool and adapter suites and strict clippy, notices, feature
audits; storage: one shared directory against the private directories it
replaces; documentation of the shared directory, its removal and the
retained-output obligation. The user repeats `:dep polars` then `:dep
postgres` on slim and reports the second wait.
