# ADR 0326: Projects run on the installed engine image

Status: Proposed

Base: published fef4fcb10376d865f7f4f68d8301e50b35493f42 (ADR 0325).

## Problem

0324 made the installed image the default for `rnx run FILE`. Projects
still run on the old core: `rnx project run` and a generated app's own
run command. The packaged pair from 0322 is opt-in. Until projects move,
the old core keeps every project user, and the file fallback cannot be
deleted on the strength of file counts alone.

Three things make projects harder than files:
- The pair image enters with `Extensions::none()` and a checked handler
  table plus names that resolve only. A typed refusal cannot simply
  continue on legacy, because a legacy-only adapter operation needs the
  project's full adapter implementation.
- Several generated apps can share one user cache. 0324's prune keeps
  only the current generation, so two apps used alternately would evict
  each other.
- Native sets other than [] and [candle, polars], custom registrations,
  executable overrides and source aliases have no image route yet.

This is the second half of isolation record 6. Its first screen found
that a self method call's lowering depended on an unrelated registered
host name. 0325 removed that dependency, so the empty and Polars/Candle
project routes now score against their frozen runners on the same
image.

## Decision

1. Pair continuation by exec. The pair image cannot continue to legacy
   inside its own process. The full compatibility runner links its own
   copy of rnx and std, so interrupt state and output flushing would
   have two owners. This was shown in the review-stop receipt for
   bc33db5c. Instead:
   - The image keeps process ownership for admitted executions.
   - On a typed refusal before any effect, the process execs once, with
     the same PID, into an explicitly legacy-only entry of the same
     verified, pinned artifact. Nothing is rebuilt and the artifact is
     not looked up again.
   - A private, versioned descriptor handoff carries the saved entry,
     the ordered observed module outcomes including failures, the source
     budget, the original arguments and budget, and the route evidence.
     The legacy run keeps `src/program.rs`'s replay contract. Each
     observed request, including a miss, is replayed in order per
     (parent path, item) without touching disk. A request the image
     never made is resolved normally, under the transferred allowance
     and the project's source configuration. The entry is never
     reopened.
   - Signals are blocked from the refusal through exec. A pending
     SIGINT survives exec in the kernel and is delivered once the legacy
     side has installed its handler. The image asserts that its stdout
     and stderr buffers are empty before exec; anything else is a
     terminal fault.
   - The legacy process has one Rust startup, stdio and exit owner. Its
     status is the invocation's status.
   - Image, loader, provider and handoff faults are terminal. The route
     is temporary and its deletion follows the counts.
2. One dispatch point. `rnx project run` keeps resolving and verifying
   the lock, and keeps passing argv, source settings and the budget
   hint. Running the app directly enters the same dispatch.
   - Project build embeds the common engine bytes and a startup-linked
     helper in the one generated executable. The empty set uses the
     stock helper. The pair helper links the reviewed provider archive.
   - Publication binds the image, the helper, the ABI, the target, the
     recipe and the resolved project identity.
   - Launch reuses 0324's verified-descriptor cache and sealed-memory
     paths, and the image's rustc-generated main owns process startup.
   - A moved app runs offline, with no checkout, no adjacent library and
     no Cargo. A provider-only rebuild keeps the engine digest.
3. Shared cache. Several embedding executables (the CLI and any number
   of apps) coexist in one cache root. They must not evict each other's
   live generations on alternate runs, and retention stays bounded
   without anyone acting.
   - The policy is measured, for example a last-use time limit or one
     namespace per embedding identity, and its bound is stated.
   - Prune is best-effort after successful verification, so
     housekeeping never fails a launch.
4. Ineligible projects. `package_image`'s declaration and resolved-source
   checks apply to both supported manifest formats. The following run
   through their current artifact, with a distinct machine-readable
   reason given before any script effect:
   - other native sets;
   - changed registration or source;
   - executable overrides, whose event is emitted at the manager
     boundary;
   - source aliases.

   Lock and verification failures stay terminal. Each invocation is
   counted once, by kind: manifest ineligibility, explicit legacy, or a
   typed language refusal with its original location.
5. Deletions now. The direct in-process new-engine branch of an eligible
   generated run is removed, and so is its opt-in package step.
   Compatibility handling and existing explicit package ABI clients
   stay.
6. Parity is established once. Both public invocations are compared
   against frozen fef4fcb1, covering:
   - stdout, stderr, status, route and reason, and effect counts;
   - argv and SIGINT;
   - a buffered `process::exit`;
   - EPIPE and stack exhaustion;
   - the >2 GiB CSV through `polars::read_csv`.

   The dual-run comparison then retires.
7. Setup cost. The 0324 stock setup freeze stays. Project-manager
   startup and direct-app startup are measured separately, for the empty
   set and the pair, and for cold, verified warm and cacheless launches.
   Report matched setup W and T, the execution bands, build cost and
   artifact size. The measured project-only setup delta is brought for
   acceptance before it is frozen.
8. Runner nit carried in: the final runner's JSON stdout split matches
   every `--message-format=json*` form.
9. Not in this record: deleting the file and project fallback, and
   moving non-file commands. The counts from 0324 and 0326 drive the
   next record.

## Controls

1. Continuation. A pair file that uses a legacy-only adapter operation
   is refused once and runs once on legacy after the exec:
   - a module changed on disk after observation still runs the observed
     bytes, and failed lookups stay failed;
   - a module created after a missed lookup stays missed;
   - an entry path long enough that the image refuses before indexing
     imports still resolves its unobserved module once on legacy;
   - an effect counter shows each effect once;
   - a SIGINT sent between refusal and exec is delivered once;
   - output written before an explicit exit is flushed exactly once;
   - the PID is unchanged.
2. Install. A moved generated app, empty set and pair, runs offline with
   no checkout, no adjacent library and no Cargo, on an empty cache and
   then warm. A provider-only rebuild keeps the engine digest.
3. Cache. Alternating runs of two apps and the CLI cause no repeated
   materialization. Retention stays within the stated bound. A stray
   file in an old generation does not fail a launch. The 0324 safety
   refusals hold for app generations.
4. Route. Each ineligible kind is routed with its reason and counted
   once. Explicit legacy and typed refusals are counted separately.
   Lock and verification failures are terminal.
5. Packet. The comparison in Decision 6.
6. Pre-sign. Descriptor, environment and temporary-directory tests run
   under the final's real recipe with fd 9 occupied.
7. Finals. Targeted pre-sign checks, then one complete final on the
   signed candidate.

## Constraints

- Target: x86_64-unknown-linux-gnu. No new third-party crate. Tabs.
- ABI v1 to v5 callers keep working unchanged. The handoff is private
  and versioned and is not a public ABI.
- No engine arithmetic or dispatch change.
- README's one-executable promise holds for generated apps.
