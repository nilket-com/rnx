# Implementation plan 0329: Fixed-route measures fixed random and clock inputs

Status: Implemented

Paired ADR: ../adr/0329_fixed_route_measures_fixed_random_and_clock_inputs.md.

1. Boundary: published 4f0195be and the v3 attribution receipts. Run
   the input census (Decision 3) and report it, including the seed
   and clock paths on supported historical images. Resolve any
   potentially count-affecting input or stop.
2. Add the seed hook and the clock supply to the observer, with
   per-thread accounting and capture invalidation.
3. Freeze the method before scoring. Bind it into capture keys and
   exercise rejection of an old-method receipt. Inventory the policy's
   execution anchor (48fe3b15), native setup (ff5bf81e), installed
   setup (the 0324 freeze on ed6b1a07), and the later outer reference
   (5530079d). For each applicable quantity
   record its original revision and artifact hashes, measure the new
   method on the same artifacts, and report its transition delta. If a
   pinned artifact is unavailable, report that before substituting a
   rebuild. Preserve historical receipts and band limits.
4. Add the repeatability, second-seed, sensitivity, negative and
   planted-change controls.
5. fmt, Clippy, preflight and the targeted checks under the final's
   real recipe. Sign. Source review, one complete final, Results.

## Results

The frozen observer measured the original binaries for 48fe3b15
(execution), ff5bf81e (native setup), ed6b1a07 (installed setup)
and 5530079d (outer reference). Across eight fixtures, two sizes,
both native modes, two seeds and two launches, all 512 captures
passed. Every repeated per-function instruction vector matched,
including libc. All 84 reference comparisons had zero delta in both
work and total instructions. The measured launch identity changed
when Valgrind output moved to its own file.

The loader cache is a measured input. Adding two unrelated entries
cost 224 instructions in the dynamic loader. The production observer
binds its reviewed bytes and checks the loaded libraries and profiler
preload. Running the whole window at PID 99 and the separate query at
100 gave a four-instruction discrepancy. Private namespaces at PID 2
reproduced the inclusive query exactly. The allocator-path proof
observes glibc's successful entropy read and accepted branch. The real
entropy read remains charged.

The control seed changed all three derived constant hashes and kept the
program checksum. The original +11 collision was measured separately
by forcing the constant-map keys. A planted 7,170 execution instructions
failed both work and total limits with the meter enabled. The observer
also rejected a missing seed hook, the wrong seed owner, a clock reset,
and a substitute loader cache. The frozen method and reference deltas
are recorded in target/0329/method-v3/FREEZE.json and TRANSITIONS.json.

The standalone fixed-route stage on the clean implementation snapshot
3a896aae passed all 32 per-record and all 32 cumulative rows.
Formatting, Clippy, observer, gate and repository preflight controls
passed. PRESIGN.json binds the stage and reference receipts.

The complete final on fd017382 passed all 107 stages, with 53.6 minutes
of accepted run and 54.1 minutes end to end. Both fixed-route bands
held. There were no memory.high, memory.max or OOM events; the cgroup
recorded two sock_throttled events. The host was in the performance
profile.
