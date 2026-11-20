# ADR 0317: The gate measures a runner and its engine image

Status: Proposed

Base: published 30ba88570fe34be1123ba4a813d539ae82937529 (ADR 0316).

## Problem

Adding adapter code to the program changes how the engine is compiled.
Under whole-program ThinLTO, the Candle binary family's six methods
raised native setup on six benchmarks that never call them, by +301 to
+4,956 instructions inside Cranelift (MEMBERS-v2). Earlier, an unrelated
registry type moved the interpreter loop's register allocation
(+389,780 on `fib`). Most future adapter records would trip the setup
limits the same way.

A separately linked engine fixes this at the source. In LINK-v1, two
launchers with different adapter code ran a real changed callback (42
and 43) against one byte-identical engine shared object. Its boundary
has a cost. With engine-owned initial-exec allocator TLS (TLS-v1), it
still adds +198 W and +225 T to `shared_rows` setup, against limits of
37 and 11. On 2026-11-20 the user authorized that boundary cost as a
deliberate one-time cost, so that adapter changes never alter engine
code again. The binary family is parked until the separation lands,
kept at local `refs/parked/0317/binary-family`.

The path has six records:
1. this one: gate and cache awareness of a runner plus its engine image;
2. an independently built engine with a minimal versioned C ABI;
3. engine-issued handles for checked file invocation, and opaque provider
   handles;
4. the existing 0305 to 0315 capabilities routed through provider
   callbacks;
5. packaging with the stock and generated launchers;
6. parity qualification and the cutover.

The engine is a versioned ELF shared object with ThinLTO inside its own
build. It uses initial-exec TLS for its hidden allocator budget slot only,
and is loaded as a startup dependency. Late-loaded adapters are
supported. Loading the engine itself late, and other targets, need their
own evidence.

The gate cannot measure such a program yet. `tools/column/paired_build.py`
builds one executable, and the observer resolves `PreparedInvocation::invoke`
and the seven `invoke_args` closure selectors in that one ELF.

## Decision

1. The fixed-route stage and its capture children get a versioned
   description of the measured artifacts. Two forms are supported:
   - the existing single executable;
   - a runner plus one named engine shared object.
   Both hashes, the launch recipe, the target and the canonical paths are
   bound to capture, revalidation and cache identity. A cache entry
   retains both artifacts and verifies them before reuse. A runner change
   invalidates its measurement even when the engine is reused.
2. In split-image mode:
   - both files are mounted at fixed paths under the 0316 canonical
     launch layout;
   - the observer waits for loader readiness and verifies the mapped
     engine file's identity;
   - it resolves `PreparedInvocation::invoke` and the seven loop
     selectors in that loaded image;
   - it verifies that the entry and split PCs lie in executable mappings
     of the bound engine;
   - it arms the finish witness (the existing `gdb.FinishBreakpoint`) on
     that exact invocation frame and thread, and records exactly one
     normal return to that frame's saved caller continuation. That
     continuation may lie in the runner: a genuine engine call returns
     across the boundary. An out-of-scope or unwinding exit, a wrong
     thread or frame, or an unmatched return fails.
   - The dump stays at the existing finish stop, so 0316's measurement
     window is unchanged.
   Runner and engine instructions are each counted as product work
   exactly once, and each object's contribution is recorded separately.
   A real capture may legitimately have zero runner instructions in a
   measured phase.
   - The source-identity precheck (`executor_identity.py`, run before
     capture when the revisions differ) selects the owning ELF for each
     role, and retains both role identities. It never compares launcher
     stubs in place of the engine. The JIT and unknown partition, the system partition, the
   raw total, phase reconciliation and the source guards are preserved.
3. Existing single-image receipts revalidate under their original rules.
   Today's stock route and its references keep scoring unchanged.
4. This record changes measurement tooling only. It changes no routing,
   engine ABI, reference or allowance, and spends none of the one-time
   transition.
5. The one-time transition is bounded now, and frozen in full by the
   cutover record before its candidate is measured:
   - It applies once, to the cutover pair only, and cannot be reused on
     any later commit or image change.
   - Per kernel, size and metric, the setup level ceiling is the larger of
     the existing allowance and an independently measured positive setup
     delta, never their sum. Each ceiling names the reference behind it.
   - For `shared_rows`, the evidence supports +198 W and +225 T at both
     sizes.
   - Kernels without independent, frozen evidence keep ordinary limits.
   - Setup growth, both execution bands, whole-run growth, cumulative
     whole-run checks and native-off checks stay unchanged.
   - There is no allowance for the per-call cost of crossing into an
     adapter. That recurring cost must fit the ordinary execution bands.
   - After cutover, the published setup counts become the single new
     cumulative setup reference. The execution anchor and every allowance
     width stay unchanged.
   - A breach goes back for decision with the failed numbers intact.

## Constraints

- Tooling only: `tools/` and `plans/`, with one exception. No product
  source changes, and none of the 40 inventoried items move.
- The exception is a test-only fix in `tests/capture_bound.rs`. Two of its
  tests failed both complete finals on this record (FINAL-v1 and FINAL-v2,
  on the unchanged 94e5766b), although this record changes no source or
  test. They background a `setsid sleep 6` holder and give it only a
  `sleep 0.1` before the parent exits, with no acknowledgement that
  `setsid` took effect. A traced diagnostic (HOLDER-v1) reproduced the
  exact failures under a forced pre-`setsid` delay. The holder was still
  in the child's process group, `rnx` sent that group `SIGKILL` (as
  designed), and the stdout reader saw EOF. The production capture is
  correct; the fixture raced it. Why the race started losing now is not
  established, and no claim is made about it.
  - The fix covers exactly the two witnessed cases plus a shared ready
    and never-ready control.
  - Each test uses an owned scratch directory and positional marker
    paths. The detached holder acknowledges after `setsid`, with the
    capture's stdout already inherited.
  - Each test requires the acknowledgement and the parent's success before
    asserting the original `cut_short`, the 0 or 7 bytes, the timeout, the
    truncation and the prompt return.
  - The startup wait is bounded, with an explicit failure.
  - A deterministic pre-detach delay proves the race is closed. The
    never-ready control proves that waiting for the acknowledgement is
    what makes the test pass.
  - Both failed receipts are kept.
  - A `fix:` commit is not allowed while this record's plan is open, which
    is why the fix is part of this record.
- `final.sh --required` passes on the signed candidate before the
  complete final.
- No new crate in the product. Tabs.

## Controls

- Committed controls use a small, buildable split-image fixture in which
  the runner calls the engine shared object directly and the return lands
  in the runner. The fixture has known positive self instructions in BOTH
  objects, and each object's contribution is checked independently,
  because moving runner cost into "other" would leave W, T and
  reconciliation unchanged. The retained real LINK-v1 and TLS-v1 images
  are external evidence.
- Return witness: the genuine return to the runner passes. A wrong or
  missing finish fails.
- The identity precheck runs on a split pair with differing revisions,
  and compares the engine images, not the launchers.
- Cache behavior:
  - a hit;
  - a miss on a runner-only change;
  - a miss on an engine-only change.
- Each of the following is refused:
  - a wrong or missing loaded engine, including the correct basename at
    the wrong identity;
  - a decoy symbol in the launcher;
  - a planted accounting omission.
- Canonical matched captures of the two historical triggers succeed in
  split-image mode.
- An ordinary stock final passes unchanged.
- Any calibration recipe for later records is frozen before new counts.
- One complete final.
