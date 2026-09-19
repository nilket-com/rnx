# 0068 gate 1: stop at the registration API boundary

Baseline: plan `dfb089d`; product source unchanged. The draft's F1 is corrected:
the post-install unverified-coordinate observation on slim remains a separate
question. The wide/tall frame and top-level Result qualifications are in the plan.

## Candidate and observed break

The isolated candidate tested a return-type extension to the existing builder:
`Extensions::with<R: Into<Registration>>`. Existing builders could return their
help Vec, while new ones could return help plus explicitly typed native
presenters. Registration would move through extension installation into the
Session, and only top-level interactive presentation would consult it. The
ordinary formatter and its protocol-free contract stayed unchanged.

This was intended to retain the generated wrapper's existing `.with(...)` and
avoid new manifest hooks. It does not preserve source compatibility. An unchanged
root test with a panic-only builder fails with E0283. More directly, this valid
embedding caller compiles against the baseline and fails against the candidate:

```rust
let _ = rnx::Extensions::none()
    .with("fixture", |_| Err("registration failed".into()));
```

The original signature fixes the successful result type even when the builder
only returns Err. The generic signature does not. The compiler requests a new
type annotation because it cannot infer `Into<Registration>`. Adding annotations
to our tests would conceal the same break for downstream callers.

A first local compilation also found a missing dereference of Rune's BorrowRef
in the experimental callback wrapper; that scaffolding error was corrected
before the compatibility result. It is not the stop condition.

## Decision requested before continuing

Keep `with` and `with_lifecycle` signatures intact. Add a separate opt-in
`with_presentations` API whose builder receives the module and a scoped
presentation registrar, with the existing help return type. Existing builders,
including failure-only closures, remain usable without annotation.

For generated applications, propose an explicit `presentation` hook selecting
that method, and a new Polars builder name while retaining the old plain builder.
The exact schema/version and catalogue implications must be reviewed before a
product change. They are not hidden inside the renderer or automatically inferred
from a native type's name. The alternative is a separate declarative presenter
registration field; this checkpoint recommends the new hook for its direct match
to the existing plain/lifecycle builder selection.

This is the plan's stop for a required new builder/wrapper form, not a passed
gate. The Polars registration, explicit DISPLAY_FMT, output escaping and limits,
context teardown, worker path, and actual generated application remain to prove.
No claims are made for those parts of the exploratory patch.

## Reproduction

`rnx-bench/probes/frame-presentation/` preserves the baseline revision, the
isolated source patch and a driver. It archives the baseline, applies the patch
in a separate tree, and compiles the identical runner-only consumer against both.
The baseline succeeds; the candidate fails with E0283. The initial root-test
compile log and both consumer logs are retained under
`results/frame-presentation-0068/`.

No production source, manifest, lockfile, adapter, worker or generator changed.
This evidence and the plan status are the only root changes after the plan commit.
