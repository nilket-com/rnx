# Implementation plan 0267: an opaque host value in an execution session

Status: Proposed

Paired ADR: ../adr/0267_opaque_host_values_in_an_execution_session.md.

1. Pin 093e7caf. Read how the legacy session holds a host-typed value
   and dispatches its methods; the execution engine's value and store
   representations and what "Root" means in the host bridge; how the
   host bridge marshals arguments and results today and where it
   refuses; the presenters; the fixed-route inventory.
2. Before editing, send a note: the representation proposed for an
   opaque value and who owns it; how a shared-reference receiver
   reaches the host function without the engine seeing inside; the
   capability table of the ADR with a proposed disposition per row
   and its cost; how a `Dense` is obtained in tests only; every
   inventoried item the design touches, by name. If any is touched,
   send the note and stop.
3. Implement the representation, ownership, receiver passing and
   display, and the named refusals.
4. Add the controls and the parity rows; regenerate the pinned table.
5. Run Miri, a final in required mode, preflight and the commit
   checker. Bring the packet with the capability table, the class of
   each row, the totals before and after, and the per-stage
   durations. The candidate is frozen once the packet is sent.

## Implementation and development results

The admitted cut is a zero-argument host factory and a shared-reference
method with no additional arguments. The checked session entry, rather
than a spelling convention, grants admission. Ordinary execution
programs cannot reference these enrolled types, factories or methods.

The Store payload owns a legacy `runtime::Value`, which owns the native
Any object. Existing host handlers receive that object through the
existing typed bridge and shared-borrow guards. This is a remaining
legacy-runtime dependency, not its removal. Retiring that runtime must
replace its ownership/borrow implementation and handler ABI; the live
removal inventory and BACKLOG record this dependency.

The representation preserves the existing Heap handle and Call opcode.
On this 64-bit host, Value is 16 bytes, Entry 72, Payload 40, Slot 80,
HostSignature 7 and HostBinding 56, unchanged from the baseline. The
logical charge includes the entry and wrapper plus the type's declared
backing bytes. Dense declares its numeric element bytes, name bytes and
Dense size. Shared backing can be charged more than once; this is not
an exact measurement of unique physical memory.

| Capability | Disposition and control |
| --- | --- |
| Factory, binding, alias and rebind | Admitted; native session and last-release counters |
| Shadow across host/plain/different host | Admitted; verified native identity changes between inputs |
| Shared method, zero extra arguments | Admitted; Dense and a second native type with the same method name |
| Shape, names and dtype results | Existing builtin representations; exact pair/list certificates |
| Prompt, presenter and default explicit display | Admitted; shared callbacks, callback count, bounds and failure-label parity |
| Debug and other unsupported formatting | Named OpaqueDebug or FormatSpec refusal before effects |
| Explicit drop, last release, failed input and close | Exactly-once native destruction; moved-read diagnostic retained |
| Cancellation, allocation refusal and host panic | Ownership controls and allocation-failure sweep |
| Destructor panic | Contained after taking ownership; primary error preserved; close keeps draining |
| Wrong native type or hash collision | Typed borrow rejects before callback; both type names reported |
| Clone and equality | Admission refusal; private-Code typed backstops |
| Mutable/owned receiver and extra method arguments | Not enrollable through this API; compile-fail twins |
| User vector, tuple or object containing a host value | Admission refusal; only the checked session envelope is exempt |
| Host capture, script argument/return or constant | Admission refusal or compile error before factory effects |
| Factory as callback, function value, field/index call | Admission refusal before effects |
| Mixed or otherwise unproven host receiver | Conservative refusal; no runtime method lookup fallback |
| Ordinary non-session ingress | Closed at admission; restored records checked before prepare |
| Test factory in a shipped context | Unresolvable; registration is cfg(test) only |

The session proof joins local initialization and assignment, branches,
loops and restored bindings. Mixed facts do not resolve a method. Its
bounded graph and work counters produce a named refusal on exhaustion.
The wrapper's restoration prefix and final delta/result expression are
validated structurally. The whole state record cannot be captured,
exported or used as an unchecked container. Actual Store roots and Rust
TypeId are checked independently of those HIR facts.

The four Dense observations are independently measured on legacy and
execution sessions, with equivalent test-only factories. The generated
pin changes only these four rows and their summary counts:

| Row | Legacy and execution observation | Class |
| --- | --- | --- |
| shape | (2, 2) | Equal |
| names | ["x", "y"] | Equal |
| dtype | "f64" | Equal |
| DISPLAY_FMT | "Dense[f64; 2 x 2](x, y)" | Equal |

Equal increases from 232 to 236; Not classified decreases from 4 to 0.
The other classes remain: Equal by property 3, Intentional difference 3,
Refused at admission 1, Refused for a host function 10 and Wording 20.
The complete report still has 273 rows. No other row or work group was
changed. The measured test classifies observations; it does not assert
Equal before producing its report.

Development evidence under target/0267:

- RUNE-CONTROLS-v15.log: PASS, 41 native ownership/backstop controls.
- REVIEW-2-CONTROLS-v2/RESULT.json: PASS, current session controls,
  no-execution Rune Clippy and API-suite discovery controls.
- HOST-API-DOCS-v1.log: PASS, both factory twins and five method twins.
- SCHEMA-CONTROLS-v1.log: PASS, forged schema/state escape and ordinary
  member ingress controls.
- ASSOCIATED-BASELINE.json and ASSOCIATED-ENROLLMENT-v1.log: PASS,
  pre-existing associated APIs do not gain implicit host enrollment.
- CANONICAL-CLIPPY-v3/RESULT.json: PASS, no new root diagnostics.
- FORMAT-v1.json and PROTECTED-SOURCE-v3.json: PASS, formatting and all
  40 protected selector token hashes unchanged; policy bytes unchanged.
- PARITY-v2.json: full measured report. Its first comparison failed
  against the old pin as expected. PARITY-DIFF-v2.json and
  PARITY-PIN-INSTALL.json establish the four-row-only replacement.

The API discovery suite now also requires the new positive/compile-fail
twins. An existing clock-property test conditional was collapsed to
satisfy the canonical Clippy comparison; its observation and property
rules did not change.

The exact ownership Miri inventory is committed as data, including
failure, cancellation, panic and reference-edge controls. Publication
requires that inventory and a complete final to pass, including the
fixed-route compiled identity/bands, preflight and commit checking.
The publication packet carries those receipts and per-stage durations;
the source-token proof does not claim compiled identity or performance.

The second review required two cost repairs before signing. Plain-only
inputs skip fact construction and solving, while keeping linear wrapper
and state-escape validation. Host proofs use a reverse-edge worklist and
hashed variable lookup. Construction propagates host containment into a
lazy per-slot side array; no Entry or Slot grows. Public Root mutations
refuse a marked child before changing an edge. Boundary identity checks
read that mark instead of walking data, including through cyclic aliases.
Schema fields are checked in their declared order in linear time.

The suite_stage change requires the new factory/method positive and
compile-fail doctest twins (2 and 5 respectively), rejecting either
missing or extra twins. Because this changes gate tooling, acceptance
will run a complete final, replacing the plan's required-mode minimum.
The clock conditional change is an unrelated lint repair required by
the canonical Clippy comparison, not part of the host-value behavior.

Current cost-repair controls pass in REVIEW-RECOVERY-v5/RESULT.json
(ownership and both Clippy checks) and SESSION-CURRENT-v1/RESULT.json.
The current ownership inventory has 42 exact names, including the
reference-edge oracle and the new containment/mutation control. A
separate native worklist control checks conflicting facts, unseeded
cycles and the named work limit.

Cost attribution is recorded in COST-ATTRIBUTION.md and
COSTS-v4/RESULT.json under target/0267. Two unoptimized processes per
role ran in ABBA order against 093e7caf. Plain 20,000-local compilation
was 668.60 ms before and 675.86 ms after (median of six compiles).
With an empty vector, 100 further inputs took about 66.7 ms before and
69.4 ms after. With five million elements bound, the two 100-input
runs took 57.404/57.494 seconds before and 58.000/58.025 seconds after.
Large-input per-call timing ranges overlap. No zero-overhead timing
claim is made. The 20,000-local host compile also passes; that baseline
had no host representation, so no successful same-capability baseline
comparison is claimed. The direct domain API bypasses the pre-existing
32 KiB REPL text cap only for that compile control.

The existing program collector still walks live data after each input;
that unchanged traversal is a source-identified contributor to baseline
large-vector latency. Exclusive phase times were not measured. This
record removes the additional admission walk and its two-million-node
refusal, not the pre-existing collector.
First host construction realizes the implicit false slot marks once;
that metadata initialization is O(existing slot count), without
reading payload graphs. Later boundary checks depend on binding count.

### Allocation cost after holding a host value

The additional source-review measurement uses four fresh sessions in
ABBA order (without/with/with/without a bound test Dense), on the same
engine-dev executable. Both contexts install identical modules; only
the preconditioning input differs. Generated work source and scalar
results are checked equal between states. Times include compilation
and execution, not an exclusive allocator phase or a release profile.

| Input | Without Dense, median | With Dense, median | Change |
| --- | --- | --- | --- |
| 30,000 iterations, small nested vectors and formatted strings | 341.86 ms | 369.81 ms | +8.17% |
| One 14,000-element vector literal, evaluated 100 times | 1806.56 ms | 1830.27 ms | +1.31% |

The small-vector/string cost is more than a few percent and needs the
reviewer's explicit decision in the publication packet. No threshold
was installed for this measurement. The mark-join path remains active
after a host value has been held, including after it is dropped; this
cost is not described as zero or hidden inside the plain-session cost.
The literal remains within the existing input-text cap. Both explicit
cost controls are ignored by the routine suite and run separately.
Raw ABBA samples and source equality: ALLOCATION-COST-v1/RESULT.json
under target/0267; derived numbers: its SUMMARY.json.

### Complete-final formatting repair

The complete final stopped at formatting in three files unchanged from
published 093e7caf: constant_loops.rs, constant_patterns.rs and
constants.rs. The repair is exactly pinned rustfmt 1.98.1 output,
including import/assert normalization and a redundant closure-body
block. There are no hand logic edits. FORMAT-BASELINE-PROOF.json and
FORMAT-REPAIR-v1/RESULT.json under target/0267 record the before/after
hashes and reproduce the formatter output from baseline bytes.

The earlier Miri pass belongs to the pre-format candidate 0d2dd452;
the amended bytes require a fresh Miri key and complete final. The
plain/large-binding cost comparison above was measured on that
pre-format version. The active-host allocation control was remeasured
after formatting with the same ABBA protocol:

| Input | Without Dense, median | With Dense, median | Change |
| --- | --- | --- | --- |
| Small vectors and strings | 341.59 ms | 368.24 ms | +7.80% |
| Large vector literal | 1798.86 ms | 1819.64 ms | +1.16% |

The small-allocation cost still needs the reviewer's packet decision.
ALLOCATION-COST-v2/RESULT.json holds the raw samples and identical
source/result assertion; its SUMMARY.json holds the derived values.

### Exhaustive bridge route repair

The complete final then found an input classification defect in the
existing exhaustive HostType::ALL control. The new OpaqueShared case
sent function and type tokens through host_tag, producing a generic
store Type error. They now retain the existing Input::Function and
Input::TypeValue classifications before native-receiver inspection.
Valid native/moved receivers and every earlier signature route keep
their behavior. The negative assertion was retained, not weakened.

The complete standalone rune-suite passes after the repair
(RUNE-SUITE-RECOVERY-v1/RESULT.json under target/0267). That same
minimal negative control is included in the committed Miri inventory,
which now has 43 exact names. Fresh Miri and a complete final on the
amended runtime remain required; the previous final's passed stages
are not carried forward as publication proof for this change.

Allocation timings above were measured before this cold invalid-token
route repair. The two workload sources exercise no such tokens or
native shared-receiver calls; no revised timing claim is inferred.
The observed active-host allocation cost remains a packet decision.


### Bounded plain-store heap repair

The complete final on 85321662 passed all stages except fixed-route.
Protected source tokens passed, but program instructions grew outside
both bands: sieve off +8.69%, permute off +3.81%, shared rows off +2.81%
at the larger size. No performance exception or policy change was made.

One attribution pair reproduced both binary hashes and the sieve/I64
vector counts exactly. Instruction annotations show index_tag outlined
from dynamic indexing and tag outlined from push. Payload tag dispatch
also added one instruction per heap lookup. The admission path added
mode checks and metadata spills. Function and instruction counts are in
HEAP-ATTRIBUTION-v1 under target/0267; raw profiles and recreated builds
were released after extracting those counts.

Repair attempt 1 stores the Host Type inline in the payload; its native
object remains boxed. Payload::tag, Store::tag and index_tag inline at
callsites. Only the native constructor activates containment marks.
Admission makes one store-mode check, then specializes the existing
transaction for plain stores or a cold, out-of-line marked path. Mark
reservation still precedes edge retention; publication remains
nonfallible. Entry, Payload, Slot and Value size assertions pass.
No executor inventory item or band policy was edited.

HEAP-DEV-v2/RESULT.json passes the ownership/reference-edge controls and
Rune Clippy. The initial native harness omitted default diagnostic
features and failed to compile unrelated existing tests; correcting the
harness required no production change.

The same ABBA allocation control was remeasured after the admission
repair, before changing this results text:

| Input | Without Dense, median | With Dense, median | Change |
| --- | --- | --- | --- |
| Small vectors and strings | 338.34 ms | 369.67 ms | +9.26% |
| Large vector literal | 1708.93 ms | 1721.73 ms | +0.75% |

ALLOCATION-COST-v3/RESULT.json records raw samples and matching source
and results. These remain unoptimized engine-dev compile-plus-execution
measurements. Earlier corresponding costs were +7.80% and +1.16%.
The reviewer accepted recording the active-host cost without installing
a threshold; the revised numbers are disclosed, not hidden. The mark
path remains active after a host object is dropped until store close.
Plain-store fixed-route bands do not cover this active-host cost.
Fresh exact Miri and a complete final on the amended source remain
required. At most two heap repairs are authorized before a residual
band failure must be reported.


Repair attempt 1 did not pass its fixed-route screen. After the heap
helpers became cheaper, forcing the outer tag lookup inline changed
the complete interpreter loop: fib off +2.00%, loops off +3.67% and
numeric calls off +2.70%. The native fib symbol witness also failed
because invoke_args had folded into invoke_recursion; the original
FAIL is retained. Measurement-only accounting of those two profiles
does not turn the failed witness into acceptance evidence. This screen
is HEAP-SCREEN-v1 under target/0267.

Attempt 2 removes forced inlining of Payload::tag and Store::tag. The
Host arm shares the existing inline-tag return with List and Record.
The small Entry::tag accessor and index_tag inline at their use sites;
the cold marked admission transaction remains. No third heap attempt
is authorized if a fixed-route workload is still outside its band.
HEAP-DEV-v3 passes ownership, reference edges, sizes and Rune Clippy.

The active-host ABBA allocation cost was remeasured on attempt 2:

| Input | Without Dense, median | With Dense, median | Change |
| --- | --- | --- | --- |
| Small vectors and strings | 336.28 ms | 368.71 ms | +9.64% |
| Large vector literal | 1796.59 ms | 1811.54 ms | +0.83% |

ALLOCATION-COST-v4/RESULT.json and SUMMARY.json retain these samples.
The earlier +7.80%/+1.16% and attempt-1 +9.26%/+0.75% remain disclosed.
This active-host bookkeeping cost is accepted for recording without a
threshold; it is separate from the unchanged plain-store bands.
