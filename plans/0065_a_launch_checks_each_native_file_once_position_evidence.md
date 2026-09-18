# 0065 gate 5: positional observations and the remaining 0.016 ms miss

Status: **stopped for review under the unchanged numerical gate**. F5 removes the
path-map bookkeeping cost. Five first-adapter mode/repeat comparisons pass; one
eval repeat adds 1.016205 ms, exceeding the strict 1 ms bound by 0.016205 ms. It is
retained as a failure, not rounded into a pass or rerun to obtain one. Gate 5
remains open and gate 6 has not started.

The accepted F3/F4 checkpoint is pushed at rnx 248e29a / bench 175b2a9. This new
checkpoint preserves its own measured source and all samples. Drivers are in
`probes/nested-position`, results in `results/nested-position-0065`.

## F5 representation and checks

Production changes are confined to reuse.rs. after_read appends a Stamp to a
Vec in read order, without cloning or comparing a path. The existing reader visits
files in the same sorted string order it stores in Tree::files; successful
independent observation checks that the lengths agree. A child's prefix includes
its trailing slash. Two partition points select the contiguous sorted range;
file and stamp are then addressed by the same index. This also removes the full
parent-path scan and temporary selection vector from each derivation.

before_read compares the parent-directory bytes against the last directory's
bytes first. On a change, it walks ancestors against a HashSet of byte vectors
and records directory/.git observations in a plain vector. Repeated files in one
directory perform no path allocation or tree-map search; file-stamp append is
amortized constant time. Byte comparisons and hashing still cost proportionally
to path length. No directory or file observation persists beyond the invocation.

F4's validation schedule remains: administration and boundaries once before first
reuse of an enclosing observation; each child's ancestry and its own file stamps
before derivation. The same observations are checked, now in collection order
rather than path-map order. Its accepted between-child index window is unchanged.
The test-support clocks and their span boundaries are byte-identical to F3/F4.
No change to hashing, bounds, formats, dependencies, Git-variable policy, artifact
stamps, root runtime, adapters, kernel or server is included. The README now states
once that three tool Git calls create four Git processes in the measured topology.

Formatting, strict all-target clippy in both configurations, 46 default and 47
test-support tests pass, with the same two ignored integration tests. Notices
are current. The product same-process test now includes a similarly named sibling,
a file sorting before the child slash, and a Unicode nested filename. It asserts
the exact child file range, paired stamps through a later edit, one shared recheck
for two children, and fresh state after both a successful and a failed invocation.

The unchanged F3/F4 fixture sequence passes: 36 primary oracle/mutation cases,
16 topology/roster cases, six real 14,000-file/GIT_EDITOR controls, the untracked
nested-repository parent/child refusals, and the timed between-child window.
Real pre-launch restored-mtime edits still refuse both default and --verify;
restoring the source permits launch again, with lock pair and receipt unchanged.
For eligible zero-to-three rosters, all 443 files are read once with three Git
calls. Shared rechecks are zero at zero adapters and one for all children. With
GIT_EDITOR, one/three adapters still use six/twelve Git calls and 466/508 reads.

## Same uninstrumented matrix

All **4,500 headline samples** are retained: 75 cells, two repeats, 30 samples per
cell, two warmups, seed 655001, CPU 0 and one Polars thread. The unchanged runtime
has 443 files / 6,988,177 bytes including the complete copied third adapter at all
counts. Each product has its matched direct artifact. Run, eval and first prompt
are separate; terminal settings remain xterm-256color, 120 columns by 30 rows.
Every result, prompt and status is checked; no builds overlap measurement.

Eval overhead above direct, milliseconds:

| Adapters | SHA-256 baseline | Format-only BLAKE3 | F5 |
| --- | ---: | ---: | ---: |
| 0 | 17.639 / 17.584 | 12.305 / 12.236 | 12.239 / 12.309 |
| 1 | 22.532 / 22.601 | 16.310 / 16.354 | 13.255 / 13.141 |
| 2 | 26.548 / 26.607 | 20.032 / 20.074 | 13.376 / 13.398 |
| 3 | 30.670 / 30.655 | 23.841 / 23.829 | 13.689 / 13.730 |

The runtime floor still improves by more than 5 ms. The first-adapter increments
are run 0.775 / 0.802 ms, eval **1.016 / 0.832 ms**, and first prompt
0.869 / 0.988 ms. Later increments are 0.121–0.372 ms. Five first increments meet
the bound, but gate.json records the one failure exactly. No slope fit or rounding
replaces that decision. The expected half-millisecond increment was a prediction;
these are the measured results.

Full verification has its own one-native row: baseline 95.39 / 95.40 ms,
format-only 58.72 / 58.59 ms, F5 55.51 / 55.56 ms. This is the launch fixture that
returns 42, not a new pipeline-compute measurement.

Another **360 samples** retain the practical three-adapter controls:

| Shape | Over direct, milliseconds |
| --- | ---: |
| Eligible, clean | 13.78 / 13.73 |
| 14,000 ignored build files | 13.73 / 13.77 |
| GIT_EDITOR exported | 24.38 / 24.29 |

Ignored build output stays eligible. The Git-variable policy still selects the
independent slope. The fixture removes only its own ignored target directory;
source, lock pair and receipt are checked unchanged afterwards.

## Same exclusive attribution

The unchanged opt-in test-support clocks produce another **480 samples**, with
two repeats of 30 observations per count/configuration, seed 655800. Each raw
profile's exclusive intervals sum exactly to its invocation duration. Serialization
and output occur outside that clock but inside the separately measured profiled
process wall time. The ordinary executable has no profiling clocks or variable.

Median milliseconds, first / second repeat:

| Phase | Runtime alone | +1 adapter | +2 adapters | +3 adapters |
| --- | ---: | ---: | ---: | ---: |
| Independent runtime work excluding observation | 9.241 / 9.247 | 9.326 / 9.301 | 9.318 / 9.326 | 9.338 / 9.349 |
| Observation bookkeeping | 0 / 0 | 0.252 / 0.253 | 0.244 / 0.249 | 0.249 / 0.250 |
| Shared rechecks | 0 / 0 | 0.105 / 0.105 | 0.105 / 0.105 | 0.105 / 0.105 |
| Eligibility | 0 / 0 | 0.039 / 0.039 | 0.077 / 0.078 | 0.112 / 0.112 |
| Derivation and child file rechecks | 0 / 0 | 0.031 / 0.031 | 0.059 / 0.059 | 0.085 / 0.086 |
| Child framing | 0 / 0 | 0.011 / 0.011 | 0.019 / 0.019 | 0.025 / 0.025 |
| Other inventory setup, copies and teardown | 0.018 / 0.018 | 0.058 / 0.058 | 0.055 / 0.055 | 0.065 / 0.065 |

Bookkeeping drops from F3/F4's roughly 1.5 ms to 0.24–0.25 ms while retaining 886
hook calls. The representation change accounts for the large improvement without
reducing reads or validation syscalls. Shared rechecks stay at roughly 0.11 ms.
Derivation is also smaller with positional selection. Independent work includes
Git, file checks, reads and hashing, not hashing alone. Other inventory work and
checks outside many() are not silently assigned to the bookkeeping phase.

These are medians of separate exclusive intervals, not additive median totals.
The profiled process costs 0.22–0.29 ms more at zero adapters and 0.39–0.47 ms more
for nested rosters, including test counters, clocks and output. The phase table
locates the cost; it does not replace the ordinary-build gate with profiled times.
No extra source change or sample rerun follows these results.

Diagnostic strace runs occur after timing. F3/F4 and F5 have identical statx
counts at all three inspected roster sizes: 2,734 at zero, 3,142 at one and 3,214
at three. Both have four successful Git processes at each count, besides the tool
and final artifact. Raw logs include the format-only and F2 controls too. This
supports a bookkeeping improvement with unchanged filesystem work, rather than
an unreported reduction in checks. Traced durations are not latency measurements.

## Preserved checkpoint and scope

The F5 implementation, exact matrices and attribution are complete. One strict
numeric comparison still fails by 0.016205 ms. All samples and that result remain
for review; this checkpoint does not assert either a pass or a statistically
established regression from that small difference.

conditions.json pins source and binaries; implementation.patch preserves the tool
diff from 248e29a, alongside the signed source checkpoint. No previous source,
measurement, runtime or assembly entry is removed. Full topology/path-depth
product slopes, fresh attachment and installed Polars/PostgreSQL journeys remain
beyond this stop, followed by gate 6. The removal record remains next after 0065.
