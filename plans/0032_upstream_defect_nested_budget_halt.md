# Upstream defect draft: a budget halt inside an awaited function loses its result

Ready to file against `rune-rs/rune`; the report is everything below the rule.
This is a **defect**: the value a program computes is silently replaced by `()`.
It is reachable only by a host that resumes after a budget halt, which is what
a host must do to use the budget for slicing, and it is why the companion
enhancement, `0032_upstream_enhancement_interrupt_hook.md`, asks for something
that is not the budget.

Not filed. Filing it is the operator's call. Measured on Linux on 2026-09-13
against 0.14.2, Rust 1.98.1. Not measured on `main`; the two functions named
below were read there and are the same by reading.

---

**Title:** Resuming after a budget halt that landed inside an awaited `async fn`
completes with `()` instead of the function's value

**Version:** 0.14.2. Unchanged on `main` (0.15.0 at `bb8e6937`) by reading.

### What happens

Drive an execution in slices, resuming after each halt for want of budget —
the documented way to bound a script while keeping it interruptible. If a slice
runs out *inside* an `async fn` that the entry point awaited, the execution
does not resume where it stopped. It completes, successfully, with `()`.

```rune
async fn work() { let n = 0; for i in 0..2000 { n += i; } n }
pub async fn main() { work().await }
```

Driven with a slice of 10,000 instructions and a total budget of 2,000,000,
resuming on every halt whose guard is exhausted:

| slice | result |
| --- | --- |
| 10,000 | `()` after 2 slices |
| 2,000,000 (no halt inside `work`) | `1999000` after 1 slice |

The loop is about 14,000 instructions, so a slice of 10,000 stops inside it.
A synchronous helper in the same shape is correct at any slice: a synchronous
call is a frame in the same execution, and its halts resume.

### Where

`runtime/vm.rs`, `call_async_fn` makes the awaited call a separate execution
wrapped in a future value:

```rust
let future = Future::new(async move { execution.async_complete().await })?;
```

A budget halt inside that execution leaves `inner_async_resume` returning
`VmErrorKind::Halted { halt: Limited }`, which resolves the future as an error.
`runtime/future.rs`, `Future::poll`, then drops it:

```rust
Poll::Ready(result) => {
    this.future = None;
    (this.vtable.drop)(future.as_ptr());
    Poll::Ready(result)
}
```

The outer execution's `await` has nothing left to wait on, and resuming it
finishes the entry point with the unit the await left behind.

### Why it is worth reporting even though the host asked for it

A host that resumes past a halt it cannot classify is doing something
questionable, and a host can avoid this by never slicing an execution that may
await — which is what rnx now does. But the failure is silent: it produces a
wrong number rather than an error, and the host has no way to detect it, because
the nested halt is indistinguishable from a program error (the companion
enhancement on halt visibility). Either an error on resume, or a note in
`budget`'s documentation that a halt is terminal for an execution containing an
awaited call, would have saved the search.
