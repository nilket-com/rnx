# Upstream defect draft: dropping a deeply nested value aborts the process

Ready to file against `rune-rs/rune`. Not submitted; filing it is the
operator's call. This is a **defect**: a safe-API program aborts with no way
to prevent or catch it. The companion draft,
`0019_upstream_enhancement_rtti_field_names.md`, is an enhancement request and
is deliberately separate.

---

**Title:** `Drop` for a deeply nested `Value` recurses and aborts the process

**Version:** 0.14.1

### What happens

A `Value` holding a chain of nested containers is dropped recursively, one
stack frame per level, so dropping one deep enough overflows the stack. The
process aborts with `fatal runtime error: stack overflow` and a signal; there
is no error to catch and no way for a caller to refuse the drop, because the
drop happens when the value goes out of scope.

Construction is not the problem — building the value succeeds, and the abort
happens afterwards, which is what makes it hard to guard against: a program
can hold such a value, decide not to use it, and still die.

### Reproducer

One file, `rune` as the only dependency:

```rust
use rune::runtime::Vec as RuneVec;
use rune::Value;

fn main() {
    let depth: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(200_000);
    let mut value = Value::from(0i64);
    for _ in 0..depth {
        let mut vec = RuneVec::new();
        vec.push(value).unwrap();
        value = Value::try_from(vec).unwrap();
    }
    println!("built {depth} deep");
    drop(value);
    println!("dropped"); // never reached
}
```

Output:

```
built 200000 deep

thread 'main' has overflowed its stack
fatal runtime error: stack overflow, aborting     # exit 134
```

`built` prints, `dropped` does not.

### The thresholds are specific to this build

The depth at which it aborts is a property of the frame size and the stack
available, not of Rune, so these numbers locate the behaviour rather than
define it. All measured on:

- rustc 1.95.0 (59807616e 2026-04-14), `x86_64-unknown-linux-gnu`
- Linux 7.0.0-31-generic, main-thread stack `ulimit -s` 8192 KiB
- rune 0.14.1, `default-features = false`, `features = ["std"]`

| Path | Deepest that survived | Shallowest that aborted |
| --- | --- | --- |
| Reproducer above, debug | 16,384 | 24,576 |
| Reproducer above, release | 65,536 | 131,072 |
| Value built by a Rune script, debug | 49,152 | 65,536 |
| Value built by a Rune script, release | 65,536 | 131,072 |

A different stack size, optimisation level, or platform will move all of
them. What does not move is that the drop is recursive and unbounded.

### Why it matters to a caller

A host embedding Rune cannot bound this. Checking a value's depth before
using it does not help: the value already exists, and refusing to use it does
not refuse the drop. In rnx this means a script can abort its own interpreter
by building a deep value and letting it fall out of scope, with the
interpreter never asked to print or serialize anything.

### Suggested direction

An iterative drop for the container types — a worklist that takes ownership of
children and drops them in a loop rather than recursing — would remove the
depth dependence entirely. A depth limit would not, because the value is
already built by the time anything could refuse it.
