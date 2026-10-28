# 0193: version cross-repository verification

Authority: Claude's V1 plan and Step 0 ruling, rune-v1 seq 23100; the user
requested V1b after accepted V1a. Rnx starts at 9bb28ebb. Rune's published
head is c7aee7e072dd84604fe41c6d94270942e49da4c8; the S7b2 anchor remains
f0baf933382039f2b8c44b84a04532ba5703b459. V1a is complete.

## Result and ownership

Move the recurring real-registration census and host-enrollment observer,
comparisons and classifiers into checked-in Rust test code inside rnx.
Expose one small tools/verify.py command:

```
python3 tools/verify.py cross --suite census|enrollment|all \
  (--rune-rev REV | --rune-path PATH) --fixtures-root ROOT --out NEW
```

Python prepares a temporary Cargo manifest and launches the versioned Rust
controller. It does not observe behavior or decide semantic acceptance.
Rust owns fixture validation, behavior capture, comparison, classification,
selection guards, source/binary binding and result reporting. Default mode
uses the real private install_core and registration modules. A control copy
requires explicit --registration-fixture and is labeled as a control.

Do not update the product Rune pin, normal locks, default features/routes,
public API or registration behavior. All seven Rune packages, including
tracing, must resolve to the requested identity in the temporary graph; the
compiler-only project dependency cannot silently retain a second Rune.
Normal rnx remains pinned as before. V1b is tooling and does not count M7.
V1c orchestration, measurement protocols and the resource DAG remain separate.

## Reference before migration

First replay the frozen 0192 observer against f0baf933, then against the
published Rune head. Keep the observer/controller bytes unchanged and use
actual rnx registration, with complete source/dependency/binary receipts.
The missing S7b2 enrollment replay must be established before the migrated
observer is accepted; an unexpected difference is a decision point.

The old controller's canonical F path must refer only to private scratch
storage inside a separate mount namespace. Each run owns a fresh parent;
each child owns its fixture tree. Never remove a shared frozen path.
Preserve all historical scripts, inputs, observations, seals and receipts.
The old observer is an explicit reference, never an implicit fallback for
the new command.

## Test-only process boundary

Add cfg(test) observers with access to real private registration. Enrollment
uses an explicit harness-free dev test target, enabled only by a non-default
verification feature in the temporary manifest. It runs as a separate process, so ordinary stdout and
stderr are not mixed with libtest output. No FD capture or production observer
API. Keep src/lib.rs as the actual child source; validate target identity,
cfg(test), private-module resolution and default-build behavior. Claude's
seq 23159 ruling places the direct-root target and autolib=false only in the
temporary manifest: no source link, nested library module, or normal-manifest
change. The normal build script declares the temporary cfg feature for rustc's
check-cfg lint. Compare normal build/test warnings with 9bb28ebb. A minimal
Cargo probe confirmed that a harness-free test enables cfg(test).

Claude's seq 23169 adjustment keeps only metadata and compilation inside the
private manifest/lock namespace (`cargo test --no-run --message-format=json`).
The controller runs outside it, so enrollment has one namespace per child,
matching the frozen reference. Nested bubblewrap was refused by the host during
the candidate2 mandatory self-check; that failed attempt remains preserved.
Python locates a launch candidate as process glue. Before observing behavior,
Rust validates the complete Cargo artifact stream: one matching dev target,
source, executable path/hash, cfg(test), feature and profile. Compiled Cargo
pins bind the private build graph; host Cargo files bind the saved originals.

The child and controller report distinct identities. Verify both rnx and
Rune compilation-unit roots, copied executable hashes, features, effective
arguments/environment, all Rune package identities and source/input hashes.
Temporary manifests point at real repository source files, not injected
copies of registration. Capture registration and product-source identity
before and after. Reject missing/zero/multiple test selections, stale or
wrong-source executables and fixture drift. Output directories are new and
immutable; failed attempts retain their raw evidence.

## Exact acceptance

Census observes all 244 cases, compile-only: no VM or execution invocation.
Preserve complete registry/resolved-item/diagnostic/lifecycle observations,
244 unchanged and zero changed, including the frozen full census identity
f5dccd6009e68ea051a33a74c5c674d8a821d776f8c79a4c23af1399dd530886.
Keep historical logical case names while reading relocated pinned inputs.

Enrollment preserves 110 stock and 110 execution observations: 107 semantic
matches, two retained arity refusals and one retained StoreCapacity result.
Retain exact typed values and float bits, stderr, stdout policy, filesystem
kinds/modes/content hashes/symlinks, lifecycle balance and clock envelopes.
Preserve five runtime controls, four helper checks, four projection checks,
classifier subjects and fixed-string/chain refusals, and fuel budgets 0..66
with first success at 66. Reproduce the frozen classifiers in Rust without
normalizing unexplained differences or discarding observations.

Version small reviewable sources, case manifests and policies where practical.
Keep large immutable oracles external by digest, report bytes required per
suite, and document acquisition. No new Git file exceeds 1 MB; the existing
6.29 MB census oracle and large enrollment trees need external indexed data
or a separately reviewed lossless split. Generate repeat-byte stdin and
scratch files only from pinned specifications with verified original hashes.
Filesystem-visible canonical names remain fixed through private namespace
mapping; physical reads never depend on a developer's historical workspace.

## Implementation and gates

1. Sign this subject-only plan commit before implementation.
2. Establish and freeze both old-observer reference replays, with isolated
   canonical filesystem namespaces and all changed lock edges reported.
3. Implement checked-in observers, explicit dev child, Rust controllers and
   small temporary-manifest launcher. Parallelize independent development.
4. Test wrong registration/Rune/source/binary, missing/duplicate selections,
   malformed outputs, missing/changed fixtures, classifier boundaries, float
   bits, scratch isolation, unchanged original F and failed-child retention.
5. Run real census and complete enrollment through the new commands on the
   published Rune head. Compare all counts, complete observations, policies,
   source/binary identities and frozen bytes with the reference. Test both
   rev and path inputs and relocation; do not substitute receipt replay.
6. Run formatting, strict linting and relevant rnx tests. Validate ordinary
   product build/CLI and unchanged production source, defaults and active locks.
   Record completed wall times, build/cache/resource details and failures;
   the full before/after critical-path acceptance belongs to V1c.
7. Obtain Claude's code/equivalence review, sign the subject-only implementation
   commit, verify its tree against frozen tested source, then obtain publication
   approval. Stop copying the census/enrollment controller families when this
   slice lands. Leave V1c explicitly pending.

Heavy correctness runs on Strix with the actual 30-CPU affinity budget and
separate targets. Use sluice and direct blocking completion, no notifiers or
process-status polling. Keep the existing native/Miri exclusion and Nano lock
policy; no performance samples or protocol changes are needed for this slice.
