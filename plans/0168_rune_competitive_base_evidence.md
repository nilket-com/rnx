# rnx 0168: Rune base decision — evidence

**Status:** closed; accepted by Claude after independent checks. This record changes no rnx source,
shipped dependency, fork base or public upstream issue. Measurements and tooling
are in rnx-bench `probes/rune-base-0168` and `results/rune-base-0168`.

## 1. Decision

Choose **upstream main bb8e69372353c50e271c9f115bc771c77aa6b83e** as the
isolated engine-development base for the competitiveness campaign. Keep shipping
**Rune 0.14.2** until a separately reviewed migration meets the product gates.

This is a foundation choice, not a speed upgrade. Main is materially slower on
these workloads and executes more instructions. It handles deep source and
nested data that abort 0.14.2, and contains the heap-stack architecture and the
subsequent compiler/formatter maintenance. Starting the engine work on main
avoids immediately reimplementing those changes on an older branch. That is an
engineering judgement weighed against the measured deficit; no experiment here
proves that main is easier to optimize or that the campaign will reach its goals.

The next engine changes should target complete standard-module registration and
then profile VM dispatch/value work on main, always using 0.14.2 as a retained
performance comparison. A focused fork record may expose private module phases
for measurement without omitting them. Adoption is a separate core/adapter
migration, not a prerequisite for experimenting with the engine. A JIT decision
comes after attribution; this record establishes no JIT feasibility or schedule.

## 2. Fresh invocation and VM work

Same stable toolchain, release opt-level 3, no LTO or CPU-specific flags; full
standard contexts. The engine harnesses normalize only Arc construction and the
async result API. Main additionally needs `anyhow` for fmt. Locks retain all
package differences. These are engine harnesses, not a replacement measurement
of the stock rnx executable.

| Whole-process source-to-answer / phase endpoint | 0.14.2 median ms | main median ms | main / old |
|---|---:|---:|---:|
| Process floor | 0.478 | 0.480 | 1.003 |
| Empty Context | 0.481 | 0.484 | 1.006 |
| Full default Context construction/drop | 3.412 | 3.643 | 1.068 |
| Context plus runtime extraction/drop | 3.626 | 3.892 | 1.073 |
| Compile print-42 (no execution) | 3.715 | 4.280 | 1.152 |
| Empty script execution | 3.676 | 3.961 | 1.078 |
| Print 42 | 3.732 | 4.294 | 1.150 |
| Numeric modulo loop, 1M iterations | 83.826 | 125.059 | 1.492 |
| Strings/vector, 20k items | 14.377 | 19.257 | 1.339 |
| Recursive fib(27) | 38.561 | 53.347 | 1.383 |
| Late, separate unpinned print-42 control | 10.976 | 12.815 | 1.168 |

90 samples per fast endpoint, 15 per compute endpoint, 30 per unpinned endpoint:
1,410 total, none removed. Three shuffled rounds; every successful invocation
checks exact stdout, status and empty stderr against independent Python fixture
output. Min/max, p10/p90 and every raw sample are retained. Large outliers remain
(e.g. main runtime max 11.05 ms). Pin logical P-core 4 on an i7-14700; unpinned
is later, not randomized alongside pinned. The machine survey taken afterwards
reports intel_pstate/powersave; this is not a trace of each sample's frequency.
The native spawn/capture/blocking-wait clock passed the predeclared <=0.15 ms
median agreement with contemporaneous hyperfine -N on true and cached Rust 42.
No overhead subtraction.

Separate FIFO-bracketed core PMU captures (three repeats; allocation tracking
absent) also show more work, not just different elapsed time:

| Mode | old median instructions | main median instructions | main / old |
|---|---:|---:|---:|
| Full Context | 25,318,055 | 26,416,461 | 1.043 |
| Compile 42 | 27,037,195 | 29,450,662 | 1.089 |
| Run 42 | 27,043,148 | 29,457,250 | 1.089 |
| Numeric | 1,378,164,363 | 1,756,638,881 | 1.275 |
| Strings/vector | 191,619,340 | 227,267,179 | 1.186 |
| Fib | 679,040,402 | 830,599,527 | 1.223 |

These count all work inside the window, including native printing; they are not
VM-only instruction counts. Cycles and raw perf JSON are retained. All 90 windows
have positive runtime and >=99% event running. Controller core 0, worker core 4.
Instrumentation is excluded from deciding wall times. Analyse validates counts
and output for the counter, allocation and RSS passes too.

Compile-once reused VM call means over 20 invocations (three process replicates):
old numeric 80.64/80.67/80.94 ms; main 120.70/120.73/120.91 ms. Fib old
34.85/34.86/34.79 ms; main 48.73/48.87/48.90 ms. Thus registration alone does
not explain the compute gap. Reused print means are 5–8 microseconds, including
println; this is not fresh startup or a cross-request ownership test. The
harness constructs an unused empty fresh VM outside each call clock; this is
included in process/counter evidence, not in the call clock.

## 3. Registration and allocation

Full default Context on the allocation-only binary:

| Metric | 0.14.2 | main |
|---|---:|---:|
| Allocation/reallocation calls | 30,067 | 32,304 |
| Cumulative allocated bytes (realloc counts new size) | 3,520,031 | 3,562,349 |
| Peak live bytes | 1,842,107 | 1,767,837 |
| Live after context drop | 548 | 548 |

A separate **public-module subset**, 30 old / 31 main modules, measures
construct/install/drop by module. Private hash_map/hash_set/vec_deque cannot be
called from the harness; they remain included in all full-context deciding
measurements. Main additionally has f64::consts. This breakdown is explicitly
incomplete and cannot replace a full registration census.

Subset construct sums: old 0.229–0.249 ms, main 0.255–0.308 ms; installation
sums old 1.868–1.910 ms, main 1.966–2.048 ms; context drop old 0.272–0.275 ms,
main 0.341–0.357 ms. Median install leaders: iter 0.550/0.579 ms, ops
0.404/0.481 ms, string 0.189/0.193 ms (old/main). This points toward registry
installation, not module value construction, as a useful next investigation;
it does not prove a cache or prebuilt registry is safe or sufficient for sub-ms.

Allocation snapshots, cumulative bytes, peaks and process maximum RSS for all
15 modes per base are retained separately. Registration mode resets allocation
counters at each mark; its final ALLOC is **not** a full-process registration
allocation count. Full-context figures above use the dedicated context mode.
Run-42 peak live bytes old/main 2,261,012 / 2,614,871. No unrelated medians are
subtracted to manufacture a phase duration. Phase differences use cumulative
marks inside the same invocation; module/drop clocks are direct intervals.

## 4. Correctness and robustness

The fixed corpus passes 26 rows: exact outputs for integer boundaries, floats,
Unicode, alias mutation (vector/object), closure, generic iterator sum and async;
same semantic error classes for untyped sum (MissingInstanceFunction), integer
overflow, division by zero, missing local and budget exhaustion. Error debug
layouts/addresses differ; this is semantic parity on a small corpus, not exact
diagnostic text parity or complete language equivalence. The five timed fixtures
also produce the same independently computed output. Compute budget is 1e9 on
both; the separate 100-instruction control refuses the infinite loop.

Bounded child probes (2 GiB address space, 5/6 s CPU soft/hard, 10 s wall,
core dumps disabled):

| Control | 0.14.2 | main |
|---|---|---|
| Script recursion 10k, 100k, 1M | Correct at each | Correct at each |
| Parenthesized source depth 100, 1k | Correct | Correct |
| Parenthesized source depth 10k | SIGABRT, native stack overflow | Correct |
| Nested vector literal depth 100, 1k | Correct | Correct |
| Nested vector literal depth 10k | SIGABRT, native stack overflow | Correct |

Sources, stdout, signals and stderr are retained. Aborts are not clean errors.
No inference about depths above these controls or all adversarial input is made.
The deep-source difference weighs heavily toward main as the development base.

## 5. Migration and trajectory

Scratch root/project pin update from rnx 4fbbd3b fails the first all-targets
check with test-support/server-runtime: 23 lib / 30 lib-test errors (duplicated
sites included). Arcs, diagnostic access, SourceLoader and native results change;
resume becomes a Future/complete builder and async_resume disappears. Public
replacement diagnostic accessors exist. Their port and resumed execution still
need retained-origin and budget/cancellation ownership audits. No source repairs
were attempted after the capture: that boundary already prices a dedicated
migration record. The four-hour limit is a maximum, not a mandate to spend it.

`PORT.md`, original diagnostics, manifest/lock patch and consumer-census.json
cover direct consumers and gates. Core 36 source files refer to rune directly;
Polars 57 (including generated bindings), Candle 23, Postgres 2, generator 19,
project 1. These are source file counts, not bindings. Jupyter depends on the
worker rather than direct Rune paths. **Main rnx, adapter, project and Jupyter
suites are blocked/not run**, not credited by engine harness correctness.
Estimated migration scope: core runtime/API record, adapter/generator compatibility
record, and integration validation; duration/downstream failures remain unknown.

Baseline root release/test-support/server-runtime suite: 523 passed, 0 failed,
3 ignored across 51 summaries including docs. Standalone adapter/project/Jupyter
baseline suites were not separately rerun in this record.

0.14.2-to-main: 158 commits, multi-area path incidences (a commit may count in
several areas): compiler 58, VM 55, fmt 13, LSP 9, modules 23, alloc 26, deps 21,
tests 66, other 105. Rules and all changed paths are retained in trajectory.py
and trajectory.json. Important anchors:
- 7e5e3ab1, "Avoid recursive stack use", includes v2 compiler/runtime work;
- 20b26957, configurable indentation and LSP formatting options;
- e9ae73fb template literal corruption, f0f6884f token separation, 5d8e6338 stray
  hash loop, 4adde93a trailing whitespace and b15bfa6f label colon formatter fixes.

Pinned main's last commit is 2026-08-29, "Fix nightly build". crates.io's retained
survey still reports 0.14.2 as max stable; the manifest's 0.15.0 is unreleased.
Upstream HEAD was surveyed separately and never substituted for the pinned base.
The count/classification is maintenance evidence, not a throughput forecast.

## 6. Reproduction, repairs and limits

README/build.sh describe exact independent binaries and command order. analyse.py
fails closed on sample identities/counts, clock gate, counter windows/output,
control parity and the measured Rust/fixture/manifest/lock source hashes. Both
locks and binary hashes are retained. Initial setup failures are saved: old
async VmResult adapter, private-module access, float printed form, untyped sum,
and one accidental baseline check in the wrong worktree. Correctness was later
rechecked explicitly using primary with stronger error-class assertions; earlier
captures are retained. No deciding timed Rust source changed afterwards.

Build logs are not an apples-to-apples clean-build timing: initial old failed,
initial main built in 40.26 s, final harness rebuilds were cached ~0.7–0.8 s.
This is disclosed rather than credited as engine compile superiority. Tooling
Python syntax, build.sh shell syntax and analysis replay pass. No extra product
unit tests were added for this evidence-only record.

The results establish current costs and bounded robustness differences on one
machine and a small corpus. They neither demonstrate sub-ms full-capability
startup nor imply that LuaJIT-level execution is attainable. That larger mission
remains authorized and active; the next changes must earn improvements while
preserving these capabilities and the rnx contracts.

## 7. Independent review and closure

Claude accepted the base decision and evidence. Independent taskset-core-4
hyperfine -N checks (2 warmups, 10 runs) on the same primary binaries: old/main
fib 38.8/53.2 ms, numeric 84.0/125.7 ms, answer 3.7/4.3 ms; approximately
0.5% agreement with the deciding medians. Claude reproduced old expression-10k
exit 134 with stack-overflow stderr versus main output 1 and exit 0, and fib
196418 on both. The jointly agreed long-term roadmap is plans/rune_roadmap.md.

After review, analysis validation was strengthened to bind raw PMU values and
unique counter/allocation/RSS identities to retained aggregates; build.sh also
refuses tracked dirty fork files. These change tooling validation only: replay
passes, all measured sources/binaries/results remain unchanged.
