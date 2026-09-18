# 0064 gate 1: installed sources survive their checkout

Status: the independent-source/index prototype passes on Linux, ready for review.
Gates 2–6 remain open. No product implementation changed.

Plan/source baseline: `b4510d4`. Bench checkpoint: `0ed32a6`, containing
`probes/runtime-install/` and `results/runtime-install-0064/`.

## Source preservation and scope

The probe imports the complete tracked base tree, modifies only the isolated
tool's `main.rs` and `workflow/transition.rs`, and adds `runtime_probe.rs`.
All 408 other baseline files are byte-identical, including the root CLI, native
fingerprinter, inventory, generator, cache identity, catalogue, startup probe,
serving code, manifests, lockfiles and notices. A colour-free patch plus the
retained base commit reconstructs every installed file; `checks.py` asserts this
rather than relying on a source hash alone. `source-correspondence.json` records
all base-file hashes and the launcher/tool executable digests.

The fixture is committed locally before building its stock launcher and tool,
which are copied to a sibling directory. The installed runtime therefore comes
from the exact same snapshot as those binaries. That fixture commit is provenance,
not the sole archive: the base and patch preserve its content. This does not test
launcher/tool/runtime version skew.

The new probe-only command copies fingerprinted files, makes an independent Git
index/object database, and writes installation/current documents directly in a
private store. It is **not** a product installer. Atomic publication, concurrency,
managed-path defenses, static Cargo escape validation, reinstall/select commands,
and the full refusal/discovery matrix remain later gates. No public Rune API,
protocol version, dependency, lock/receipt format or inventory rule changes.

## Independent index and byte controls

The existing native fingerprinter runs unchanged in a subprocess with inherited
Git routing/config environment removed. Installation Git commands also disable
global/system configuration, hooks, attributes-file configuration and autocrlf.
The source's own attributes are never used for object creation: `hash-object
--no-filters` writes raw blobs, and `update-index --cacheinfo` records explicit
paths, modes and object IDs. The installed repository has no history requirement,
source configuration, hooks, alternates, worktree link or absolute worktree setting.
Git remains a prerequisite for the unchanged inventory.

Eight control groups pass:

| Control | Observation |
| --- | --- |
| Dirty bytes, executable bit and literal names | Installed bytes match the working tree, not HEAD/index blobs; CRLF, NUL-containing content, quotes, apostrophe, emoji, newline/comma filenames and executable mode survive. Ignored content is excluded. |
| Linked worktree | Source `.git` is a routing file. Both that worktree and its common repository are renamed; the installed `.git` is a standalone directory whose blobs and fingerprint still work. |
| Hostile Git | A positive ordinary `git add` invokes the hostile clean filter and writes a different blob. Installation invokes no filter and preserves raw CRLF bytes despite local/global autocrlf and inherited GIT_DIR, GIT_WORK_TREE, GIT_INDEX_FILE, object-directory, alternate and config overrides. |
| Symlink | Existing fingerprinter refuses before creating the installation store. |
| FIFO | Refuses without blocking or creating the store. |
| Untracked non-ignored file | Refuses before creating the store. |
| Backslash filename | Existing unsupported-name refusal is preserved. |
| Non-Unicode filename | Existing refusal is preserved. |

Every staged blob in the small installed controls is read from its independent
object database and compared against working bytes. The full installed runtime
also passes `git fsck --full --no-reflogs` after the original checkout is renamed.

**Finding for implementation:** the first provenance implementation used
`git diff` to detect dirty state. The hostile-filter marker caught a filter call
there, despite raw copying/indexing being correct. The final prototype compares
raw HEAD tree entries against unfiltered working-file object identities instead.
Product provenance collection must preserve this property; `diff`/`status` are
not harmless read-only substitutes for this purpose.

## Full snapshot and real unavailable-source journey

The installed snapshot has **411 tracked files / 6,386,750 source bytes**. Both
source and installed tree digest are:

`09dced002cd9705d03ea0628e52e3827a70ae3ea451623e3633dfb46dcdb7abe`

The private fixture `target/original` is physically renamed to
`target/unavailable-original` before the first installed assembly build. Its old
path remains absent through every journey and final inspection. The user's rnx
checkout is untouched. `RNX_DEP_RUNTIME` is absent; stock sessions discover the
private XDG data installation. The tool's pre-consent `Runtime:` line names the
installation ID, source provenance and canonical installed source. Selection
reuses the existing notice field, so no protocol change is needed. The prototype
re-fingerprints the selected installation before creating a scratch project.

The inherited 0063 dogfood driver is reused with exactly one substitution: its
fixture runtime root is the installed source. The override variable it uses to
select that Python fixture path is not read by product code. Driver provenance
records the substitution and both source hashes. Real Cargo, the real serving
entry, real Polars and a private PostgreSQL cluster are used throughout.

| Journey | Result |
| --- | --- |
| First stock `:dep --offline polars` | Decline writes no scratch/cache and keeps bindings. Consent builds from an empty private assembly cache, probes, restarts in the same PID, and completes CSV/filter/group/Parquet/read-back/error-recovery work. |
| Second stock Polars request | Same installed assembly/artifact; compilation traps with positive controls prove no compilation. The exact quoted scratch-reopen command also succeeds. |
| Absolute project, mixed Polars/Postgres | Adds only postgres, cold-builds the combined recipe, keeps Polars available after restart and executes a typed query on a private cluster. |
| Relative project, same mixed recipe | Attaches to the combined assembly with compilation trapped and runs the same typed query. Relative adapter declarations remain relative. |

Both transitions discard old bindings, restart numbering at one, preserve saved
history and working directory, and leave no session children after handover/quit.
All session processes and the fixture postmaster are gone; the independent
cluster observer sees no client backend after quit.

First Polars and combined journeys took about 123 and 120 seconds; second-consumer
handover took about 2.08 and 2.02 seconds. These are diagnostic journey durations,
**not benchmarks**: registry sources were cached, assembly targets were cold,
and tool checks overlapped some compilation. No launch-cost gate is claimed.

## Relocation and build-path inspection

A separate lock-only control uses the same cache root and native recipe at the
source and installed locations. The source digest agrees, but canonical-path
identity differs as 0061 requires:

- Source-location assembly key: `db1bc1f0e05c8941789edf275cf36302973fa1ce7bcc35058931f053b821ba33`.
- Installed-location assembly key: `6dbca6af34a317598330df2dbec7e18e2de1617f9b7584e023693349891066dc`.

That identity-only cache builds nothing. The actual journeys use a separate cache
that does not exist at their start; no checkout artifact is promoted or reused.

Both real assemblies' generated Cargo manifests, wrapper mains and full locked
Cargo metadata contain no path to the original/renamed fixture or the user's
source checkout. Every local Cargo package is beneath the installation or its
cache-owned assembly. Four resulting project locks' native inventories likewise
contain only installed native roots. Metadata and lock documents are retained.
The installation document intentionally retains original source provenance outside
the fingerprinted source/build inputs; that is not a surviving build dependency.
The final installed digest remains unchanged after all builds and checks.

The retained installation uses 8,352,127 apparent file bytes including Git objects,
index and installation/selection documents. This is separate from the much larger
Cargo assembly cache and is not a general disk-size promise.

## Validation and qualifications

Isolated tool formatting and strict all-target clippy pass in ordinary and
`test-support` configurations. The library suite passes **40 tests** in each
(two pre-existing ignored integrations); support fixture targets additionally pass
32 and 27 tests (one ignored in each). The eight index groups and four real
journeys pass. Root product code is unchanged; the full root regression is gate 6.

Two harness corrections did not change the measured prototype: patch reconstruction
needed its own disposable Git repository so `git apply` did not ignore paths
relative to the enclosing bench repository; path inspection needed component
boundaries because `/home/me/work/rnx` is a textual prefix of `rnx-bench`. Exact
file equality and local-package ancestry assertions now cover those mistakes.
The successful cold journey results were preserved, not rerun or replaced.

The source/index/discovery stop condition is not reached. Gate 2 can implement
the bounded product installer and its publication/failure matrix after review.
