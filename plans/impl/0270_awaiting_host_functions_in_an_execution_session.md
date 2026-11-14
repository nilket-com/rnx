# Implementation plan 0270: awaiting a host function in an execution session

Status: Proposed

Paired ADR: ../adr/0270_awaiting_host_functions_in_an_execution_session.md.

1. Pin ee4cf69e. Read how the asynchronous host functions are
   registered and tracked by the session's request scope, how the
   legacy session drives and interrupts them, where the execution
   session refuses await and the five functions, the typed host bridge
   and its enrollment, the interrupt check between work slices, and
   the fixed-route inventory.
2. Before editing, send a note with: what legacy does, observed, for
   each input class of the ADR, including a call that is not awaited
   and an interrupt while pending; the alternatives for waiting (for
   example the host call waits on the session's runtime with
   cancellation, or the invocation suspends and the session resumes
   it) with what each costs a program that awaits nothing; the one
   proposed; how the form is recognized at compile time and what each
   refused neighbor reports; how interrupt reaches a pending call; the
   fate of `http::request`; and every inventoried item touched, by
   name. If any is touched, send the note and stop.
3. Implement the design.
4. Add the controls of the ADR and regenerate the pinned table. State
   the class of each new or changed row and the totals before and
   after. Run the paired fixed-route screen before the full gates.
5. Run Miri on a committed inventory with no case near the time limit,
   a required final (complete if gate tooling changes), preflight and
   the commit checker. Bring the packet with the interrupt delay, the
   fixed-route result, the three slowest Miri cases and the per-stage
   durations. The candidate is frozen once the packet is sent.

Results are added here by the implementer.

## Implementation results (source accepted; final gates pending)

The implementation admits direct `time::sleep`, `http::get` and
`http::request` waits in an execution-session input. The two HTTP byte
functions remain named bytes refusals. No inventoried executor item,
host bridge, owned-input exporter or fixed-route policy changes; the
two inventoried files remain byte-identical to the base, covering all
40 selectors. There is no new opcode, executor/frame field, thread or
runtime per wait. HostBinding remains 56 bytes and HostSignature 7
bytes on this 64-bit host.

### Enrollment, ownership and driving the native operation

`Module::host_function_await` retains the original asynchronous handler
for legacy and installs a separate synchronous execution adapter via
the existing alternate-handler mechanism. Its sealed inputs are i64,
borrowed string views over owned snapshots, and the request's two
strings plus OwnedTreeInput. The terminal channel is
`Result<Value, VmError>` and successful output requires an explicit
certificate. No Future becomes an execution Value. API controls have
a compiling twin and reject a non-async original, a future-returning
adapter and arbitrary Value input at the intended trait bounds.

Actual function calling convention and original FunctionHandler
allocation identity grant permission. Mutable documentation flags and
matching names do not. Metadata is prepared fallibly and committed
only after successful module installation. Failed installation, invalid
output certificate and replacement-handler controls preserve refusal.
The additional metadata is cold Context/module data, not a larger host
binding or a per-instruction lookup.

The session loans its existing owned Runtime to an invocation-scoped
thread-local slot. A native waiting adapter drives only its owned
native future, with no script callback or Store borrow while pending.
The runtime cannot be captured at enrollment: one Context installs
handlers before independent sessions create their own runtimes.
RAII restores the loan after success, script error, native panic and
interruption. An enter guard gives other synchronous handlers the
same Tokio context as legacy; it drops before the owned loan returns.
A polling guard refuses reentry. Two sessions on one thread do not
drive each other's queued tasks; shutdown still drains the restored
runtime. Pure synchronous persistent sessions have no entered Tokio
context in either engine; the one-shot/file run_sync driver is a
different route. A corresponding-mode differential control pins this.

The interpreter continues to slice between instructions. Inside an
admitted host Call, native waiting is synchronous from its perspective;
a pending script execution/future is never suspended or retried by a
work slice. This avoids the previously observed nested-async budget
failure. Arguments run once. A native wait is never repeated, and its
call-owned inputs release once on terminal error. A pending 35 ms
sleep requires exactly the same hard work budget as a zero sleep;
one less fails for each. The elapsed lower-bound property also passes.

### Admission and diagnostics

Only direct `.await` of an actually asynchronous host call is lowered,
and only in the checked session entry. It uses the existing Op::Call.
Futures as values, user asynchronous functions/blocks, select and
other await forms stay named refusals before effects. Bare calls name
the resolved function and give its direct-await remedy; taking a
function value also names it. Exact text and input/line/column controls
pass. The ordinary-function await-outside-async compiler diagnostic is
retained, including its location.

The initial synchronous compilation retries exactly once with an async
wrapper only for the structured requires_async_context diagnostic.
Unrelated compile errors, including prose containing "await", are not
classified by text; their legacy message/location controls pass. A
20-round alternating native phase measurement (first two excluded)
found median plain compilation 0.476 ms and awaiting two-attempt sum
0.761 ms. Individual first/second medians were 0.252/0.507 ms. These
are small-input, unoptimized engine-dev observations, not a universal
compilation bound. Timers exist only in test builds.

The opaque_session field carries ProgramDomain's checked envelope:
roots and restored names, one state parameter, exact delta fields and
a final result local are validated. It does not make arbitrary embedding
source trusted. In the product adapter, prepare_input reserves main
before generating the wrapper; a user `async fn main` is refused by
that existing reserved-name rule without effects. Non-session async
main compilation is separately refused as AsyncFunction and calls no
adapter. Other user async functions remain refused even within the
session compilation. No generic asynchronous-function support is
claimed.

### Request options and the reviewed snapshot access rule

The request parser reads the existing bounded owned packet without a
second tree or quadratic field lookup. Legacy and owned views share
option validation. A typed fields error distinguishes non-object,
invalid packet and handler failure; diagnostic text never controls
which case is selected. Headers/body, URL errors, option errors,
cycles and request response bounds are controlled against a loopback
listener; no test reaches another host.

A host argument taken by snapshot reads its whole reachable value.
A moved descendant is therefore an access error at the call, before
the handler and its effects, even if its parser would skip that field.
This is an intentional difference from legacy. Four exact observed
pairs are pinned: moved unknown field under invalid/valid URL, moved
headers and moved body. Legacy returns a script Err in these cases;
execution reports the moved access at the request. Negative controls
alter the legacy result, new message and origin to reject any broader
exception. Product controls require no connection, earlier bindings
intact and a usable next awaiting input. Unknown fields containing live
values (including nested and diagnostic-only closure leaves) retain
legacy URL/key selection. Selective host-object inspection remains on
the backlog; this access rule is not a general type-error allowance.

Owned export retains its existing peak-byte accounting against the
Store's configured bytes limit, including descriptors, text, keys and
validation/traversal scratch. Default session Stores use isize::MAX,
so this record introduces no practical body-string ceiling. Existing
session allocation, representational and memory limits still apply.
A control with an explicitly configured 20,000-byte embedding Store
shows a 16,000-byte body fits in the Store but fails its snapshot as
`owned host input bytes: StoreCapacity` before a request. The legacy
parser accepts the same body, checked independently. Those numbers
are that test's explicit limits, not a default product body bound.

### Cancellation and measured parity

Pending work retains the existing 5 ms interrupt cadence. Pre/post
completion checks and an invocation-local cancellation latch make
interruption terminal, never a catchable script Err or a prose match.
Heap effects already performed remain; scalar/staged bindings do not
publish. A genuine script error is not replaced by a coincident global
interrupt flag. Only awaiting inputs gain the late-Complete check;
ordinary inputs retain their published completion boundary.

A native real-SIGINT control requires return within 250 ms, pending
socket EOF, no catch/continuation effects, retained earlier bindings
and a working next synchronous/awaiting input. Latest samples were
1.591/0.826 ms for legacy sleep/HTTP and 1.534/1.508 ms for execution.
They do not establish a tighter latency guarantee. Request scope
cleanup and runtime restoration are exercised together.

The regenerated classification contains 284 rows, each independently
observed. Before/after class counts:

| Class | Before | After |
| --- | ---: | ---: |
| Equal | 236 | 241 |
| Equal by property | 3 | 3 |
| Intentional difference | 3 | 10 |
| Refused at admission | 1 | 1 |
| Refused for a host function | 10 | 9 |
| Wording | 20 | 20 |

The three bare enrolled async calls versus legacy's inert futures are
intentional differences, with named remedies; three awaited calls are
admitted. Both HTTP byte functions remain bytes refusals in bare,
function-value and awaited forms. The options panel adds four moved-value
intentional differences and two Equal live-value neighbors. No
unreviewed difference was promoted. The generated pin is 155,728
bytes under the unchanged 163,840-byte cap, with one row per line.
Its mutation and enrollment-removal controls pass.

The committed seven-case minimal Miri inventory covers trusted
metadata, scoped direct-call lowering, non-session refusal, slicing,
terminal cleanup and the mixed owned-options bridge. Actual input-key
Miri and the paired fixed-route screen precede final acceptance.
Native controls cover Tokio, real signals, HTTP and runtime isolation;
Miri is not claimed to exercise those OS facilities. The complete
final, preflight and commit checker remain mandatory. Miri data under
tools/column adds no controller implementation. The census classifier
repair described below does change gate implementation, so acceptance
uses the complete catalogue, not only the path-required stages. The
final packet carries the receipts, per-stage durations, added stages
and three slowest Miri cases.

The live census fingerprint for src/time.rs is refreshed with the
existing checked refresh tool from the pinned base to the signed
implementation source. The diff changes that one live hash only;
historical provenance and the registry witness are unchanged.


### Validation repairs before final acceptance

Native pre-runs exposed an ordinary lowering-stack regression. The
new asynchronous refusal path had enlarged the unoptimized call frame.
Bare-call and function-value errors now share one cold, non-inlined
helper, and await_host is explicitly out of line. Matched engine-dev
Rune library-test builds with the execution-serde feature composition
measure 1,112 bytes for Lower::call in both base and repaired source,
and 1,624 bytes for expr in both. The refusal helper's separate frame
is 616 bytes; await_host's is 3,400 bytes. Neither belongs to ordinary
recursive call lowering. The normal 2 MiB boundary and unchanged 90%
headroom controls pass; focused checks cover every execution-bearing
feature composition used by the native suite and Clippy, including the
doc/languageserver composition whose canonical suites select other
tests. Legacy without the execution feature has no such lowering path.
No depth, stack or assertion limit is relaxed.

Cross verification initially rejected the newly specific refusal for
non-session async main in census case 0212. Its exact admission class
and admission-only execution reason are now AsyncFunction, replacing
Await. It remains refused before effects. No other census admission
or execution policy row changes. The classifier accepts this exact
class and rejects an unknown neighboring class. The harness-free
self-check exercises those controls and checks agreement of the two
companion tables before live cases. Its full live cross pre-run and
whole root test suite pass.

The registry check exposed an incorrect remedy for an asynchronous
function without a wait adapter: awaiting it is not available either.
AwaitRequired and FutureValue now apply only when the actual installed
function has a wait adapter. The published std::future::join registry
expectation stays Native. Bare, function-value and checked-awaited
forms of unenrolled async functions keep their named native refusal;
both HTTP bytes functions keep their bytes refusal in all three forms.
Product controls require the right function and reason, no unavailable
await recommendation, no earlier array mutation and no TCP connection.
The minimal Rune controls cover an unenrolled asynchronous function as
well, including checked await. The observed table changes exactly the
two bare bytes-function rows and their derived summary. No other
observation is changed.

A subprocess control compiles and executes nested awaited arguments
on both the 2 MiB stack and the unchanged 90% headroom stack. Direct
nested calls admit 127 and refuse 128; a nested block argument admits
64 and refuses 65. These exact frontiers are pinned. Every admitted
nesting calls the adapter exactly once per level; refused construction
calls none. Neither control accepts a signal or a smaller bound. This
covers await_host's recursive frame, in addition to the existing
ordinary lowering controls. All execution-bearing feature compositions
must pass these controls, and the matched call/expr prologues are
measured again on the corrected source before the full native pre-run.
The earlier acceptance-v4 on the rejected remedy variant was stopped
before Miri or final acceptance and supplies no acceptance evidence.
