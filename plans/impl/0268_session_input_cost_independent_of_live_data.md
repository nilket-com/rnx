# Implementation plan 0268: the cost of a session input does not depend on live data

Status: Proposed

Paired ADR: ../adr/0268_session_input_cost_independent_of_live_data.md.

1. Pin 2b1b1c43. Read the program domain and its collection, the
   store's reachability walk, the session's call after each input and
   its revision maps, every producer and consumer of function values
   and closures in the store, and the fixed-route inventory.
2. Before editing, send a note with: a phase timing of one input in
   the three sessions of the ADR on the pinned commit, so the walk's
   share is measured and not inferred; the alternatives considered
   (for example reachability metadata kept at construction, counts
   per program, or collection on a schedule) with what each costs on
   ordinary heap operations and what each does to release timing; the
   one proposed, the list of function-value paths the ADR asks for,
   and the release bound. If any inventoried item would change, send
   that instead and stop.
3. Implement the design and its failure handling.
4. Add the controls of the ADR, including the long rebinding session
   and the three measured sessions. Large fixtures run natively; Miri
   covers the ownership paths at small sizes.
5. Run Miri, a complete final if any gate tooling changes and a
   required final otherwise, preflight and the commit checker. Bring
   the packet with the measurements before and after, the release
   bound, the fixed-route result and the per-stage durations. The
   candidate is frozen once the packet is sent.

## Implementation and development results

Each member stores an immutable may-escape fact, computed once from validated
code in both compile_insert and compile_session. The cold census checks every
function's constants and cached instructions, including unused public bodies.
Function literals, CLOSURE_CREATE and native method tables are possible
own-program producers. Unknown call IDs remain conservative. Heap literals
are forbidden by operand validation; malformed heap constants are still
conservative in the census.

Local bodies are all inspected. Typed host calls are nonproducers: the existing
closed input/output bridge cannot import an executable script function. There
is no Function HostType. Scalar imports accept only their declared leaves;
dynamic imports reject stock Function, and opaque objects cannot become core
Function tokens. No bridge API was extended. The exact wrong-input control
requires HostExpectedNumber at argument zero, zero host effects, and retention
of the function's defining member until its last root is dropped. A supported
iterator callback forwards an old program's function through a second program;
only the original program remains after collection, and that function still
executes successfully.

Intrinsic effects are explicit. Numeric, numeric-method, variant, object and
iterator-terminal families use exhaustive enum matches. Fixed text, sort,
range, collection and utility IDs have individual entries. Coverage checks
classify every registered intrinsic and each numeric/iterator family; unknown
IDs fail closed. Forwarders/containers retain existing tokens' original code
IDs. Sealed variant predicates are descriptor-validated and return Bool.
INSTANCE_METHOD can produce an own-code token only from Code.methods, which
is tested before the census; an empty table cannot return a script method.
Typed host methods use host descriptors. No blanket reserved-ID range is safe.
Opcode and Value matches also remain exhaustive.

Collection drains pending releases, then builds the set of unleased
may-escape members. An empty set skips collection traversal. Otherwise every
direct root and pending work value is checked before descent, independently
of binding order. A Function or Closure removes its defining ID from the
missing set. Traversal stops when every candidate is found. Only heap handles
enter the work list; scalar/function bits are inspected in place.

Leases always retain their member. A noncandidate unleased member is released
by the negative proof; a candidate is retained when found. Captures of a found
or leased member are still descended while another candidate is missing.
Suspended calls and undisplayed errors keep their exclusive program borrows.
Candidate allocation and traversal errors leave the member vector unchanged;
source-map pruning still follows successful collection. Existing effects,
cancellation, root drainage and release timing remain unchanged. A dead member
is released at the next successful idle collection after its final root,
lease and diagnostic borrow ends. There is no deferred collection schedule.

Remaining collection-dependent shapes are function values/closures, native
method tables, and unknown producer IDs (including future unclassified calls).
An unused producer body is conservatively included. A candidate hidden behind
other data, or an absent candidate being proved dead, may require a full walk.
Once a dead candidate is released, later nonproducing inputs skip the walk.

No ordinary heap operation, host bridge, executor or native commit path was
edited. Code, Function, Value, Entry and Slot layouts are unchanged; Member
has the one cold fact. PROTECTED-SOURCE-v2.json proves both inventoried files
and the fixed-route policy byte-identical to published 2b1b1c43 (40 selectors).
No gate runner, guard or policy logic changed.

### Development controls

DEV-v11/RESULT.json under target/0268 is PASS: all census and domain tests,
including the host-session module; direct-root exits in both orders; first
function-child exit; cross-member captures; no-walk common library inputs;
one full dead-candidate walk followed by none; typed host rejection and
callback forwarding; unchanged cycle controls; in-walk allocation failure;
source-map failure handling; 10,000 same-name rebindings; and Rune Clippy.
Each rebinding retains one defining program and one source map.
ROOT-CLIPPY-v2/RESULT.json also passes with no new diagnostics relative to the
prior canonical reference; the final still runs its canonical stage.

Miri inventory is generated from the native test binary's actual --list:
48 exact census/domain names, including the separate host_session_tests
module, strict provenance, small fixtures. Large data and 10,000 session
inputs are explicit native controls. Fresh Miri keys actual final inputs.
Source review precedes signing; Miri, fixed-route and required final gates
remain required before any publication packet.

### Same-harness common-input measurements

COMMON-PAIR-v1.json is PASS and proves source stayed frozen through both
roles. Each role uses identical committed collection_costs.rs bytes. Before
is published 2b1b1c43; after is the current runtime snapshot. Phase timers are
injected only into isolated exports. Both engines hold 5M integers plus a
small Dense value. Session order is execution/legacy/legacy/execution, with
one warm input and three measured inputs per case/session: six samples per
engine/case. Setup and observer reads are outside timing. These unoptimized
engine-dev numbers size phases, not release performance.

| Input | Before execution ms | After execution ms | After collection walks |
| --- | ---: | ---: | ---: |
| 7 | 434.970 | 0.581 | 0 |
| v.len() | 575.506 | 141.705 | 0 |
| v[0]+1 | 576.557 | 141.776 | 0 |
| small for-range loop | 436.217 | 0.872 | 0 |
| println! | 436.466 | 0.830 | 0 |
| typed host scalar | 436.020 | 0.661 | 0 |
| dense.shape() | 435.844 | 0.786 | 0 |

Every measured execution input asserts zero collection-traversal calls after;
every corresponding baseline input records two calls. Legacy makes none in
both roles, with medians approximately 2.4--2.8 ms except println! at 4.6 ms.
Full ranges and observations are in COMMON-SUMMARY-v1.json and role receipts.
The native small controls independently assert the collector's walk/value
counters, so no-walk claims do not depend on timing thresholds.

The remaining 141 ms on len/index is not removed by this record. Additional
scratch-only attribution, COMMON-ATTRIBUTION-SUMMARY-v2.json, measures two
existing validate_format_roots calls per input: together 140.388 ms for len
and 140.336 ms for index, versus total run phases of 140.477 and 140.419 ms.
The actual len operation is about 0.0002 ms; dynamic indexing/restoration
about 0.004--0.006 ms. Collection is about 0.02 ms with no traversal.
ProgramDomain::start prepares once, then start_cross prepares again;
formatting admission traverses referenced arguments on each preparation.
That separate pre-existing data-sized walk remains and is assigned to ADR 0269.
This record does not claim all session operations are independent of data.

### Earlier sizing evidence

The pre-review conservative-census prototype's PHASES-BEFORE/AFTER-v1 pair
measured input 7 with held scalar, integer, function, closure-order and
post-drop fixtures. Those older receipts remain preliminary sizing evidence,
not captures of the revised classifier. Before/after medians were:
scalar 0.566/0.560 ms; 5M integers 435.348/0.586 ms; 5M functions
1579.122/0.576 ms; live closure before/after 5M integers
436.105/0.577 and 439.202/0.584 ms. The closure-dropping input required one
full dead proof (88.528 ms traversal, 89.325 ms total), while the next input
made no collection walk. The unchanged traversal's small counter controls
prove these early-exit and one-dead-walk cases on the current implementation.
The current classifier's common-input pair above is separate and explicit.

The host-error assertion was tightened after measurement; the runtime prefix
and all measured runtime files remain identical to the captured candidate
(POST-COMMON-TEST-ONLY.json). This is not used to reuse Miri evidence.

Review acceptance seq 31122 replaces the eighteen bare text IDs with named
constants shared by registration and the census, using the existing text and
pipeline Method IDs where available. No inventoried file or runtime dispatch
was edited. Native controls are rerun after this naming-only change; final
Miri uses the committed actual inputs.

Fresh Miri v1 stopped on two census fixture timeouts (repeated default Context
construction) and a test-output/parser collision. No UB was reported. Repair
confirmed in seq 31128 shares an immutable Context within each census test
and the eight-case callback matrix, retaining fresh domains and all assertions.
The printing control uses the existing test capture seam and additionally
asserts the exact emitted bytes; the exact-name parser is unchanged.

The same audit found four default Context constructions in the owner-publication
aggregate. The first run's roughly 524-second one-Context census cost projects
that aggregate beyond the existing 1800-second limit; this is a sizing inference,
not a completed failure of that aggregate. Its assertions are therefore shared
with four separately named exact Miri cases, following the review's splitting
rule. The original aggregate native control remains. This expands 40 selected
names to 43 without removing any assertion, raising a timeout, adding a Miri
skip or editing a runner. DEV-v14 passes all native controls and Rune Clippy.
MIRI-FIXTURE-REPAIR-v2.json proves the runtime prefixes unchanged; fresh Miri
must still run every selected name on actual new input hashes, with no old-pass
substitution. Any remaining case above roughly 1200 seconds will be split too.

Fresh Miri v2 passed all 43 selected names on committed 0ecb77e1, verified
against the actual committed input blobs with no reused test passes. Its
1368.4-second context/install aggregate, 1262.2-second drain aggregate and
1229.1-second cancellation aggregate exceed the review's roughly 1200-second
splitting threshold, so that pass does not close the record's Miri gate.

The confirmed repair (seq 31134) splits those examples and the remaining
repeated-context root-order and returned-function examples into exact cases.
All original aggregates remain native controls through shared assertion
helpers. Origin rejection uses two empty Contexts in the selected minimal
case; new positive checks prove both admit the identical source before the
foreign context is rejected. The failed-install case similarly needs no
standard-library function. Full-default-module variants remain in the native
aggregate. The inventory now selects 48 names, retaining every original
assertion with no timeout increase, parser change or Miri skip. DEV-v15 passes
the native controls and Rune Clippy. Runtime prefixes remain unchanged.
All 48 require another actual-input-key run; the packet must report its three
slowest cases and retain the older failed and timing-limited receipts.
