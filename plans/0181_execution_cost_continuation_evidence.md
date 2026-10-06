# rnx 0181: execution-cost diagnostics, COMPLETE

**Outcome: COMPLETE.** One official attempt from the clean bench tree 52ae9c49,
using driver source 11ceb6e4. All four groups R/C/D/E completed 140 samples each
and reproduced the frozen 0179 anchors. No repair, replay or added sample after
results. No engine change, build-profile change or adoption follows. The earlier
0179 optimization STOP and 0180 group B validity STOP remain unchanged.

This is a new study selected after seeing 0180's STOP, not completion of that
study. Groups A and B were neither discovered nor measured here.

## Provenance and method

Plan 7d0f280, `plans/0181_execution_cost_continuation.md`. Bench driver and
receipt history is retained on the integrated 0181 branch:

- 433208f4: initial driver and controls1, 19/19.
- ea5045e3: controls2, 20/20, including zero-denominator completion.
- d3cf6e39: reviewed discovery-format and pre-import library checks; controls3,
  22/22.
- 07b1e84d: fresh discovery, eight probes, two for each R/C/D/E; all eligible.
- 11ceb6e4: reviewed discovery pins and controls4, 23/23.
- 52ae9c49: one untimed rehearsal, no experiment subjects executed.
- 1fa88a8b: sole official attempt and driver-side evidence.
- 1601a0bc: independent read-only closure audit and qualified interpretation.

Both retained 0179 PRIMARY binaries and all seven scripts remain hash-bound.
Base SHA-256: af06e8b3b1621b7f763dfb39f967fab1cca0b9b615b145152634463cefbb0c87.
Candidate: e4a5f207c38a8ae59bc311e3611a6bdd3d542fe82665a8e12b201f61eaf9a787.
No rebuild. Same stage path, E0, CPU 4 affinity, budgets and fresh-process windows.

Fresh discovery identity SHA-256:
c281c11d5b0a0d3552415836e66635a9aed03c6fa3162c06f7f921848882f2aa.
Availability SHA-256:
298b3d5dd83c8f5b80075ebd21e6e868f4e09c806d7eecc21fed57213290d2de.
The freshly observed identity is byte-identical to 0180's: GenuineIntel family
6/model 183, CPU 4 on cpu_core, kernel/perf/microcode unchanged. The driver
compares current identity at admission and before every group. Retained version
receipts pass lifecycle and status checks; other identity fields are read from
proc/sysfs by the reviewed helper.

All groups include instructions and cycles, with unchanged 0180 cpu_core,
user-only, strong pinned grouping and `--no-scale`:

| Group | Additional events |
| --- | --- |
| C | retired branches c4/00; retired mispredictions c5/00 |
| D | DSB uops 79/08; MITE uops 79/04; instruction-cache stall cycles 80/04 |
| E | retired L1-miss loads d1/08; store-forward blocks 03/82 |

Five repetitions, seven windows, ABBA per window: 560 total samples. The official
phase took about 76 seconds, exit 0. The independent-group/global-prerequisite
policy was fixed before discovery; no local counter failure occurred. Both
instruction and cycle anchors passed separately in all four groups. Sentinel
scan completed with zero occurrences. Raw SHA-256:
4b83a82b650d0aa8739d0e6ac38e822719694d0e9ad3c46167b655ea17d0d418.

## Independent reconstruction

Bench `probes/execution-cost-0181/audit.py` executes no subjects and opens no
counters. Its receipt and command ledger are
`results/execution-cost-0181/audit.json` and `closure-audit-ledger.json`.
It checks all 560 rows' exact order, argv, pre/post binary hashes, E0, affinity,
status, output, lifecycle and counter validity. It rebuilds every shared summary
and 0179 reference and independently recomputes all 247 same-sample quantities,
medians, paired contrasts, widths, verdicts and reproduction boundaries. All
match exactly. It also reconstructs seven printed anchor rows and 70 diagnostic
table rows; E's per-instruction ratios are retained in official.json and checked
there, while the printed E table contains raw counts.

Read-only primary/script/manifest hashes, discovery/library pins, eligibility,
version receipts, phase duration, registered status and sentinel receipt pass.
The scan receipt is inspected, not independently reproduced with the original
secret sentinel (which is not persisted).

The first audit invocation expected printed E ratio rows and failed that table
assertion. The reviewer corrected this assumption; every ratio remains checked
against the retained summary. No experiment source, sample or receipt changed.
The final audit exits 0. Controls4 were independently replayed before official
approval: 23/23, including the inherited 0180 26/26 controls.

## Findings

The excess cycles reproduce while instructions fall on numeric and range
windows. Across R/C/D/E: numeric +8.22% to +8.47% cycles; signed range +15.87% to
+16.24%; negative range +16.01% to +16.16%; fib +2.46% to +2.77%; calls +2.59% to
+3.30%; while -0.08% to +0.09%. Nonempty instruction effects match 0179 to three
decimal percentage places. Run-empty remains included, with its cycle effect
descriptive under the frozen exception.

The frozen descriptive rule requires all five paired contrasts to share one
strict sign and the pooled difference to exceed the base p10-p90 width. It is
neither a confidence interval nor a causal test.

| Window | MITE uops, base → candidate | DSB share of DSB+MITE, base → candidate |
| --- | --- | --- |
| numeric | 12.87M → 144.27M, resolved up | .9925 → .9192, resolved down |
| signed range | 12.65M → 70.18M, resolved up | .9910 → .9516, resolved down |
| negative range | 13.57M → 69.97M, resolved up | .9903 → .9517, resolved down |
| calls | 11.09M → 34.03M, resolved up | .9935 → .9800, resolved down |

While and fib have no resolved MITE increase or DSB-share decrease. Fib does
have resolved DSB-uop growth of about 2.1%, alongside +2.28% instructions; it is
incorrect to say that fib has no frontend-delivery change at all. Fib also has
positive excess cycles, despite lacking the numeric/range delivery-share signal.

Retired branch mispredictions resolve down on numeric, both ranges, calls and
while. Numeric/range retired branches rise by about 2.65 per iteration, so this
does not exclude other branch costs. E's store-forward blocks have lower pooled
medians everywhere (resolved down on negative range, fib, calls, while and
run-empty). Retired L1-miss loads are unresolved everywhere, with lower pooled
medians. Neither observation excludes unmeasured backend costs.

Instruction-cache stall cycles resolve up only on numeric, about 0.38M cycles,
small beside its roughly 37M excess cycles. They are stall cycles, not miss
counts; their difference is not a causal accounting decomposition.

## Interpretation and limits

The **frontend-delivery category is supported as a compatible explanation for
Q1**: whole-process numeric/range windows show increased legacy-decoder delivery
and a lower DSB share alongside reproduced excess cycles. This is a sharper
measurement than static layout differences alone. It is not proof that the
interpreter loop lost uop-cache coverage or that this caused its slowdown.
DSB/MITE omit other delivery sources, and the counters do not measure switch
counts/costs or locate a particular loop, boundary or instruction.

The association is directional, not proportional: numeric's MITE increase is
about 131 uops per script iteration with +8.3% cycles; ranges about 57 with +16%.
Per-operation figures divide whole-process differences by the script's operation
count; they do not isolate a hot region. Q2's fib instruction growth receives
no dynamic instruction attribution from these counters. Compiler partition,
layout and inlining causation remain unresolved. 0180's failed B cannot provide
corroborating Topdown fractions. Falling branch misses and these two load events
do not refute every speculation/backend mechanism.

## Disclosures and closure scope

The driver-side OFFICIAL.md retains all seven-window tables, controls history
and limitations. An initial synthetic dry run (17/19) was not retained; its real
eventless-row gap and ineffective metadata-tamper control were corrected before
discovery. Review additionally required discovery to use the official classifier
and library hashes to be checked before import. Later mock raw files are removed
only after assertion and hashing, with counts/hashes retained; real discovery,
rehearsal and official rows are retained in full. Actual perf JSON has no PMU or
enabled-time fields; the PMU binding comes from frozen argv, not invented fields.

This closes a diagnostic record, not an optimization. No gate was waived and
no future experiment, source change or product adoption is assumed.
