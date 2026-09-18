# 0065 gate 5: product port and a measured floor stop

Status: **stopped for review**, as decision 6 requires. Gate 5 has not passed and
gate 6 has not started. The source port and all measurements are preserved; no
post-measurement optimization or threshold change is substituted for this result.

## Product port and correctness

The accepted gate-4 candidate is now selected by inventory::native on Unix.
Other platforms retain independent per-root inventory, with no new execution
claim. The same combined Git discovery, boundary/index/file checks, 4096-entry
descendant budget, fallback policy, tree framing and duplicate logical accounting
are retained. No format or dependency changes, global Git cache, persistent source
cache, or root runtime changes are introduced.

Trace counters and pause hooks compile only with test-support, and the existing
private assembly probe gains a nested-inventory arm. No ordinary CLI option is
added. The port additionally guards the reader completion hook when no multi-root
inventory is active, so ordinary artifact and installer reads cannot populate
otherwise inactive observation storage. The invocation guard clears storage on
success and failure, as the accepted candidate did.

The product private probe passes all 36 gate-4 primary cases and all 16 additional
topology/roster cases, with assertions unchanged. It reproduces exact quiescent
trees/refusals, logical allowances, one read per overlapping file, independent
fallback counts and the documented restored-metadata observation-window difference.
The existing real 443-file runtime and complete third adapter remain the roster.

A new product test executes two inventories in the same process, checks identical
read/Git events both times, changes content and sees both trees change, then checks
that a failed call also leaves no retained observation. The first test attempt
inherited GIT_PAGER=cat from the runner and correctly took the independent path;
the fixture now spawns the same exact test with Git overrides removed, without
mutating the test process's environment. Its failed log is retained as a fixture
correction, not a product failure or discarded performance sample.

Tool formatting, strict all-target clippy in both configurations, 46 default tests
and 47 test-support tests pass, with two pre-existing ignored tests in each suite.
Notices are current. Root source/Cargo, kernel, adapters, server and the tool's
Cargo files are unchanged. Gate 6's full regression remains outstanding.

## Matched headline measurement

Sources: `probes/nested-product`; results: `results/nested-product-0065`.
The ordinary, uninstrumented release executable is frozen before measurement.
Three products are interleaved against their respective direct artifacts:
7cd3205 SHA-256 baseline, accepted BLAKE3 format-only/F1 tool, and this reuse port.
All use the same accepted constant source roster and existing real locked builds.
The middle binary's digest is checked against gate 4's saved provenance.

There are 75 cells, two repeats, 30 observations per cell: all **4,500** samples
are retained. One CPU, one Polars thread, pinned terminal dimensions, fixed seed,
output validation and two warmups per cell follow the existing measurement.
Ordinary run/eval clocks span spawn through exit; the session clock ends at the
full first prompt, with quit/reap outside the interval. No builds run concurrently.

Eval overhead above each direct artifact, milliseconds:

| Adapters | Baseline, repeats | Format-only, repeats | Reuse port, repeats |
| --- | ---: | ---: | ---: |
| 0 | 17.63 / 17.52 | 12.28 / 12.16 | 16.87 / 16.92 |
| 1 | 22.41 / 22.53 | 16.27 / 16.21 | 17.72 / 17.66 |
| 2 | 26.57 / 26.56 | 20.09 / 19.95 | 18.15 / 18.16 |
| 3 | 30.58 / 30.69 | 23.72 / 23.75 | 18.74 / 18.72 |

Run and first-prompt rows reproduce the same pattern; full medians, successive
increments and fitted slopes are in summary.json. Every successive adapter
increment is under 1 ms in both repeats of every mode (approximately 0.43–0.88 ms).
Every count improves over the SHA-256 baseline. **The zero-adapter requirement
fails in all six mode/repeat combinations:** improvement is only about 0.60–0.80 ms,
not the required 3 ms. gate.json lists those six failures explicitly.

Compared with the accepted format-only tool, the port adds roughly 4.6–4.8 ms at
zero adapters and is also slower at one adapter; savings appear at two and three.
The floor still contains the entire 443-file, 6,988,177-byte runtime. The code
collects its boundary/file observations even when no child exists to reuse them.
That is the clear next place to investigate, but this measurement does not isolate
each metadata lookup, clone or Git-discovery cost. No precise phase attribution
or fix is claimed from this end-to-end difference alone.

One-native full --verify totals are separate: baseline 95.38 / 95.35 ms,
format-only 58.62 / 58.70 ms, reuse 59.94 / 59.94 ms. They do not enter the everyday
gate. The port has not changed artifact hashing or the receipt stamp policy.

## Explicit fallback cost

After the headline failure, no implementation changed. The specifically requested
fallback control retained another 360 samples: two interleaved repeats of 30
project/direct eval pairs for three-adapter shapes. The shapes are measured in
separate blocks, rather than presented as an interleaved shape comparison.

| Three-adapter shape | Over direct, repeats |
| --- | ---: |
| Eligible, clean | 18.87 / 18.81 ms |
| 4097 ignored files under the copied adapter's target directory | 23.20 / 23.21 ms |
| GIT_PAGER=cat forces independent inventory for all roots | 29.28 / 29.11 ms |

The discovery budget is 4096 entries. Its ignored target directory deliberately
exhausts that proof budget and restores the independent path for that adapter.
The fixture confirms Git ignores the directory, then removes only its own newly
created directory. Lock pair and receipt stay byte-identical. This is the stale
build-directory cost the review requested, not a sample dropped from the slope.
The GIT_PAGER control also makes the conservative inherited-variable policy's
cost explicit; it is not exempted just because the variable appears harmless.

## Stop and remaining work

The accepted equivalence proof survives the port, but call-count reduction alone
was insufficient to meet the product floor. This checkpoint preserves the port
and its measurements for review; it does not mark gate 5 complete or push a fix
that was never measured. A new iteration should investigate limiting observations
to actual overlap and avoiding redundant boundary work while preserving the
independent oracle, mutation checks and no-cross-call-state rule. Simply skipping
all observations at zero adapters would shift the cost into the first-adapter
increment, which must still meet its own bound.

Full external/nested topology slopes, shallow/deep product comparisons, fresh
attachment measurements and the complete installed Polars/PostgreSQL journey have
not been rerun after this stop. Gate 6 likewise remains outstanding. The accepted
prior evidence is not relabelled as new-port validation. No fixture processes
remain; the reused source fixture is restored. No entries are pruned.
