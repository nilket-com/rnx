# rnx 0176: helper and borrow checkpoint

**Submitted before any engine edit.** This is an implementation audit for the reviewed plan, not a measured result. Starting tree: `863370da279031b369ebb0ac1c0d80cd9ca159f6`.

## Exact proposed carrier and signature

Under `cfg(all(feature = "std", not(feature = "tracing")))`, add one module-private, by-value enum:

```rust
enum RangeNextCall {
    Associated(Hash),
    InlineType(Hash),
    NativeFunction(Address),
}
```

Replace the existing `try_range_next` method with a single combined helper:

```rust
#[cfg(all(feature = "std", not(feature = "tracing")))]
#[inline(never)]
fn try_range_dispatch(
    &mut self,
    call: RangeNextCall,
    addr: Address,
    args: usize,
    out: Output,
) -> Result<bool, VmError>
```

No `#[cold]`, trait object, boxed closure or heap carrier. The enum contains only existing Copy operands. Its storage/code-generation effect is measured; no guaranteed size or register placement is claimed.

## Dispatch edits

CallAssociated retains exactly the current rejecting condition: NEXT hash, one argument, diagnostics absent. It calls the helper with Associated(hash); success continues the VM loop. Failure calls the unchanged `op_call_associated`. All associated hash construction/context lookup/tag inspection moves inside the outlined helper.

CallFn retains the existing one-argument/diagnostics condition and reads the callable representation. Inline::Type whose hash equals range_i64_next_hash produces InlineType(hash). Any value with Function::HASH produces NativeFunction(function-address). Other values do not call the helper. No native Function borrow or unit/context lookup occurs in this hot classifier. The classifier scope ends before borrowing `&mut self` for the helper; it exports only owned Copy operands.

The inline-Type hash helper remains the existing inline associated-hash expression. It is a rejecting predicate, not handler proof. A failed helper falls through to the original `op_call_fn`, including its VmHalt handling. No existing trace-enabled or no-std arm is edited beyond cfg-disabled new code.

## Helper resolution and borrow lifetime

First resolve a boolean `tagged`, before entering the unchanged range-operation body:

- Associated: calculate the receiver-associated hash from stack.at(addr), then inspect context.function; context-first precedence matches the existing call.
- InlineType: unit.function(hash).is_none() first, then context.function(hash) with intrinsic tag exactly 1. Unit shadowing always falls back.
- NativeFunction: re-read stack.at(function-address), require its Any Function representation, borrow_ref::<Function>(), inspect native_intrinsic, and drop that borrow at the end of this match arm. No Value clone, argument take or model mutation occurs. Any changed representation falls back. A borrow error is propagated through the same borrow_ref call as 0175; it is not swallowed or converted.

Only the boolean leaves resolution. Neither a Function borrow, a stack reference, nor a context handler reference lives across the mutable range operation or ordinary fallback. Native Function aliases carry the original vtable tag as before; no pointer identity check is introduced. The carrier is private, only constructed after the hot classifier; the helper does not reinterpret arbitrary public values as carriers.

If not tagged, return false before the old test counter is incremented. This preserves the existing counter meaning: the old try_range_next was invoked only after tag proof. On tag success, increment the old attempt counter at exactly that point and execute the existing body in the new method without changing its statements/order.

## Unchanged operation and exits

The range body keeps arity, remaining-budget and explicit-scope checks; output address and alias rejection; receiver type; old output Empty/Signed; lookahead decode and matching IterNext output/addr. These refuse before taking an argument or advancing.

Admitted operation keeps take -> into_mut with BadArgument wrap -> typed next -> drop range guard. Some keeps its separate second budget charge. On failed second permit, ordinary Option allocation/store remains; on success, ip/last_ip_len updates and integer store remain. None keeps its ordinary allocated Option and subsequent IterNext. Hit increment stays after the successful Some store.

True returns to the identical diagnostics-gated continue. False reaches the identical native fallback. Errors propagate at their original logical instruction boundary. No pending field, new control-flow halt or extra budget charge. Explicit memory scopes, diagnostics, tracing, raw mismatched shapes, exhausted budget and incompatible old slots retain ordinary dispatch.

## Test and source boundaries

All 0175 goldens and tests are reused byte-for-byte, including the base-first unit-shadow fixture, all-feature zero hits, named non-tracing positive hits and terminal fallback counts. No recapture. Full source comparison must show only vm.rs's private carrier, helper resolution relocation, inline-never directive and hot-arm replacement. No registration, allocation-limit, compiler, bytecode or budget source edit.

Build and source review must confirm the outlined helper exists as an out-of-line call in the named non-tracing release build; if the intended transformation cannot be observed, report it rather than assume the attribute achieved H1. Callgrind/disassembly cannot change the candidate. A failed semantic gate stops before diagnostics/deciding timing.
