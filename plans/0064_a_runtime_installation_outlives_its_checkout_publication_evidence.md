# 0064 gate 2: install completely, then select

Status: the product installer and publication matrix pass on Linux, ready for
review. Gate 1 is accepted; gates 3–6 remain open. Session discovery is not enabled
by this checkpoint: stock `:dep` still uses its existing explicit runtime override.

Baseline: rnx `5fc4843`. Bench checkpoint `534241d` contains
`probes/runtime-publication/` and `results/runtime-publication-0064/`, including the
exact measured tool patch, source and executable hashes, matrix results and raw
failure logs. Changes are confined to the project tool, this evidence and plan
status. Root source/manifests/lockfile/notices, kernel, adapters and server are
unchanged. The tool's manifest, dependency lock and notices are unchanged too.

## Product boundary

The tool now implements:

```text
rnx-project runtime install --from PATH
rnx-project runtime show
rnx-project runtime select FULL_ID
```

The new private installer separates source/layout validation, supervised Git,
managed storage and publication. It uses the standard data store from decision 3,
allows the user's symlink spelling before canonicalization, and checks managed
paths below that root. Source/cache/scratch containment refuses, with the stated
exception for the store's own installed source roots: reinstalling from that exact
source path validates/reselects its containing ID, without copying over itself.

All tracked Cargo manifests are inspected for relative path closure. Dependency,
target/build/dev, patch/replace, explicit package/target paths and workspace paths
cannot escape the tracked snapshot. Absolute paths and unsupported workspace
inheritance/glob indirection refuse by file and field. Catalogue validation reuses
0062's shipped package names and each adapter's direct dependency on the runtime.
This is not analysis of Rust source or arbitrary build-script inputs, and does
not claim hermetic builds.

The native fingerprinter has one private injected-Git entry point. Ordinary
callers still use the original Git helper; installer calls use sanitized, owned
subprocesses. Path enumeration, versioned encoding and production fingerprint
bounds are unchanged. Smaller test allowances enter the same bounded reader.
The copy loop independently checks cancellation, expected bytes/mode/digest,
remaining allowance and at most one detection byte. Copy directory creation and
retained traversal charge directory entries as well as files, preventing the
walk itself from growing outside the 400,000-entry bound. Retained source plus
Git administration is checked against 2 GiB before publication.

## Git and provenance

Index objects use `hash-object --no-filters`; explicit `--cacheinfo` records
validated paths, executable bits and raw object identities. The new Git index is
independent, with no history requirement. Administration checks refuse unexpected
routing/configuration/alternates and validate objects with real Git fsck.

Provenance compares raw HEAD tree entries against unfiltered working-file object
identities, never `git diff` or `git status`. An unborn HEAD records an uncommitted,
dirty snapshot rather than requiring a synthetic commit. Source commit and dirty
state are provenance only; the installed ID depends on the source digest.

Git children discard inherited Git routing/global configuration, disable hooks,
filesystem monitoring, replacement objects and optional index locks, and create
private outputs with a child-only umask. No source configuration is copied. The
source is fingerprinted again after acquiring the writer lock and after copying;
selection also checks it has not changed. The parent's umask remains untouched.

The existing subprocess owner gains a bounded-stderr variant for installer Git.
Both streams have independent 16 MiB bounds; Git and its process group are killed
on interruption/overflow and the direct child is waited. Ordinary Cargo still
uses its existing streamed stderr path. The matrix observes descriptors after
real Git exec and proves the writer lock is absent every time.

## Publication and retained identity

One close-on-exec advisory writer lock serializes installers/selectors. Waiters
honor interruption and re-examine state after acquiring it. Exclusive staging and
current-document temporaries live inside the canonical store. Retry inspects and
reclaims unpublished leftovers under the lock; it never removes a published entry.

Files, generated Git administration and metadata are synced before entry rename.
The entries directory is synced after rename; the installation is fully validated
again at its final path. Only then does a synced temporary replace current.json,
followed by a store-directory sync.

The failure outcomes are intentionally different:

| Boundary | Observed outcome |
| --- | --- |
| Before entry rename | No published entry and the old selection is unchanged. Ordinary failure/interruption removes owned staging. A SIGKILL copy-stage leftover remains undiscoverable until retry reclaims it. |
| After entry rename, before selection rename | A complete entry remains. The error says **installed but not selected**, the old default is unchanged, and real `runtime select` can activate the retained ID. |
| After selection rename | The error says selection **may already have changed**. The matrix observes the new selection; no rollback is claimed. |

A same-ID reinstall fully validates the retained entry, including Git objects and
source identity. It never rewrites the entry or original provenance, even from a
different source path or the installed source itself. An explicit selection can
replace a malformed current document, but it cannot overwrite a special file or
select invalid installation metadata/content. Show reads strict bounded metadata
without resolution, builds or mutation; full validation belongs to install/select.

## Ninety product matrix cases

`check.py` freezes the support executable and drives the product CLI against tiny
Git-tracked trees with the real supported manifest layout. These isolate installer
behavior; they are not substitute adapter builds. The matrix covers:

- Same-ID byte/inode/mtime identity, original provenance, no source writes,
  retained IDs, dirty/uncommitted/linked-worktree sources and relative `--from`.
- Fifteen injected copy/index/metadata/entry/selection boundaries, including both
  sides of the entry rename and selection rename.
- Unknown/oversized/invalid documents, corrupted source and independent Git objects,
  missing files, conflicts, untracked files, unsupported names and escaped Cargo paths.
- Source and retained bounds at equality and one below, plus actual copy-read
  accounting: allowance 1 reads exactly 2 bytes before refusal.
- User symlink root, managed symlink/FIFO refusal, store selection and nesting rules.
- Source changes after snapshot and growth after the copy file is opened.
- SIGINT/SIGTERM during copying, a Git step and after entry rename; interruption
  inside a running Git child with a sleeping descendant; bounded Git output/errors.
- Concurrent same-ID installers, a queued selector, a killed waiter, and retry after
  SIGKILL of a copying writer. A waiter never assumes lock release means publication.
- Positive hostile clean-filter and filesystem-monitor controls. Ordinary Git runs
  each hook; installation does not, and raw CRLF/source-index bytes remain intact.

All 90 pass. At final inspection all recorded child PIDs are gone and no executable
or working directory remains under the fixture. During an interrupted child-group
check an orphaned descendant is allowed only to be already absent or a zombie
awaiting init; no remaining live descendant is accepted. Arbitrary SIGKILL during
an active external writer is not presented as a stronger process-ownership guarantee.

## Full source, ordinary build and regression

The ordinary product installs the complete working-tree snapshot:
**417 files / 6,423,616 bytes**, digest
`07cea1a98725fa5d7c70eeab765a3e339bcee8f468e01a5a786b0f05c7210302`.
A second install leaves every retained file's bytes, inode and mtime unchanged.
This measured snapshot is baseline plus `measured-tool.patch`, before adding this
evidence/status text; the archive preserves it exactly. All shipped Cargo layouts
pass, including the other independent workspaces. Test-only failure/allowance
variables are present in the ordinary smoke environment and are ignored.

Formatting and strict all-target clippy pass in both tool configurations. The
library suite passes 40 tests in each (two pre-existing ignored); other all-target
suites pass 32 in each and 27 additionally with test-support (one ignored each).
Notices are current: the existing inventory remains 100 packages, 63 texts and six
named unavailable texts. The existing 13-group real cache/legacy command regression
passes, and its original result directory is restored afterwards. Only its newly
produced result files are archived here, not unrelated historical replays.

The first product matrix caught the inherited umask causing group-writable Git
administration. The child-only umask corrected that. Final review of the plan's
own-source exception also added the installed-source reinstall case. Neither
required weakening a gate. The gate-1 rerun cleanup/load guidance is clarified in
its bench README; no handshake timeout or test expectation was changed.

No timing, root regression, new Polars journey or Windows execution claim is made
here. Those remain the later gates. Gate 3 can now connect validated installation
discovery to the existing describe/consent/prepare flow after this checkpoint's review.
