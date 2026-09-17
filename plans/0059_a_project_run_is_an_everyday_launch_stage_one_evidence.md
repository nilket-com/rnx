# 0059 stage one: remove work, keep full verification

Ready for review on Linux. Plan 6f35403 is committed. Gates 1 and 2 are exercised
here; the metadata default, --verify, receipt migration, native-inventory
subdivision and final everyday-launch gate remain open. This is intentionally
the intermediate full-verification implementation the accepted plan requested.

## The user's launch

Ordinary release binaries, same generated Polars artifact, entry and locked
inputs. Two interleaved repeats, twenty samples per product/workload, one pinned
CPU and one Polars thread. All 240 outputs/statuses/stderr are checked and all
samples retained. Medians in milliseconds:

| Product | Init | Whole tiny pipeline |
| --- | --- | --- |
| Unchanged tool | 159.85 / 156.16 | 155.08 / 155.49 |
| Stage-one tool | 90.62 / 90.59 | 95.03 / 94.77 |
| Generated artifact directly | 6.52 / 6.48 | 10.80 / 10.77 |

The whole pipeline saves about 60 ms, approximately 39%, with no artifact
verification skipped. The old init blocks vary more than the pipeline; both
are retained without removing samples. This remains around 84 ms above direct
and does not satisfy the final default-launch target. No claim is made that
this stage already implements the metadata-based default.

No profiler runs in these product executables. The clock includes spawn,
capture, exec and wait as paid by the user. Output-directory setup/cleanup and
project lock/build happen outside timing. The accepted before-tool hash matches
the 0059 attribution probe's ordinary stock binary. The current tool's hashes,
source diff and generated artifact identity are recorded. Both tools accept the
same refreshed locks, providing a real check that their input identities agree.

## Implementation and coverage

fingerprint::one now calls a shared bounded file reader with no tree hasher.
Tree callers supply their hasher to that same reader. Content digest, filename,
mode bit, size, path/regular-file checks, non-blocking/no-follow open, byte/entry
allowances, early EOF, growth detection and post-read checks are preserved.
The tree framing is unchanged. No digest algorithm or public lock schema changes.

Unit cases compare the single-file and tree file records plus allowance effects
at empty/read-buffer/allowance boundaries. Test-only after-open callbacks provoke
truncation and growth through both callers. The production reader contains no
callback or hook. Mode, symlink, FIFO and non-Unicode cases exercise the direct
path as well as existing tree tests.

A small private map helper compares bounded existing regular-file bytes with the
canonical map. Only missing or different regular maps call the existing atomic
publisher. Identical maps perform no write/rename/fsync. Symlinks, directories,
FIFOs, unreadable and oversized maps refuse. Publication failure is returned,
leaving the old incorrect map in place rather than pretending it was repaired.
This is derived-map handling, not repair of public input locks.

Publication counters live only in unit-test callbacks. Every ordinary after-tool
launch also brackets the map's inode and mtime outside timing, proving unchanged
identity on reuse. A real corrupted-map case republishes from the lock and runs
correctly. A same-size in-place artifact edit with restored mtime is refused by
hash before any script output; the fixture restores the original byte/times in
a finally and verifies the full digest. This distinguishes the measured tool
from both filename-only map reuse and the future metadata default.

## Checks and scope

The tool passes 34 tests in each feature configuration, zero failures. Two
legacy assembly integrations remain ignored and are not counted as executed.
The existing black-box 0057 workflow fixture passes all sixteen groups, including
stale/tampered inputs, interruption/publication, relocation and executable
overrides; its original tracked results were restored. The real Polars example
runs through product lock/build/run and direct controls during this measurement.

Formatting and strict all-targets Clippy pass in both configurations. Notices
remain current at 100 packages/63 texts/six previously unavailable texts. The
tool checks successfully for x86_64-pc-windows-msvc; no Windows execution is
claimed, and its existing product command refusal remains. The unreadable-file
unit case ran as uid 1003, not root, so the permission assertion was exercised.

Changes are confined to the independent project tool and plans. Root src/tests,
manifest/lock/notices, kernel, adapters and server are unchanged. No root runtime,
public API, dependency or interpreter startup change is proposed. No test-speed
optimisation is counted as a product benefit.

Artifacts: rnx-bench/probes/project-run-stage/ and
results/project-run-stage-0059/. The latter holds commands/logs, hashes/source
patch, public lock snapshots, all samples, journal and product checks. Locks
capture the measured native tree before the subsequent evidence/README edits;
reruns explicitly lock/build again, as normal for tracked native dependencies.
