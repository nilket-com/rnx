# rnx 0163: reusable invocation slots for servers

**Status:** plan.

**The direction:** 0162 found the one blocker between a Rune website and "the perf of axum": `Program::prepare` costs about 2.7 ms per request. Codex's review asked to design the fix alongside the first usable host and a real site that uses it, preserving per-request state, budgets and cleanup, and not committing to reuse before proving it sound. This record is that proof and API. The next, 0164, is a generic product host (`rnx serve`) built on it.

## 1. Where the 2.7 ms goes (measured before this plan)

A scratch release build on CPU 2, median of 300 after 20 warm-up runs:

| piece | µs |
|---|---|
| Rune's `Context::with_default_modules()` | 2,451 |
| `context.runtime()` | 115 |
| rnx's whole `prepare` + `close` (all of `context()`: Rune defaults, rnx batteries, HTTP, extensions, runtime) | 2,752 |

**About 89% of the cost is Rune's own standard-library registration,** which the September profile traced to trait-impl expansion. That's an upstream matter and out of scope here. rnx pays it per request because `server::context()` builds a fresh context for every invocation. Two parts of that context are genuinely per-execution:
- the **HTTP state and lifecycle scope** that `http::install` captures;
- the **lifecycle** that extensions register into.

## 2. The model already exists: the session

rnx's REPL and notebook worker already reuse one context across many executions. `Lifecycle` has execution **generations** (`begin` and `finish`):
- a failed or interrupted execution revokes only the work it polled;
- the context survives;
- a cleanup failure **retires** the lifecycle.

The HTTP battery's documented contract (records 0034 and 0055) is execution-scoped cancellation:
- requests polled by a failed, interrupted or budget-exhausted execution are revoked;
- pooled connections survive.

What the server adds is concurrency: up to 4 invocations interleave on one worker's `LocalSet`, and a lifecycle has one active execution at a time.

## 3. The design: slots

**A `Slot`** is one context, its runtime and its owner (the lifecycle and HTTP state), built once by `Program::slot(extensions)`. It serves **one invocation at a time**, sequentially, exactly as a session serves one input at a time.
- **`slot.prepare(handler, arguments, budget) -> Invocation`** consumes the slot. It `begin`s a lifecycle generation and builds a **fresh VM** (fresh stack, values and budget) on the shared runtime.
- **`invocation.close() -> Result<Slot, (Failure, Option<Slot>)>`** `finish`es the generation (revoking that execution's work on failure, as the session does) and gives the slot back for the next request.
- **A retired slot** (a cleanup failure, as with a cleanup failure today) is never returned. The caller builds a new one.
- **A worker holds as many slots as its active-invocation limit** (4 in 0054's host). Exclusive use is enforced by ownership: an invocation owns its slot, so no two executions ever share a lifecycle.
- **What's reused:** the immutable function tables and runtime, the extension registrations, and the HTTP client's connection pool (as across session inputs).
- **What's per request:** the VM and every Rune value, the budget, the lifecycle generation and its owned work, and cancellation.
- **Unchanged:** `Program::prepare` and every existing caller (`servers/http-postgres` passes a per-request lease through its extensions, so it keeps per-request contexts).
- **Extensions on a slot** are fixed for the slot's life; per-request data travels in the handler's arguments.

## 4. Soundness: what must be proved

A gate test suite in rnx (server-runtime) shows, each on reused slots:
1. **No state carries between requests:**
   - a handler that mutates everything it can reach (locals, objects, vectors, closures, and module-level `const` reads) leaves the next invocation's observations unchanged;
   - a returned value that escapes conversion is dropped at close;
   - no Rune `static` mutable state exists in rnx's batteries, which is checked by listing them.
2. **The budget is per invocation:** exhaustion on one leaves the next with its full budget.
3. **Failure and cleanup:**
   - a panic, a budget exhaustion and an invalid response each `close` with the slot returned, and the next invocation succeeds;
   - an HTTP request polled by a failed invocation is revoked and doesn't complete into the next (against a local test server that holds the response);
   - a forced cleanup failure retires the slot, which isn't returned.
4. **Concurrency:** four slots on one `LocalSet` with interleaved async invocations (`time::sleep` and HTTP) each see only their own lifecycle and HTTP owner, with revocation in one leaving the others untouched.
5. **Parity:** the existing server tests (0054, 0056 and 0107) pass unchanged on `prepare`. The same handler on `slot.prepare` gives identical results and failures.

## 4a. Amended after Codex's review (R1–R3), before implementation

These contracts replace section 3's sketch where they differ. The session analogy is a guide, not a proof; isolation is proved by the gates below.

**R1. An additive, separate API with a complete lifecycle.** `server::Program::prepare`, `Invocation` and their tests and semantics are unchanged. The new types are separate, and both are non-`Send` (they hold `Rc` lifecycle state). Compile-fail doc tests show that neither `Slot` nor `SlotInvocation` is `Send`.

- **`Program::slot(extensions) -> Result<Slot, Failure>`:** builds the context, runtime and owner once (the HTTP state and the lifecycle).
- **`Slot::prepare(self, handler, arguments, budget) -> Result<SlotInvocation, (Failure, Option<Slot>)>`:** validates the budget, then builds a fresh VM. Nothing has begun at this point.
  - A validation or VM-construction failure returns `(failure, Some(slot))`, since nothing ran and nothing is owned. The arguments are dropped before return.
- **`SlotInvocation::run(&mut self)`:** behaves as `Invocation::run` does today. It `begin`s the generation when first polled, runs once, and refuses a second run with the existing "already run" failure.
  - A dropped run future, polled or not, finishes its generation as failed (the existing `Finish` drop path).
- **`SlotInvocation::close(self) -> Result<Slot, (Failure, Option<Slot>)>`,** the only way to get the slot back. In order:
  1. drop the VM and any unused arguments;
  2. `finish` the generation if one began;
  3. **`Lifecycle::clear()`**, revoking every slot-owned operation (R2);
  4. return the slot.
  - A cleanup failure from `finish` or `clear` retires the slot and wins over an execution failure: `Err((cleanup_failure, None))`.
  - An execution failure with clean cleanup gives `Err((failure, Some(slot)))`. An unrun invocation closes to `Ok(slot)`.
- **Dropping a `SlotInvocation` without `close`** retires its slot: the lifecycle is closed, everything is revoked, and the slot is dropped, never silently returned.
- **Every path disposes the generation exactly once.**

**R2. Request-boundary cleanup revokes all slot-owned operations, not just the failed generation's.**
- **Why:** `finish(false)` revokes nothing, and `Tracked::poll` restamps an owned future with the generation current when it's polled. So a native future escaping one request, unpolled or pending, could otherwise become work in the next.
- **The rule:** `close` therefore always `clear`s (revoke all) without retiring. Only the HTTP client's connection pool, which isn't an owned operation, persists.
- **Value ownership:** values returned by `run` belong to the caller. The host must convert them to owned response data and drop the request and result `Value`s before calling `close`. `close` can't destroy a `Value` retained elsewhere, and this record doesn't claim it does. Native futures inside a retained value are revoked by `clear`, and polling them afterwards yields the cancellation error, never a resurrected operation.
- **Gate tests,** for retained futures on success and on failure:
  - an **unpolled** `http::get` future and a **pending** one (polled once against a held local server) are returned inside the result and retained past `close`;
  - the next invocation on the same slot begins and the retained future is polled: it must fail with cancellation and the server must see no completed request;
  - an abandoned invocation (dropped without `close`) retires the slot;
  - a dropped `run` future followed by `close` returns a clean slot.

**R3. A narrowed isolation promise, with an audit of captured state.**
- **The trusted-extension contract,** for slot reuse:
  - an extension may capture only state intended to live as long as the slot;
  - per-request state travels in handler arguments;
  - request-owned operations must use the lifecycle (`Scope::track`), so the boundary revokes them.
- **Not covered:** arbitrary mutable state captured by a trusted native extension isn't covered by the isolation guarantee; this is stated in the API docs. Per-request database leases stay on `prepare`, as planned.
- **The audit** inventories each built-in installed by `server::context()` (json, io, interchange, process, fs, path, time, text, env, http and Rune's defaults) for captured mutable state, from its install code, and justifies what persists:
  - the HTTP client pool, by design;
  - `time::ORIGIN`, a process-wide `OnceLock` monotonic origin, by design and not per slot;
  - `env`'s fixed empty arguments;
  - `process`'s exit-refusal closure, which is stateless;
  - filesystem, process and environment effects, which are external effects, not slot state.
- **A fixture extension** (`test-support`) has one intentional slot-lifetime counter, documented as persisting, and one lifecycle-tracked operation, shown revoked at the boundary.
- **Four interleaved slots** on one `LocalSet` prove that one slot's `clear`, or its retirement, leaves the other slots' pending operations alive and completing.

## 5. Measurement

The 0162 prototype host (B) gets a slot variant, B′. It keeps every 0162 gate: fixtures, fail-closed benchmark checks, wire reuse, and the instrumented phase sample written to a file. It reports, against 0162's saved A, B and C (and B′ beside a rerun of B):
- **phases:** prepare on a slot, run, convert, close;
- **HTTP:** requests per second and p50/p99 at concurrency 1 and 32, with keep-alive.

No target number is promised; the result decides whether 0164 can claim Flask-or-better.

**Measurement clarifications (review):**
- The 0162 bounded host arrangement is kept, and B (`prepare`) and B′ (slots) run **interleaved in the same session**, with successful-response counts and the fixture, wire and lifecycle gates.
- The context decomposition becomes a committed rnx-bench probe, with its source and method, not just the scratch numbers.
- The slot pool's start-up cost and RSS (4 slots per worker) are reported.
- 0162's A and C rows are historical context only. Any Flask or axum comparison for 0164 needs current matched runs.
- The 2.7 ms is described as the dominant measured bottleneck, not the only one, and removing it isn't proof of axum-like performance.

## 6. Out of scope

- Rune's registration cost itself (upstream).
- The product host and `rnx serve` (0164).
- Web helpers and route tables.
- Changes to `prepare` or to `servers/http-postgres`.
