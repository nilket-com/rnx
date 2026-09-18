# 0065 gate 5: ignored build directories, shared rechecks and attribution

Status: **stopped for review**. F3 restores reuse on checkouts with ignored build
output; F4 removes repeated shared checks. The first-adapter increment remains
2.17–2.30 ms, above the 1 ms gate. No further implementation change follows this
measurement. Gate 5 remains open and gate 6 has not started.

The accepted F2 checkpoint is preserved and pushed at rnx 1a15f9d / rnx-bench
5d0b985. Its measurements and the earlier stop remain reproducible. Sources for
this iteration are in `probes/nested-scoped`, results in `results/nested-scoped-0065`.

## Changes and equivalence

F3 removes the descendant directory walk. Independent Git inventory does not
inspect ignored descendants, so their size or repository content cannot justify
that walk as an equivalence check. Non-ignored, untracked nested repositories
refuse through the untracked listing. The child-root ancestry and observed paths
to tracked files still detect repository boundaries. No Git-variable policy changes:
**any inherited GIT_* variable, including GIT_EDITOR, forces independent inventory**.
A whitelist is a separate decision.

F4 borrows the administration and boundary maps instead of cloning them, checks
both once per enclosing observation before its first reuse, and records that
check locally. Later children still check their own ancestry and file stamps;
shared checks are not repeated. Separate enclosing observations each need their
own check. State is discarded at the invocation boundary, including on error.
There is no persistent cache, file-stamp vector conversion, format change or
new dependency. All counters and clocks compile only with test-support.

The product replay passes 36 primary oracle/mutation cases and 16 topology/roster
cases. The old ignored-nested-repository case now requires eligible reuse rather
than fallback. The old 4097-entry discovery-limit case becomes 14,000 ignored
entries and also requires reuse. These are explicit assertion changes justified
by F3; tree/allowance equality is never relaxed. Other cases retain their oracle,
including represented nested repositories, child-root .git files, linked worktrees,
submodules, external roots, Git overrides, symlinks, non-Unicode names and bounds.
A separate non-ignored nested repository with no parent-tracked files refuses in
both parent and independently scoped child inventory.

Six additional real-roster controls use the accepted complete 443-file runtime
and copied third adapter. With 14,000 ignored files in polars/target, zero through
three adapters each issue three tool Git calls and perform 443 physical content
reads. Zero adapters do no shared check; one through three perform exactly one.
With GIT_EDITOR exported, one and three adapters issue six and twelve Git calls,
read 466 and 508 files, and perform no shared check. The isolated target directory
is removed afterwards. This exercises the real checkout shape missing from the
original fixture; it is not an installed-runtime-only success.

The same-process product test now uses two children, requiring one shared check,
two reuses, one read per file and identical fresh observations on the second call.
It also retains the failed-call cleanup check. Tool formatting, strict all-target
clippy in both configurations, 46 default and 47 test-support tests pass; the same
two integration tests remain ignored in each configuration. Notices are current.
No root runtime, adapter, server, kernel, Cargo manifest or lockfile changes.

## Observation schedule

The original pause-after-parent mutation cases still pass: ordinary file, index
and boundary changes refuse; a same-size restored-metadata edit after the single
read can be missed until the next invocation. A pre-launch restored-mtime edit in
the real PostgreSQL source still refuses both default launch and --verify. The
lock pair and receipt remain unchanged and restored source launches again.

F4 also changes the interval between children. The new timed case pauses after
child a, then adds and stages a file in child b before continuing. The candidate's
shared index check has already happened, so child b uses the original listing and
sees the addition on the next invocation. The independent reader sees it during
child b's later listing. This case needs no restored metadata. Both complete with
their observed inputs; neither implements an atomic filesystem snapshot. The
plan and README name this additional window rather than claiming F4 has an
identical concurrent-edit observation schedule. Quiescent results remain exact.

## Uninstrumented everyday launch

The unchanged three-product matrix retains all **4,500 samples**: 75 cells, two
repeats, 30 observations per cell, two warmups, seed 655001, CPU 0 and one Polars
thread. Run, eval and first prompt have separate rows and matched direct controls.
The runtime stays at 443 files / 6,988,177 bytes at every declaration count. No
builds overlap timing. Every output, prompt and exit status is checked; terminal
settings remain xterm-256color, 120 columns by 30 rows.

Eval overhead above direct, milliseconds:

| Adapters | SHA-256 baseline | Format-only | F3/F4 |
| --- | ---: | ---: | ---: |
| 0 | 17.60 / 17.56 | 12.32 / 12.29 | 12.27 / 12.26 |
| 1 | 22.55 / 22.54 | 16.39 / 16.28 | 14.45 / 14.51 |
| 2 | 26.57 / 26.59 | 20.08 / 20.09 | 14.68 / 14.82 |
| 3 | 30.61 / 30.77 | 23.93 / 23.90 | 15.05 / 15.15 |

The zero-adapter floor passes. All six mode/repeat first-adapter increments fail
at 2.17–2.30 ms; later increments are 0.23–0.37 ms. gate.json retains every failure,
and summary.json retains every mode, successive increment and fitted slope.
Later low increments do not substitute for the failed first increment.

One-native full --verify remains separate: baseline 95.41 / 95.47 ms, format-only
58.65 / 58.65 ms, candidate 56.79 / 56.86 ms. These are launch-validation costs for
the fixture returning 42, not a new claim about pipeline compute.

Another **360 samples** measure the three-adapter practical shapes, with project
and direct interleaved inside each sequential shape block:

| Shape | Over direct, milliseconds |
| --- | ---: |
| Eligible, clean | 15.22 / 15.22 |
| 14,000 ignored build files | 15.25 / 15.20 |
| GIT_EDITOR exported | 25.88 / 25.67 |

The ignored directory no longer changes the slope. The conservative Git-variable
fallback still does. Inputs, lock pair and receipt are checked unchanged afterwards.

## Exclusive phase attribution

Only after the uninstrumented matrix, a separate test-support release is compared
against the frozen ordinary tool: **480 samples**, two repeats, 30 per count and
build configuration. RNX_INVENTORY_PROFILE enables invocation-local exclusive
spans. Nested spans subtract from their enclosing category. Each sample asserts
that category intervals sum exactly to the invocation clock; profile serialization
and output are outside that clock but inside the measured process wall time.
The ordinary build contains neither these clocks nor the profiling variable.

Median milliseconds within the inventory, first / second repeat:

| Phase | Runtime alone | +1 adapter | +2 adapters | +3 adapters |
| --- | ---: | ---: | ---: | ---: |
| Independent runtime work, excluding observation hooks | 9.256 / 9.250 | 9.372 / 9.354 | 9.366 / 9.350 | 9.360 / 9.350 |
| Observation bookkeeping | 0 / 0 | 1.506 / 1.506 | 1.497 / 1.503 | 1.505 / 1.503 |
| Shared administration/boundary rechecks | 0 / 0 | 0.107 / 0.107 | 0.107 / 0.107 | 0.107 / 0.107 |
| Child eligibility | 0 / 0 | 0.040 / 0.040 | 0.074 / 0.074 | 0.115 / 0.115 |
| Derivation and child file rechecks, excluding framing | 0 / 0 | 0.058 / 0.058 | 0.107 / 0.108 | 0.157 / 0.157 |
| Child tree framing | 0 / 0 | 0.010 / 0.010 | 0.019 / 0.019 | 0.025 / 0.025 |
| Other inventory setup, copies and teardown | 0.004 / 0.005 | 0.071 / 0.071 | 0.070 / 0.070 | 0.082 / 0.082 |
| Total inventory | 9.260 / 9.254 | 11.168 / 11.149 | 11.251 / 11.248 | 11.350 / 11.352 |

These are per-phase medians, whose sum need not equal the median total. The raw
intervals do sum. Independent work includes Git, file validation, reads and hashes;
it is not pure hashing. Observation spans cover before_read and after_read: 886
calls for the 443-file parent, paid only when nesting activates. Derivation includes
selection, allowance charging, file-stamp checks and cloning. Other includes root
validation and observation teardown. Ancestor auditing and lock/receipt checks
outside many() are outside this phase table.

Profiling adds 0.18–0.23 ms to the zero-adapter process and 0.32–0.42 ms for nested
rosters, including counter work and output. Thus 1.5 ms is an instrumented phase
measurement, not an exact subtraction from the ordinary 2.2 ms first increment.
It identifies observation bookkeeping as the dominant added inventory phase;
eligibility and framing are small. The residual outside these categories is not
assigned to a guessed cause, and no map/vector optimization is claimed or applied.

After all timing, strace captures the same locks with format-only, F2 and F3/F4.
At zero adapters all three have 2,734 statx calls. At one adapter F2 has 3,334 versus
F3/F4's 3,142; at three adapters 4,152 falls to 3,214. Successful execs include four
Git processes plus the tool and final artifact for both reuse builds at every
count. Three tool-issued Git calls therefore mean four Git processes here, not
three. The independent format-only tool grows to eight Git processes at one
adapter and sixteen at three. Raw logs are retained; traced times are not used as
launch measurements.

## Preserved stop and remaining work

The requested F3/F4 changes and attribution are complete. The first-adapter gate
is still unsatisfied, so this checkpoint returns for review with reproducible
source instead of changing the threshold or implementation after measurement.
conditions.json pins all tool source and binary hashes; implementation.patch
retains the diff from 1a15f9d, and the signed root checkpoint preserves the source.

Full external/nested topology product slopes, shallow/deep product comparisons,
fresh attachment and installed Polars/PostgreSQL journeys remain beyond this stop,
as does gate 6. Old runtime and assembly entries remain intact. The removal record
is still next after 0065; the review's 15 GB cache/quota observation is motivation,
not authorization to delete entries with possible surviving consumers.
