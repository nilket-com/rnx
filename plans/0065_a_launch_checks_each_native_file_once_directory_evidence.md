# 0065 gate 5: directory observations and the first-adapter stop

Status: **stopped for review**. F2 fixes the zero-adapter floor, but the first
adapter still exceeds the separately required 1 ms increment. Gate 5 remains open;
gate 6 has not started. The accepted earlier stop is preserved at rnx 2bf4871 and
rnx-bench fc30b5c, now pushed. No post-measurement optimization is substituted here.

## F2 implementation and checks

Only reuse.rs changes in production. before_read stops at the first directory
already represented in the observation map. A new directory contributes its own
metadata and one .git observation; the result of that observation also decides
whether discovery stops. It no longer repeats either lookup for every file or
performs the redundant third .git lookup. The existing pre-reuse administration,
boundary and file-stamp checks are unchanged.

many() first checks the bounded set of roots for strict nesting. With no nested
pair it uses the independent native reader without activating observations or
collecting topology state. With nesting it uses the accepted eligibility and
fallback algorithm. No file-stamp Vec conversion is included; that optional
representation change remains unmeasured. No format, dependency, root runtime or
persistent-cache change is introduced.

The unchanged product oracle/mutation matrix passes all 36 primary and 16 topology
cases, including same-process double inventory, content changes, bounded fallback
and the documented same-size restored-metadata observation-window counterexample.
Formatting, strict clippy in both configurations, 46 default tests, 47 test-support
tests and notices pass; two existing tests remain ignored in each configuration.
The same-process product unit test is unchanged.

The replay wrapper initially rewrote its own nested substitution needle as well
as its outer path assignment, so a generated driver tried to create a target below
the result directory and refused before testing. The wrapper now replaces the
outer assignment once. Final driver assertions, matrix, workload, seeds and sample
counts are unchanged. This was a path-adaptation fixture error, not a source or
refusal-equivalence finding.

## Matched everyday launch

Sources: `probes/nested-directory`; results: `results/nested-directory-0065`.
run.py reuses the accepted nested-product drivers with only isolated target/result
paths. The same three uninstrumented tools, fixed 443-file/6,988,177-byte runtime,
complete third adapter, per-product direct artifacts, modes, seed, one pinned CPU,
one Polars thread, two warmups and two repeats of 30 samples per cell are retained.
All **4,500 samples** are saved. No builds overlap timing and outputs are validated.
The prior measured stop is retained separately, not added as a different cell in
this unchanged three-way matrix.

Eval overhead above direct, milliseconds:

| Adapters | SHA-256 baseline, repeats | Format-only, repeats | F2, repeats |
| --- | ---: | ---: | ---: |
| 0 | 17.55 / 17.54 | 12.26 / 12.19 | 12.16 / 12.23 |
| 1 | 22.47 / 22.45 | 16.26 / 16.32 | 14.60 / 14.57 |
| 2 | 26.49 / 26.50 | 20.02 / 20.00 | 15.06 / 15.15 |
| 3 | 30.56 / 30.61 | 23.72 / 23.73 | 15.65 / 15.64 |

Run and first-prompt rows agree; their exact medians, successive increments and
fitted slopes are preserved in summary.json. The floor improves by about 5.3–5.5 ms
over the baseline and is effectively back at the accepted format-only floor.
Every count improves over baseline and the previous port.

**The first adapter adds 2.30–2.44 ms in all six mode/repeat combinations**, above
the 1 ms gate. The subsequent increments are approximately 0.46–0.63 ms. gate.json
records all six count-1 failures; no threshold is weakened and the low later slope
is not used to hide the first increment. The review's 12.2/13/13.5/14 ms figures
were predictions. Only the first of those matches this measurement.

One-native full --verify remains separate from everyday launch: baseline
95.43 / 95.43 ms, format-only 58.65 / 58.64 ms, F2 56.85 / 56.91 ms.

## Syscall and fallback diagnostics

After timing, one-native eval was traced on the same lock using the frozen
format-only, previous stopped port, and F2 tools. These counts include Git children;
strace timings are not used as launch results:

| Tool | statx calls | ENOENT calls |
| --- | ---: | ---: |
| Format-only | 2,883 | 70 |
| Previous stopped port | 7,073 | 1,988 |
| F2 | 3,334 | 351 |

This fixture reproduces the review's repeated-stat pattern, while using its own
paths and launch environment rather than replacing the review's counts. F2 removes
3,739 statx calls from the stopped port. It confirms that the requested redundant
ancestor work was removed; it does not attribute the remaining first-adapter cost
entirely to the 451 calls above the format-only control. Map insertion, path creation,
cloning, eligibility discovery and repeated pre-reuse validation still have costs.
No further implementation change was made to chase those costs in this checkpoint.

The unchanged fallback control retains another 360 samples, with project/direct
interleaving inside each sequential shape block:

| Three-adapter shape | Over direct, repeats |
| --- | ---: |
| Eligible, clean | 15.72 / 15.68 ms |
| 4097 ignored files exhaust the 4096-entry discovery budget | 19.96 / 19.99 ms |
| Inherited GIT_PAGER=cat forces all roots independent | 25.62 / 25.57 ms |

The fixture-owned ignored target directory is removed afterwards; the lock pair,
receipt and source fixture remain intact. No compiler runs in these measurements.
No fixture processes remain.

## Stop and retained scope

The floor gate is now satisfied; the first-adapter increment is not. Further
work must reduce the cost of beginning reusable observation, while retaining the
oracle and existing validation/observation-window contract. This checkpoint records
the measured state and returns for review under decision 6. It does not claim the
optional Vec change or any other unmeasured optimization would close the gap.

Full external/nested topology slopes, shallow/deep product comparisons, fresh
attachment and installed Polars/PostgreSQL journeys remain outstanding after this
stop, as does gate 6. Old runtime/assembly entries remain retained. The review's
reported 15 GB scratch cache and tmpfs quota failure are motivation for the removal
record immediately after 0065, not permission to prune entries or evidence that
those entries have no remaining consumers.
