# Implementation plan 0336: Result callbacks run on the new engine

Status: Proposed

Paired ADR: ../adr/0336_result_callbacks_run_on_the_new_engine.md.

1. Boundary: the published base. The replay of all 26 cases in both
   native modes found 52 refusals at the recorded categories and
   spans. Record any deviation before editing.
2. Certify the two providers by identity. Prove the receiver, the
   callback and the `and_then` return as Decision 3 requires. Lower
   each admitted call with the existing variant test, extraction and
   construction and an ordinary closure call. The step 2 probe found
   14 ownership behaviors of the old core; Decision 3 governs them.
3. Add the parity table, the ownership observations and the refusal
   controls. Update the contract rows and the affected inventory
   witnesses.
4. Replay the six cases in both native modes from full source, and
   confirm the other 20 keep their boundary refusals.
5. Score the file gate and both project screens before signing. Check
   every delta against its band.
6. Repair the live memory control's fixture as Decision 7 states and
   add its controls. The first complete final failed there with the
   unit's cache 2,281,472 bytes short of the line; the candidate's
   own change doesn't reach that code.
7. Repair the cancellation test as Decision 8 states and add its
   controls. The second complete final failed there: the test's child
   took 8.51 s and the suite 36.13 s, against about 15 s in the two
   finals before, while nine other stages ran. Under load on ext4 the
   assertion reproduced with a 1.4 s drain and a 4.6 s `fsync`; on
   tmpfs 24 of 24 runs passed. The final's stage temporaries are on
   ext4. The final recorded no phase times, so its own split is
   unknown. The candidate's own change doesn't reach that code.
8. fmt, Clippy, preflight and the targeted checks in separate state.
   Sign. Source review, one complete final, Results. This final is
   the first with a product change under the 0335 schedule. Report
   when the Rune suite, its contract batch and the fixed-route gate
   started and how long each ran.

## Results

The six full-source entries ran with fallback disabled and matched
the published runner in both native modes. The other 20 kept their
first refusal and span. All 112 replay observations passed, including
the 14 ownership probes and six closure-exit probes in both modes.
The iterator-adapter contract passed all 1,134 rows.

The compiler admits a certified standard method when the receiver
ends in a one-argument standard Ok or Err constructor and the
argument ends in an immediate closure literal from this arena.
Parentheses and unlabeled block tails can wrap either expression.
For and_then, every normal exit must build its own standard Result.
The proof follows complete if/else tails and checks explicit return
operands. It recognizes panic and an empty infinite loop as
nonreturning. Stored receivers, named callbacks, and_then returns
of existing values and exits the proof cannot establish refuse at
the call. The receiver and callback expression each run once, in
that order, on both variants. Only Ok calls the closure body.

The compiler appends each closure's exit fact in arena order and
clears the facts when it changes arenas. Each callback lookup
compares at most one hash per recorded closure in that arena.
For C recorded closures and M candidate callback sites in one
arena, the table takes O(C) space and at most M times C hash
comparisons during lowering. Execution performs no such lookup.
An unknown closure identity refuses at the method call.

Two earlier unsigned snapshots exceeded the native-on shared_rows
setup limit of 11 instructions. The first added 84 instructions.
Checked HIR access and arena-local facts removed its seven extra
relocations. Sorting the facts then introduced two relocations for
specialized sorting routines and cost 24 instructions. The final
lookup uses a linear scan. Both installed images have 31,573 relative
relocations, with matching GOT and data.rel.ro target inventories.
The shared_rows setup count matched at 871,577 instructions for
both sizes. All 32 per-record and 32 cumulative file comparisons
passed. All 128 comparisons for the empty and adapter-pair projects
had zero delta. Formatting and Rune Clippy passed.

The first complete final on ff351243 failed in controller-controls.
Its RESULT.json SHA-256 is
a13b513e5bd577825a14a9e431b46bdcd9ef035db1246ef5865906553b0058be.
After the fixture synced its 2 GiB file, the unit had 4,297,248,768
bytes of raw headroom, 2,281,472 bytes above the 4 GiB boundary.
Cache-aware accounting admitted one worker. The fixture failed
before it tested anonymous memory. Preparation took 2,276.7 s and
the timed run took 1,128.6 s. The Rune suite ran from 29.5 s to
1,118.4 s and released the debug target at 122.9 s.

Under a concurrent Rune compiler, kernel counters showed reclaim
in the fixture's own unit. That run retained enough cache to pass.
Evicting pages from just the fixture file after fsync reproduced
the first final's failure. The final recorded the insufficient
charge. Its observations cannot identify what reclaimed its pages.

The successor has the same Rune tree as ff351243,
d7d2392f9be5aa84663fc5c354cbf88b2be75eeb. The file gate, both
project screens and the 112 replay observations carry over.

The repaired fixture passed the same eviction control under a Rune
compiler. The first sample showed 4,528,259,072 bytes of raw
headroom. One synced 512 MiB addition brought raw headroom to
3,976,151,040 bytes. The fixture scored that sample once and
admitted one worker. Held anonymous memory then refused with zero
workers under the same unit's memory.high. All four provisioning
controls and all 236 controller-controls tests passed.

The second complete final on 2f2717f6 failed in project-suite.
Its RESULT.json SHA-256 is
4d49c056c1bd25f2fbd95dfdac82864484002cd577a40fb898fb5ac801a3645e.
Restored preparation took 3,932.2 s. The timed run recorded 86
checks in 2,041.9 s. Project-suite ran from 1,793.1 s to 1,902.4 s.
Its library tests took 36.13 s. The isolated cancellation test's
child took 8.51 s and failed the five-second limit around finish.
The stage used ext4 and recorded no timings for the individual
operations in finish.

Under concurrent CPU and disk writes, the same assertion failed
once in 24 ext4 probes. All 24 matched tmpfs probes passed. In
the failing probe the drain reached EOF in 1.363 s and the log's
fsync took another 4.629 s. The report contained the flood output.
The test thread accumulated 3.68 ms of runnable scheduling delay.
Delaying only fsync by 5.5 s also reproduced the assertion, with
a successful 44-microsecond drain. Across six loaded runs of the
original fixture, all 109 active project library tests, all 236
Python controller tests and all four measurement deadline tests
passed each time.

The repaired test passed with a successful fsync delayed to 5.513 s.
Holding a duplicate pipe writer through finish failed the EOF check.
Restoring stdout or stderr to the other stream failed the respective
descriptor identity check. The project suite (109 active library
tests), Python controller suite (236 tests) and measurement deadline
controls (four tests) each passed three runs under concurrent CPU
and file-write load. The production capture code, the lingering-writer
test, the Rune tree and the Decision 7 fixture match 2f2717f6.

The complete final on fffee6cf passed all 107 stages. Its
RESULT.json SHA-256 is
7c0f3f8c1a402f120629476004beca2eea539292c240feaed0e1f0fba11848c1.
Preparation restored the checkpoint after the failed final and
rebuilt it at the 6cefc349 baseline in 3,850.9 seconds, outside the
clock. The timed final took 2,801.8 seconds of the 3,600, and the
total wait was 6,652.8 seconds. The Rune suite started at 30
seconds, released the debug target directory at 227 and ended at
1,781. Its contract batch took 845.1 seconds, and every contract
passed. The fixed-route gate started at 2,122 and ran for 641. In
the 0335 final, which rebuilt only test targets, the suite ran from
79 to 1,981 with a 1,094.5-second contract batch, and the gate
started at 2,028 and ran for 673. Project-suite ran from 1,773 to
1,863 beside six other stages, and its library tests took 15.65
seconds. The controller controls passed with the Decision 7
fixture. The early disk check saw 279,366,062,080 bytes free
against 237,278,648,994 needed. The check before the clock saw
167,510,409,216 against 124,670,046,324. All 32 per-record
comparisons passed. Twenty-five were exact. Seven native-on T rows
moved by the same amount at both sizes, from -4,709 to +1,452
instructions, inside their bands. The shared_rows rows were exact.
All 32 cumulative comparisons passed. There were no memory.high,
memory.max or OOM events, and the host was in the performance
profile. The developer cache snapshots matched before and after the
run, and the final added no temporary entries.
