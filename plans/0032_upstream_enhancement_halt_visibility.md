# Upstream enhancement draft: a host cannot tell a halt from an error

Ready to file against `rune-rs/rune`; the report is everything below the rule.
This is an **enhancement request**, not a defect: the information exists and is
simply not reachable from outside the crate.

Not filed. Filing it is the operator's call. Established on 2026-09-13 against
0.14.2 by reading the public surface and by the reproducer below.

---

**Title:** Expose whether a `VmError` is a halt, and which halt, so a host can
report a budget separately from a failure

**Version:** 0.14.2. The same on `main` (0.15.0 at `bb8e6937`) by reading.

### What is missing

A host that bounds a script with `budget::with` has to answer one question when
the execution ends: did this stop because the budget ran out, or did the program
fail? Nothing public answers it.

- `VmErrorKind` is `pub(crate)`, and so is `VmHaltInfo`.
- `VmErrorAt::kind()` is `pub(crate)`.
- What is public is `VmError::first_location()`, `VmError::at()`, `chain()`, and
  `Display`.

So a host classifies on two proxies: the error carries no location, **and** the
budget guard was exhausted when the execution settled. That is correct for a
halt raised at the entry point, which carries no location, and wrong in two
directions otherwise:

1. A budget halt raised inside an `async fn` the entry point awaited carries
   that function's call-site location, so it is not recognised as a halt and is
   reported to the user as
   `Halted for unexpected reason \`limited\``, with a line and column.
2. A program failure that lands on the last permitted instruction leaves the
   guard exhausted exactly as a halt does. A host that trusts the guard alone
   reports the budget and loses the failure:

```rune
pub async fn main() { panic!("boom") }
```

| budget | what the guard says | what actually happened |
| --- | --- | --- |
| 4 | exhausted | the panic |
| 5 | not exhausted | the panic |

The only way to tell 1 from 2 is the error's `Display` text, which is not
something a host should be parsing to decide what to print.

### What would close it

A predicate on `VmError` — `fn halt(&self) -> Option<VmHaltInfo>`, or making
`VmHaltInfo` public and reachable through `at()` — is enough. A host could then
say "the budget ran out" when that is what happened, wherever it happened, and
report the program's own failure when that is what happened, even if the budget
ran out in the same instant.

### A smaller one, while here

`Unit::function` and `UnitFn` are `pub(crate)`, so a host cannot ask a unit it
compiled what calling convention its entry point resolved to. That matters when
the entry point is reached through an alias — `pub use inner::work as main` —
where the source near `main` says nothing about it. rnx no longer needs this: it
stopped trying to classify entry points and drives every one the same way. It is
noted only because it was the second place the same wall was met, and a host
that wants to treat an async entry differently has no way to know it has one.
