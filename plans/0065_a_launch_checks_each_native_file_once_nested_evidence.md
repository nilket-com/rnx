# 0065 gate 4: nested-root equivalence

Status: candidate passes on Linux, ready for review before enabling product reuse.
Gates 5–6 remain open. The only production edit is gate 3 F1's recovery command.
The candidate, counters and workflow integration are confined to an isolated tool
copy in rnx-bench. No production inventory, root code or dependency input changed.

## Reproduction and provenance

Sources: `probes/nested-inventory`; results: `results/nested-inventory-0065`.
The README states prerequisites and absent-target requirements. The source base
is the accepted 7b0bb65 tool; the dependency lock is byte-identical. The effective
modified sources, candidate patch, source hashes and both isolated binary hashes
are retained. The independent oracle invokes the original native/native_using
reader with counters. Additional observation hooks are inactive in the oracle.

The candidate invokes that same parser/reader for independent roots. It derives
an eligible child's root-relative tree by framing the already-read file digests
with the accepted v2 preimage. Package associations and roots remain distinct.
No source map, lock, receipt, cache identity or installation version changes.

## Eligibility and validation

One combined Git rev-parse call returns the enclosing --show-toplevel, absolute
Git directory, index path and superproject result. Reuse requires the ordinary
.git-directory/default-index layout, no inherited GIT_* settings, and a child
inside the observed root. It checks intervening .git entries, including files and
symlinks. Absence of another discovery boundary establishes the same top; the
real-roster fixture independently invokes --show-toplevel at every root as an
oracle for that proof. It does not spend a new Git process per eligible child.

The parent's existing staged parser refuses gitlinks, conflicts and unsupported
modes before reading files. Thus a child cannot be derived from a gitlink entry.
The parent's scoped untracked listing still refuses untracked files under a child;
ignored files retain their established meaning.

Discovery also walks a proposed child's descendants for repository boundaries,
including ignored subdirectories. This is bounded at 4096 directory entries per
child; exhaustion, unusual entries or lack of proof chooses independent inventory,
not a new source refusal. No ignored content is hashed. Its cost remains to be
measured in gate 5; a large ignored target tree can therefore lose eligibility.

Linked worktrees, separate .git files, nested/external repositories and inherited
routing/configuration variables use independent reads. Unrecognized topology
replies fall back too; a newline path demonstrates a case with an extra discovery
call. Counts below are tool-issued Git calls, not Git's internally spawned helpers.

Before reuse, the candidate rechecks observed Git/index and directory/boundary
metadata, then the relevant file metadata. Stamps include device, inode, mode,
size and nanosecond mtime. Every represented child entry and byte is charged again
against the existing 100,000-entry/512 MiB allowance. All child entries are charged
before byte accounting, as in the independent parser. An empty child still refuses.
Observation storage is local to one call and cleared on both success and error.

## Equivalence, counts and observation window

All 36 primary cases and 16 additional topology/roster cases pass. Quiescent cases
compare the exact Result (trees or diagnostic) and remaining byte allowance against
the oracle; no digest/refusal tolerance is used. Coverage includes untracked and
ignored files, additions/deletions/conflicts, missing files, working/index modes,
symlinks and symlinked components, FIFO/non-Unicode/backslash refusals, empty roots,
gitlinks/submodules, linked worktrees, separate administration, nested repositories
at and below the adapter (including ignored directories), routing overrides,
entry/byte limits, multi-level roots and discovery-budget fallback.

The real fixture is the same constant 443-file, 6,988,177-byte runtime used by the
accepted measurements, containing both shipped adapters and the complete renamed
PostgreSQL copy at every declaration count:

| Declared adapters | Independent reads | Candidate reads | Independent Git calls | Candidate Git calls |
| --- | ---: | ---: | ---: | ---: |
| 0 | 443 | 443 | 3 | 3 |
| 1 | 466 | 443 | 6 | 3 |
| 2 | 487 | 443 | 9 | 3 |
| 3 | 508 | 443 | 12 | 3 |

Each physical content path appears exactly once in each candidate call. All tree
bytes/digests remain equal, and logical duplicate charges remain. External and
nested cases take six calls and six reads in the two-root small fixture, matching
the independent path. Routing overrides may make the oracle refuse sooner; the
candidate preserves its actual counts rather than assuming every fallback succeeds.
Two inventories in one process perform six Git calls and read each physical file
twice, proving that no observation survives the call boundary.

Timed cases pause after the parent read and before child processing. Ordinary
mtime, index and boundary changes are refused by the candidate's validation.
The explicitly required counterexample is reproduced: a same-size in-place edit
with restored mtime is missed by the candidate's reused child but seen by the
oracle's later child reread. The next invocation reads it and changes both trees.
The candidate does not claim identical concurrent-edit observation windows or an
atomic snapshot, and deliberately does not use ctime to erase this tested limit.

An isolated workflow integration runs the accepted real two-native project through
its existing lock/receipt. Default and --verify launches both work; a same-size
source edit made before launch with restored mtime refuses under both modes.
Restoring bytes/timestamps restores successful launch. The lock pair and receipt
are byte-identical throughout. This temporarily uses and restores a fixture-owned
source, never the user's checkout or projects. No new latency claim is made.

## F1 and checks

The product's legacy runtime recovery now quotes current_exe() as well as the
retained source path. A real old-format store and tool path each contain spaces
and apostrophes. Show/select print the exact shell-parsed absolute command without
modifying the old state. With only Git on PATH and no rnx-project discoverable,
/bin/sh executes the printed line successfully and migrates; old entry bytes,
inodes, modes and timestamps remain unchanged. The 30-case migration replay also
passes, adapted only for isolated paths and the absolute executable expectation.

The current tool passes formatting, strict all-target clippy and 46 tests in each
feature configuration (two ignored each), with notices current. The isolated
candidate passes strict all-target clippy with test-support and its 46-test library
suite. Root source/Cargo, adapters, server, kernel and tool dependency files remain
unchanged. Gate 6 owns the full root regression.

Corrections during development are recorded rather than suppressed. Two fixture
Git commands needed correct staged-deletion/gitlink setup; a routing count assertion
incorrectly assumed six calls even when the oracle refused early. The first
candidate observed directories above the worktree, so the pause marker triggered
a spurious boundary-change refusal; observation now stops at the worktree boundary.
Descendant discovery was made explicit so ignored nested repositories cannot be
assumed eligible. Final matrices were rerun after these corrections. No unexplained
quiescent digest/refusal difference remains and no product fast path was enabled.

After review, the next checkpoint ports this candidate with counters/hooks confined
to test support and measures its actual launch slope, including fallback and depth
controls. It must still meet gate 5; this equivalence result does not predict that
cost by counting calls alone.
