# 0066 gate 2: product commands and interruption evidence

Status: implemented, ready for review. Gate 1 was accepted at `bafa2a0`, with
bench `94539e1`. Gate 2's source archive, drivers and retained results are in
rnx-bench `14440f3`, under `probes/removal-commands` and
`results/removal-commands-0066`. Gates 3–4 remain open.

## Product boundary

The change is confined to the project tool. A private maintenance module handles
cache/runtime list and remove, with exact IDs, root precedence, read-only dry
runs, explicit quiescence, pending-only resume and repeatable annotations on
inspection commands only. The workflow dispatches maintenance separately from
launch/build/install. No dependency, document version, identity generator,
launch operation, root source, adapter, server or kernel changes.

The accepted directory-handle primitive is ported without a path fallback:
openat2 beneath/no-symlink/no-mount-crossing guards, O_PATH leaf inspection,
no-replace rename, both rename-parent syncs before deletion, permanent existing
writer-lock paths and guarded recursive unlink. Internal symlinks are leaves.
Only owned, preflighted data is deleted. Current-executable/current-directory
protection, selected-runtime protection in both formats and uninterpretable
selection refusal remain. The product starts no subprocess. Linux-only hooks,
forced failures, reduced bounds and pause controls compile only with test-support.

The report is bounded pretty JSON. Entry metadata is an unauthenticated envelope;
corrupt or missing entry documents do not make owned data undeletable. Explicit
project annotations inspect manifest/lock/receipt envelopes and reference fields,
not dependency contents, executable hashes or freshness. They cannot certify
receipt-to-lock content binding. Local/override forms never match a shared entry
by digest alone. Shared entries match recorded store plus key; runtimes match the
locked runtime source path. No default selection or provenance is used to infer a
project reference. Missing, unsupported or observably changed documents make that
annotation indeterminate and the report exits nonzero. Relative and symlink
manifest spellings coalesce after canonicalization; no project search occurs.

## Builds and checks

The two frozen release binaries used by the final replays have SHA-256 evidence
hashes:

| Configuration | Hash |
| --- | --- |
| ordinary | `f0338e5b57f6ca8d7568b5f5426d1713bd4ca779b11a15aa43a8076b6dfc4d50` |
| test-support | `1b93cf24d6fefb9a456b18bf0ac76e982cfd8ee6db402681dc9304b2582144fc` |

Tool formatting and strict clippy across all targets pass in both configurations.
Serial tests: ordinary 47 passed, support 48 passed, zero failures, two ignored in
each (the ignored integration tests have their own fixture requirements).
Notices check: 104 packages, 69 texts, six previously named unavailable texts;
no inventory change. `build.json` records every tool source hash and unchanged
Cargo files; `source/` preserves changed source, not hashes alone. `finish.json`
rechecks the source and binary correspondence, unchanged root paths, notices,
Python syntax and absence of fixture processes/mounts. Broad root suites and
matched launch regression belong to gate 4 and are not claimed here.

## Filesystem and command matrix

All 42 applicable gate-1 filesystem controls pass on the product binary,
including the six real bubblewrap mount shapes. Same-device bind directories,
nested tmpfs, entry/control mounts and a bound regular file all refuse EXDEV
before rename and preserve outside sentinels. No unprivileged-namespace gap is
substituted with a mocked device number. The gate-1 namespace stop and bootstrap
positive control remain preserved in their accepted evidence.

All 52 command contract groups pass: exact options/root precedence, absent-root
inspection without creation, sorted reports, unknown metadata and escaped names,
entry/memory/output limits, old/current reference envelopes, local/override forms,
different stores, missing/visible/pending references, duplicate and relative
paths, document edits/replacements, and refusal of mutating annotations. Ordinary
removal ignores the test-hook environment, including forced ENOSYS and a pause.
Inspection's exec trace contains only the tool, no Git/compiler child.

The interruption matrix covers SIGINT, SIGTERM and SIGKILL before rename, after
rename and during deletion. Additional real SIGKILL cases bracket each of the
two rename-parent syncs. The actual held writer descriptor has CLOEXEC observed
in /proc; its inode remains at its path. Failure hooks bracket root sync when
creating `removing`, both rename-parent syncs and final pending-parent sync.
Before-commit failures preserve entry contents; postcommit errors identify the
pending state and print the quoted resume command. Errors after final unlink may
leave nothing to resume, and do not claim confirmed durability. These are errors
injected around actual sync calls, not simulated power-loss durability tests.
Actual parent/child permission failures separately prove before-rename
preservation and post-rename resumability. No automatic resume occurs.

## Genuine same-key rebuild

The independent old tool is built from `7cd3205`, with evidence hash
`26c84d6caf76d268173a2d0806709437e53e081a4029d29a084ccba4379f4c17`.
Setup installs a fixture checkout, renames it, and builds Polars plus a native
retained-OUT_DIR reader from an initially empty private cache. The wrapper and
locks do not point back to the unavailable checkout. This is a real old shared
assembly, not a fabricated visible directory.

On the final product, removal is killed after the two rename-parent syncs. The
entry is absent from entries and present in removing; the old receipt is intact.
The old tool then runs its real offline build under exec tracing: 394 compiler
execs, approximately 112.5 seconds, the same assembly key, and a new artifact
inode. This asserts compilation rather than inferring it from duration.

Listing shows the same ID in both locations, with the named project referencing
only the visible entry. Executing the exact printed resume command removes only
the pending copy. Every file in the rebuilt visible tree retains its bytes,
inode, mode, size and mtime; both project locks and the newly written receipt stay
unchanged. The old tool then evaluates its retained-output reader and returns
`"before"`. `rebuild.json`, `old-rebuild.exec` and the build log retain this proof.
A development run preceded the frozen-binary replay; only the latter rebuild is
reported here.

All deletion was in disposable fixture stores with no live consumer at mutation.
Gate 1 already proved inspection against live old sessions/kernels. Gate 3 still
owes the full migrated-default Polars/PostgreSQL journey and its old-key kernel;
gate 4 owes real costs and full regressions. No user store was removed.
