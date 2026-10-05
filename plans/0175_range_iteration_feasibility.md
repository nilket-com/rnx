# rnx 0175 feasibility checkpoint: exact range NEXT boundaries

**Status:** source audit only, for Claude's review. No production code edited, no build or benchmark run. Base `bb8e69372353c50e271c9f115bc771c77aa6b83e`. The accepted plan is `0cb2228`; this checkpoint incorporates reviewer D1-D4 but requests a ruling on one unresolved contract issue.

## 1. Choice of design

Prefer D1's two existing instructions executed inside one dispatch iteration over a VM-persistent pending result. The persistent design makes stack/clone/reset/error/suspension surfaces much larger. D1 can keep its transient Rust `Option<i64>` in local scope, consume a second permit at the original boundary, and return with no pending VM state.

This is not permission to fuse budget charges: the bytecode stays unchanged, and each original instruction still consumes its own permit. `run()` currently consumes a permit, decodes `instruction_at(ip)`, advances `ip`, records `last_ip_len`, traces, then dispatches. The admitted fast path must repeat those steps for the following matching `IterNext`, with identical jump translation and original failure order. A failed second permit must leave the base's post-NEXT state visible.

## 2. Handler identity and scope

`FunctionHandler` is a dynamically generated callable wrapper (`runtime/mod.rs`); functions propagate through module metadata, Context/RuntimeContext maps and `Function::Inner::FnHandler`. The candidate cannot identify the native NEXT by associated hash alone. Proposed representation: a crate-private typed intrinsic tag attached only to the actual built-in handler, carried with that handler when cloned/memoized, inaccessible to external handler constructors. Default tag is none. Mark only `RangeIter<i64>` NEXT in the std::ops module; every other iterator/handler is untagged.

Before production edits, review the precise wrapper/tag locations: changing the shared wrapper affects unrelated functions and startup, so those costs remain subject to every workload gate. If tagging cannot be propagated without public contract or broad representation changes, STOP rather than replace identity with an unsafe hash assumption.

Receiver must downcast to exactly `RangeIter<i64>` and reproduce native argument consumption and mutable-borrow checks. Iterator aliases still share advancement. The macro's existing `next()` delegates to Rust's range; reuse it, do not hand-code arithmetic. The following instruction must be the exact `IterNext` consuming this call's output, with valid non-aliasing argument/output addresses and an admitted temporary slot state. Otherwise use the original native call without advancing anything first.

## 3. Observation and cleanup boundaries

- **Diagnostics/stepping/tracing:** ordinary path whenever VmDiagnostics is installed, a trace subscriber can observe the instruction sequence, or a stepping entry point is active. Even if no current callback observes the stack between these two instructions, do not specialize diagnostic execution. Control proves no fast-path hit there.
- **NEXT borrow/access failure:** return the original error with CallFn/CallAssociated's original ip/last_ip/budget, without taking the second permit. Match argument destruction/borrow-guard scope first; do not validate away an original error after consuming the iterator.
- **Second permit unavailable:** base constructs/stores its Option BEFORE discovering the unavailable permit. This ordering is the unresolved allocation issue below. A simple 'materialize after failed take' is not sufficient without a proof for allocation/store failures.
- **Success:** second instruction's ip/last_ip tracing bookkeeping, slot overwrite/drop and jump must match base; exhaustion must leave the final None Option as the base does if that slot survives. The None/cleanup case may use fallback rather than remove its allocation. No leaked mutable borrow or pending result.
- **Invalid bytecode/instruction lookup/translation/store:** preserve error state and remaining permits, or fall back before mutation. Optimizing well-formed compiler patterns doesn't allow corrupt bytecode to panic or reorder errors.
- **VM return/error/unwind/clone/reset/stack access:** D1 has no cross-call pending state. Diagnostics fallback and ordinary unwind guards must still leave iterator and stack in the base state.

## 4. Allocation-limit obstacle: a concrete contract issue

`ToValue for Option<T>` (`runtime/to_value.rs:188`) constructs the Option and calls `Value::try_from` during native NEXT. That allocation is fallible. `rune_alloc::limit::with` installs an explicit thread-local memory budget; a depleted limit can make NEXT fail BEFORE the next instruction's budget check. Removing that allocation would let an execution continue that previously failed, or return Limited instead of allocation error. This is observable, unlike merely changing allocation counters.

A fallback only when `limit::get() != usize::MAX` is not a valid active-limit detector. `limit::get()` returns remaining bytes, and even the default unbounded sentinel decreases when Rune allocations are live. It cannot distinguish a deliberately huge finite budget from the default budget with resident allocations.

A read-only **active explicit memory-limit scope** query could make the ordinary path mandatory during every limited execution, preserving these errors and budget boundaries. It would need correct nesting, restoration and future poll enter/exit behavior in rune-alloc. Under no_std the optimization could conservatively be disabled. This is an additional cross-crate contract surface outside the original production scope and needs an explicit plan amendment; it is NOT implemented or silently authorized by this audit.

Even with that fallback, strict equivalence of physical allocator exhaustion at every removed allocation point cannot be promised: an allocation-free path does not ask the allocator to fail. This is intrinsic to allocation elimination. The plan's hard requirement must distinguish explicit Rune memory-limit behavior (preserved by fallback) from physical OOM/fault-injected allocator behavior. If identical physical allocator failure points are required, this allocation-removal design is infeasible and 0175 must close STOP at feasibility.

Alternative: execute the fast path only under a new opt-in VM setting that promises no explicit memory limit. That changes public API and weakens default coverage, so it is not proposed as a silent escape hatch. A broad inline Option Value representation is likewise outside this record.

## 5. Reviewer ruling requested before any production change

Preferred next step, if approved: amend the scope to allow the small read-only explicit-limit query; fallback for all limited scopes/diagnostics/stepping and for rare exhausted-iteration cases; qualify physical allocator failure-point equivalence as impossible when allocating less, without weakening explicit resource limits or instruction budgets. Add nested memory-limit/future-poll and boundary-error goldens before any fast path. Complete the handler wrapper and exact argument/store audit before production edits.

If that amendment is not acceptable, retain this audit and close feasibility STOP, with no candidate benchmark or merger. No optimization result is claimed.

## 6. Reviewer smaller points

D2 diagnostics fallback is mandatory above. D3 hit counters use cfg(test) ONLY; no non-default counter feature in measured libraries, no atomic/branch/counter in release. Record the release executable/.text hashes and verify release source configuration excludes the instrumentation; testing hits never supplies performance samples. If a feature-enabled binary is ever used, it must independently match the uninstrumented .text or be excluded.

D4: unrelated codegen/layout changes are a realistic risk, but an observed whole-process instruction delta cannot be attributed specifically to unchanged VM symbols without instruction/assembly evidence. Report the change as a measured candidate-associated whole-process difference, with mechanism unproven, and honor the >0.5% STOP rule. 0174 cannot change this record's profile/gates mid-run.

## 7. Second audit: handler, argument and stores (still read-only)

Proposed handler carrier: keep `declare_dyn_fn!`'s existing pointer/vtable implementation unchanged under a private inner handler name; expose the existing `FunctionHandler` callable/Clone/TryClone/Pointer surface through a safe wrapper with a private `Option<Intrinsic>` tag. The wrapper defaults to no tag; tag constructor/setter is crate-private. This adds no unsafe block and changes no native call signatures. A wrapper may grow every handler's size, so startup allocation/size and all workload gates apply; no exemption. An alternative descriptor map would allocate and needs separate review; not silently substituted.

Mark the built-in via a private metadata adapter around `RangeIter::<i64>::next__meta`, modifying ONLY its associated function data's handler tag before standard `Module::function_meta` insertion. This leaves keys, metadata, conflicts and insertion order unchanged. Custom module function metadata cannot attach the private tag. `Context::insert_native_fn` clones the actual handler into all aliases; `RuntimeContext` moves that same map; `Function::from_handler` stores that same clone in `Inner::FnHandler`. Add a crate-private access method returning a borrowed resolved handler for `Function`; arbitrary Function/offset/closure entries do not acquire a tag. Both dispatch paths consult this resolved object, never synthesize identity from a script-visible hash.

Native NEXT's actual argument order is in `function/mod.rs::access_memory!`: check arity, obtain the mutable argument slice, replace the slot with `Value::empty`, convert that owned value into a mutable receiver with BadArgument context, advance the Rust iterator, drop its guard, convert Option to Value, then `Memory::store` with a FRESH Worklist. The specialization must follow this same consuming-argument and guard-release order. Use safe `Value::into_mut` if its RuntimeError/BadArgument wrapping matches the generated conversion; test the exact conflict/type errors against base. If it does not match, call the ordinary handler on that path; no raw-pointer conversion is added.

Before consuming anything, require a tagged resolved handler, one argument, valid argument/output addresses, a following matching `IterNext`, and a private output slot currently empty or an inline signed integer. Any deep/resource-owning old output, same-slot argument/output alias, invalid address or ignored output uses ordinary dispatch. This keeps all old resource destruction on the ordinary path and makes skipped first-store destruction unobservable on the admitted path. Native Function borrow itself is checked before attempting specialization, as on the base. Bytecode look-ahead errors mean fallback, NOT early propagation, preserving original side effects/error order.

For Some(i), drop the iterator guard at the original boundary. The second instruction's budget check and decode/ip/last_ip bookkeeping then happen in the same dispatch iteration; its final store writes i64 using the VM's existing Worklist (as original IterNext does). For None, materialize/store the ordinary None result BEFORE the second permit and run the ordinary IterNext jump path; exhaustion isn't claimed allocation-free. Storing a None preserves the final stack slot and normal loop cleanup. A failed second permit with Some materializes/stores its Option using native Memory::store's fresh Worklist, then returns Limited with the original Call instruction bookkeeping; there is no deferred field on Vm.

An additional conservative guard is possible entirely inside Rune: a crate-private `budget::remaining` query, using its existing TLS getter, rejects specialization when the next permit is already unavailable. In the admitted built-in next there is no user callback or other budget consumption, so one-permit stepping takes the ordinary path. Keep the actual second `budget::take` regardless and handle failure normally; the query never replaces charging or reserves permits early. Audit/controls must verify this stepping guard, nested ambient budgets and zero/default/unlimited cases. This read-only helper is not a cross-crate visibility issue.

The limit-query bridge remains unresolved: Rust has no pub(crate) visibility across rune/rune-alloc. A safe feature-gated `#[doc(hidden)] pub` runtime-internal query is technically a public symbol; reviewer approval of that exact scope is requested separately. No unsafe extern/symbol bypass, inferred numeric memory-limit threshold or public VM opt-in is approved.


## 8. Cross-crate bridge ruling received

The reviewer explicitly approved the safe std-only hidden public query described in plan §6. It uses no additional feature and no unsafe extern bridge. The prior §4/§5/§7 visibility questions are resolved by this scope amendment, not by pretending pub(crate) crosses crate boundaries. Active-limit semantics, nesting and future polling must be tested; no_std fast path remains disabled. The handler/argument/store audit in §7 is still awaiting its independent review; no production edits have been made.

## 9. Final reviewer challenge and stronger same-slot guard

The vtable tag substitution is adopted in place of §7's wrapper. `FunctionHandler` stays {ptr,vtable}; tag is a field of the existing static vtable, read by value. The same declaration macro also generates other handler types, whose tag is none. Avoid runtime-tag const-promotion problems by a private const-generic u8 constructor (`new_tagged<F, const TAG: u8>`), with ordinary `new` delegating to TAG=0. One internal enum interprets the byte; only the private built-in adapter constructs TAG=range_i64_next. This is subject to a compilation proof; if no static promotion is possible, return for review. Existing dyn-call safety implementation is not rewritten and gains no unsafe block.

The reviewer suggested qualifying raw stack differences if an Option temporary survives the second instruction. Source check shows the admitted compiler sequence is narrower: `for_body` emits `.iter_next(binding.addr(), ..., binding.output())`, and `needs.rs` maps output to that same address; `op_iter_next` writes Some's value via `self.store(out,some)`. Thus after a successful IterNext the base slot already holds i64. Enforce three-way same-slot equality in the guard. If an arbitrary bytecode sequence consumes Option into a different slot, it is unadmitted and falls back. Under this constraint, the mid-body Limited/resume golden can and must retain exact raw stack equality; there is no reason to weaken the original contract. The only temporary divergence is within the unobservable paired dispatch before the second effect; failed permit materializes the original Option, and None follows the ordinary path.

These final amendments are submitted for acknowledgement. No production edit has begun.

## Accepted implementation clarifications (before timing)

Claude confirmed the generated-metadata adapter: obtain `RangeIter::<i64>::next__meta()` unchanged, replace only its associated handler with a private tagged handler invoking the original typed `next`, and register it through the ordinary `function_meta` path. This adds one temporary handler allocation during one built-in registration; context allocation measurements must report it explicitly. No resident wrapper is retained. The strict explicit-limit equivalence claim concerns VM execution, not the changed metadata-construction allocation point.

Tracing-enabled builds never specialize, even with no subscriber installed. This deliberately conservative scope preserves every tracing span/event, including INFO-level native-call instrumentation. The all-features tests require zero specialization hits. Positive-hit tests run separately with the named non-tracing feature set `alloc,bench,byte-code,capture-io,cli,disable-io,doc,emit,fmt,languageserver,musli,serde,std,workspace` (all Rune features except `tracing`, with defaults disabled). The measured harness keeps its frozen default, non-tracing features; its actual rustc feature arguments are retained. No-std builds likewise use the ordinary handler and dispatch.

The memoized callable can be an inline `Type(hash)` as well as a `Function` value. In the former case the candidate resolves the actual context handler and checks its private tag, exactly as ordinary `op_call` does. A type hash alone never authorizes specialization. In the latter case only an actual native handler's carried tag authorizes it. The hit assertion must succeed in both compiler memoization modes.

The source audit additionally requires unit-first precedence for memoized inline `Type(hash)` callables: any `UnitFn` at that hash prevents specialization, matching `op_call`. A base-first raw-unit golden places a unit function at the native range-NEXT hash and proves that it is executed instead. The original candidate is an expected-failure control on this fixture.

Rejecting hash and arity prefilters avoid extra map lookups on unrelated calls: associated calls first require `Protocol::NEXT` and one argument; inline Type callables first require the exclusive i64 range-NEXT hash and no unit definition; native Function values first require one argument before borrowing. Acceptance still requires the actual resolved handler's private tag. The pinned revision's `Hash::associated_function` is not const, so a private always-inlined helper uses its constant inputs instead of copying the private hash formula or hard-coding a hash value. Measured release feature/argument records and ordinary gates remain required.
