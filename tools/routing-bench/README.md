# 0197 actual CLI benchmark

Run only after fresh correctness acceptance, on the measurement host under an exclusive lease, with no competing work.
Use an exact-source release build and record its build command, duration, binary
size and SHA256 separately. BUILD.json must bind `commit` and
`binary.sha256` to that exact source and executable, alongside the raw build
command/log and duration. The controller requires committed, clean rnx and
Rune sources matching the rnx pin. It holds the exclusive measurement lock for
the entire finite run, pins to the specified CPU, and rejects a busy host or
unavailable/multiplexed counters. The existing Rune readiness collector checks
competing tools and the thermal counter at the run boundaries. Dynamic
libraries, performance governor and perf configuration are pinned and checked.
Launch it in the detached operator unit with
the approved cgroup memory limit; block on exit and read RESULT.json once.

```
python3 -B tools/routing-bench/run.py \
	--rnx /absolute/exact-source/rnx --luajit /absolute/luajit \
	--rune-root /absolute/pinned/rune --build-receipt /absolute/BUILD.json \
	--cpu 2 --out /absolute/new/output
```

All four modes run the actual command: forced legacy, execution/native off,
execution/native on, and LuaJIT with its normal JIT enabled. Rune commands receive the largest valid finite CLI budget (`usize::MAX-1`);
the unlimited sentinel stays refused. Script output is
compared live, including each calibration and repetition; no observed expected
output is stored. A separate debug-engine invocation must prove admission of
every execution kernel. The regular actual-CLI correctness gate also runs
these exact committed kernel bodies at one round in all three rnx modes.
There is no runtime retry or timing of a fallback as
though it were execution.

The six kernel pairs cover recursive fib, loops, numeric calls, shallow shared
rows, and admitted AWFY Sieve and Permute. AWFY source provenance and notices
are adjacent. Both AWFY implementations additionally check the algorithm's
published integer result on every inner invocation. Lua tables use explicit
zero-based indices where AWFY's Rune arrays do; shared_rows makes a shallow
copy and mutates the same inner row in both languages.

Inputs cycle through 1..97. Numeric intermediates stay below 2^53; checksum
addition is bounded by 1,000,000,007 plus the kernel result, below 1,021,000,000.
The numeric-call multiply/add is below 18 million, shared_rows' encoding below
20 million, and AWFY returns 669 or 8660. Round counts are limited to 2^31.
These bounds make Rune's signed integers and LuaJIT's numeric arithmetic exact
for the operations performed. The checksum is printed as an integer in both.

Full-source zero-round controls measure process and compilation startup for
each kernel and mode. Calibration gives every mode the same work until the
fastest exceeds 0.5 seconds and twenty times the largest startup median. Each
of five seeded interleaved repetitions must independently satisfy the duration
and <5% startup checks. Raw streams, exit statuses, schedules, perf counters,
wall times and source/binary identities remain in the output tree. No progress
watcher or polling is used.

Instructions and user cycles use Rune's existing B1 parser, event table and
five-repeat cycle soft rule from the exact pinned source. Counter data must be
unscaled and fully scheduled. Reports include medians, full samples, spreads
and ratios to LuaJIT, plus native-on versus native-off cycle flags. The soft
threshold is 1 + max(2*reference spread, 0.02). A performance difference does
not fail correctness; unavailable evidence or violated calibration does. The native runtime default is requested on as of 0198; this controller sets
off/on explicitly for each measured invocation. Its route evidence records
requested policy, not proof that a region was entered. Region coverage needs
separate operational evidence.

The controller's fail-closed protocol controls run with
`python3 -B -m unittest discover -s tools/routing-bench -p test_protocol.py`.
They reject fallback route evidence, the wrong native mode, terminal failures,
unexpected diagnostics, changed checksums and individual short or startup-heavy
samples before any benchmark can count them.
