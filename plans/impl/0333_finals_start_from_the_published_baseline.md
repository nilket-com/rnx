# Implementation plan 0333: Finals start from the published baseline

Status: Implemented

Paired ADR: ../adr/0333_finals_start_from_the_published_baseline.md.

1. Boundary: published ee52b405 and the 0332 schedules and logs. Take
   the cache inventory in Control 1, including the bootstrap and
   stage-owned caches, and return any gap before editing.
2. Separate the canonical final lanes from pre-sign and branch state.
   Give the final its own dependency home and private invocation
   caches for stage children, and stage anchor-cache writes.
3. Add the baseline checkpoint and its preparation before the
   bootstrap: continuous exclusive ownership, the in-use mark before
   the first canonical write, restore or rebuild from baseline inputs
   alone, refusal of an unprovable lane, and its own report.
4. Keep the end-to-end clock from admission. Bind the baseline
   identity, preparation outcome and monotonic boundaries into the
   receipt, report the three durations, and record a time-only failure
   as the final.
5. Promote only a verified terminal publication-eligible PASS, by
   build inputs, including a docs-only successor.
6. Add the planted routes, the write-boundary control and the real
   preparation measurements.
7. Fix the competing-client scheduling fixture and add its regression
   control and loop.
8. Refuse a disk shortfall before preparation and before the clock,
   with its planted control. Report the 16b0a07a final in Results.
9. fmt, Clippy, preflight and the targeted checks in separate state.
   Sign. Source review, one complete final, Results.

## Results

The published ee52b405 catalogue had 103 Cargo recipes in 36 sharing
groups. The checkpoint holds those lanes, the bootstrap target, a
final-owned Cargo home seeded from the published locks, and the
one-entry fixed-route anchor. Stage product caches and the temporary
home start empty. On promotion the final fetches the published locks
into a clean home and rebuilds the bootstrap to exclude obsolete units.
An interrupted or foreign generation is rebuilt in place.

At the same physical paths the real baseline took 4,520.89 seconds
to prepare cold and 4,380.57 seconds to prepare after a foreign Cargo
compile and plants in the other three classes. Both stopped at the
timed handoff. All four class identities matched, and all plants were
absent after recovery. The stores occupied 112,087,687,168 and
112,087,556,096 bytes, respectively. Peak allocations sampled at build
boundaries were 112,714,895,360 and 112,714,752,000 bytes.

An exploratory run was discarded after its input catalogue changed
while it was running. A subsequent frozen cold treatment stopped before
admission because the archived xtask build queried its enclosing Git
checkout. The published lint command passed when its source export
was placed under the final-owned checkout. The successful cold and
foreign treatments both used that layout and one frozen catalogue.

The env and time file-launch fixtures passed with private cache paths.
Their syscall traces recorded no successful opens in the developer's
Cargo or product caches. In the separate passwd-fallback control,
clearing the home variables placed all three bundle files in the
masked cache. Clippy, the controller controls, preflight and replay
of the historical 0332 receipt passed.

Review found that the desktop indexer writes to tracker3 under the
same XDG cache directory. The write check now compares sorted metadata
manifests for the Cargo homes, the rnx and rnx-file-images directories
under the applicable cache homes, and any explicit project cache. On
a mismatch the receipt directory retains the differing path names.
Settle passed against nano's actual Cargo and two product cache roots.
The planted control accepted an indexer write and reported the planted
file in each protected root. The first controller suite run timed out
in the competing-client scheduling fixture. The same test in isolation
and the entire suite on the same code then passed.

A scheduling trace caught the competitor taking the gate's token
before admission. The same order reproduced the timeout on published
ee52b405. The fixture now forces the competitor's early turn and
waits for gate entry before reading the token. Removing that wait
alone reproduced the original 20-second timeout. With it, 128 runs
of the competing-client test passed in batches of 16 on four logical
CPUs shared with four CPU load workers. The waiter was refused while
the competitor held the token and started after its release, before
the returner ended.

The complete final on 16b0a07a failed disk admission before its first
stage. Cold preparation took 3,744.5 seconds and reached 112,738,422,784
allocated bytes. Free space fell from 229,807,611,904 to 117,348,175,872
bytes. At 56.9 seconds on the candidate clock, the controller required
124,669,975,202 bytes and saw 117,976,432,640. Zero checks ran.
The home snapshots matched and the run left no new temporary entries.

The source-only admission reads the candidate's declared stage count,
bulk demands and recipe changes before compiling it. A control regenerates
that declaration from the Rust catalogue. The cold estimate rounds the
sampled peak up to a GiB. The v1 lane estimate is the first admission
less its reserve, bulk and 13 temporary allowances. The entry charges
extra cold lanes for catalogue changes. Only a validated promotion gets
credit for retained lanes, capped at the 110,403,301,376 bytes measured
in the v1 lane root. The check after preparation reads actual marks,
allocated blocks, apparent sizes and device identities.

At 13 slots on the v1 device layout the early check needed
237,412,866,722 bytes. Replaying the retained prepared lanes reproduced
the controller's 124,669,975,202 bytes exactly. Planted shortfalls
produced preparation refusals with null admission, zero timed seconds
and no final receipt. With sufficient space the same entry crossed the
clock boundary.

The complete final on 1985f5aa passed all 107 stages. Preparation
restored the checkpoint from the baseline's inputs in 3,737.6 seconds,
outside the clock. The timed final took 3,140.4 seconds of the 3,600,
and the total wait was 6,878.1 seconds. The five passing complete
finals from 0327 through 0331 took 2,275 to 3,304 seconds. The early
disk check saw 341,631,582,208 bytes free against 237,278,648,994
needed. The check before the clock saw 229,798,027,264 against
124,669,975,201. All 32
per-record comparisons matched the base exactly and all 32 cumulative
comparisons passed. There were no memory.high, memory.max or OOM
events, and the host was in the performance profile. The developer
cache snapshots matched before and after the run, and the final added
no owned entries to /tmp or /var/tmp.
