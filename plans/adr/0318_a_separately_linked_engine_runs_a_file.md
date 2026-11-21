# ADR 0318: A separately linked engine runs a file

Status: Proposed

Base: published 1a8d5187549ec33a68c2153aac2447f75e81df98 (ADR 0317).

## Problem

ADR 0317 made the gate able to measure a runner plus a separately linked
engine. No such engine exists yet. Record 2 of the six isolation records
builds it as a working slice that runs alongside stock, so that a change
to runner or adapter code does not alter a single engine instruction.
The binary family stays parked until the isolation path completes.

The LINK-v1 and TLS-v1 prototypes showed this is feasible. They returned
42 and 43 through a legacy binding, which proves loading, not that the
new engine executed the script. This record must prove that the new
engine executed it.

## Decision

1. Build and ownership.
   - One non-default workspace `cdylib` package (for example
     `engine/image`), with a C header and a small C fixture launcher. The
     launcher is built in a separate step.
   - The package uses the existing locked dependencies, the pinned
     toolchain, and release ThinLTO inside the shared object.
   - It may depend on the rnx library with default features off, plus a
     narrowly gated internal Rust entry for file execution.
   - The engine owns:
     - loading and compilation;
     - Context and Store;
     - native-code generation;
     - value rendering;
     - diagnostics;
     - allocation.
   - No runner or adapter Rust dependency or feature flows back into the
     engine build.
   - The shared object is built once. Two different launchers are then
     built using only the header, the finished library and its expected
     digest.
   - Build input identities, commands and artifact hashes are kept.
   - An adapter-only edit requires zero engine compilation and leaves the
     installed engine bytes identical. Separate clean builds are not
     claimed to be reproducible.
   - The new package and its controls are registered with the existing
     selectors.
2. ABI v1. One exported function, `rnx_engine_run_file_v1`, returning a
   fixed-width status.
   - The request starts with a version and a byte size, both readable
     before anything else.
   - It carries:
     - a borrowed path span;
     - the native mode;
     - the fuel budget;
     - an optional single callback, with the fixed host name
       `host::apply(i64) -> i64` and an opaque caller context.
   - Empty script argv is sufficient. Execution is synchronous.
   - Header layout, known flags and the required prefix length are pinned
     on both sides.
   - Each of the following is rejected before any file I/O, callback or
     script effect:
     - an incompatible version;
     - a short struct;
     - an unavailable mode;
     - a bad path representation;
     - an inconsistent descriptor.
   - Aligned, valid storage is a caller precondition.
   - Terminal statuses distinguish invalid request, admission refusal,
     runtime or host failure, and success.
   - Diagnostics go into caller-owned bounded storage, which reports the
     written and needed lengths. Truncation never re-executes a script.
   - The engine renders normal output once.
   - No Rust types cross the ABI, and neither side frees memory the
     other allocated.
   - Unwinding is caught on the Rust side under the supported panic
     strategy. C callbacks must not unwind across the boundary.
   - The context, callback and buffers live only for the call, and
     callbacks run on the same thread. No registration is retained.
   - State is per call. If an existing static host trampoline is
     unavoidable, it is scoped to the invoking thread with an RAII restore.
     Unsupported re-entry is rejected before any effect.
3. Invocation.
   - It uses the existing file loader and the execution admission,
     prepare and invoke path. A small feature-gated shim at the existing
     Option-returning execution boundary is preferred over copying the
     compiler or run loop, or dispatching the whole CLI through
     `main_with`.
   - Refusal is strict: this sibling returns an execution-only outcome and
     never falls back to the legacy engine.
   - The scalar function is registered through `Module::host_function`.
   - Proof of the new route: both edited answers, 42 and 43, come from the
     same script, through the new engine route, with exactly one callback
     call each. Neither a legacy fallthrough nor a runner that prints the
     answer can pass.
   - The sibling parses only the file, debug, native and budget options
     the gate uses.
   - Unchanged:
     - the installed stock `rnx` and its legacy fallback;
     - CLI, REPL, Jupyter and embedding;
     - generated projects;
     - the stock fixed-route measurements.
4. Loading and binding.
   - The engine is a `DT_NEEDED` startup dependency, found by an
     origin-relative search, colocated as `libengine_image.so` as 0317
     expects.
   - The ABI major version is part of the exported symbol and the header.
   - The launcher build binds the expected engine digest after the shared
     object is linked.
   - At startup, before calling the API, the launcher:
     - resolves the mapping that actually owns the bound symbol;
     - compares its device and inode with an opened expected sibling;
     - hashes that file against the pinned digest.
   - There is no search-path fallback to a different copy.
   - This check establishes the identity of the measured image and
     refuses before any script effect. A check after loading cannot
     protect against malicious ELF constructors, and none is claimed.
   - Hidden initial-exec TLS is used for the engine allocator budget slot
     only, gated by feature and target, so stock keeps its usual backing.
5. Deferred to records 3 to 6:
   - persistent engine and program handles;
   - checked payloads and credits;
   - dynamic adapter registration and unloading;
   - Polars and Candle enrollment;
   - installed packaging;
   - routing or default changes;
   - anchor transitions;
   - support for loading the engine itself through `dlopen`.
   No protected loop or allowance width changes.

## Constraints

- Target: x86_64-unknown-linux-gnu.
- No new third-party crate.
- The new package lives in the workspace and is not a default member.
- None of the 40 inventoried items move.
- One harness exception: census case directories move to a private
  tmpfs. Census case `0007` timed out on the stock engine in required
  runs REQUIRED-v3 (on 84a3fbb3) and REQUIRED-v5 (on b8e26b4d).
  - Diagnosis:
    - Instrumented required run 18 caught a 3.5 s stall at action 3.
    - The journey fixture's mock was in uninterruptible sleep
      (`folio_wait_bit_common`) in `openat(O_WRONLY|O_CREAT|O_TRUNC)`
      of `/work/run/journey-state.json`. Truncating a file whose pages
      are under writeback waits for that writeback.
    - Under synthetic buffered write load on the same ext4 filesystem,
      with the same observer, limits and 30 s watchdog, stock `0007`
      stalled 9.0 s and timed out at 30.003 s.
    - Paired runs with the case directories on tmpfs finished in 0.44 s
      and 0.42 s.
  - Neither engine is at fault. The census case's wall-clock watchdog
    depended on disk writeback latency, which the required run's own
    build writes make unbounded on this host.
  - No claim is made about the drive's internal behavior.
  - The fix is in `src/verification/census_execution.rs` and the
    journey setup only:
    - each case's work and report directories live on a private tmpfs
      with a size cap;
    - the fixture and inputs are copied in;
    - the sandboxed execution of the fixture is unchanged;
    - the host still observes and snapshots the case;
    - the evidence the harness reads afterwards is retained.
  - Unchanged: the 30 s watchdog, `prlimit` limits, census policy and
    outcomes.
  - A control reproduces the timeout under sustained write load with
    the old placement and passes with the new one. A cap control shows
    an oversized write fails clearly rather than silently.
  - The failed receipts (REQUIRED-v3 and REQUIRED-v5) are disclosed.
  - A `fix:` commit cannot land while this plan is open, which is why
    the fix is part of this record.
- `final.sh --required` passes on the signed candidate before the
  complete final. Tabs.

## Controls

1. Callback and isolation.
   - With one build of the engine, the plain and tagged launchers return
     42 and 43 through checked execution, with a callback count of
     exactly one, against an identical engine SHA-256.
   - A second call does not leak state, and neither do distinct contexts.
   - Two callers do not share callback state.
2. Refusals and failures. The fixed header's size and version are
   validated before any tail field is read, and before anything is written
   through an output pointer the tail carries.
   - Invalid ABI or loader mismatch: no script effect and no callback.
     This covers:
     - a wrong ABI version or size;
     - an incompatible request;
     - an absent shared object;
     - the correct basename with the wrong bytes or the wrong mapping.
   - Valid relocated pair: the launcher and the engine are moved together
     to another directory. The same script succeeds on the new route, with
     one callback and the correct answer.
   - Admission refusal: no effect and no fallback.
   - Runtime error after a callback or another observable effect:
     - exactly one effect is preserved;
     - the runtime diagnostic is returned;
     - nothing is replayed, including when the diagnostic buffer is too
       small or has zero capacity.
3. TLS-v1 controls, repeated:
   - take, refusal and release;
   - nested restoration;
   - two live threads;
   - a late provider callback;
   - inspection of the final shared object's TLS symbol and relocation;
   - its executable mapping, ELF dependencies and export list.
4. Measurement through the committed 0317 path.
   - An explicit split-pair receipt, with both launcher roles hash-bound
     and the same shared object.
   - The ordinary matrix, including `fib` and `shared_rows`, sizes 1 and
     5, native off and on.
   - The bound image, PC and finish evidence is kept, through the
     unchanged 0317 accounting:
     - each object's own instruction count (runner and engine
       separately);
     - the reconciled W and T for each phase and for the whole
       invocation.
   - T includes system and dynamic work, so it is not an independent
     total for each ELF.
   - The two sibling launchers are compared like for like, per record,
     under a comparison frozen before counts. The ordinary stock
     `--complete` run still protects the stock cumulative anchor.
   - Stock-to-sibling deltas are diagnostic only, never a re-anchor.
     They are reported separately, including any setup over the limit,
     and never spend or widen the cutover allowance.
   - If the canonical launch or the seven selectors cannot be exercised,
     work stops. There is no alternate observer.
5. Finals.
   - A signed `--required` run.
   - Then one ordinary stock `--complete` run.
