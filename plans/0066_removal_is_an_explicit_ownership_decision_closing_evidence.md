# 0066 gate 4: costs and closing regression

Status: ready for review; acceptance closes 0066 on Linux with the plan's stated
ownership and platform qualifications. Gate 3 was accepted at `021f401` and bench
`39467e8`. Closing drivers, source patch and results are in bench `e628c97`,
`probes/removal-final` and `results/removal-final-0066`.

## Handshake test correction

The direct stream reader is moved unchanged from inside the handshake to private
module scope. A deterministic in-memory test checks the 4096-byte boundary, an
oversized stream refusal and consumption of exactly 4097 bytes. There is no
subprocess or clock in that assertion. The oversized subprocess still must
refuse, but does not demand a size error when the production deadline legitimately
wins on a loaded host. Its other invalid-reply and child-retirement controls stay.
The real one-second timeout test still checks that its child is reaped.

`finish.json` compares the extracted reader and remaining production handshake
body with gate 3, ignoring indentation/blank lines only: both are unchanged. The
one-second production bound remains literal and unchanged. No dependency, manifest,
identity format, generator, public API or root source changes. The twelve concurrent
support suites all pass without retries or failed-run filtering; logs are retained.
This is deterministic stream-boundary coverage, not a promise that arbitrary
external subprocess scheduling can always meet a wall-clock deadline.

## Storage costs

| Copy | Nodes | Logical bytes | Allocated estimate | Total observed free increase |
| --- | ---: | ---: | ---: | ---: |
| polars | 3,745 | 1,710,427,835 | 1,722,871,808 | 1,722,859,520 |
| combined | 3,905 | 1,740,572,120 | 1,753,546,752 | 1,753,530,368 |
| runtime | 1,093 | 8,475,141 | 11,497,472 | 11,485,184 |

| Copy | List ms | Dry-run ms | Remove ms | Resume ms |
| --- | ---: | ---: | ---: | ---: |
| polars | 19.92 | 15.30 | 592.79 | — |
| combined | 18.89 | 13.67 | — | 604.02 |
| runtime | 6.23 | 5.79 | 240.35 | — |

The total free delta starts after the copy and includes inspection/control-path
creation as well as removal. The just-before-remove deltas are 1,722,867,712 bytes
for Polars and 11,493,376 for the runtime. Combined resume alone frees
1,644,924,928 bytes: its first large artifact was already unlinked before the kill.
Spawn-to-pause is 16.99 ms; pending inspection is 12.76 ms over 3,904 nodes and
1,631,956,480 logical bytes. These quantities are not conflated with whole-entry
removal or with the allocated estimate.


These are separate single-run observations on whole-directory copies of the real
gate-3 Polars, combined and installed-runtime entries. They are never executed.
No padding or explicit reflink is requested. Copying warms the filesystem cache;
there is no cold-cache claim. Node counts are checked against an independent
filesystem enumeration. The combined row is killed during unlink and resumed;
its spawn-to-pause and pending dry-run are separate fields in the raw result.

Free-space deltas use statvfs available blocks times fragment size. os.sync brackets
the readings outside the command timer. The filesystem is shared with the host,
so unrelated allocation can affect those deltas; they are not exact reclamation
promises. Lock inodes and retained control directories remain. Every source
entry's metadata and the original runtime selection stay unchanged. Raw reports,
conditions, before/after block readings and timings are in costs/.

## Ordinary launch

| Adapters | Before overhead ms | After overhead ms |
| ---: | ---: | ---: |
| 0 | 12.37 | 12.34 |
| 1 | 13.23 | 13.16 |
| 2 | 13.55 | 13.49 |
| 3 | 13.78 | 13.78 |

Each row is the median of six mode/repeat project-minus-direct medians. No
speedup is claimed; the matched results show no material launch regression.

| First-adapter cell | Repeat | Before increment ms | After increment ms |
| --- | ---: | ---: | ---: |
| run | 1 | 0.892 | 0.729 |
| eval | 1 | 0.830 | 0.772 |
| session | 1 | 0.803 | 1.016 |
| run | 2 | 0.855 | 0.947 |
| eval | 2 | 0.968 | 0.819 |
| session | 2 | 0.875 | 0.759 |

The 1.016 ms session cell remains visible; the other repeat is 0.759 ms. This
is the existing 0065 qualification, not a new threshold or a discarded sample.
One-native full verify medians are 55.36/55.52 ms before and 55.47/55.47 ms after.


The baseline binary is the accepted 0065 final-reuse tool, checked against its
archived SHA-256; its tool source is identical between `4855dbd` and `d7d8b0d`.
The final tool uses the same BLAKE3 locks and direct artifacts. The constant
443-file runtime and zero-to-three roster, including the full-size copied third
adapter, are the accepted measurement inputs. No relock changes those inputs.

There are 3000 retained samples across two interleaved repeats on one pinned CPU,
with one Polars thread, 30 samples per cell and output validation for every sample.
Run, eval and first prompt have matched direct controls, with verify separate.
All six first-adapter cells for both tools are retained in first-adapter.json.
The accepted 0065 noise qualification is not reopened or replaced by a new
absolute one-millisecond condition. Locks and receipts remain byte-identical.

## Regression and scope

Serial root suites: 376 default, 419 test-support and 438 combined-feature
tests pass, zero failures. Tool suites: 48 ordinary and 49 support pass, with
two designated integrations ignored in each suite and run separately below.

Root notices, a fresh default release/selfcheck, normalized default graph and
public docs pass. The source/dependency boundary is unchanged from d7d8b0d for
root code, Cargo files, notices, adapters, server and kernel. Tool strict clippy
passes across all targets in both configurations, notices are current, and the
ignored source-map and real repository inventory integrations pass.

Current-format replays pass for preparation/startup/commit, discovery/reopen,
runtime publication/repair, project workflow/cache publication, authenticated
migration/recovery and interactive contracts. The final binaries also pass the
42 removal filesystem cases, including six real mount shapes, and 52 command
contract groups. The final ordinary tool opens two new installed-default scratch
sessions with compilation trapped and positive controls, Polars and typed
PostgreSQL queries. All sessions and private clusters are reaped.

One new fixture trap initially classified Cargo's `rustc --print` target query
as compilation. The actual invocation and refusal are retained under
installed-trap-attempt. The trap now uses the same `--print` exclusion as the
accepted dogfood driver, with controls proving that compilation refuses while
metadata passes. Both installed journeys pass on rerun; no product change or
retry of a failing product assertion was needed.

Existing fixture adaptations are archived with source hashes, not silently applied
to accepted results. Root source.patch preserves the measured tree relative to
d7d8b0d, including the tracked-file set when applied with --index. Closing prose
is written only after build-dependent replays finish. No fixture processes or
mounts remain. Linux execution only; no new Windows claim.

## Closure

Removal now gives users explicit listing, named-project annotations, dry-run,
quiescent one-ID removal and pending-only resume. It makes no claim to discover
all projects, kernelspecs or live consumers. Old tools and surviving build children
must be stopped explicitly; a writer lock is not a lifetime lease. Selected
runtimes remain protected, current-format missing assemblies refuse launch until
explicit build, and future references can remain broken after intentional removal.

No automatic eviction or version-based sweep is introduced. Git-variable
whitelisting, path-depth cost and launcher-versus-runtime skew remain separate
follow-ups. Non-Linux maintenance refuses; Windows execution remains open.
