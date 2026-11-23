# ADR 0325: Proven self calls lower directly

Status: Proposed

Base: published 5530079d447d1c7362d9a9d4434c2cd67f5ccd08 (ADR 0324).

## Problem

A script method calling itself through `self.name(args)` is lowered to a
dynamic method call whenever the name is closed and dynamic dispatch is
admissible. `lower.rs:2849` takes the general route before it checks the
self method. `lower/self_receiver.rs:53` refuses the self proof whenever
dynamic dispatch is already admissible. The direct self route is
therefore reached only when something else disables the general lookup.

The projects cutover exposed this. On AWFY Permute at size 5 with native
off, the old generated Polars/Candle app ran 50,400
`instance_method_call` executions and the image ran 93,700. Candle
registers `Tensor::permute`, and that unrelated name disabled the general
lookup in the old route. Renaming the script method to a different name
of the same length removed the old route's advantage, and the image's
cost did not change. Lowering depends on a registered name that has
nothing to do with the call.

The projects record (now 0326) is parked until this is fixed. The
published gate stays as it is.

## Decision

1. Qualifying calls. A plain `self.name(args)` qualifies only when all
   of these hold:
   - it appears inside the declaring script impl;
   - it is on the exact first `self` variable, parentheses allowed;
   - the existing whole-body write scan finds no possible rebinding.
     Field and element writes keep the binding.

   Captures, aliases, explicit generics, wrong arity, and an unresolved
   method or receiver identity do not qualify. The callee is the exact
   (nominal receiver, name) associated hash, verified against the script
   impl item. An installed native handler at that hash keeps its existing
   precedence and admission. An unrelated type's registered name can
   never select the callee. Exact native absence is recorded with the
   existing `static_method` transaction.
2. Lowering order. `self_receiver.rs` separates the binding and target
   proof from its current "dynamic unavailable" gate. `lower.rs` keeps
   the opaque, static and missing precedence, then considers this
   specialization before the general closed-name dynamic route. The
   existing proof for formerly refused sites is preserved.
3. Receiver typing. An immutable binding alone does not type a publicly
   invocable first argument, and already-admitted bodies must not lose
   host entry or path and function-value ingress.
   - At an existing dynamic site, the receiver and operands are
     evaluated once, in their original order.
   - The existing nominal test then branches: an exact typed self takes
     the direct script call, and a mismatch takes the original dynamic
     lookup and call on the saved values.
   - An unconditional direct call is used only where ingress already
     proves the receiver.
   - Where the nominal test, arity, absence of an override or the
     existing dynamic admission cannot be established, the current route
     stays.

   The guard's cost is measured, not assumed.
4. Native analysis. The guard blocks native compilation of a recursive
   method, because `region/recursion/methods.rs:435` declines `Op::Is`.
   The first screen of b5e34f61's guarded design showed it: interpreted
   Permute improved 9.89%, but native-on execution rose 211% once the
   region fell back. The native recursive-method analysis therefore
   learns one narrow fact. When a region is entered with a validated
   concrete receiver, the nominal test on the immutable self is proven,
   and the guarded edge folds to the direct call in the native
   projection.
   - Charged instructions, bailout and resume PCs and the ordinary
     mismatch route are preserved.
   - Nothing else in the analyzer changes. Any other `Op::Is` is still
     declined.
   - A different receiver at entry is rejected or falls back safely; it
     never takes the folded edge.
5. Scope. No opcode, dispatcher, host ABI or legacy engine change.
6. Contract. `frontend/self_receiver_contract.json` gains an explicit
   0325 overlay, and the historical rows stay legible. It pins:
   - old admission and external-entry eligibility;
   - existing refusals and runtime errors;
   - every unaffected normalized op.
7. Correctness on both engines. An unrelated host method name never
   changes which function a call reaches. The legacy engine is used as
   an oracle only.
8. Scoring, before signing.
   - On the candidate file image: unchanged Permute against the
     published bands, plus the ordinary file gate across all kernels.
   - The same image on the empty and the Polars/Candle project routes,
     against their frozen generated runners (empty format 1, pair
     format 2). The parked project route is staged only as a
     measurement witness.
   - Both sizes and both native modes. Report execution W and raw T,
     level and growth, and keep the checksum and the measured image
     digest. The matched rename is an attribution control.

   The two project deltas stay separate. The empty native-on size-5
   attribution is the +603,584 closure self Ir. The pair's count also
   includes its dispatch change. If the empty project still exceeds its
   band, the measured residual comes back for a decision before
   signing. No allowance. The 0326 rebase and its own final follow
   publication of 0325.

## Controls

1. File witnesses with different return sentinels for the script
   method and an unrelated host method of the same name. Run them under
   the original name and the matched rename, checked on both engines.
2. An exact associated native override keeps its target.
3. A right and a wrong self arriving through host, path and
   function-value entry, including a later declaration.
4. Wrong arity, rebinding, and a side-effecting argument that runs
   exactly once.
5. The selected call form and an unchanged target for every qualifying
   site. Native: Permute's recursive region is selected with the folded
   edge, and a different receiver at entry is rejected or falls back
   safely, with charged instructions and bailout and resume PCs
   unchanged. The existing cancellation, effect and closure controls pass.
6. The contract overlay in Decision 6.
7. The file and project measurements in Decision 8, with the guard
   cost reported.
8. Finals: targeted pre-sign checks under the final's real recipe, then
   one complete final on the signed candidate.

## Constraints

- No new third-party crate. Tabs.
- No opcode, dispatcher, host ABI or legacy engine change.
- Programs admitted today stay admitted, with the same results and
  effects.
