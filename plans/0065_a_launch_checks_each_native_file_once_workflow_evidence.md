# 0065 gate 2: current-format workflows and format-only cost

Status: gate 2 passes on Linux, ready for review. Gates 3–6 remain open.

## Product boundary

The production change is confined to the project tool's workflow. Old or invalid
lock rejection now precedes receipt removal in build. A refused lock/receipt
names its envelope version where readable and prints the current tool's exact,
shell-quoted lock/build commands for the canonical manifest. Envelope inspection
runs only on refusal; it neither authenticates old content nor accepts unknown or
mixed formats. Old files are not rewritten or refreshed on refusal. The message
states the retained-disk consequence rather than implying relock reclaims space.

Executable overrides now require an explicit build after relock. Build fully
checks the artifact against the lock's existing BLAKE3 digest, rechecks inputs
and lock, checks the artifact stamp again and publishes receipt 4. It invokes no
Cargo. A missing, wrong-kind or mismatched override receipt cannot be implicitly
established by launch. Normal stamp equality, mismatch/full-check/refresh and
`--verify` retain 0059's policy. No native inventory reuse, persistent source cache,
reader change, parallel hashing, root source change or dependency change lands in
this checkpoint. Installation-1 authentication/recovery remains gate 3.

## Workflow evidence

The bench fixture is `probes/inventory-workflow`; results are in
`results/inventory-workflow-0065`. Its README lists exact commands, prerequisites,
rerun cleanup and the distinction between product and isolated driver tests.

The current-format command matrix uses the actual 7cd3205 product to create a
shared lock 2/receipt 3 and an override lock 1/receipt 2 in private temporary
projects, including spaces and an apostrophe. It checks the exact recovery
commands by shell parsing, byte-identical old files on run/eval/session/build
refusal, new-key compilation instead of promotion, retention of the old artifact,
and current receipts/ready bindings. A second consumer attaches with compilation
trapped and a positive trap control.

Both stamp and full-read counts are observed with the existing test-support
reader hook: ordinary hit reads zero artifact bytes, touch/replacement reads the
whole file, an identical replacement refreshes against the recorded digest, and
a different replacement refuses. A same-size/restored-mtime in-place artifact
edit is accepted by default and refused by `--verify`, as documented. Restored-
mtime source edits still refuse before artifact reads. The matrix covers unknown
and mixed documents, missing/mismatched receipts, map scope, corrupt/missing ready
state, publication failure, lock-pair rollback and signal interruption, receipt
refresh interruption and explicit override-build failure/interruption. The tiny
runtime reports arguments to isolate orchestration; real Rune/Polars journeys are
separate below.

The accepted 37-case publication replay imports current product modules byte for
byte and changes only its audit helper's digest field/algorithm. All concurrency,
compiler traps, publication-boundary failures, corrupt-entry refusals and signal
assertions remain. It kills a real compiling builder, observes its child reaped,
and makes the waiter rebuild; killing the waiter leaves the builder able to
publish and a new consumer able to attach. Its fixture receipt is not offered as
product-receipt evidence.

## Matched product measurement

The accepted 7cd3205 runtime fixture is unchanged: 443 tracked files and
6,988,177 bytes at every adapter count. It includes the complete renamed
PostgreSQL adapter even in the zero-adapter row. The runtime is the separately
reported floor, not an adapter. Added trees are Polars (23 files/984,808 bytes),
PostgreSQL (21/517,330), and the renamed copy (21/517,312). Each still receives an
independent inventory pass. The same-worktree fixture is nested inside the bench
checkout: each root's three Git commands include a submodule check that invokes
one additional parent Git listing. No reduced Git count is claimed yet.

The tools use identical runtime paths, generated wrapper and exact Cargo lock
bytes, but separate valid locks/keys/receipts and their own direct artifacts.
Build paths can affect output; old artifacts are never relabelled or substituted.
Setup compilation is outside launch timing and makes no cold-build claim.

The final journal contains 3,000 observations (50 cells × 30 samples × two
repeats), with the 120 attachment observations retained separately. `summary.json`
contains every mode's direct, project, difference, increment and fitted slope.
Representative eval overhead over its own direct artifact, in milliseconds:

| Declared adapters | Baseline, repeats 1 / 2 | BLAKE3 format-only, repeats 1 / 2 |
|---|---:|---:|
| 0 (runtime floor) | 17.56 / 17.55 | 12.23 / 12.11 |
| 1 | 22.42 / 22.39 | 16.22 / 16.20 |
| 2 | 26.55 / 26.40 | 20.04 / 19.92 |
| 3 | 30.66 / 30.53 | 23.71 / 23.59 |

Run and first-prompt rows agree: zero-adapter overhead is 12.17–12.23 ms versus
17.63–17.68 ms; three-adapter overhead is 23.65–23.80 ms versus 30.53–30.74 ms.
Every count improves in every mode/repeat. The zero-adapter reduction is over
5 ms, while the remaining slope is about 3.8 ms per adapter. This does **not**
claim gate 5's less-than-1-ms nested-reuse slope: no reuse has landed.

One-native eval costs, kept separate from overhead:

| Launch | Baseline, repeats 1 / 2 | BLAKE3, repeats 1 / 2 |
|---|---:|---:|
| Direct artifact | 6.66 / 6.60 ms | 6.59 / 6.62 ms |
| Default project launch | 29.08 / 28.99 ms | 22.81 / 22.81 ms |
| Project `--verify` | 95.47 / 95.33 ms | 58.60 / 58.65 ms |

Real one-native attachment costs 264.48 / 263.70 ms before and
192.07 / 191.77 ms now. Each sample fully checks the artifact; compiler traps
with positive controls prove zero compilation. Both products attach a second
consumer with different application text to the first consumer's key. Attachment
is not a normal launch and is not presented as free.

The real current shared and override projects both pass run, eval and a PTY
Polars journey: keep a frame, filter/group/sum/sort/collect across inputs, preview,
catch a missing-column error, preview the retained frame, write/read Parquet with
equal previews, reset, and use Polars again. Files land in the caller's fixture
working directory. Both sessions quit and are reaped.

## Checks and reproducibility

Tool formatting and strict all-target clippy pass in both configurations; each
suite passes 46 tests with zero failures and the same two ignored integration
tests. Notices are current. All 16 command groups, 37 publication cases, real
journeys and complete-journal checks pass. Root source, Cargo files, kernel,
adapters, server, and tool dependency files are unchanged from gate 1.

`source.json` pins the measured release and test-support executables and every
tool source file. `tool.patch` retains the implementation difference from
22b9386; the signed checkpoint also preserves that source. The collector checks
those identities, 30 unique samples in each repeat/cell, exact Cargo bytes across
products, constant runtime counts, and every mode/count improving. The generated
contract and publication drivers expose their adaptations rather than claiming
verbatim replay of obsolete migration expectations.

The measured application returns 42; these are launch/inventory observations,
not a repeat of the CSV pipeline's compute benchmark. Every run/eval result is
checked as exact stdout `42\n`, empty stderr and zero status. Session timing ends
at the complete first prompt; quit and reaping occur afterwards. Terminal is
xterm-256color, 120 columns by 30 rows. The run fixes one core and one Polars
thread before spawning, interleaves two repeats with 30 observations per cell,
and keeps every measured observation. Full verify and attachment have separate
rows rather than being averaged into ordinary launch.

## Qualifications and fixture corrections

The initial setup resolved from the current offline registry cache instead of
seeding the accepted Cargo lock. That picked cc 1.4.7/find-msvc-tools 0.1.13 in
place of 1.4.6/0.1.12. The second-consumer compiler trap caught the different key;
this was a real graph difference, not failed cache reuse. The first 3,000 launch
samples, input locks, summaries and scripts remain under
`graph-drift-preliminary/` and are not the matched result. The corrected setup
seeds the baseline Cargo bytes and asserts equality after lock. Its new entries
are built and the entire timing matrix rerun. Neither earlier entries nor
preliminary samples are deleted to improve the result.

Other driver corrections were mechanical: a malformed generated Python quote,
an eval refusal attempted without its required source argument, an old-tool
control accidentally left with a new receipt, and a replay insertion that missed
its anchor. Each was corrected before the final contract run; none weakened a
product assertion or required a product workaround.

Linux, one host, warm launches. Runtime migration, old kernelspec/live-session
survival, topology eligibility and nested reuse remain their later gates. No
runtime store or user's scratch project was used. Gate 6 owns full root regression.
