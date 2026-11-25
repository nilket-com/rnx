# Implementation plan 0335: The final's longest chain starts early

Status: Proposed

Paired ADR: ../adr/0335_the_finals_longest_chain_starts_early.md.

1. Boundary: published 5c853ce8 and the 0333 and 0334 final receipts.
   From the cache routes and both `SCHEDULE.json` files, confirm which
   recipes of `rune-suite` write `target/debug`, when the last such
   write happens, and whether stage teardown writes there. Record any
   deviation from the ADR's figures before editing.
2. Choose between releasing the hold and a separate directory, and
   state the expected finish with its assumptions before changing
   code.
3. Change the exclusion. Keep producer order, quiet for `fixed-route`
   and the wait evidence, and record the release in the receipt.
4. If a directory is added, measure its prepared size and candidate
   growth, then update the disk declaration, its catalogue control
   and both admissions.
5. Add the receipt check of the published schedule, the
   counterfactual and the planted violations. If the predicted finish
   exceeds 3,000 seconds, return with the bounding chain before
   signing.
6. Move the unsandboxed gap-contract children's result handoff to
   final-owned, memory-backed temporary storage with a stated bound
   and cleanup on every exit. Route the access sidecar through the
   child's temporary directory and publish it after the deadline.
   Add the pinning, stuck-child, sidecar and cleanup controls. Retain
   the attribution receipt's figures for Results.
7. fmt, Clippy, preflight, the controller controls and the targeted
   checks under the final's real recipe. Sign. Source review, one
   complete final, Results.

## Boundary

In both receipts only recipe 0 uses the shared debug directory. The
0334 outer test summary reports 17.83 seconds. Summing nested test
summaries counted some time twice in the plan's 30-second estimate.
The stage context and the completed recipe log are 90.73 seconds
apart. Cargo commits its journal, products and marks before returning.
The remaining recipes use engine-dev, the contract runner uses its
own build target, and the receipt hashes stage logs. The controller's
post-stage hook only handles the Polars oracle.

Keep both caches. Move rune-suite behind the initial commit/fetch
barrier and release the debug lane after its last routed recipe.
Before editing, the event model predicted about 2,600 seconds at the
recorded 13 tokens. It held recorded work durations fixed, drained
other holders for the exclusive contract, and kept fixed-route alone.

## Results

The recorded 0334 stage intervals cover 3,538.45 seconds of the
3,591.34-second clock. The remaining 52.89 seconds are carried into
every prediction. The counterfactual uses catalogue precedence, each
stage's recorded duration and admission width, the twelve measured
recipe intervals, and the exclusive contract's 92.25 seconds. At
exclusive acquisition it lets running stages finish before starting
more. Ordinary child borrowing and changes to build speed under a
different mix of work are not separately measured.

With 13 tokens the predicted finish is 2,943.8 seconds. The chain
runs through cross at the exclusive contract's drain, the remaining
debug writers and finally fixed-route. At 12, 11 and 9 available
tokens the projections are 2,746.8, 3,124.9 and 3,430.1 seconds.
Admission order changes which holders the exclusive case must drain.
This candidate rebuilds Rune test targets during the clock. Its final
gives a first partial measurement of the schedule on nano.

The live-controller control admits a debug peer during the suite and
keeps an engine-dev peer waiting until completion. A planted later
debug recipe delays release until its own writes finish. The replay
refuses an early debug writer, an engine-dev overlap after debug
release, and a consumer that starts before its producer finishes.
The existing cache directories and growth declarations suffice.
Both disk admissions passed on the retained store.

The complete final on e1671ed1 failed in rune-suite after 1,846.3
timed seconds and 77 checks. Its RESULT.json SHA-256 is
a09fc77a6014184ba56194d2ffa3244fe0c3f7347f0be630ed2efcb6c22a924e.
Stock row 82 of range-index hit its 10-second wall. In the focused
reproduction the working thread waited for ext4 journal access while
opening the child result file. The process used 40 ms of CPU. The
disk arm matched 308 observations before timing out. The RAM arm
matched all 640 observations, with a maximum of 112 ms. The original
final retained the timeout without a child stack.

Under overlapping builds the full 522-row comparison took 98.71 s
with scratch on disk and 99.02 s with only child scratch in RAM.
The slowest child took 8.670 s on disk, including 8.600 s in the
result-file open, and 0.162 s on RAM. The RAM arm's parent still
waited on a durable output write outside the child deadline.

Inside a final, each unsandboxed gap case mounts a private tmpfs
for TMPDIR. Its data limit is the existing 128 MiB per-writer
allowance. Used pages count against the final's memory cgroup.
Both disk admissions retain the existing scratch reservation.
The mount expires with the case and its process group. The mount
point is removed on success and failure. The existing stuck-child
control exercised the 10-second watchdog through the new route.
All 3,668 range-index evidence files matched the disk control.
The earlier TMPDIR-only candidate passed the bounds control in
11.03 s and range-index in 82.73 s under concurrent Polars and CLI
builds. It left access-error sidecars in the timed disk path.

The revised gap harness writes each optional sidecar beside the
temporary result. After the child exits, the parent copies the
sidecar to its original evidence filename. A blocked destination
lets both engines finish their observations before publication
fails. Reusing scratch after that failure produces no stale sidecar.
The rebuilt harness passed the bounds control and all 522 range
rows. A syscall trace across all 1,044 range children showed zero
writes to the evidence directory. Fourteen children opened the
temporary sidecar for writing. The parent published all fourteen.
All 3,668 files matched the disk control in name and bytes.
The existing bounds control now exercises the sidecar path. The
contract manifest, selectors and library test counts are unchanged.

The complete final on 4e79b6ea passed all 107 stages. Its
RESULT.json SHA-256 is
bd74a132b9f23f152785e522427295abe728cca86cd0f00b90eb08cb9d56b003.
Preparation restored the checkpoint after the failed final and
rebuilt it at the 5c853ce8 baseline in 3,718.0 seconds, outside the
clock. The timed final took 2,742.6 seconds of the 3,600, and the
total wait was 6,460.6 seconds. The Rune suite started at 79
seconds, released the debug target directory at 266 and ended at
1,981 after rebuilding the Rune test targets. Its contract batch
took 1,094.5 seconds while other stages built, against 522.4 in the
0334 final, and every contract passed. The fixed-route gate started
at 2,028 and ran for 673. In the 0334 final the suite and the gate
started at 1,769 and 2,907 and ran for 1,071 and 642.
The early disk check saw 315,821,834,240 bytes free against
237,412,866,722 needed. The check before the clock saw
203,966,820,352 against 124,670,046,263. All 32 per-record
comparisons passed exactly, and all 32 cumulative comparisons
passed. There were no memory.high, memory.max or OOM events, and the
host was in the performance profile. The developer cache snapshots
matched before and after the run, and the final added no temporary
entries. The candidate changes one Rune test source and no product
source. The next record's final is the first measurement of this
schedule with a product change.
