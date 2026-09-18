# 0065 gate 1: the reader and current encodings

Status: gate 1 passes on Linux, ready for review. Gates 2–6 remain open.
Plan: `601ce23`, following the accepted inventory/hash probes at bench `c2af5de`
and `921ffe3`. The source-map and user manifest remain format 1. No root source,
root Cargo file, adapter, server or kernel changes.

## Implemented boundary

The tool pins BLAKE3 1.8.7 with default/std features only: no Rayon, mmap, new
thread pool or buffer-size change. One 16 KiB bounded reader still performs all
existing file/component/metadata checks and the final detection read. It computes
one content digest and frames the raw 32-byte result after name, executable bit
and content length under `rnx-tree-v2\0`. It no longer feeds each content chunk
to both file and tree hashers.

Current tool-owned digest fields now say blake3. Lock 3, receipt 4, identity 2 /
generator 2, ready 2, installation 2 and selection 2 are wired together. Installation
2 already reserves the strict optional migrated_from record (old format and ID),
so implementing its writer at gate 3 will not silently invent another schema.
Unknown/mixed fields and versions refuse. Local-generated locks cannot validate
as lock 3; new generated projects still select shared assembly. Source maps and
manifest parsing are byte-identical to the plan baseline.

The SHA-256 reader is retained privately at fingerprint/legacy.rs for installation
migration and conformance tests. Its function bodies match 7cd3205 exactly except
qualification of the test-only after-open hook. Its File type still says sha256;
it cannot be confused with the new File type. The ordinary tool has no call into
that reader yet. Gate 3 must add the explicit authenticated installation migration;
there is no legacy launch fallback in this checkpoint.

This is a reader/encoding checkpoint, not completion of the new-format user
workflow. In particular, actionable old-lock/receipt recovery, override build
verification, old-default migration commands and migration publication are still
gates 2 and 3. No claim is made that the new tool can already upgrade an existing
installation. No cache entry is removed or relabelled, and no nested-root reuse
or performance claim is introduced here.

## Checks and evidence

`rnx-bench/probes/inventory-encoding` builds and drives the actual private
rnx-project-assembly-probe target. The test-only fingerprint-v1/v2 commands are
not new product commands. Results and reproducible checks are retained under
`results/inventory-encoding-0065`.

- 37 paired filesystem cases pass. Semantic records agree, while each algorithm's
  digests match its own references; refusal messages agree byte for byte.
- The corpus covers zero length, 16 KiB buffer edges, allowance edges and entry
  exhaustion, executable mode, Unicode/quotes/nesting, dirty working-tree content,
  ignored/untracked files, missing tracked files, symlink roots/index/components,
  FIFOs, non-Unicode names, gitlinks and unmerged entries. Every call is bounded
  to five seconds. Temporary fixture trees and Git children are gone on completion.
- Python independently computes v1 file SHA-256 and v2 framing. The accepted
  whole-input BLAKE3 reference hashes those independently framed bytes. Six fixed
  vectors cover empty trees/files, ordering, binary content, executable mode,
  unusual names and name/content boundary ambiguity. Unit tests use their literal
  expected digests, not only another call to the same tree constructor.
- Unit tests retain source-vs-single-file accounting and deterministic mid-read
  shrink/growth checks. Both legacy and current readers refuse the injected EOF
  and growth with the same allowance consumption. Duplicate framed names refuse.
- The six document versions have strict current/mixed/unknown-version tests,
  including ready-to-identity/artifact binding, shared-lock/native binding,
  receipt stamp/assembly-key shapes, installation ID binding and selection shape.
  Identity keys are explicitly checked against BLAKE3 of canonical document bytes.
- Tool fmt, strict all-target clippy and suites pass in both configurations:
  **46 passed, 0 failed, 2 existing opt-in integrations ignored** in each. No root
  suites are claimed here; the root is unchanged and full regression is gate 6.
- The locked tool graph adds BLAKE3, arrayvec, constant_time_eq and a separate
  cpufeatures 0.3.1 alongside SHA-256's 0.2.17. No existing package version moves.
  Notices report 104 packages, 69 texts and the same six pre-existing unavailable
  texts. All newly added packages have texts. Historical licence-provenance hashes
  remain labelled SHA-256; they are not executable identity fields.

One paired-fixture correction was necessary: a directory replaced by a symlink
was reported as untracked by Git before the component check. Giving that replacement
an ignore rule reaches the intended component check in both readers. Both refuse;
no product check was bypassed or relaxed. Existing codec test injections were
updated to the new versions so they still mutate the input instead of becoming
no-ops. The final checks above are against the final source.

Root source/dependency/API, manifest parser and handoff correspondence are asserted
by the checks driver. It also records current source digests and the probe binary
hash. Published implementation and bench commits preserve the actual source;
these hashes do not substitute for a recoverable revision.

Next: gate 2's real current-format workflows and matched format-only timing,
then gate 3's retained-runtime migration and old-consumer survival, including
the old-key kernelspec. Removal remains the record immediately after 0065.
