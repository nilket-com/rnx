# Implementation plan 0264: loops and assignment in constants

Status: Proposed

Paired ADR: ../adr/0264_loops_and_assignment_in_constants.md.

1. Pin a079db4a. Read the bounded evaluator's flow type, frame and
   locals, its block and call paths, the matcher of ADR 0262, how the
   loop and assignment forms arrive in the representation the evaluator
   reads, and how the run-time lowering gives them meaning.
2. Before editing, send a note: each form of the ADR at run time in both
   engines and as a legacy constant, with any disagreement; every loop
   and assignment form the language accepts and which are proposed; how
   break, continue and labels are carried without a native frame per
   iteration; what one iteration costs in work and where it is charged;
   what assignment to a part of a local needs from the value
   representation, and whether it is in or out; the work spent and the
   time for ten thousand and for the largest admitted number of empty
   iterations in both profiles; anything in the ADR the code makes
   wrong. If an inventoried item would change, send that instead and
   stop.
3. Implement the forms.
4. Add the controls of the ADR and regenerate the pinned table.
5. Run Miri, a final in required mode, preflight and the commit checker.
   Bring the packet with the class of each new row, the totals before
   and after, the stack table, the work and time table, and the
   per-stage durations. The candidate is frozen once the packet is sent.

Results are added here by the implementer.

## Implementation measurements (before source review)

Constant-only target IDs carry break/continue through the existing flow
helpers. A call saves the target base, hides caller targets, and restores it
on every exit; caller argument evaluation precedes that change. Loop bodies
and pattern conditions truncate locals before consuming or propagating flow.
All iteration is iterative; only syntactic nesting adds native frames.
Bare-local assignment evaluates the RHS before reading the LHS for compound
operations. Part assignment remains a reached named refusal, including field,
index and compound forms. No executor or run-time lowerer is edited.

Direct ranges include signed/unsigned integers and characters, inclusive,
exclusive and start-only, with transparent grouping. Vectors, tuples, unit
(empty tuple) and Option are iterable. Object iteration is a reached named
refusal: its sorted-key constant representation loses insertion order.
A range stored as a value and a range without a start stay named refusals.
The open range at MAX follows the corrected ruling: numerical overflow
before yielding, matching execution runtime. MAX-1 can be yielded and an
immediate break succeeds; the next step fails. Inclusive MAX..=MAX finishes
normally. Signed, unsigned and character controls pass. Legacy integer
open-range panic is retained as a known defect, not pinned as Equal. Valued for-break is the legacy error as ruled; the existing
execution-runtime acceptance of that form remains a separate follow-up.

Production constant tests: engine-dev 81 PASS, engine-release 81 PASS;
clippy with warnings denied PASS. Session parity controls 25 PASS, including
the regenerated pin. Exactly eight constant rows are added, all measured
Equal: counting loop, while assignment, range for, vector for, valued break,
continue, nested early return and a 10,000-iteration constant function.
Equal rises from 194 to 202; every prior row and other class is unchanged.

Single-run compile-plus-evaluate measurements, not steady-state benchmarks:

| Case | Work | engine-dev seconds | engine-release seconds |
| --- | ---: | ---: | ---: |
| Empty for, 10,000 iterations | 30,005 | 0.04279 | 0.00363 |
| Counting function, 10,000 iterations | 70,011 | 0.05810 | 0.00588 |
| Largest admitted empty for, 666,665 iterations | 2,000,000 | 0.49047 | 0.05033 |
| Next empty for, 666,666 iterations, refused | 2,000,000 | 0.49208 | 0.04775 |
| Empty infinite loop, work refusal | 2,000,000 | 0.26102 | 0.03484 |

Empty for costs 3*n+5: one attempted cursor step including exhaustion,
pattern work and body-block work per successful step, plus expression/range
setup. The shared work counter is never reset. Repeated `v=[0,0]` succeeds
32,768 times; replacement 32,769 exceeds 65,536 cumulative expanded nodes.
Repeated `v=[v,0]` succeeds 122 times; iteration 123 exceeds nesting 128,
with 29,768 nodes charged, before reaching the node limit. Dropping replaced
values refunds nothing. Boundary controls assert these outcomes.

2 MiB thread controls retain at least 512 KiB of measured reserve:

| Shape | engine-dev bytes | engine-release bytes | Outcome |
| --- | ---: | ---: | --- |
| 21 nested loop/break expressions | 240,376 | 53,356 | accepted |
| 22 nested loop/break expressions | no evaluation | no evaluation | existing HIR depth refusal |
| Loop reached through 171 live constant calls | 1,316,472 | 429,804 | accepted |

The pre-existing constant depth/call matrix also passes in both profiles.
Measurements include the compile-to-evaluation prefix. No limit changed.

Read-only alias audit: admitted constant operations expose scalar values,
structural reads, destructuring and fresh owned results, but no aggregate
identity predicate, mutation or native host call. Rebinding a local does not
mutate its old referent in either representation. Session controls cover a
shared-looking local then rebinding one name while reading the other. Thus
no already-admitted read-only form was found that observes this difference;
part mutation remains refused because it would expose it immediately.

Source review accepted with the following completed checks. Four independent
processes per form/mode produced first key z and joined zamb from a freshly
constructed object with insertion order z,a,m,b. Constant object storage sorts
keys, so iteration during constant evaluation is refused by name. Core and
session controls pin this refusal; the old order-insensitive sum control is
removed. A published constant defined in an earlier input displays sorted
keys, iterates as abmz, and stringifies as a,b,m,z in BOTH sessions; that
existing behavior agrees and is unchanged.

Break/continue in while conditions and while-let scrutinees keep the while's
own scope, matching legacy. Four core controls return3. Execution runtime
currently targets the outer loop for break (returns1) and refuses continue;
this is a separate lowering defect, as is its acceptance of valued for-break.
Neither defect is changed or pinned as a constant parity row here.

Pattern failure text matches execution: Panicked: pattern did not match.
Reassigning an iterable local during iteration still visits the original
sequence (sum6); assigning a pattern-shadowed variable leaves outer11.
These controls pass. Range endpoint controls cover eager open overflow,
MAX-1 immediate break, subsequent-step overflow and inclusive MAX, for signed,
unsigned and character cursors. Legacy open integer range process panic is
recorded as a legacy defect; no runtime change is included.

Miri inventory has one compile per exact test. Miri and the required final
remain pending; this is not publication evidence.

After the reviewed corrections: all 99 constant controls pass in both
profiles, 25 session parity controls pass, clippy passes and preflight passes.
Final release stack measurements are 53,180 bytes for 21 nested loops and
426,892 bytes for the loop through 171 calls (debug figures unchanged).
The signed candidate proceeds to Miri and required-mode final verification.

Miri first invocation: 50 exact tests PASS; the two object-refusal tests
FAIL on an existing strict-provenance-unsupported zero-size allocation cast
in hir/arena.rs, before reaching loop evaluation. Both test initial strings
are now nonempty markers, preserving the same refused iteration and unsorted
keys. No arena or evaluator production change. Original failures remain in
the receipts. Repaired tests pass natively; exact Miri recovery and reuse
proof for the unchanged tests remain pending. Empty-string arena provenance
is a separate follow-up; this record does not claim it repaired that path.

Required final v1 stopped on two obsolete session refusal controls. The
failure-preservation case now uses still-refused part assignment; the
cross-input infinite loop expects the work refusal instead of unsupported
loop syntax. Focused session controls pass. This is test-only; the ordinary
required final is rerun on the amended signed candidate.
