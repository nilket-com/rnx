# rnx 0180: execution-cost explanation study

**Outcome: measurement-validity STOP, with valid partial evidence.** No engine,
profile or product change; Rune fork main stays at bb8e6937 and shipped rnx stays
on Rune 0.14.2. The earlier 0179 optimization STOP is unchanged.

This record asked why the retained 0179 candidate used fewer instructions yet
more cycles on numeric/range loops, and separately why fib grew instructions.
The accepted static checkpoint is in `plans/0180_static_checkpoint.md`. The
measurement protocol is section 7 of `plans/0180_execution_cost_explanations.md`.
The sole official attempt stopped at the registered Topdown validity check.
There was no repair, replay, changed tolerance or added sample after that result.

## Identities and review sequence

Plan/checkpoint commits are 3cf1e7a, ee2826b and ee3a5de. Discovery showed that
cpu_core/ref-cycles resolves to documented CPU_CLK_UNHALTED.REF_TSC_P,
0x3c/0x01. Commit fac2b00 corrected the plan's fixed-counter wording without
changing the selected event, semantics or gates.

The two retained PRIMARY subjects, authoritative for both deciding PMU and wall
samples in 0179, were staged at the original path and hash-checked before and
after every sample:

- base: af06e8b3b1621b7f763dfb39f967fab1cca0b9b615b145152634463cefbb0c87;
- candidate: e4a5f207c38a8ae59bc311e3611a6bdd3d542fe82665a8e12b201f61eaf9a787.

Bench driver history starts at 063c44b3, with discovery/rehearsal receipts at
566707ce. Admission/parser/deadline/scan repairs are 902b5635; the final
rehearsal status/affinity gate is b92ee986. The accepted rehearsal2 and clean
launch tree are e89da845; the official attempt is retained at 9448c0f2. Claude's
closure report is b0e46fae. These commits are integrated unchanged, with an
additional read-only audit and two prose corrections by Codex.

Discovery availability SHA-256 is
29b03834d480690511f760cfe2e4f01372e953c59271c3b1d94c55ed4504de0b;
identity SHA-256 is
c281c11d5b0a0d3552415836e66635a9aed03c6fa3162c06f7f921848882f2aa.
All six frozen groups passed both availability opens. Admission bound these
bytes, their contents and the current filtered host/PMU/perf identity before
staging. The official raw-row SHA-256 is
edfa125ad3ae2aeedc464f313f071d2d8c28d76a9884395402857c99723e83e0.

Disclosures remain part of the record:

- Static1 stopped on a missing binutils v0 VM-symbol spelling. A reviewed,
  spelling-only repair preceded static2; both attempts remain retained.
- Rehearsal1 preceded the required availability review. It executed only the
  affinity probe and deliberate failure command, no experiment subject. It is
  retained and was superseded by reviewed rehearsal2.
- Driver review found admission, runtime/unit, deadline and scan-status gaps,
  then the unchecked rehearsal probe status/output. All were repaired before
  any experiment workload ran. Discovery was revalidated, not repeated.
- An intermediate 25/26 extended-controls scratch run was not retained. Its
  archive assertion expected exactly one match; the scanner searches both
  stored and decompressed bytes, so it can count the same short marker twice.
  The assertion became count >= 1, with no scanner change. No receipt was
  retrospectively manufactured. Retained controls are 19/19, 26/26 and 26/26.
- Inclusive anchor-boundary rounding uses the reviewed EPS of 1e-12. No
  post-result boundary or tolerance change occurred.
- This perf JSON has no explicit PMU or enabled-time field. PMU binding comes
  from the frozen cpu_core argv. The strong pinned groups, no scaling and
  reported 100% running with positive runtime are retained; missing fields
  are not invented.

## Sole official attempt and stop

The run started from clean e89da845 under the shared lock, with the frozen E0
child environment and CPU 4. It took about 59 s and exited 1. Raw rows were
retained before checking: R 140, A 140, B 125, C/D/E zero, 405 total. Lifecycle,
status/output, hashes, units, runtime and sequence checks passed on all rows.
The sentinel scan completed with zero occurrences.

The last row, B sample 125 (repetition 4, fib, base), had a sum of Topdown
fractions of 1.020378019849108. That exceeds the pre-registered requirement
|sum - 1| <= 0.02. The driver stopped immediately. B is incomplete and invalid,
has no group summary, and its earlier 124 rows are not interpreted as a group
result. Why the sum exceeded the bound is not established; the row was not
clamped, discarded or explained away. C, D and E never started.

## Valid partial findings

Both completed groups reproduced the registered 0179 instruction/cycle anchors.
Their cycle changes remain consistent with the earlier slowdown:

| Window | 0179 cycles | R cycles | A cycles |
| --- | ---: | ---: | ---: |
| numeric | +8.32% | +8.30% | +8.16% |
| signed range | +16.11% | +16.06% | +16.20% |
| negative range | +16.38% | +16.23% | +15.98% |
| fib | +2.94% | +2.64% | +2.65% |
| calls | +2.72% | +2.86% | +2.90% |
| while | +0.22% | +0.18% | +0.05% |

A's same-sample actual/reference-cycle ratio has no resolved difference for any
nonempty window under the frozen five-contrast/base-width descriptive rule.
Reference cycles per instruction resolve upward for numeric (+9.01%) and
signed/negative ranges (+16.98%/+16.78%). This weakens a frequency-only
explanation on these windows. It does not establish equivalent frequency,
absence of host/scheduling effects, or a causal mechanism. Zero reported
context switches/migrations are observations of these user-filtered per-task
counters. Task-clock quantities in the retained tables are milliseconds.

The static checkpoint establishes changed generated interpreter code, not just
relocated identical code. Its startup-subtracted whole-process instruction
contrasts are approximately -12/-10 per numeric/range iteration and +33 per
fib invocation, with the reported residuals. They do not dynamically bind an
executed block or prove the same cost on every path. The identified return/drop
code differences are possible contributors, not a measured attribution.

Q1's cycle mechanism remains unresolved. Q2's instruction growth has no dynamic
attribution beyond R/A. Frontend/speculation/backend mechanisms are untested by
the incomplete B and uncollected C-E groups, not refuted. Compiler causation
remains a separate question. No optimization, profile or engine adoption follows.

## Verification and closure boundary

`probes/execution-cost-0180/audit.py` in rnx-bench pins the official raw hash,
checks all 405 rows against the frozen sequence/argv/output/lifecycle/hashes,
reproduces R/A summaries and anchor decisions exactly, and confirms that only
the final B row fails validity. It independently reconstructs the seven printed
anchor rows and 28 group-A table rows from per-sample counts/ratios, including
paired contrasts and the frozen descriptive verdict. It opens no counter and
executes no subject. Its receipt, command/source/output hashes and the 22
static-control replay are retained beside the official evidence.

Full tables, commands, statuses, raw counts, paired contrasts, scan receipt and
all preceding attempts are in rnx-bench `results/execution-cost-0180/`;
`OFFICIAL.md` and `audit.json` are the closure entry points. No core/adapter
source changed in this record, so product suites were not rerun for this
analysis-only closure.

Any continuation on the previously uncollected C-E questions requires a new
pre-registered record, explicitly selected after seeing this STOP, with fresh
anchors and unchanged event/window definitions. Nothing here approves a second
official run within 0180.
