# ADR 0331: The shipped image builds reproducibly

Status: Accepted

Base: published 870413ea2516195ef1beb2ac95464fb68bf5df6a (ADR 0330).

## Problem

Two independent builds of the same baseline source, 4f0195be, produced
different engine image and CLI bytes. Only the startup helper matched.
Two causes are confirmed:
- Build identity. The paired build extracts a `git archive` inside the
  final's checkout without its own `.git`. The build-identity emitter
  then follows Git's upward discovery and records the enclosing
  checkout's HEAD. At the same export path, changing only the outer
  HEAD changed the baseline's label to that commit, with dirty=false.
- Build paths. Generated Cranelift sources are compiled from randomly
  named nested directories, and their absolute paths (the final payload
  and `.stock-file-XXXXXX`) land in loaded `.rodata`. The CLI embeds
  the image bytes, so it inherits the difference.

The gate binds each measured artifact by its bytes and reuses one
finished image across siblings, so this record doesn't invalidate
published results. But "the same source gives the same binary" is an
assumption the image-to-image comparisons make, and a label that names
the wrong revision is a provenance defect in its own right.

## Decision

1. Bind each paired export to its source. The exporter resolves each role
   to a full commit ID and its `<commit>:rune` subtree ID in the source
   repository, then archives that same commit. Both builds of the rnx
   crate (outer CLI and nested image) receive that role's identity.
   A missing, incomplete or inconsistent declaration fails the export
   build. The emitter and its Cargo rerun watches must ignore any
   enclosing repository. Ordinary builds in real checkouts, linked
   worktrees and Cargo Git checkouts retain their existing identity,
   dirty-state and acquisition behavior. An export declaration conveys
   no Cargo acquisition authority.
2. Give each build root a stable compiler-visible name. Cover the
   outer target, nested Cargo target, generated sources and direct
   compiler invocations. The stock CLI also embeds a physical source
   hint as a generated Rust string in `coordinates.rs`. Remapping
   compiler paths alone cannot remove that literal. For a shipped
   export, use the existing placeholder form of the runtime-override
   help. Keep the useful checkout hint for ordinary local builds.
3. Scope the byte guarantee to the stock Linux x86-64 release export
   built with the same source, locked dependencies, features, compiler,
   linker and build recipe. Check the entire three shipped files.
   Record and diagnose any remaining variation with the binaries
   retained. Return for review if a repair would change execution,
   acquisition semantics, the measurement method or its limits.
4. Preserve the measured paths and both fixed-route bands. Record the
   revised build recipe in its identity. Keep historical revisions
   buildable from their own source; their artifacts carry no
   retroactive reproducibility claim.

## Controls

1. Run two independent cold builds of the same candidate through the
   shipping recipe. Give them fresh targets and no reusable compiler
   outputs, unequal-length physical roots, distinct TMPDIRs and
   different enclosing Git HEADs. Use the public CLI to materialize its
   installed helper and image. Compare whole-file SHA-256 for each of
   the three artifacts and retain build inputs and per-section
   differences if a pair differs.
2. Check the embedded identity against IDs computed from the source
   repository independently of the emitter. Exercise a real baseline
   and candidate with different revisions, including a change of the
   rune subtree, so a role swap or stale identity is observable.
   Exercise the missing-declaration refusal inside an enclosing Git
   checkout and a warm rebuild after changing the declared identity.
   Keep ordinary checkout identity and project acquisition controls.
3. Plant an observable image-code change. Verify the behavior through
   the packaged CLI, and require the image and embedding CLI hashes to
   change. The helper may stay identical. A changed commit label alone
   cannot satisfy the behavior control.
4. Search the loaded segments of all three artifacts for the actual
   source, target, nested build and temporary roots of each build.
   Record whole-file hashes even for differences outside loaded segments.
5. Run targeted pre-sign checks under the final's real recipe, with
   every measured delta checked against its band, then one complete
   final on the signed candidate.

## Constraints

- No new third-party crate. Tabs.
- Programs admitted today keep the same results and effects.
