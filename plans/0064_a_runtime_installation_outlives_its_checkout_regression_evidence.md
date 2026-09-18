# 0064 gate 6: installation regression

Status: passes on Linux, ready for review. Gates 1–5 are accepted. This checkpoint
completes implementation and the closing checks without claiming Windows execution
or resolving launcher/runtime skew. Native inventory remains the next record.

## Measured source and scope

The measured source is published rnx `5198758` plus
`results/runtime-final-0064/source.patch` in rnx-bench. That patch is preserved,
not merely hashed; its SHA-256 is
`6ab72933036f101dbc8953bf21c58b64d069b5e5dfed4964c00b1e5d18f82ac9`.
The bench baseline is `e11ff62`. Later edits to this evidence and the plan status
are not claimed as inputs to the builds. As with earlier records, those tracked
text edits affect a future project's native fingerprint and require a relock.

The only production change is the fsck diagnostic follow-up in
`tools/project/src/runtime_install/git.rs`. A failed fsck exit now names a
**corrupt installed runtime** and retains Git's bounded error text. Failure to
start Git retains the actionable prerequisite guidance. Command supervision,
validation order, permissions, selection and publication are unchanged.

Root source, manifest, lockfile, notices, kernel, both adapters, server and tool
dependency files are byte-identical to the accepted pre-installation `1ecc35c`.
The normalized default dependency graph matches that baseline, has one workspace
member and contains neither Polars nor the PostgreSQL driver. Generated public
docs expose the same entry point, extensions/scope and three server types; the
Rune re-export and all public source declarations are unchanged.

## Checks

All configurations ran serially with one test thread. Full commands, stdout,
stderr and statuses are retained under `results/runtime-final-0064/regression/`.

| Check | Passed | Failed | Ignored |
|---|---:|---:|---:|
| Root default | 376 | 0 | 0 |
| Root test-support | 419 | 0 | 0 |
| Root test-support + server-runtime + project-sources | 438 | 0 | 0 |
| Tool default | 40 | 0 | 2 |
| Tool test-support | 40 | 0 | 2 |

The two otherwise ignored tool integrations were executed separately and pass:
real manifest-to-runner handoff and real native inventory against this checkout.
Root/tool formatting, strict all-target tool clippy in both configurations,
root/tool notices, ordinary root release selfcheck and generated docs pass.

The root's combined features type-check for Windows GNU with the recorded MinGW
compiler/archiver. All tool targets with test-support type-check for Windows
MSVC. The existing Unix-only unused/dead-code warnings remain in the tool's
Windows check (13 production warnings and one additional test warning).
No Windows binary was linked or executed. The non-Unix runtime CLI and discovery
branches explicitly return unsupported errors before installation mutation;
ordinary root session code is unchanged. Linux remains the execution claim.

## Product replays

Every replay uses current product code. Tiny adapters in the earlier transition
fixtures are retained fault injectors, not substituted engine-performance claims.
Effective sources and original hashes are saved; historical results are untouched.

| Replay | Result |
|---|---|
| Preparation/cancellation | 44 groups pass |
| Startup/probe | 17 groups pass |
| Commitment with HTTP, SQL and tracked owners | 8 cases pass; private cluster reaped |
| Installation discovery and ownership | 25 recorded groups pass |
| Old-scratch reopening after selection changes | Pass |
| Installation publication/failure matrix | 90 cases pass |
| Repair/error matrix, test-support and ordinary | 8 cases each pass |
| Installed Polars/PostgreSQL journeys | 4 real journeys pass |
| Shared command/migration groups | 13 pass |
| Shared publication/failure replay | 37 cases pass |
| Legacy project workflow | 16 groups pass |
| Artifact stamps, full verification and interactive contracts | Pass |
| Polars override PTY and real mapped PostgreSQL workflow | Pass |

The preparation replay updates only the historical missing-runtime expectation
from the 0063 export line to 0064's exact installation command. Its installation
store is isolated under the fixture's XDG data home. Started-operation, descriptor,
old-prompt preservation and cancellation assertions are retained. Startup keeps
the real bounded eval and failures; commitment retains the irreversible boundary
and the actual HTTP/SQL owner observations.

The repair matrix provokes ordinary Git index permission drift for select and
same-ID reinstall. Both repair without changing bytes, inode, mtime or original
installation provenance. Corrupt blobs and source bytes refuse before permission
writes. The added wording assertions require corruption to name the installed
runtime without the install-Git suggestion. A missing-Git integrity check requires
the prerequisite guidance and leaves the entire entry and selection unchanged.
Both ordinary and test-support binaries pass these assertions.

The installed journey builds the stock launcher, tool and installed source from
one exact snapshot. Its original fixture checkout is physically renamed before
the first cold assembly and remains unavailable. The first scratch runs the
Polars frame journey; a second stock consumer attaches with compilation trapped.
Absolute and relative mixed projects preserve Polars and run the typed PostgreSQL
query. Binding loss, numbering, history, working directory, reopen command, same-PID
handover and process cleanup assertions remain. Generated manifests and consumer
locks contain no path back to the unavailable checkout. Equal source content and
different checkout/installation assembly identities are checked again.

The cache replays retain the distinction between formats: only historical local
lock creation uses the frozen pre-cache tool; current code builds and launches.
Current shared lock/build/run, stamp refresh, full verification, legacy preservation,
publication failure, ordinary-build hook exclusion and interactive ownership all
pass. The Polars override uses this gate's installed-journey artifact. The mapped
PostgreSQL workflow uses a fresh private cluster and current shared commands.

## Provenance and qualifications

The ordinary project tool is 3,501,920 bytes with SHA-256
`26c84d6caf76d268173a2d0806709437e53e081a4029d29a084ccba4379f4c17`.
The independently built installed-journey tool has the same digest. Root, installed
root and test-support binary hashes are recorded separately in `conditions.json`;
source relocation is not advertised as bit-reproducible binary production.

`probes/runtime-final/README.md` gives the serial reproduction order. Its collector
checks command statuses, the source patch, unchanged accepted bench files and no
remaining fixture executables. Driver-level checks also reap the private clusters
and observe the session/probe processes gone. Targets remain as ignored fixture
artifacts, not registered Git worktrees or running owners.

One archive-only correction was needed: the first repair loop reused a script/log
name for its two configurations. The executed driver is preserved, and both small
variants were rerun with unique source/log paths. Assertions and product policy
were unchanged; both runs are retained. Future runs use group-prefixed names.

Gate 5's accepted timing remains the cost evidence; this correctness sweep makes
no speedup claim. Ordinary project launches still fingerprint native source content,
including installed sources, but do not discover or validate installation metadata.
The roughly one-millisecond installed/check-out delta remains unexplained. The
next inventory record must measure cost against adapter count and investigate any
path-depth hypothesis before attributing it. No fixed 25 ms promise is reintroduced.
