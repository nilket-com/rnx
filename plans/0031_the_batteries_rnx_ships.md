# rnx 0031: the batteries rnx ships

Status: proposed 2026-09-13. This record sets the integration direction and
the evidence required before batteries ship. It installs no module and
changes no execution path. The async foundation and each battery need their
own implementation records and gates.

Source checks and measurement provenance are in
[the accompanying evidence](0031_the_batteries_rnx_ships_evidence.md).

## Context

rnx is to be a practical scripting environment with Rust's vocabulary and
low ceremony. HTTP, files, structured data, environment access, and dates
should be available in the distributed tool. This does not require owning
a compiler fork: rnx continues to use released upstream Rune, currently
0.14.2, under record 0027's reviewed upgrade process.

The published `rune-modules` 0.14.2 crate already supplies optional host
modules. It is a starting point for this work, rather than evidence that
every needed API has to be written here. Its dependencies are feature gated:
HTTP uses reqwest with rustls, filesystem and time use Tokio, and JSON uses
serde_json. rnx does not currently depend on this companion crate or Tokio.

Its HTTP, filesystem, and timer operations return futures. rnx currently
executes scripts synchronously: `runner.rs` budgets a VM call, while
`session.rs` resumes execution in instruction slices so it can observe
interrupts. The integration decision is therefore about execution and host
contracts as well as names in a module list.

The preserved companion probe in `rnx-bench` at commit
`53fc3bd554befb5b55cb25aacebf07a0a96c9c1b`, measured on nano with Rust
1.98.1 and 100 runs pinned to CPU 4, takes 3.509 ms for a default-context
process, 3.816 ms with eight selected companion modules, and 4.242 ms when
also constructing the runtime context, compiling, creating a current-thread
Tokio runtime, executing an async function, and printing its result. These
committed rerun figures replace the earlier unpreserved 3.5/3.9/4.3 ms report.

The modes use one binary with the dependencies already compiled in. The
async function constructs a Duration and returns 42; it does not await I/O
or a timer. The companion probe is 13.894 MiB; the separate context-phases
probe is 7.757 MiB, with different source and resolved features. Those sizes
are not an isolated measure of adding HTTP to rnx. In particular,
`rune-modules` enables Rune's default features, including `emit`, through
its dependency. Source, raw exports, exact scope, and these qualifications
are identified in the evidence file. Build time, first actual I/O,
cancellation, memory behavior, and integrated rnx costs remain to measure.

## Decision

### 1. Own the delivered experience, reuse upstream components

rnx selects and installs the batteries it distributes. A user should not
need Cargo features or dependency installation to perform a documented
operation. Build features may select dependencies internally; the default
distributed binary must contain the documented default surface.

Use an upstream module directly when its registered API and behavior meet
rnx's contracts. Otherwise adapt it, or provide the required host API using
the underlying Rust library. Changes useful to other hosts can be proposed
upstream independently. No compiler fork or production Cargo patch override
is introduced by this record. Startup optimization upstream is separate
from supplying these batteries.

### 2. Module disposition follows the registered surface

This inventory was checked against the published 0.14.2 companion source.
An implementation in a Rust file is not sufficient: it must be registered
and exercised from a Rune script.

| Module | Existing surface | Direction |
| --- | --- | --- |
| `http` | Async `get`, client, requests, headers, authentication, response bodies and status | Adapt and validate before shipping. Preserve the simple `http::get(url).await?` entry. Define operation deadlines, body limits, cancellation, status handling, and decoding. |
| `json` | `from_string`, `from_bytes`, `to_string`, `to_bytes` | Wrap or selectively register. Preserve record 0019's guarded serialization and define parsing behavior. |
| `fs` | Async `read_to_string` only | Adapt reading to the established size/encoding contract; add write, directory operations, and other needed breadth in later records. |
| `time` | Duration, monotonic instant, sleeps and intervals | Candidate for reuse after async execution and cancellation gates. This supplies no wall-clock date/time API. |
| `toml` | Parsing and serialization | Candidate for reuse after conversion, nesting, and error behavior are checked. |
| `rand` | Random-number facilities | Candidate for reuse; document the actual generator and supported uses. |
| `base64` | Encoding and decoding | Candidate for reuse after bytes, malformed-input, and error checks. |
| `process` | Tokio command/child wrapper | Keep rnx's existing process API and records 0016–0023 as the supported path. Do not expose a second path that bypasses those contracts. |
| `signal` | Async signal waiting | Defer until ownership of Ctrl-C and interaction with REPL interruption are defined. |
| `path`, `env`, arguments, datetime | No corresponding modules in this companion crate | Design the missing surface in rnx, reusing existing host capabilities where present. |

In particular, the HTTP Rust source defines `Client::patch`, but its module
constructor does not register that method. It registers five client verbs,
not six. No request timeout setter or body-size limit is exposed by that
module. These are reasons to verify the script-facing surface rather than
declare the HTTP battery complete from its implementation alone.

### 3. One async execution model for run, eval, and the session

The intended foundation drives Rune futures on one current-thread Tokio
runtime per command invocation, retained across inputs for an interactive
session. It enables the I/O and timer services needed by selected modules.
This is a starting design to validate, not a claimed measured optimum.
There is no runtime creation per request or per REPL input. `version` and
`help` continue to return before runtime or context construction (0030).

Existing synchronous scripts retain their behavior. Async scripts use
explicit `.await`; no implicit awaiting of arbitrary returned values is
introduced. The foundation record must give executable examples for file
entry points, eval, and top-level REPL awaits, including source mapping and
bindings after success or failure. It must verify which generated wrapper
needs to be async rather than assume Rune accepts every form unchanged.

Rune 0.14.2's `budget::Budget<T>` implements `Future`: each poll restores
the remaining budget, polls its inner future, and saves the remainder.
That is an available building block. Creating a fresh full budget on every
wakeup is not acceptable. Preserve each entry point's existing budget
and REPL slice behavior; record 0021 continues to own the run budget flag.
The adapter must distinguish exhaustion from other VM errors structurally
and keep source-positioned diagnostics.

A budget counts VM instructions, not time awaiting a socket or native work.
Interruption must work while an operation is pending and while pure Rune
code is running. The foundation must prove both: selecting an interrupt
future alone does not preempt a VM poll that never yields. Completion,
timeout, cancellation, and reset must settle ownership of pending work.
No detached script task may outlive an input without a separate design.

Existing host process functions remain synchronous and supervise their own
children. They can occupy the executor thread while running; async support
does not make them concurrent. Do not move VM values onto worker threads
or promise concurrent progress without proving the ownership and lifecycle
requirements. Any blocking-worker design needs bounded shutdown behavior;
dropping an awaiting future is not itself proof that native work stopped.

### 4. Preserve bounds and give new operations their own contracts

Record 0005 measures tracked live allocation request bytes through the
global allocator and samples between session commands. It is neither an
in-flight allocation refusal nor a resident-memory limit. Runtime/client
allocations routed through that allocator count too; they are not attributed
to Rune bindings. Preserve the existing latch, reset, inspection, and
accounting-disabled behavior. A reset need not free runtime infrastructure.

New whole-file and whole-body operations therefore need explicit bounds;
the session ceiling cannot substitute for them. File reading must retain
the intent of the existing eight-MiB read limit and UTF-8 refusal. The HTTP
record chooses and documents its deadlines and response-body limits,
including whether limits apply after decompression and whether header
waiting and body consumption share a deadline. Its local fixtures exercise
those choices. This record does not invent the numerical HTTP policy.

JSON output continues through the guarded serializer in `src/json.rs`.
New names must not offer an unguarded route around cycle, depth, or
unsupported-value refusal. Parsing must specify numeric conversion, null,
duplicate keys, encoding, malformed input, and nesting behavior. HTTP JSON
helpers must use compatible parsing rules. Do not silently replace
`host::json_stringify` or change its observable behavior.

### 5. Familiar names and ordinary errors

Use domain namespaces such as `fs`, `http`, `json`, and `time`, retaining
established Rust/upstream names where they fit. Put calendar and wall-clock
semantics in their own design. New path-taking functions should accept
strings; a later path type must interoperate without forcing a script to
construct it for simple operations. Non-Unicode paths require an explicit
representation decision and must not be converted lossily by default.

Expected operation failures are Rune `Result` values usable with `?`.
Reuse upstream error types when their exposed behavior suffices; do not
collapse every error into an unstructured string solely for uniformity.
Specify any supported programmatic inspection in the battery's record.
Host operations return values/errors; they do not print diagnostics behind
the caller's back. Existing outer error rendering and exit-status behavior
remain authoritative. HTTP status policy must be documented separately from
transport failure rather than inferred from an `Ok(Response)`.

## Acceptance gates and implementation order

This record's inventory and direction are a plan. None of the following
implementation gates is claimed passed by writing it.

1. **Complete adoption evidence.** The scratch crates, lockfiles, commands,
   raw timing exports, and binary sizes are preserved at the benchmark
   commit above. Record resolved feature trees, build time, linking, and
   target for the shipping configuration, and measure the actual integrated
   rnx build before attributing costs to the product.
2. **Establish async execution.** A separate record covers all three entry
   points, synchronous compatibility, instruction accounting across real
   pending polls, interruption during pending I/O and CPU loops, source
   mapping, failed-input behavior, and shutdown. Exercise a controlled
   future and local timer/I/O fixture, not a public network service.
3. **Ship coherent JSON.** Complete parsing plus the existing guarded
   serialization contract, with conversion and refusal evidence. It can be
   developed independently of the async adapter.
4. **Deliver HTTP early.** This is an explicitly requested battery. After
   the async foundation, prove a local HTTP round trip, response decoding,
   error/status policy, finite bounds, cancellation, and client reuse.
   A broad filesystem library is not a prerequisite for HTTP.
5. **Add the remaining breadth by use case.** Files/path, environment and
   arguments, datetime, and other companion modules each carry a record,
   script-level examples, and evidence appropriate to their behavior.
6. **Keep existing guarantees.** Run the affected existing suites, measure
   version/help/eval/run and the committed JSON workload, and report any
   new startup or memory cost. Re-run record 0029's dependency/notices
   workflow for the resolved shipping features and targets. Platform
   claims stay limited to what was exercised; Linux linking does not
   establish a portable static binary.

## Open implementation questions

- Exact async entry-wrapper behavior and cancellation/slicing adapter.
- Which upstream types can be reused while enforcing rnx's I/O contracts.
- Concrete HTTP defaults, JSON conversion policy, and path representation.
- Runtime/client lifetime and measured allocation after session reset.

These are bounded follow-up decisions, not reasons to fork Rune or to wait
for upstream to build the scripting product. This record changes no release
gate, manifest toolchain claim, dependency pin, or existing public function.
