# rnx 0067: one install provides the dependency workflow

Status: accepted for implementation after the revised draft review, with F1
allowing clean known-revision builds to proceed through consent without prior
acquisition evidence. The accepted Cargo Git source probe is bench `edd1383`.
This replaces the unimplemented payload design in the unpushed plan commit.
Gates 1–3 are accepted. Gate 4 is implemented and ready for review;
gate 5 (costs and regression) remains open. See [gate 1 evidence](0067_command_boundary_and_coordinates_evidence.md),
[gate 2 evidence](0067_management_and_session_protocol_evidence.md),
[gate 3 evidence](0067_git_source_workflow_evidence.md), and
[gate 4 evidence](0067_one_install_journey_evidence.md).
Baseline: rnx `94f5f3f`, rnx-bench `e628c97`; 0066 is closed on Linux.

## Context

The user installed rnx, typed `:dep polars`, and encountered a missing companion
program followed by a requirement to install runtime sources. Folding that second
installation into consent did not answer the objection. Neither does renaming it
materialization. The default dependency workflow must use Cargo's source
acquisition, rather than make the user manage an rnx runtime installation.

The [accepted probe](https://github.com/nilket-com/rnx-bench/blob/edd1383/results/cargo-git-runtime-0067/README.md)
resolves the root and both independently-workspaced adapters at one published Git
revision without changing their manifests. Both adapter `../..` dependencies
resolve to the same Git-sourced rnx package. Actual combined executables run the
Polars pipeline, and second consumers attach with networking disabled and
compilation trapped. Git and developer-path sources coexist under distinct keys.
The fetched repository pack is about 2 MiB; the reviewer's repack was under 2 MiB.
That is a source-fetch size observation, not the size of registry dependencies,
compiled artifacts, or the cost of compiling Polars.

The probe also sets the policy boundary. Reading coordinates costs about 2 ms
and does not verify checkout content. Its compiled raw-blob verifier costs about
26 ms for 448 tracked files / 6.8 MB, against about 11 ms for today's BLAKE3 path
fingerprint. A separate full object fsck costs about 90 ms. Cargo metadata accepts
a same-size restored-mtime checkout edit. Git coordinates identify committed
content, not the current bytes in Cargo's directory. The choice below explicitly
trusts that directory on everyday launches, keeping full checks for build,
attachment and `--verify`.

The intended first-use journey is:

```sh
cargo install --git https://github.com/nilket-com/rnx --rev FULL_REV rnx --locked
rnx
# :dep polars
# y
```

`FULL_REV` is a published, tested full revision carrying this implementation, not
the pre-implementation probe revision. There is one installed user-facing
program. Cargo fetches build dependencies after consent. There is no embedded
source payload, rnx-managed default source copy, or runtime-store selection in
this journey. Rust/Cargo and native build prerequisites remain necessary; Git
is required for the explicit source verification. Cold compilation is still
reported before consent. This is not a prebuilt-adapter or crates.io release.

## Decisions

### 1. One installed command, separate implementation crate

Keep management under `tools/project` as a private implementation crate with a
narrow dispatch entry. It must not import the runner or form a dependency cycle.
The stock binary dispatches management before entering `main_with`; management
must not construct Rune contexts, arm runner signals or load session settings.
Ordinary runner startup must not initialize tool signals, storage, Git or Cargo.

An explicit stock-management feature is enabled for the ordinary installation.
Generated applications disable defaults and explicitly retain allocation
accounting, `project-sources` and their other required features. Audit shipped
adapter dependency edges and binaries and the server for feature unification:
none may accidentally bring management back into an assembled application.
Preserve the public embedding API and allocator contract. A runner-only library
consumer must not run coordinate discovery or need Git to compile.

The default stock dependency graph and notices change honestly. Polars and the
PostgreSQL driver remain absent from stock. There is no source archive, extraction
code or compression dependency. The fresh Git installation must find the
internal crate as well as the runner; this is gated rather than inferred from the
adapter-discovery probe. Existing publication/licensing guards remain in force.

### 2. Command names and compatibility entrypoint

```text
rnx project adapters
rnx project add --manifest FILE NAME...
rnx project lock|build|run|session|eval --manifest FILE ...
rnx runtime install --from PATH
rnx runtime show|select|list|remove ...
rnx cache list|remove ...
```

Bare invocation, repl/session, eval, run, worker, help, version and selfcheck keep
their meanings. File arguments remain after `run`; test files named `project`,
`runtime` and `cache` without treating them as management commands. Project modes
keep 0060's flag positions, settings, cwd, source-map and exec contracts. All
recovery/reopen commands render an absolute shell-quoted executable and its
argument prefix, including absent PATH and paths containing quotes or spaces.

Keep a separately built `rnx-project` compatibility entrypoint for bench fixtures
and old scripts, not as another installed requirement. It uses the same internal
implementation and old argument grammar. Test private descriptor inheritance,
association routing, process groups, signals and exit status. A shell forwarding
example is not protocol evidence. Old peers/capsules either interoperate or
refuse before commitment; version the boundary if compatibility cannot be kept.

### 3. Build coordinates have a state, not merely a revision

A management-capable stock build records the configured repository URL, exact
full revision when known, raw working-tree cleanliness, and an acquisition state:
`acquired`, `unverified`, `dirty`, or `unknown`. `acquired` means local evidence
binds the clean source to Cargo acquisition from that URL at that revision. It
is not a promise that the remote will remain reachable. A clean arbitrary local
checkout is `unverified`, not automatically published. Dirty wins over a known
revision; unknown must not borrow a revision from an unrelated enclosing repo.
The version 0.0.0 is not a source identity.

Coordinate discovery anchors the actual rnx package to its source root, compares
raw tree entries and unfiltered object identities, and never trusts status/diff
clean-filter output. Rebuild tracking covers source edits, staged additions and
deletions, executable modes and provenance inputs. If discovery cannot establish
the revision and cleanliness, record unknown rather than inventing coordinates.
Missing acquisition evidence for otherwise clean known coordinates records
unverified. A build-time path can be recorded as
a diagnostic hint, never as an implicit runtime dependency or fallback.

Gate 1 must prove a local acquisition-evidence provider for real, fresh and
cached `cargo install --git` builds, including multiple revisions in one Cargo
cache. A checkout-shaped path, origin string, `.cargo-ok`, clean HEAD or an
unqualified remote-tracking ref alone is insufficient. Bind the configured URL,
Cargo source and available fetched-object evidence. Do not add network calls to
ordinary builds to manufacture the answer. Ambiguous/missing evidence yields
unverified. Acquisition classification informs notice and diagnostic wording; it
does not gate the default workflow. The accepted probe proved revision/dirty
discovery, not this acquisition classification. Never label a merely clean source
acquired to avoid reporting uncertainty.

For default stock `:dep`, only dirty and unknown builds refuse before any
fetch, scratch write or consent. One message names the URL/revision when known
and the state, and prints an exact shell-quoted line:

```sh
export RNX_DEP_RUNTIME='/absolute/path/to/checkout'
```

Use the build-time source directory as a suggestion only when it still exists
and has the supported runtime layout; otherwise label the placeholder explicitly.
Both acquired and unverified clean builds with known URL/full revision proceed
to the describe notice and consent. For unverified sources, the notice says the
revision at that URL is "not yet confirmed reachable". Consent authorizes the
requested Cargo acquisition/build, not a speculative fetch to classify the build.
Do not call an unverified revision "unpublished" without evidence. If acquisition
fails for either state, report it after consent, preserve Cargo's underlying
missing-revision or network diagnostic, keep the old session usable and give the
same override recovery. There is no way to promise future network availability
from a compile-time bit.

Project-owned sessions use their declared runtime; an explicit developer override
also bypasses the default-coordinate requirement. Neither legitimizes an
unidentified Git default. The clean-but-unpushed control reaches consent as
unverified, then refuses at fetch time with Cargo's missing-revision error.
A clean pushed checkout installed through `cargo install --path` must succeed
through the same consented acquisition without an environment-variable override.
Describe and decline perform no fetch in either case.

### 4. Cargo coordinates in declarations and assembly identity

Add a versioned manifest form for native locations, while continuing to read
format-1 path projects. Format 2 permits exactly one of `path` or `git` plus
`rev` for the runtime and each native dependency. Git revisions are full
40-character hexadecimal commit IDs in this first Git/SHA-1 repository scope;
no branch, tag, implicit HEAD, version range or independent version resolver.
Reject partial/mixed/unknown fields. Pure Rune source locations remain paths and
retain their bounded-loader and content-fingerprint rules. Executable overrides
retain their existing mutually exclusive form.

For example, a generated scratch contains ordinary declarations, not an opaque
catalogue name that could later be reinterpreted:

```toml
format = 2
[runtime]
git = "https://github.com/nilket-com/rnx"
rev = "<full 40-character commit ID>"
[native.polars]
git = "https://github.com/nilket-com/rnx"
rev = "<the same commit ID>"
package = "rnx-polars"
builder = "build"
hook = "plain"
```

The example placeholders are explanatory, not valid revisions. The shipped
catalogue uses the declared runtime's URL/revision for shipped Git adapters;
path runtimes retain 0062's path spelling and structural checks. Describe can
validate known names and authoring decisions without fetching; after consent,
Cargo metadata must confirm packages and the runtime dependency edges. Mixed
Git/path declarations are explicit, never created by silently replacing a
project's runtime with the stock manager's source. Arbitrary Cargo graph solving
remains Cargo's job, and the first real Git journey covers the shipped repository.

Only lock resolves. Build is locked. Run/session/eval neither resolve nor build
nor fetch, including under `--verify`. Missing source needed for verification
refuses with an explicit build/recovery command. All Cargo children retain the
bounded I/O, cancellation and process-group ownership of the existing tool.
`--offline` applies to source acquisition too; cached sources work and an empty
Git cache refuses without network. Failed preparation may leave Cargo's fetched
cache or a completed assembly, but keeps the old session before commitment.

The identity retains every 0061 input: exact generated manifest/main and native
registration associations, locked Cargo graph, toolchain observations, target,
profile, features/configuration, Cargo/Rustup homes, cache-owned build location,
canonical native locations and allowed external inputs (including absences).
For Git packages replace content-tree fingerprints with package-associated
URL/revision coordinates and retained canonical checkout locations. Keep package
association and repository-relative manifest paths. No cross-location equivalence
is asserted. Application scripts/mounts remain outside assembly identity.

Audit Git-owned tracked manifests as source inputs during lock/build/verification,
not as a back door to content-reading all Git sources on everyday launch.
Genuinely external Cargo configuration/toolchain candidates still follow the
existing content audit on launch. Record this ownership partition explicitly;
do not drop ancestor configuration merely because a package came from Git.

Use new lock/receipt and assembly identity/ready envelopes for source-kind data,
with exact versions and encoding fixed by gate 1 before publication. Keep existing
path-only envelopes readable under their original validation/generator rules;
relocking is explicit, never promotes an old artifact, and may produce a new key.
Do not make old tools reinterpret new fields. Unknown versions refuse with quoted
recovery commands and the retained-disk warning. Existing installation formats
need no change. Identity is not reduced to just the wrapper and Cargo.lock.

### 5. Git verification is explicit; path verification is unchanged

Everyday Git-sourced run/session/eval trusts the recorded Cargo source coordinates
and the contents of the retained Cargo checkout. It performs no source-content
read, Git command, object verification, resolution or fetch. It still validates
project/lock/receipt bindings, application and mapped-source content, external
inputs and artifact identity under 0059's metadata-stamp policy. Required canonical
source directories must exist at their recorded locations; structural checks are
not advertised as content authentication.

State the miss plainly: edits or corruption inside Cargo's source checkout can
go unnoticed by an everyday launch, whether deliberate or accidental. This is
broader than 0059's same-size restored-metadata artifact miss. Artifact verification
and source verification remain separate promises. A source file that a build
script arranged to read at runtime is not made immutable by its Git coordinate.

Lock, build (including a hit/attachment) and `--verify` run the bounded raw-blob
verifier against the locked revision. Deduplicate shared repository checks across
its packages. Verify tracked paths, bytes and executable bits against raw Git
objects with filters and routing overrides disabled; refuse unsupported entries,
unmerged/differing tracked sets and unexpected untracked source. Permit exactly
Cargo's root `.cargo-ok` as transport bookkeeping when it is a regular non-link
file with bounded contents; never treat its presence or bytes as authentication.
Keep ignored-output limitations explicit, as with today's path inventory. No
blanket exemption for extra files or arbitrary `.cargo*` paths.

Recheck source inputs after compilation before publishing readiness/receipts.
Verification never repairs a checkout or accepts new bytes under the old revision.
Build, attachment and `--verify` refuse mismatches and name the project's lock
command as recovery. Missing/corrupt referenced objects refuse.

Explicit lock owns acquisition. Cargo may reset its checkout when HEAD differs
or its completion marker is absent, discarding edits inside that Cargo-owned
checkout. Lock observes those transport states for reporting only, reports a
reacquired checkout, and authenticates the resulting bytes **after Cargo**, before
publishing either lock file. A mismatch remaining after acquisition still refuses;
Cargo's success is not authentication. The observer performs no pre-acquisition
content verification and is not used by build, attachment or launch. A checkout
Cargo considers fresh can retain an edit: lock then refuses too, and the user
must restore the checkout before retrying. No rnx verification step resets it.
The 90 ms whole-database fsck is not required on every operation: the gate proves
the narrower verifier's object/content checks and does not call them a full
history fsck. `--verify` also fully hashes the executable as before. This is
non-atomic trusted-local verification, not a hostile-concurrent-writer sandbox.

Path natives, including `RNX_DEP_RUNTIME`, retain current full BLAKE3 content
fingerprinting on every launch and 0065's nested-root reuse/fallback rules. In a
mixed project apply policy per source kind. Restored-mtime path edits still refuse
by content. Default Git launch must demonstrably accept the documented edited-
checkout counterexample while verification/build/attachment refuse it.

### 6. Default source ownership and the session transition

A stock session delegates preparation to its own management-capable executable.
Associated assembled sessions use their validated manager path. Unassociated
assembled sessions use an explicit `RNX_PROJECT_TOOL`, otherwise the installed
stock rnx on PATH with a bounded capability check; an invalid explicit override
refuses rather than falling back. The capability reply does not replace the real
eval startup probe. Preserve the private socket dispatch before runner startup,
FD sealing, process groups, interrupt escalation, reaping and commitment boundary.

For a stock scratch request the default is the manager's clean known Git
coordinates, whether acquired or unverified.
`RNX_DEP_RUNTIME` is the explicit path override and retains its no-fallback refusal.
An associated project keeps its declaration, not an ambient replacement. The
notice names the source kind and URL/revision or path, requested additions,
already-declared names, scratch/project owner, offline mode, cold build facts,
binding loss and retained history. Describe and decline create no scratch or
storage and perform no fetch. Consent permits Cargo acquisition, lock/build,
startup probe and the existing same-PID handover; not another runtime install.

Explicit `runtime install --from`, show/select/list/remove and authenticated
legacy migration remain available for developers and old consumers. Existing
selected installations do not silently override the new Git default. The new
stock `:dep` does not open the runtime store absent an explicit source choice.
To use a retained installation, explicitly point RNX_DEP_RUNTIME or a project's
runtime path at its `source` directory; show/install/select print that exact
quoted override instruction. Selection still governs the store and older tools;
selection alone is no longer implicit routing for new stock scratch creation.
Document this transition, including old selected-store fixtures. Nothing removes
or migrates entries automatically, and old scratch declarations keep working.

The default assembled runtime comes from the manager's recorded revision. That
is not a universal compatibility guarantee for an older launcher delegating to
a newer manager, an explicit override or a project runtime. Notices retain source
identities; do not claim that Git coordinates solved arbitrary launcher/runtime
skew. Preserve all 0063 startup and postcommit failure contracts.

### 7. Retained data and removal

0066's explicit assembly/runtime inspection, named-project annotations, quiescence,
whole-entry rename, resume and same-ID-rebuild protection remain. Keep old entries
and explain their disk cost through relocking. Cargo owns its Git source storage;
rnx removal must not recurse into that storage, delete Git sources on assembly
removal, or call them freely removable while a consumer may read them. No new
source-store lifetime manager is introduced here. Report Cargo Git disk usage
separately from assembly size and existing explicit runtime installations.

## Gates and stop points

1. **Crate boundary, coordinates and source documents.** In an isolated prototype,
   integrate management without a cycle; inspect the actual dependency graph and
   symbols/size of runner-only consumers and all shipped adapter/server edges.
   Prove real `cargo install --git` can build the internal crate without a second
   installed program. Establish build-coordinate acquisition classification for
   fresh/cached installs, multiple fetched revisions, dirty/staged/mode changes,
   hostile filters, clean published and unpublished checkouts installed by path,
   missing Git and unrelated outer Git administration. Dirty/unknown refusal and
   all describe/decline paths must leave fetch traps untouched. Clean known path
   builds proceed through consent: published succeeds without an override, and
   unpublished refuses at acquisition with Cargo's diagnostic retained. Pin the
   source declaration, lock/receipt/identity/ready schemas, ownership partition and
   compatibility readers with fixed vectors. Missing acquisition evidence must
   yield an unverified notice, not refusal or a fabricated acquired state.

2. **Management and session protocol.** Port the command boundary and discovery.
   Compare stock runner behavior, flags and files byte-for-byte; count zero runner
   builders/settings work for management. Replay preparation, startup, cancellation
   and commitment matrices with stock self-delegation, generated applications,
   the compatibility entrypoint and old associations. Test missing tool/invalid
   override, PATH absence, inherited carriers, killed groups and exact shell-run
   recovery/reopen lines. Decline and coordinate refusal leave a started tracked
   future usable and fetch/scratch/storage traps untouched.

3. **Git/path workflow and verification.** Through product commands, resolve and
   build actual combined natives by Git and by path, then attach a second consumer
   offline with compilation trapped. Test mixed sources, empty Cargo Git cache,
   missing/replaced canonical checkout, altered source bytes/modes, symlinks,
   untracked files, the marker rule, malformed/object-corrupt repositories and
   before/after-build edits. Demonstrate the default Git miss and explicit verify,
   build and attachment refusals; preserve path restored-mtime detection. Exercise
   old path formats, project locks, receipts, installation migration and 0066
   removal/resume. Reuse the real concurrency/kill/publication matrices, not the
   accepted probe's fixed-request candidate harness. Ready remains published last.

4. **One-install user journey.** Install only rnx via the tested `cargo install
   --git ... --rev ... --locked` command into a private bin directory. For an
   unpublished implementation checkpoint, use an explicitly labelled fixture Git
   origin through the same Cargo install path; gate closure must also include the
   ordinary published-origin install at the reviewed checkpoint. Publishing that
   checkpoint follows review, not a silent probe push. Keep both observations.
   Rename/remove the fixture checkout and remove rnx-project from PATH. Start
   with no runtime-store selection or overrides. Run rnx, consent to `:dep polars`,
   bind/transform a frame, recover from an error, preview, add PostgreSQL and run
   a typed query against a private cluster. The second scratch attaches offline
   with compilation trapped. Prove no runtime store is created or consulted;
   repeat with a stale selected store to prove it cannot reroute the default.
   Then explicitly opt into that retained source and prove the developer path.
   Preserve history, same PID, input numbering, cwd, reset, interrupt, quit/EOF,
   scratch reopen and process-reaping checks, plus a real notebook smoke test.

5. **Costs and regression.** Compare stock version, eval 42, JSON, first prompt and
   ordinary cells against `94f5f3f`, interleaved and pinned with verified outputs,
   raw samples, absolute deltas and spread. Require no reproducible stock slowdown
   above 5%; a miss stops for review. Compare zero-to-three adapters for Git and
   path separately. Report all six path first-adapter cells without reopening
   0065's accepted qualification. Git launch around 1–2 ms over direct is a
   hypothesis, not an accepted bound: require improvement over the matched path
   mode, attribute the remaining overhead, and stop on any source-content read,
   Git/Cargo invocation or network access in ordinary Git launch. Measure verify,
   lock, attachment, fetch bytes/time, cold compile, startup probe, executable size
   and disk retained by each owner separately. Run root feature suites serially,
   management suites in both configurations, strict clippy/fmt, combined notices,
   packaged-manifest checks, external embedding and shipped native/server smoke
   checks. Update the quick start to the tested one-install journey, without a
   companion install or runtime-install instruction in the ordinary path.

## Guardrails and later work

This record changes source ownership, the default verification coverage and the
command installation boundary. It does not add dynamic loading, mapped sources
at the prompt, automatic frame display, a package registry, prebuilt adapter
binaries, auto-updates or automatic eviction. Trusted Cargo cache content is an
explicit policy, not immutable storage or a hermetic build. Pure Rune sources
and path-native edits remain content checked. The Git-variable whitelist and
path-depth optimization wait; Linux acceptance does not claim Windows execution.
Registry publication/licensing gates remain in force. A source URL/revision and
a lockfile are not substitutes for the rest of 0061's assembly inputs.
