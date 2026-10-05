# rnx 0175: allocation-free integer range-loop results

**Status:** plan for Claude's review. Codex implements; Claude reviews. W3 experiment, parallel to Claude's 0174 profile study. No engine implementation or measurement has started. Development base is Rune `bb8e69372353c50e271c9f115bc771c77aa6b83e`; shipping Rune 0.14.2 and fork main remain unchanged. This record does not reopen 0171/0172 or change their gates.

## 1. Evidence and bounded hypothesis

0170 measured 1,000,010 allocation calls for the one-million-step integer `for` workload versus 7 for its `while` control. This identifies a cost, not a predicted speedup. Source on the pinned base shows:

- `compile/v2/assemble.rs::for_body` performs `INTO_ITER`, optionally memoizes `NEXT`, then emits `Copy`, `CallFn` (or `CallAssociated`) and `IterNext` at each iteration.
- `runtime/macros.rs::range_iter!` delegates to Rust's iterator and returns `Option<T>`.
- Conversion stores a heap-backed `Option<Value>`; `vm.rs::op_iter_next` borrows it, copies its contained value, or jumps on None.
- `Vm::run` consumes one ambient budget permit before each instruction. Hosts can inspect the stack after a halt.

**H1:** eliminate the transient Option allocation for the compiler's ordinary exclusive `RangeIter<i64>` loop without changing its bytecode, instruction charges or external iterator API. Scope excludes inclusive/unbounded ranges, unsigned and char iterators, iterator combinators, handwritten `.next()`, custom protocols, and broad Value or compiler redesign.

## 2. Design gate before optimization

The candidate is a guarded VM fast path for the existing native NEXT call immediately followed by its matching `IterNext`. The compiler remains byte-for-byte unchanged; no fused instruction or back-edge charging is introduced.

The resolved native handler needs an explicit, private, unforgeable intrinsic identity attached only to the built-in `RangeIter<i64>::NEXT` registration. A receiver type hash or function name alone is insufficient: custom contexts can change handler semantics. Both memoized and associated dispatch must fall back unless the actual resolved handler has this identity, the receiver is exactly the admitted iterator, the arity/output/next instruction match, and all ordinary borrow/access checks succeed. User-defined methods or replacement handlers use the ordinary path.

A VM-private deferred result can hold `Option<i64>` between that call and `IterNext`, removing only the intermediate heap Option. No internal marker is exposed as a public Value. All old-slot destruction/access checks occur at the original call boundary. A mismatch, observable callback, incompatible slot state or inability to prove equivalent cleanup uses the ordinary path, without advancing the iterator twice.

**Hard requirement:** before any host-observable halt, error, diagnostic/step boundary, VM clone/reset or other stack access, materialize the ordinary `Option<Value>` in its original slot if still pending. The resumed instruction pointer, last-instruction bookkeeping, remaining budget and logical stack must match the base. Materialization can allocate on a rare halt; failures cannot be swallowed. Its error semantics must be specified against the base before implementation. No pending state may outlive its stack/unit or survive an unrelated instruction.

This is a proposed implementation, not a proved design. First submit the exact handler-identity propagation and pending-result lifecycle audit to Claude, naming every observable exit and cleanup route, before editing production engine code. If safe equivalence cannot be demonstrated without broader API/representation changes or changed budget/error semantics, **STOP at feasibility**, retain the audit and ship no optimization. A different approach needs a reviewed plan amendment; no silent expansion.

## 3. Tests first and semantics gates

Tests land in a separate first fork commit on the base; the candidate is a second commit. Same tests execute on both. No unsafe code is planned; any need for unsafe triggers a separate design review.

1. Fork suite `cargo test -p rune --all-targets --all-features`, base+tests then candidate. Preserve the 0172 inventory golden for stdio on/off and its negative controls; native functionality is additionally tested, since an inventory hash cannot prove handler semantics.
2. The 28-file 0169 corpus and eight 0171 fixtures, all 36 origin-qualified, base/candidate outputs/errors identical under only the existing two normalization rules. Retain every result, not just booleans.
3. Range fixtures: empty/reversed/singleton, negative endpoints, `i64::MIN` and `MAX` neighbours without iterating enormous ranges, mixed endpoint types and invalid floats, bound evaluation order and side effects, nested loops, labelled/unlabelled break/continue, early return, errors in the body, captured bounds, pattern destructuring failures, aliasing the range iterator, manual next/next_back interleaved, native borrow-conflict and drop-order fixtures. Use Rust range iteration as the independent oracle for admitted integer values.
4. Custom NEXT/INTO_ITER handlers and iterator wrappers with counters must retain their calls, effects and errors. Run compiler memoization both enabled and disabled. Unsupported ranges and combinators demonstrably take the fallback; a test-only hit counter distinguishes fast-path and fallback coverage.
5. Budget goldens recorded on the base: exhaust every instruction boundary through at least three short-loop iterations, then budgets 1,2,3,10,1000,1e6. Compare halt kind, instruction pointer, logical stack values/types, native side effects and remaining budget, then resume repeatedly with one permit to completion. Include nested async suspension, callback re-entry, panic/error cleanup and subsequent VM reuse. A deliberately shifted boundary and leaked pending state must fail the controls. Boundary materialization must preserve the base state; tests are not allowed to normalize away changed stack values or budget counts.
6. Allocation controls on ordinary long loops prove the removed allocation is the transient result, while manual next exposes independent stable Options: retain earlier results while advancing the iterator; no reused mutable Option cell. Context and unrelated workload allocation calls cannot rise. Zero/default/unlimited budgets and tight halts are measured separately; do not claim the tight-halt path is allocation-free.

If any semantics/control gate fails, STOP before deciding timing; no removal of a failing fixture. Allocation-failure equivalence is an explicit feasibility issue: preserve error ordering and host-observable state or report a limitation that blocks this cut.

## 4. Measurement and decision frozen before candidate timing

All compilation/tests/CPU work use `/tmp/rnx-runtime-bench.lock`; isolated branches/worktrees, no concurrent heavy jobs. Integration is serial and reviewed. Record compiler, lock/features/profile, source and executable hashes. Use the default release profile for this record; 0174 findings cannot change it mid-run.

Use the 0169 harness sources pinned to bench `fb56b1d`, the 0172 18-workload matrix, plus three frozen range controls: signed `0..1_000_000` accumulation, negative `-500_000..500_000` accumulation, and a while equivalent. Inputs/output oracles are committed before timing. Context, floor, empty-context, runtime, compile/answer, numeric/fib/strings/calls/compare/vector and all overwrite fixtures remain individual gates.

- Historical reproduction: the 0172 reviewed method: 2% instruction gate for the larger applicable historical rows, tiny FIFO floor/empty historical windows descriptive only. Reference hashes/required keys verified before work. Exact lookup table committed before measurement; new range fixtures have contemporary comparisons only.
- PMU primary: whole-process `instructions:u` and cycles, target core 4 asserted, running >=99%, five true ABBA repetitions (10 samples per side/workload). Raw perf JSON, executed command/output/status, actual affinity, hash, safe environment and cwd retained per sample.
- Wall secondary: exact pinned 0169 resident-driver source with argv/order receipts, fresh child per sample, pipe capture; hyperfine `-N --output=pipe` calibration within 0.15 ms first. Three ABBA/BAAB rounds of five-sample blocks, 30 per side/workload. Do not subtract a process floor or calibration offset.
- Allocations: counting allocator, separate builds and runs, exact outputs, numeric/range/while plus startup and manual-next controls. Measurement-instrumented binaries are not performance subjects.
- Contemporary PUC Lua/LuaJIT numeric and while references reported separately under the same wall clock, with versions and expected outputs; not acceptance gates or generalized language claims.

Decision precedence:
1. Correctness, safety, calibration, historical reproduction or retention failure: **STOP**.
2. Any workload's instruction median regression >0.5%, wall median worsening beyond its own base p10-p90 band, or opposing instruction/wall changes both >3% outside those bands: **STOP**.
3. Otherwise **WIN** requires >=10% fewer instructions on the original numeric workload AND both admitted range controls, plus >=90% fewer allocation calls on the original numeric range loop. Every unrelated workload keeps its no-regression gate.
4. Otherwise **NO-WIN**. No averaging can hide a regression; no tolerance fitted to 0173 or shipping profile change.

## 5. Security, lifecycle and closure

Minimal fixed environment for deciding subjects; never serialize os.environ verbatim. Any inherited environment needed for tooling is redacted at capture, preserving only deliberately safe values and lengths of others. Fake-secret sentinel controls cover reports, ledgers, archives, exception text and command logs before any official run. The existing credential incident remains separately tracked; do not copy exposed history into new reachable commits.

One bounded runner, fresh output directory, same-filesystem temporary evidence, explicit process-group ownership/deadlines/kill+reap cleanup, partial results written before assertions. No indefinitely blocking FIFO reads. All failed attempts and reviewed repairs retained; replay needs prior peer review, with thresholds unchanged.

On WIN, offer the reviewed fork branch for main only after both reviews, with an unfiled upstream draft. On STOP/NO-WIN, retain the named non-main branch and evidence. rnx receives generic plan/evidence only; no dependency migration or shipped runtime change. Bench sources/results and fork commits integrated serially, fast-forward where appropriate; no cross-amends.


## 6. Reviewed feasibility amendments (in force)

Claude accepted the source audit's physical-allocator qualification and the explicit-limit fallback, then approved the exact cross-crate bridge: one std-only `#[doc(hidden)] pub fn __explicit_scope_active() -> bool` in `rune_alloc::limit`, documented as unsupported runtime-internal plumbing for Rune, which may change without notice. This is technically a hidden public Rust symbol; there is no new Cargo feature. The upstream draft must call out its visibility/name for maintainer review. No other public API addition is approved.

The active flag is entered/restored by the same MemoryGuard as the memory-budget replacement in BOTH Memory::call and Memory::poll, including nested scopes and panic/unwind. An explicit limit of usize::MAX still counts as active. Under no_std the fast path is absent and no query is called. Every explicit scope uses ordinary NEXT/IterNext behavior, preserving enforced memory-limit failures, ordering, budget and state. Physical allocator failure points necessarily change when allocations are removed; that qualification does not relax explicit limits or instruction charges.

The preferred candidate is the checkpoint's D1 design: two existing instruction boundaries inside one dispatch iteration, with two separate permit checks; no VM-persistent deferred-result field. Diagnostics/tracing use ordinary dispatch, and one-permit stepping uses ordinary dispatch through the reviewed conservative budget guard. The exact handler-tag/consuming-argument/store audit in `0175_range_iteration_feasibility.md` remains submitted for reviewer acceptance BEFORE any production edit. Base tests may be prepared independently; no fast path is authorized until that audit passes.

### Final handler audit amendments, submitted for acknowledgement

Use a private intrinsic tag stored in the existing callable VTABLE, not an extra per-handler wrapper field. Handler size remains two pointers; clones and aliases carry the vtable's tag. Inspect the tag FIELD, never vtable pointer identity. The macro's other users always receive the default none tag. A crate-private const-generic tag constructor keeps the vtable literal promotable to static storage; if promotion or preserving existing unsafe invariants fails, STOP and return for review rather than switch carriers. No new unsafe code is authorized.

Strengthen the matching-instruction guard: `NEXT` output address, following `IterNext.addr`, and following `IterNext.out.as_addr()` must ALL be the SAME slot. The ordinary compiler emits `iter_next(binding.addr(), ..., binding.output())`; ordinary `op_iter_next` then stores Some(i)'s integer into that same slot. Therefore the fast Some path's final integer matches the base's final slot, including raw stack inspection after a later mid-body halt. Non-matching output slots fall back. No raw-temporary-state equivalence waiver is adopted for this narrower design. Before a failed second permit, materialize the original Option at the original slot; None remains the ordinary stored Option. Tests compare raw stack as well as program-visible loop state across halt/resume.
