# rnx 0064 gate 4: ordinary installed runtimes serve the real adapters

Gate 4 passes on Linux, ready for review. Gates 5 and 6 remain open.
Measured product source is `6198669` plus the exact `source.patch` in
`rnx-bench/results/runtime-installed-journey-0064`. The plan qualification,
README repair explanation and this evidence were written afterwards. Product
changes are confined to the project's runtime installer; root Rust, manifests,
lockfiles, notices, adapters, kernel and server are unchanged.

## F3: ordinary Git inspection is recoverable

An ordinary `git status`, with a stat change forcing refresh and host umask 002,
replaces the independent index with mode 0664. The recorded source is unchanged.
The two installation writers now have a narrow validation mode: inside `.git`
they may inspect owned regular files/directories with drifted permissions. Source
and installation metadata permissions remain strict. Symlinks, special files and
foreign ownership remain refused; traversal retains the installer allowance.

Before changing a mode, validate the allowed administration, bounded config,
independent object graph through `git fsck --full --no-reflogs`, tracked source
fingerprint, counts and Cargo layout against the installation document. Only then,
under the installation writer lock, open administration with no-follow/nonblocking
handles, remove non-owner/special permission bits and sync changed handles. Validate
private storage again afterwards. No object, index entry, source byte or provenance
is regenerated. A repair error may leave some modes tighter, without selecting an
unvalidated entry. Read-only describe/prepare does not silently repair.

The new seven-case matrix passes against both the test-support tool and the
ordinary release tool built from the installed snapshot:

- Reinstall and select each recover a real status-induced 0664 index to 0600.
  Bytes, inode, modification time and installation provenance stay unchanged.
- Corrupt a Git object, leaving the working source intact: both commands refuse
  before any permission or content write. The old selection stays unchanged.
- Corrupt a working source file: both commands likewise refuse without repair.
- A source file with group-write permission remains refused; the exception does
  not extend beyond Git administration.

The unchanged accepted 90-case installation/publication driver passes too. Only
its private target/results locations were redirected; the driver hash and exact
substitution are recorded. This includes interrupted writers/waiters, corrupt
objects/metadata, same-ID no-op identity, bounded reads and publication failures.

## One snapshot, actual products

`probes/runtime-installed-journey/setup.py` copies the tracked working source,
commits the isolated fixture, and builds the ordinary release rnx launcher and
rnx-project tool from that exact snapshot. There are no prototype substitutions.
The product installer copies it, including the real Polars and PostgreSQL adapters.
Its provenance commit equals the fixture commit and its installing-tool digest
matches the ordinary tool binary. All 419 tracked source paths, bytes and executable
bits compare equal independently between original and installed sources. The tree
digest is `86377470304f4ecba230dd814014dce6d7d935c6a367a9f7a88276495b30be6e`.

The source and installation resolve the same native recipe in one cache context;
their assembly keys differ because their canonical native paths differ. Before the
first assembly build, the fixture checkout is physically renamed and its old path
is absent. No assembly cache exists. No installed Git alternate, worktree link or
absolute worktree setting points back to the checkout. The source's provenance
path is informational; no command consults it.

The accepted real session-dogfood driver is reused with only its runtime root
changed to the installed source and added assertions for the runtime notice's ID,
canonical source path and provenance. Its exact effective source is archived.
The four journeys pass:

1. Stock `:dep --offline polars`, no RNX_DEP_RUNTIME, decline without state/cache
   creation, then a cold target build, actual startup probe and same-PID handover.
   A frame survives transformations across inputs and a catchable missing-column
   error, grouping/aggregation, Parquet write/read and repeated collect.
2. A second stock consumer attaches to the same installed assembly with Cargo and
   rustc compilation trapped (positive controls prove the traps). The printed,
   shell-quoted scratch command actually reopens with Polars live.
3. An absolute-runtime project requests `polars postgres` with Polars already
   present, adds only PostgreSQL, then runs the frame journey and a typed query
   containing quotes, SQL text, a backslash and an emoji on a private cluster.
4. The relative-runtime project repeats the mixed journey with compilation trapped
   and shares the first combined assembly. Path-form assertions remain unchanged.

The original checks still assert lost bindings, input numbering restarted at 1,
unchanged cwd, saved-history recall without replay, terminal foreground ownership,
same PID, no children after handover and process/backend cleanup on quit. The
explicit-override notice is independently checked before consent and declined.
The default notice names the installed runtime in every stock transition.
Generated assembly manifests and consumer locks contain no original checkout path.
No user installation, cache, project or kernelspec is touched.

## Checks, artifacts and scope

Root formatting and the serial default suite pass: 376 tests, zero failures.
Tool formatting, strict all-target clippy in both configurations, and 40 tests in
each configuration pass (two existing ignored tests each). Tool notices remain
100 packages, 63 texts and six recorded unavailable. Command logs and binary,
source-patch and fixture identities are in the bench evidence.

The ordinary project tool SHA-256 is
`69c3a991d5cab37389861007c56f5a5890cb423c99dd6e9260ce7632cc26578d`.
The fixture, launcher, tool and installed source use one snapshot; this is not
launcher/runtime-skew compatibility evidence. Cold builds use already cached
registry sources. Journey phase timestamps are retained, but are not the matched
installation/build/attachment/launch cost experiment: that remains gate 5.
Full root configurations, broader transition/adapter replay and Windows remain
gate 6. The bench README states fresh-target requirements and all substitutions.
