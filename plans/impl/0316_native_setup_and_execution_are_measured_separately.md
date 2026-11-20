# Implementation plan 0316: Native setup and execution are measured separately

Status: Implemented

Paired ADR: ../adr/0316_native_setup_and_execution_are_measured_separately.md.

1. Implement the phase boundary and reconciliation first, so that
   calibration uses the production observer (or prove exact matched-path
   equivalence with a scratch observer before using its numbers).
2. Calibration, before any acceptance run. Record the manifests, recipe,
   seeds, invariants, matrix, repeats and formula. Generate the eight
   training, four reserved and two clean builds per revision, then check
   the invariants and coverage; stop if calibration is insufficient.
   Compute the allowances and amend this record with the frozen table,
   rule and recipe digest, signed, before the reserve is opened or any
   step 4 replay runs.
3. Complete the versioned schema and capture key, scoring, cache and
   retention. Recapture the anchor, and report the published baseline
   against its new budgets.
   Following signature of the canonical-layout amendment, use private
   mounts for all roles and both modes. Run the relocation, incomparable
   launch and within-run controls in the ADR. Recapture ff5bf81e for the
   signed cumulative setup reference and 48fe3b15 for execution. Report
   the execution delta against its earlier capture and retain both sets
   of receipts. Keep the frozen per-record and cumulative allowances.
4. Held-out evidence: the historical replays, the first 0316 candidate's
   `fib` failure, 0302's A and 0301's binary, the planted regressions, the
   scorer fixtures and the capture-failure controls.
5. fmt, Clippy, the controls and preflight. Sign. Then run
   `final.sh --required` on the signed commit. Review, one complete final,
   Results, which include the failed ROOT-REPLAY-v1 annotation experiment.

## Calibration manifest before counts

The published baseline is ff5bf81efba559573d0b8f0de84fd9fba919ef89.
The canonical anchor is clean01 at 48fe3b15f48c92be91f04aa97f1f2f3fc31cf860.
Both revisions use the same transforms, build and run paths, Cargo release
flags, four build jobs, pinned toolchain, fixture bytes and capture environment.
The manifest digest is 3714c87a16b0de78890a5bee7bd95affff7e2c5fdb85d2f0fe68ef79621076e0.
The observer and capture recipe digest is
20565e74fd7214de23d34ee5caa59dc715fcc088f4c32abf093bffd61d1cc35c.

On each revision clean01 and clean02 are independent clean builds.
For integer i = 1 through 12 the ASCII seed is `rnx-native-calibration-i`
with i in ordinary decimal. Seeds 1 through 8 train, and 9 through 12
are reserved. In runtime/vm.rs the eligible private names are
try_object_like_index_get, try_tuple_like_index_get, try_tuple_like_index_set,
try_object_slot_index_set, call_generator_fn, call_stream_fn, call_async_fn,
call_offset_fn, op_select, op_await, target_fallback_assign and internal_cmp.
In fmt/format.rs they are expr_labels, loose_expr_macro_call, compact_expr_macro_call,
item_struct, item_enum, tuple_body, struct_body, item_use_path, use_group_open,
condition_or_expr, close_group and is_runefmt_skip. A name is selected when
byte zero of SHA-256(seed + "/" + name) is below 128. Replace its declaration
and matching file-local identifiers by a lowercase z and the first
length(name) − 1 hex digits of SHA-256(seed + "/replace/" + name).
Only those two files may change in a calibration export. The identifiers
keep their lengths and all source line positions. The eight kernels enter
the execution engine and use neither the legacy VM nor the formatter.

Before scoring, retain ELF hashes and inspect analyze_header and cache::build
symbols/disassembly, including the possibility that cache::build has no
standalone symbol. On each revision require distinct instruction shapes or
resolved push-site choices among the ten clean/training binaries in at least
one of these functions. Differences confined to link addresses do not qualify.
Keep duplicate, inert and failed variants. Insufficient coverage on either
revision stops calibration before deriving any table.

Capture both sizes (1 and 5) twice for every clean and training layout, for
all eight kernels with native requested. Require identical outputs and
execution routes, the specified in-frame split, exact accounting and the
matching binary digest. Pair sizes within each replicate when computing
growth. The record level envelope is the maximum absolute same-size pair
difference among baseline clean/training observations and both replicates,
maximized over sizes. The cumulative envelope uses anchor observations
against both canonical clean01 replicates at the same size. Apply the
same two formulas to signed growth. Keep integer W and T limits separate
per kernel. Reserve counts and acceptance replays follow the signed table.

## Independent structural calibration manifest

The rename trial stopped at its predeclared coverage screen. Its eight
training variants left one compiler fingerprint on each revision. The
old reserve stays sealed. The structural trial uses baseline ff5bf81e,
anchor 48fe3b15, two independent clean builds at each revision and the
same scoring, repeats, fixture matrix, exclusions and production observer.

The sites are private functions in two Rune modules outside region
analysis and the protected inventory:

- runtime/vm.rs: op_tuple_n, call_offset_fn, op_call, try_object_slot_index_set.
- fmt/format.rs: expr_labels, item_struct, item_enum, struct_body,
  tuple_body, expr_continue, loose_expr_macro_call, compact_expr_macro_call.

The new seed strings are rnx-cold-packet/v2/training/1 through /8 and
rnx-cold-packet/v2/reserved/1 through /4. For each of the two file paths,
rank the named sites by SHA-256(seed + "/" + full repository path + "/" +
site name), then take the first half. At each selected site the original
signature forwards its inputs once in a stack tuple to a private
out-of-line helper. Rank the non-receiver parameters by SHA-256(seed +
"/" + full repository path + "/" + site name + "/" + parameter name)
for the tuple order. The helper unpacks the tuple and executes the
original body. Keep the receiver, return channel, source lines and
caller evaluation order. There is no new heap allocation. The generator
and both revisions' expected file hashes are bound in the manifest.
All twelve seeds give distinct site sets.

Eligibility smoke passed on the clean ELF for both revisions. All twelve
exact symbols were present. A whole-process Callgrind smoke covered the
eight kernels at both sizes with native requested on and off, with
identical checksums and matching execution routes. All 768 site/cell
self-instruction checks were zero. These captures measure site eligibility
only and contribute no phase counts to the table.

Sign this manifest before the first build. The first screen uses only
builds, required cold-site symbols and disassembly. Verify each selected
training site and its factored callee in that measured binary. Report
the number of training layouts that differ from clean01 and the number
of distinct training shapes per revision. Use the signed analyze_header
and cache::build criterion. If either revision lacks coverage, stop
before phase counts and return the evidence for user decision. On a pass
the production observer and the signed formulas govern the training
counts. The second signature precedes opening the new reserve or any
held-out acceptance replay.

Manifest SHA-256: d71461ba66bbbf9647c99d828a00b3461169d04bc24f1fc14a6b76511b885659.
Generator SHA-256: 639ef5dafe28d8de16a1ab836a7769ab90aaed72e552dcbd33e3a61049b1a39c.
Coverage runner SHA-256: a075b7ced76905fc2484b15a4f77cd0c057ff9baae6ffdbc8194ae213f6d7be9.
Capture recipe SHA-256: 6959e3ac820c4e6b92e9998763c4935bfdb056a6870cc42fab68ead54c6ed020.
All thirteen shared observer, accounting and tool inputs have their
original digests. The new manifest and scratch driver have distinct
bindings.

## Anchor layout rule for structural calibration

On 2026-11-20 the user authorized calibration from the layout change
on the fixed anchor. CALIBRATION-v2 failed its original coverage screen.
On baseline ff5bf81e, zero of eight training builds differed from clean01.
On anchor 48fe3b15, all eight differed from clean01 and shared one
alternative compiler layout. Retain that failure and both coverage receipts.
The user accepts the assumption that a comparable layout change on current
code costs approximately what it costs on the anchor. This amendment
supersedes the baseline-derived setup formula and resolves the coverage
stop for the signed structural sample.

Obtain a signature on this rule before collecting phase counts. On each
revision use clean01, clean02 and train01 through train08. Measure each
of the eight native-on kernels at sizes 1 and 5 twice with the production
observer. Require the signed binary, fixture, output, route, boundary
and accounting checks. This gives 320 captures per revision. Retain
all replicates and perform the following calculations in exact integer
instructions for each kernel, separately for W and T.

- **Per-change setup level.** At each size, compare every anchor training
  observation with every anchor clean observation at that same size.
  Include both clean builds and both replicates on each side. Take one
  maximum absolute difference across all such pairs and both sizes.
  Apply that allowance to each size separately.
- **Cumulative setup level.** Use the same calculation with clean01 as
  the sole clean control, including both of its replicates. Both budgets
  therefore draw on the same observed anchor flip. Their values can
  differ because the per-change calculation also includes clean02.
  Apply the cumulative budget directly against the fixed anchor on every
  record.
- **Setup growth.** First compute the signed size-5 minus size-1 count
  within each build and replicate. Take the maximum absolute difference
  between these growth values using the respective training/clean pairs
  above. A measured zero gives an exact zero allowance. Keep every
  observation without rounding or outlier removal.
- **Baseline report.** Report the maximum absolute same-size pairwise
  setup difference among all ten baseline builds and both replicates,
  across both sizes, and the corresponding spread of signed growth.
  Report the measured values even if nonzero. Only the anchor observations
  above determine the allowances.

Keep the signed execution bands, recombined-whole checks, migration check
and exact reconciliation. The 0310/0316 failures, split and root-replay
builds, 214 exploratory captures and 0277 figures remain excluded from
calibration. Bring the measured table, formulas and recipe binding back
for a second signature. Then check the sealed v2 reserve first, followed
by the historical replays including the 0316 fib refusal, 0302 A and
0301 integrated, the setup, fixed-execution and per-row execution plants,
and the scorer and capture-failure controls. Record a separate verdict
for each. The v1 reserve stays sealed and unused.

## Measured setup allowances for second signature

Rule commit 5d222764. Both revisions contributed 320 native-on captures. All 640 passed the production observer and reconciliation checks. An independent calculation from the per-cell receipts reproduced every integer in this table. W counts the executable and other/unknown objects. T also includes libc and the loader. Each cell gives level / growth in instructions.

| Kernel | Metric | Per-change | Cumulative | Baseline spread (reported) |
|---|---|---:|---:|---:|
| fib | W | 1645 / 0 | 1645 / 0 | 0 / 0 |
| fib | T | 77067 / 0 | 77067 / 0 | 8533 / 0 |
| loops | W | 37 / 22 | 37 / 22 | 0 / 0 |
| loops | T | 15046 / 22 | 15046 / 22 | 1297 / 0 |
| numeric_calls | W | 185 / 5 | 185 / 5 | 22 / 33 |
| numeric_calls | T | 28015 / 5 | 28015 / 5 | 5629 / 33 |
| shared_rows | W | 37 / 0 | 37 / 0 | 0 / 0 |
| shared_rows | T | 11 / 0 | 11 / 0 | 0 / 0 |
| awfy_sieve | W | 314 / 0 | 314 / 0 | 0 / 0 |
| awfy_sieve | T | 49109 / 0 | 49109 / 0 | 5564 / 0 |
| awfy_permute | W | 3970 / 0 | 3970 / 0 | 0 / 0 |
| awfy_permute | T | 213276 / 0 | 213276 / 0 | 36302 / 0 |
| vec_iter_i64 | W | 233 / 0 | 233 / 0 | 0 / 0 |
| vec_iter_i64 | T | 33096 / 0 | 33096 / 0 | 5663 / 0 |
| vec_iter_bool | W | 199 / 0 | 199 / 0 | 0 / 0 |
| vec_iter_bool | T | 27819 / 0 | 27819 / 0 | 4859 / 0 |

Both budgets use anchor training builds. Per-change compares them with both clean controls. Cumulative compares them with clean01. Level is the largest absolute same-size difference, maximized over sizes 1 and 5. Growth uses signed size-5 minus size-1 values within a build and replicate. Both replicates enter every maximum. Baseline spread is the full observed pairwise range, reported separately.

Manifest SHA-256: d71461ba66bbbf9647c99d828a00b3461169d04bc24f1fc14a6b76511b885659.
Production capture recipe SHA-256: 6959e3ac820c4e6b92e9998763c4935bfdb056a6870cc42fab68ead54c6ed020.
Anchor-rule collection and scoring driver SHA-256: d42501b5732db7167ab7d34f3ad4306b413dbdf1edf80f86658ad85de76fa80d.
Sorted relative row paths and SHA-256 digests, as compact JSON, SHA-256: f8916673338e0e68b98aca24054164ec627f25d02dba7fcf2c996b406e675c11.
Calibration receipt SHA-256: 8d372a375a2cd6d9fe1186f9157056fa85c84b62ffe4e92d302ad2e1f3da80c7.

## Second freeze for signature, 2026-11-20

The user accepted the historical setup cost after reviewing the migration failure.
From this freeze, compare cumulative native-on setup to the clean01 counts at
ff5bf81efba559573d0b8f0de84fd9fba919ef89. Apply the measured cumulative
allowance above to that fixed reference for every subsequent record. Per-change
setup still compares to the previous record. The training revision for both
allowances remains 48fe3b15. This ruling changes only the reference for cumulative
setup. Execution and the recombined whole keep their cumulative anchor at
48fe3b15f48c92be91f04aa97f1f2f3fc31cf860. The native-off rules still apply.

### Results of the setup migration

Carry the following table into the 0316 Results. It records all 16 historical
comparisons, including the accepted costs and the improvements. The three
deltas are in instructions. Each is the largest signed difference between
either published-baseline clean build and the old anchor's clean01 across
both replicates. Compute size-5 minus size-1 within each replicate before
comparing growth. A row fails the original migration check if either level
or the growth exceeds its respective cumulative allowance.

| Kernel | Metric | Size 1 delta | Size 5 delta | Growth delta | Original migration | New clean01 setup at sizes 1 and 5 |
|---|---|---:|---:|---:|---|---:|
| fib | W | +38147 | +38147 | 0 | FAIL | 45316758 |
| fib | T | +20809 | +20809 | 0 | PASS | 48557752 |
| loops | W | +1486 | +1486 | +11 | FAIL | 4728644 |
| loops | T | +2868 | +2868 | +11 | PASS | 5218517 |
| numeric_calls | W | +7283 | +7283 | 0 | FAIL | 6876180 |
| numeric_calls | T | +19594 | +19594 | 0 | PASS | 7636451 |
| shared_rows | W | -7700 | -7700 | 0 | PASS | 153004 |
| shared_rows | T | -8022 | -8022 | 0 | PASS | 160725 |
| awfy_sieve | W | +10895 | +10895 | 0 | FAIL | 15654389 |
| awfy_sieve | T | +68230 | +68230 | 0 | FAIL | 17258519 |
| awfy_permute | W | +214548 | +214548 | 0 | FAIL | 190935217 |
| awfy_permute | T | -6766 | -6766 | 0 | PASS | 200731550 |
| vec_iter_i64 | W | +5879 | +5879 | 0 | FAIL | 9505948 |
| vec_iter_i64 | T | +26755 | +26755 | 0 | PASS | 10669019 |
| vec_iter_bool | W | +892 | +892 | 0 | FAIL | 8634532 |
| vec_iter_bool | T | +13738 | +13738 | 0 | PASS | 9605698 |

Eight of 16 rows failed the original migration check at both sizes. Every
growth check passed. In each row the new clean01 count is identical at both
sizes and in both replicates, so its signed setup growth is zero.

### Historical controls after signature

This amendment supersedes the ADR control that predicted both historical
shared_rows shifts would fit the setup allowance. The expectation was
corrected before replay using the already disclosed exploratory measurements.
For 0310 and the first 0316 attempt, expect shared_rows execution to pass
its band and attribute the whole-level failure to setup. Score and report
each setup verdict against the frozen limits. The exploratory setup increases
were 1,597 W and 1,525 T for 0310, and 2,183 W and 2,183 T for 0316.
Both exceed the per-change limits of 37 W and 11 T. A setup FAIL remains
a gate failure even when the attribution control succeeds. These exploratory
observations supply neither calibration data nor replay verdicts.

After the second signature, check the sealed v2 reserve first. Follow with
the two historical shared_rows replays, the first 0316 native-on fib
execution refusal, 0302 A and 0301 integrated, then the setup, fixed-execution
and per-row execution plants. Finish with the scorer and capture-failure
controls. Record a separate verdict for each. The v1 reserve stays sealed.

## Results

The fixed-route gate now scores native-on workloads in two phases,
measured directly at the entry of the outer `Executor::invoke_args`
closure:
- one-time native setup (region analysis, the cache decision, emission);
- execution (frame entry, the dispatch loop, export and teardown).
Setup plus execution reconciles exactly with the whole invocation, and
with Callgrind's process total.

Execution keeps the 0.5% per-record and 1% cumulative bands on W and T,
the growth checks, and the recombined-whole checks. Setup has its own
per-change and cumulative limits, calibrated from independent layout
changes. A saving in one phase cannot pay for a regression in the other.

The user's rulings shaped the record:
- hit the twice-recurring `shared_rows` failure head on rather than
  routing features around it;
- measure the phases directly, with separate budgets;
- freeze the limits on independent calibration before replaying the
  historical failures;
- size the setup limit from the anchor's independent layout flip;
- re-anchor cumulative setup at the published ff5bf81e and record the
  historical setup growth.

The acceptance criterion was the user's: the gate names the phase that
got more expensive, and a real regression in either phase can still stop
publication.

The Rust scorer reads both Callgrind dumps for each native-on case through
the production observer's accounting. The cache owns both profiles, the
ordered debugger receipt, process stderr, fixture, stdout and binary.
Schema 1 retains its whole-invocation scoring. Schema 2 uses the separate
setup limits, execution bands and recombined checks in this record.

ROOT-REPLAY-v1 failed. Forcing the push to inline stabilized that call
edge while 274 to 390 instructions of drift remained. The rename-only
calibration also failed its coverage screen. The structural sample
exposed a compiler-layout flip on the anchor only. The failed screens
and the signed decision to use the anchor flip remain part of calibration.
The table above records all 16 original setup migration comparisons,
including the eight failures and the re-anchor at the published baseline.

The sealed v2 reserve passed all 9,728 checks on 256 captures. The earlier
v1 reserve stayed sealed. The held-out replays used the second signed
freeze. Both historical shared_rows changes passed execution and failed
setup: +1,597 W / +1,525 T for 0310 and +2,183 W/T for 0316. The historical
expectation had been corrected before replay using the disclosed
exploratory counts. The native-on fib replay also exposed a size-5
execution regression of 389,780 W against a 370,237 allowance. The 0302
A replay retained its execution failure (+1,296 W), and 0301 integrated
retained both its setup and execution failures (+13,221 and +19,002 W).

The setup plant added 7,179 instructions. The original fixed-execution
plant also changed the loop slope (+784). Its replacement used the same
ELF for OFF and ON and added exactly 7,170 instructions at each size
with zero setup and growth deltas. The per-row plant added 9,075 at
size 5 and 7,260 to growth. The replay scorer passed 57 controls, and
the observer passed 15 capture controls. The original failed plant and
the interrupted hard-link staging attempt remain in the evidence.

### Published baseline in the production capture

The initial migration under the calibration clean01 reference failed
two of 32 cumulative rows. At both sizes shared_rows T exceeded its
setup allowance (+285 against 11) and awfy_sieve W exceeded its setup
allowance (+370 against 314). Every required execution and recombined
check passed. The shared_rows difference was entirely in libc. The
sieve difference in W came from one unidentified address.

A crossed diagnostic used both binaries built from the same ff5bf81e
source archive (SHA-256 1f3b733135533fdeb4417b8fb19be6adf3a12070870679bcc258f26ddeaecd53).
Each binary ran at both original path configurations, twice at each
size, with identical script bytes. The first diagnostic wrapper added
PWD to the capture environment and failed to reproduce the original
levels. The corrected wrapper verified the ordered environment inside
each private mount. All 32 captures reproduced their respective counts.
Changing paths added exactly 285 or 370 instructions as above. Changing
the binary at a fixed path changed the affected metric by zero. These
observations attribute the differences to the combined path configuration.

Before that diagnostic the decision was made to use the published
production capture for cumulative setup if the difference followed
measurement context. The production binary SHA-256 is
e2de11eb544ad2bcf65073a8fdd3f8a7fe48e3f8b32ee0af02867f50256bfb4f.
The reference values for sizes 1 and 5 follow. Every setup allowance
still comes from the signed calibration table.

| Kernel | Metric | Calibration reference, 1 / 5 | Production reference, 1 / 5 |
|---|---|---:|---:|
| fib | W | 45316758 / 45316758 | 45316936 / 45316936 |
| fib | T | 48557752 / 48557752 | 48472850 / 48472850 |
| loops | W | 4728644 / 4728644 | 4728630 / 4728630 |
| loops | T | 5218517 / 5218517 | 5215369 / 5215369 |
| numeric_calls | W | 6876180 / 6876180 | 6876180 / 6876180 |
| numeric_calls | T | 7636451 / 7636451 | 7633804 / 7633804 |
| shared_rows | W | 153004 / 153004 | 153004 / 153004 |
| shared_rows | T | 160725 / 160725 | 161010 / 161010 |
| awfy_sieve | W | 15654389 / 15654389 | 15654759 / 15654759 |
| awfy_sieve | T | 17258519 / 17258519 | 17218292 / 17218292 |
| awfy_permute | W | 190935217 / 190935217 | 190935249 / 190935249 |
| awfy_permute | T | 200731550 / 200731550 | 200741466 / 200741466 |
| vec_iter_i64 | W | 9505948 / 9505948 | 9505920 / 9505920 |
| vec_iter_i64 | T | 10669019 / 10669019 | 10663256 / 10663256 |
| vec_iter_bool | W | 8634532 / 8634532 | 8634602 / 8634602 |
| vec_iter_bool | T | 9605698 / 9605698 | 9593662 / 9593662 |

The production-format anchor at 48fe3b15 was freshly captured alongside
the published ff5bf81e baseline. The cold cache accepted the anchor and
a reopened cache verified its 145 artifacts and hit the same key.
Rescoring the retained captures under the production reference passed
32/32 cumulative rows. The per-record self-comparison also passed 32/32.
A replay using the Rust receipt comparator accepted the verdict and
rejected a corrupted verdict and a corrupted phase total. The observed
path effects apply to these measured path configurations. The complete
final uses its own paired builds and capture paths.

The first path guard keyed the physical payload and role paths. A fresh
shared_rows size-1 control with the same ELF measured setup T of 161,010
under the production launch and 160,725 under the calibration launch.
The scorer refused the mismatched launch before scoring. Each ordinary
final allocates new payload directories, making this physical reference
unsuitable for comparing future captures.

Under signed plan ebd3289d each measured process runs in its own mount
namespace. CONFIG.json records real inputs and outputs. LAUNCH.json records
the actual argv, cwd and ordered environment. Both are retained in each
capture receipt. The cache binds the measured launch and content digests.
The signed fixed_route_policy.json contains the recaptured ff5bf81e setup
values and one launch digest per mode. All frozen allowances were retained.

In CANONICAL-v1 the same ELF at different real paths had identical setup
and execution W and T at both shared_rows sizes (setup T 160,851 each).
Changing the order of its environment produced an incomparable-capture
refusal before scoring. Both roles had identical launch bytes at all 32
matched cells. Migration scored 32/32 for both per-record and cumulative
comparisons; the reopened execution-anchor cache verified 225 artifacts.
Relative to its pre-layout capture, execution anchor 48fe3b15 had exact W
in every cell. T moved in 12 native-on cells (−156 to +6 instructions)
and eight native-off cells (−484,731 to +327). The largest change was
native-off awfy_permute at size 5. ANCHOR-MOVEMENT.json lists every cell.

Consequences:
- The 0310 and 0316 `shared_rows` flips are not noise. By independently
  measured layout noise (a `shared_rows` setup limit of 37 W and 11 T),
  their +1,597 and +2,183 setup instructions are real setup regressions,
  and the gate now rejects them as setup.
- The first 0316 binary-family candidate also carried a real execution
  regression on native-on `fib`, which the old whole-invocation gate
  diluted with about 48 million setup instructions. The binary family,
  now ADR 0317, must address both costs.
- The first fixed-execution plant showed that unrelated code can also
  move execution-loop code generation (+784 growth on `shared_rows`).
  The execution bands catch such a slope as a real cost.

A required final on 3d8d2d79 passed its six selected stages in 2.6
minutes. The complete final on the same commit then passed all 101
stages on its first attempt, including the fixed-route gate scored by
phase under the canonical launch layout. All 32 per-record and 32
cumulative rows passed. For example, native-on `fib` execution was
allowed 370,237 instructions at size 5, 5 per mille of 74,047,589, which
is the allowance that rejects the first 0316 candidate. The final took
43.5 minutes of accepted run and 44.5 end to end, with no memory.high,
memory.max or OOM events.

No product source, engine code, codegen flag or inventoried item changed.
Every change is in `tools/` and `plans/`.
