# Upstream issue draft: call frame isolation ignores the unit

Ready to file against `rune-rs/rune`. Not submitted; filing it is the
operator's call. The reproducer below is `tests/upstream_isolation.rs` in
this repository, reduced to one file with no dependency on rnx.

---

**Title:** Calling a function value across units corrupts the stack when the
two VMs share a `RuntimeContext`

**Version:** 0.14.1

### What happens

A function value created in one `Unit` and called from another runs against
the caller's stack frame when both `Vm`s were built with the same
`Arc<RuntimeContext>`. The call fails with an out-of-bounds instruction
pointer or an out-of-bounds stack entry, depending on the shape of the two
units. With a separate `RuntimeContext` per `Vm`, the same code is correct.

### Where

`crates/rune/src/runtime/function.rs`, `FnOffset::call_with_vm`:

```rust
let same_unit = matches!(self.call, Call::Immediate if vm.is_same_unit(&self.unit));
let same_context =
    matches!(self.call, Call::Immediate if vm.is_same_context(&self.context));

vm_try!(vm.push_call_frame(self.offset, addr, args, Isolated::new(!same_context), out));
vm_try!(extra.into_stack(vm.stack_mut()));

// Fast path, just allocate a call frame and keep running.
if same_context && same_unit {
    return VmResult::Ok(None);
}

VmResult::Ok(Some(VmCall::new(
    self.call,
    (!same_context).then(|| self.context.clone()),
    (!same_unit).then(|| self.unit.clone()),
    out,
)))
```

Isolation is decided from `same_context` alone, while the fast path that
keeps running on the current frame requires `same_context && same_unit`.
Those two agree whenever a `RuntimeContext` is built per unit, because the
contexts then differ exactly when the units do. They come apart at
"same context, different unit": the frame is pushed unisolated, a `VmCall`
still switches the unit, and the callee reads slots laid out for the caller.

A fix along the lines of `Isolated::new(!same_context || !same_unit)` makes
the isolation decision agree with the fast path, but the maintainers will
know whether the unit switch or the isolation flag is the half to change.

### Reproducer

```rust
use rune::runtime::{RuntimeContext, Value};
use rune::{Context, Diagnostics, Source, Sources, Unit, Vm};
use std::sync::Arc;

fn compile(context: &Context, source: &str) -> Arc<Unit> {
    let mut sources = Sources::new();
    sources.insert(Source::memory(source).unwrap()).unwrap();
    let mut diagnostics = Diagnostics::new();
    let unit = rune::prepare(&mut sources)
        .with_context(context)
        .with_diagnostics(&mut diagnostics)
        .build()
        .expect("the sources compile");
    Arc::new(unit)
}

/// Make a function value in one unit and call it from another. `share` says
/// whether the two virtual machines get the same runtime context.
fn call_across_units(share: bool) -> Result<i64, String> {
    let context = Context::with_default_modules().unwrap();
    let maker = compile(&context, "pub fn main() { || 41 + 1 }");
    let caller = compile(&context, "pub fn main(f) { f() }");

    let first = Arc::new(context.runtime().unwrap());
    let second: Arc<RuntimeContext> = if share {
        first.clone()
    } else {
        Arc::new(context.runtime().unwrap())
    };

    let mut vm = Vm::new(first, maker);
    let function: Value = vm.call(["main"], ()).expect("the maker runs");
    let mut vm = Vm::new(second, caller);
    match vm.call(["main"], (function,)) {
        Ok(value) => Ok(rune::from_value(value).expect("an integer")),
        Err(error) => Err(error.to_string()),
    }
}

fn main() {
    // Control: a runtime context each.
    assert_eq!(call_across_units(false), Ok(42));
    // Defect: one shared runtime context.
    println!("shared: {:?}", call_across_units(true));
}
```

Observed on 0.14.1:

```
shared: Err("Instruction pointer `3` is out-of-bounds `0-3`")
```

A larger pair of units reports `Tried to access out-of-bounds stack entry 13`
instead. Both are the callee running against the caller's frame.

### Why it matters to us

We host an interactive Rune session that compiles one unit per input. A
retained closure keeps its `FnOffset` alive, which holds an
`Arc<RuntimeContext>` beside its `Arc<Unit>`, so every retained function
value pins its own copy of the runtime context. We measured one context at
462,739 bytes, and a session of a thousand retained closures at 796 MB
against 333 MB when the context is shared, a difference of 462,964 bytes per
closure. Sharing one context per session is the obvious fix on our side and
this defect is what prevents it.
