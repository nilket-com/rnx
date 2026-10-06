# rnx 0175: integer range-loop specialization evidence

**Result: STOP. No optimization is merged into Rune main or shipped by rnx.**

The guarded exclusive-i64 range-loop candidate passes the retained semantic gates and removes most loop-result allocations. It also regresses unrelated call-heavy workloads. The frozen precedence rejects it even though the numeric improvement thresholds are met. This record does not reopen the earlier STOP records or change the release profile.

## Sources and implementation

The engine-development base is `bb8e69372353c50e271c9f115bc771c77aa6b83e`. The common tests parent is `3e7d4da9ce908eeb7e0e1ac119dec24f68d5449a`. Base-first tests land as `c6f57bd0`, `c2b68025` and `eaa59fc208c136ead86f8c4fa565431ea18de88b`; the measured baseline is the last of those, with unchanged production code. One candidate performance commit, `863370da279031b369ebb0ac1c0d80cd9ca159f6`, follows the tests. The named candidate branch is retained; fork main stays unchanged.

The candidate tags only the built-in exclusive `RangeIter<i64>::NEXT` handler in the existing callable vtable. Hash prefilters reject unrelated calls; acceptance still requires the actual resolved handler's tag, admitted receiver, arity, output slots and matching `IterNext`. Unit-function precedence is preserved. Two original instruction charges remain. A successful Some result skips its transient heap Option only when both boundaries can run safely; a failed second permit materializes the ordinary Option, and None follows ordinary dispatch. No persistent pending-result state or new bytecode is introduced. No unsafe code is added.

Every explicit memory-limit scope uses ordinary dispatch, including an explicit `usize::MAX` limit. A std-only hidden public `rune_alloc::limit::__explicit_scope_active` query is the reviewed cross-crate bridge; it is unsupported runtime-internal plumbing, not a stable API promise. Physical allocator failure points necessarily differ when allocations are removed. Tracing-enabled and no-std builds never specialize. The measured Rune feature set is `alloc,anyhow,fmt,serde,std`, without tracing.

## Correctness and allocation gates

Retained tests cover budget halts and one-permit resumption, raw stack and iterator bounds, borrow failures, output destruction, explicit allocation limits, nesting/unwind/poll restoration, diagnostics, unsupported/custom iterators and malformed units. The unit-shadow test was captured on the base first and demonstrated the original candidate's failure before the precedence fix. The original boundary goldens were not regenerated to fit the candidate.

The final all-features candidate suite passes 610 unit tests plus two integration tests. The named non-tracing configuration passes all 15 range tests, with positive specialization hits. All-features tests assert ordinary fallback. The candidate also passes the retained no-std allocation-only check. The independent build rehearsal repeats the base and candidate full suites (611 and 612 total passed respectively) and the 15 non-tracing tests. The stdio-on/off inventory golden and its negative comparison control pass on both subjects.

The official run retains 40 correctness comparisons: 28 origin-qualified corpus inputs, eight historical fixtures, and four range/manual-next fixtures. All agree; the latter use independent Python arithmetic oracles. Only the two previously frozen diagnostic normalizations are applied.

Separate counting builds report:

| Control | Base allocation calls | Candidate calls | Result |
|---|---:|---:|---|
| Original numeric range loop | 1,036,660 | 36,661 | −96.46% |
| Default budget | 1,036,660 | 36,661 | completes identically |
| Unlimited budget | 1,036,661 | 36,662 | completes identically |
| Zero budget | 36,659 | 36,660 | halts identically |
| Tight budget, 1,000 | 36,773 | 36,664 | halts identically |

Context construction adds exactly one temporary registration allocation; floor and empty-context add zero. Manual next retains its independent Options and changes by precisely that context allocation. The signed and negative range controls each remove 999,999 calls. Strings also contains range loops and removes 39,998. Other workloads stay within the explicitly reviewed +1 construction qualification. Tight halts are not claimed allocation-free.

## Method, attempts and controls

Claude authored the measurement tooling and performed the official run; Codex implemented the engine candidate and independently audited the retained results. Tooling is in rnx-bench `probes/range-iteration-0175`; base-first/source-test receipts are in `probes/integer-range-0175`. Both histories are preserved by the local integration merge, without amending Claude's commits.

Builds use one checked-out fork path, independent cleaned base/candidate targets, the default release profile and retained verbose feature commands. Subject binaries, clock source/binary, source revisions, build and inventory receipts, harness/fixtures/oracles and Lua binaries are frozen in `subjects.json`. The resident clock source is unchanged from the pinned 0169 observer. No wall workload warm-up block is used, as an explicit pre-timing amendment applying equally to both subjects.

Five true ABBA repetitions retain ten PMU samples per side across 21 workloads (420 samples), `instructions:u` and cycles, at least 99% counter running. Wall uses three ABBA/BAAB/ABBA rounds of five-sample blocks: 30 samples per side, 252 raw blocks. Controller/children are pinned to core 4 with ten representative affinity controls. Commands use the frozen minimal environment. Raw output, status, plan/argv receipts and hashes are saved before validation. Process-group cleanup is bounded and gated. Historical reproduction retains 30 FIFO and 24 whole-process samples; all applicable rows satisfy the 2% gate. The tiny floor/empty FIFO changes (+19/+21 instructions) remain descriptive. Pipe-clock calibration differences are −0.009 and −0.003 ms, within the unchanged 0.15 ms gate.

Driver review caught missing failed-sample retention, potentially blocking FIFO line reads, insufficient build binding, an unasserted context allocation qualification, dropped counting-output checks and ungated cleanup facts. Untimed injected-failure controls verify repairs. `controls1` failed two lifecycle checks because zombie membership was mistaken for live membership; that failed rehearsal remains retained. Later controls settle boundedly and distinguish live processes from zombies. Final controls3 passes 14/14; rehearsal4 retains 137 clean process rows. Earlier rehearsals are preserved.

Official attempt 1, tooling `513d0c44`, stops at the first historical FIFO reproduction sample. A newline parser left perf's trailing NUL acknowledgement padding pending, so disable appeared as `\0ack\n`. Correctness and allocations had passed; no deciding candidate PMU or wall samples had begun. The protocol repair was reviewed before replay: strip boundary NUL padding, retain raw acknowledgement bytes, and exercise real perf controls against a mock child plus a malformed acknowledgement negative control. Attempt 1 remains unchanged in `official1`.

Official attempt 2 runs from clean committed tooling `640a792d`; results are committed as `4f814d58`. It completes with process exit 0 and a scientific decision of STOP. Those are distinct. Sentinel scans report zero occurrences. The synthetic security controls do not establish rotation of the credential from the separate earlier incident; that remains unconfirmed.

## Frozen decision

| Workload | Instruction change | Wall change | Relevant result |
|---|---:|---:|---|
| Numeric | −30.56% | −40.14% | 113.2 → 67.8 ms |
| Signed range | −37.48% | −49.99% | 84.7 → 42.4 ms |
| Negative range | −37.48% | −50.05% | range target met |
| Fibonacci | +8.56% | +8.21% | 52.0 → 56.3 ms; exceeds base band |
| Calls | +3.96% | +4.94% | 94.9 → 99.6 ms; exceeds base band |
| Context | +2.09% | +0.52% | instruction gate fails; wall inside band |
| Runtime | +1.81% | +0.51% | instruction gate fails |
| Compile answer | +1.54% | +0.43% | instruction gate fails |
| Run answer | +1.54% | −0.55% | instruction gate fails |
| Run empty | +1.80% | +1.21% | instruction gate fails |
| Overwrite alias | +1.52% | +0.07% | instruction gate fails |

There are no opposing instruction/wall disagreements under the frozen rule. The numeric allocation and instruction improvements satisfy the improvement thresholds, but unrelated regressions take precedence. No averaging, profile substitution or relaxed gate changes this STOP.

Codex's independent `audit.py` recomputes every PMU instruction/cycle summary from raw perf rows, checks ABBA orders, recomputes wall medians and base p10–p90 widths from resident receipts, verifies lifecycle/affinity/hashes, and reproduces the exact regression list and STOP. The audit is read-only; it is not another experiment.

The startup instruction pattern closely resembles 0174's default-profile S71 pattern, but this record does not establish its cause. Hot-dispatch prefilters and code generation are hypotheses for the call-heavy regressions; no assembly attribution was performed. A future outlined fast path or paired-profile study needs its own plan. Neither hypothesis waives this result.

Contemporary Lua references use the same resident clock and are not gates: Lua 5.4.7 numeric/while medians 6.42/9.64 ms, LuaJIT 8.28/8.29 ms. These are complete process-plus-workload windows, not measurements of interpreter startup alone or general language rankings.

## Closure

Retain the candidate as a named non-main fork branch and publish the generic evidence and benchmark receipts after peer review. Do not merge the optimization, migrate rnx's shipped dependency or claim a runtime speedup for users. The candidate demonstrates an allocation opportunity and an unsuccessful implementation under the frozen no-regression contract.
