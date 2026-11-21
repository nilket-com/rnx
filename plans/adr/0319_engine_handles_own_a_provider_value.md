# ADR 0319: Engine handles own a provider value

Status: Proposed

Base: published 230ab56e48b44564f9766d1bfb7e28a43cbdb99b (ADR 0318 and
the jobs fix).

## Problem

ADR 0318 linked the engine separately and proved it with a scalar
callback through `rnx_engine_run_file_v1`. A scalar callback cannot show
that an opaque value crossed the boundary, or that the checked execution
path ran. Records 4 to 6 move the existing Polars and Candle capabilities
behind the boundary. Each depends on the engine issuing handles, and on a
provider owning a value that a script creates, aliases, reads and drops.

This is record 3 of the six isolation records.

## Decision

1. A versioned C surface sits alongside ABI v1.
   - The engine issues context, program and invocation handles.
   - Provider registration is frozen when the context is sealed.
   - A file is compiled through `FileModules` in that context, prepared
     once, driven once and released.
   - `CheckedMain::prepare` consumes the program and `FileRun::drive`
     takes its key, so a program is explicitly single-use.
   - When the program is consumed:
     - The request, its owner and the output arguments are validated,
       and the invocation slot is reserved, before anything else.
     - A failure before `CheckedMain` is taken leaves the program
       usable.
     - Taking `CheckedMain`, immediately before `prepare(self, ...)`,
       consumes the program even if `prepare` returns an error.
     - On that error no invocation is published, and the reservation is
       released.
     - On success the invocation becomes a child of the same context.
   - Abandoning a prepared invocation runs no user code.
   - An execution failure is terminal. Neither reading diagnostics nor
     releasing a handle can replay effects.
   - Diagnostics stay in bounded, caller-owned storage. Refusal still
     happens before any effect.
2. Handles are validated typed tokens.
   - Before any access, a token's owner, kind, generation and issuing
     thread are checked. A reused slot cannot revive a stale token.
   - Closing a parent that has live children fails, and leaves the
     children valid.
   - Callback re-entry is rejected across both ABI versions.
   - Rust values, references, allocators and destructors stay on the
     side that owns them.
   - Engine-owned opaque slots carry a provider identity and an opaque
     payload token.
   - One synthetic provider exposes create, shared scalar read and drop,
     through the existing checked producer and reader route.
   - A borrow lasts one synchronous callback.
   - The callback table is copied when registration succeeds.
   - The caller keeps the callback code and the cookie valid until the
     context is released successfully, which covers every drop callback.
     A sealed context may run several files, and each calls the
     provider with the same cookie.
   - A refused parent close ends nothing.
   - Owners release live handles on the issuing thread before that
     thread exits.
   - No registry lock is held across a callback or a destructor.
3. Credit and ownership transfer together.
   - Before the producer can spend bounded credit, its allowance is
     derived from the invocation.
   - A successful provisional payload stays under a drop guard until its
     type, reported charge and store insertion all succeed.
   - Temporary credit is released on every exit.
   - The retained charge moves to the last owner. Aliases are charged
     once.
   - Normal files keep their existing allowance.
4. Deferred:
   - real Polars and Candle callbacks, and migration of capabilities
     (record 4);
   - installed and generated packaging (record 5);
   - default routing (record 6);
   - sessions, cross-thread handoff, provider unloading, and a general
     plugin ABI (outside this path).
   No protected loop or allowance width changes.

## Constraints

- Target: x86_64-unknown-linux-gnu.
- No new third-party crate.
- ABI v1 and its existing callers keep working unchanged.
- Native cache eligibility is decided before the invocation in the
  image.
  - The frozen sibling comparison failed. Native `shared_rows` setup T
    was +27 against an allowance of +11, at both sizes.
  - The cause: the first native checkout calls `Cache::enabled`, which
    reads `PERF_BUILDID_DIR` through one `getenv` per thread (three
    string comparisons inside it), inside the measured invocation.
  - The environment bytes are identical for both launchers. The stack
    address of the lookup key differs by 32 bytes, so libc's
    `__strncmp_avx2` takes its page-boundary path 3 times on one
    launcher and never on the other, at 9 instructions each.
  - The engine's invocation cost must not depend on the caller's stack
    layout. In the image, the existing thread-local eligibility decision
    is therefore initialized when a v1 context is created and when a v2
    context is sealed, before any invoke or drive.
  - This is correct because handles are bound to their issuing thread.
  - The `PERF_BUILDID_DIR` refusal and the page-size rule are kept.
  - Ordinary stock callers keep the lazy default, so the stock gate is
    unchanged. No second policy cache is added.
  - Behavior changes only if the variable changes between context
    initialization and the first native checkout on that thread. Later
    invocations on an initialized thread already reuse the decision, and
    fresh threads sample on their own.
  - Cranelift 0.135.5 also reads `PERF_BUILDID_DIR` once per function it
    defines (`JITModule::define_function_bytes` calls
    `record_function_for_perf`). That happens in seven kernels'
    native setup, but not in `shared_rows`. This record does not change
    third-party code: those reads are counted and reported separately,
    and any change to them is the user's decision.
  - Controls:
    - fresh threads;
    - `PERF_BUILDID_DIR` present and absent;
    - zero Rune eligibility lookups in every measured native setup
      window;
    - zero `getenv` calls in total in the `shared_rows` setup window;
    - the Cranelift `getenv` counts reported for each kernel;
    - the same frozen sibling comparison, rerun.
  - Stock timing is measured, not assumed to change.
- None of the 40 inventoried items move.
- `final.sh --required` passes on the signed candidate before the
  complete final. Tabs.

## Controls

1. Checked-file witness. A real Rune file:
   - creates an opaque value;
   - aliases it;
   - reads it twice;
   - returns a scalar;
   - produces exactly one provider drop.
   Evidence must show the run took `FileProgram::Checked`. Plain and
   tagged test providers give distinguishable answers from the same
   engine image bytes.
2. Handle lifecycle, from C:
   - stale and reused tokens;
   - wrong-kind, foreign-context and wrong-thread tokens;
   - independent contexts on two live threads;
   - repeated drive and release;
   - callback re-entry;
   - truncated diagnostics after an effect;
   - abandonment before drive;
   - two separately compiled runs on one context, with no live payloads
     between them. Context release is attempted with a live child and
     refused, the child still works, and release then succeeds with
     every counter balanced;
   - consumption, on both sides of the consumption point. A malformed
     request is refused and a valid prepare then succeeds on the same
     token. A valid prepare with too little store credit fails, and a
     retry with the now-stale token is refused. No user callback runs,
     and the context then closes cleanly.
3. Credit, each case on its own, with drop and outstanding-credit
   counters exposing leaks and double cleanup:
   - finite credit at the exact boundary;
   - one byte short;
   - overflow;
   - provider failure;
   - rejection after creation.
4. Planted faults in handle validation, drop and credit accounting each
   fail their control.
5. An existing v1 caller still works.
6. Measurement.
   - Before counts, the sibling comparison is frozen on the 0317
     split-pair gate:
     - native off and on, at sizes 1 and 5;
     - both launcher hashes, the common loaded shared object, PCs and
       finish evidence bound;
     - each image's own instruction count, and the reconciled W and T
       for each phase and for the whole invocation, retained.
   - The ordinary stock cumulative gate stays in force.
   - Stock-to-sibling differences, including setup, are diagnostic only.
     The user's decision on setup cost gates only the cutover.
   - The checked-handle witness is reported separately from the matrix
     cells that run through v1.
7. Finals. A signed `--required` run, then one complete final.
