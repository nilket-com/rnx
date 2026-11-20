# Implementation plan 0317: The gate measures a runner and its engine image

Status: Implemented

Paired ADR: ../adr/0317_the_gate_measures_a_runner_and_its_engine_image.md.

1. Boundary: Codex's isolation proposal at 30ba8857, first record. Record
   any deviation before editing.
2. Add the versioned artifact description, split-image capture with
   loader-readiness and mapped-identity checks, exact-once attribution,
   and the cache, retention and revalidation changes.
3. Add the split-image fixture and the cache, refusal and accounting
   controls. Capture both historical triggers in split-image mode.
4. fmt, Clippy and preflight. Sign. Then run `final.sh --required` on the
   signed commit. Source review, one complete final, Results.

## Results

The fixed-route gate can now measure a runner together with a separately
linked engine shared object. This is the first of six records toward an
isolated engine, which the user authorized on 2026-11-20 after adapter
code shifted engine compilation (MEMBERS-v2).
- A versioned artifact description binds the runner and engine hashes,
  the launch recipe, the target and the canonical paths into capture,
  revalidation and the cache.
- In split-image mode the observer checks four things:
  - the mapped engine's identity;
  - that the selectors lie in the bound engine's executable text;
  - that the entry and split PCs lie in that text;
  - that exactly one normal return reaches the saved caller
    continuation on the same thread, and with the caller's stack
    pointer. That continuation may lie in the runner.
- The dump stays at the existing finish stop, so 0316's measurement
  window is unchanged.
- Each object's contribution is recorded separately.
- The source-identity precheck selects the owning engine ELF for each
  role.
- Single-image receipts revalidate under their original rules.
- No routing, engine ABI, reference or allowance changed, and none of the
  one-time transition was spent.

The two retained TLS-v1 launchers ran against the same engine image.
All 32 matched workload, size and native-mode combinations produced
identical instruction vectors. At both sizes, native-on fib and
shared_rows also reproduced their earlier setup and execution counts.
The measured runner contributed zero instructions in those phases.

In the small fixture the call returned into the runner. Its callback
contributed exactly three instructions, all during execution, and the
engine contributed instructions in both phases. The controls rejected
a decoy selector and a different mapped engine with the expected name.
Cache controls changed each artifact independently and verified reuse
after the original files were removed.

The capture-bound repair is this record's one exception to "tooling
only". Two of its tests failed complete finals FINAL-v1 and FINAL-v2 on
94e5766b. The `capture_bound` tests and the production capture code were
unchanged on that candidate.
The traced diagnostic, HOLDER-v1, reproduced the failures with a forced
pre-`setsid` delay:
- the holder was still in the child's process group;
- `rnx` sent that group `SIGKILL`, as designed;
- the stdout reader saw EOF.
The diagnostic demonstrates that the fixture can race correct
process-group cleanup. Neither failed final recorded a holder trace, so
the race's occurrence in them is unconfirmed, and why it would start
losing on these finals is not established. The repaired tests
require the holder's post-`setsid` acknowledgement and a successful
parent exit. A committed control delays detachment by 350 ms, and a
never-ready control requires the parent to exit 42. The two-second return
bound is kept. The signed
`capture_bound` binary passed all 11 tests quiet and under 16 CPU
workers. Both failed receipts are retained:
- FINAL-v1, RESULT SHA-256
  61eb5e2d2fd15853bfe60e4c39c9673c00b022ae9428db535f7138a0ba884f1f;
- FINAL-v2, RESULT SHA-256
  3727faa25b415ab0b64f27bb8d49bcfddbbaf705e53a0c9538a0c542e96a8864.

On 7b143650, a required final passed all 11 selected stages, including
root-tests, in 6.3 minutes. The complete final on the same commit then
passed all 101 stages: 41.1 minutes of accepted run, 41.8 end to end.
Both fixed-route bands held, with no memory.high, memory.max or OOM
events.

Product source is unchanged. Every change is in `tools/`, `plans/` and
`tests/capture_bound.rs`.
