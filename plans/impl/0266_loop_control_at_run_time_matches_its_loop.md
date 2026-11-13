# Implementation plan 0266: loop control in the execution engine belongs to its loop

Status: Proposed

Paired ADR: ../adr/0266_loop_control_at_run_time_matches_its_loop.md.

1. Pin b32bbaa6. Read how the execution engine's lowering keeps its
   loop targets and where a `while` condition, a `while let` scrutinee
   and a `for` iterator are lowered relative to pushing the target;
   the legacy compiler's handling of the same; the loop targets of the
   constant evaluator.
2. Before editing, send a note: the table of the ADR, measured, one
   program per row, for legacy run time, execution run time, the
   constant evaluator and Rust; the rows where legacy looks wrong; the
   cause of each execution difference in the lowering; the proposed
   change; whether any inventoried item would change. If one would,
   send that and stop.
3. Correct the lowering, and the constant evaluator where the ruling
   says so.
4. Add the controls and the parity rows; regenerate the pinned table.
5. Run Miri, a final in required mode, preflight and the commit
   checker. Bring the packet with the table before and after, the class
   of each new row, the totals, and the per-stage durations. The
   candidate is frozen once the packet is sent.

Results are added here by the implementer.

## Source-review results

Step-2 ruling seq 31055 keeps Rune's valued while/while-let break and
unlabeled self-targeting control in their conditions. These deliberately
differ from Rust E0571 and E0590. For iterable evaluation remains outside
the for target; its own label is not visible there. Constants are unchanged.

The fresh baseline binary and Rust 1.98.1 measurements are retained in
BEFORE-CURRENT.json under target/0266. Each row has one program; exact Rune
source also appears in its new pinned session row. Rust only adds mutability
annotations and a type to unconstrained None. No tracked test invokes rustc.
The before/after table follows; a compile result includes the exact message
and position in the pinned row, not a generic acceptance of errors.

| Case | Legacy | Execution before | Constant | Rust | Execution after | Class |
| --- | --- | --- | --- | --- | --- | --- |
| valued_for | compile: valued break | 7 | compile: valued break | E0571 | compile: valued break | Equal |
| valued_while | 7 | 7 | 7 | E0571 | 7 | Equal |
| valued_while_let | 7 | 7 | 7 | E0571 | 7 | Equal |
| while_break_plain | 3 | 1 | 3 | E0590 | 3 | Equal |
| while_break_self | 3 | refused: loop control | 3 | 3 | 3 | Equal |
| while_break_outer | 1 | 1 | 1 | 1 | 1 | Equal |
| while_continue_plain | 3 | refused: loop control | 3 | E0590 | 3 | Equal |
| while_continue_self | 3 | refused: loop control | 3 | 3 | 3 | Equal |
| while_continue_outer | 3 | 3 | 3 | 3 | 3 | Equal |
| while_let_break_plain | 3 | 1 | 3 | E0590 | 3 | Equal |
| while_let_break_self | 3 | refused: loop control | 3 | 3 | 3 | Equal |
| while_let_break_outer | 1 | 1 | 1 | 1 | 1 | Equal |
| while_let_continue_plain | 3 | refused: loop control | 3 | E0590 | 3 | Equal |
| while_let_continue_self | 3 | refused: loop control | 3 | 3 | 3 | Equal |
| while_let_continue_outer | 3 | 3 | 3 | 3 | 3 | Equal |
| for_iter_break_plain | 1 | 1 | 1 | 1 | 1 | Equal |
| for_iter_break_self | compile: missing label | compile: missing label | compile: missing label | E0426 | compile: missing label | Equal |
| for_iter_break_outer | 1 | 1 | 1 | 1 | 1 | Equal |
| for_iter_continue_plain | 3 | 3 | 3 | 3 | 3 | Equal |
| for_iter_continue_self | compile: missing label | compile: missing label | compile: missing label | E0426 | compile: missing label | Equal |
| for_iter_continue_outer | 3 | 3 | 3 | 3 | 3 | Equal |
| while_inner_break | 3 | 3 | 3 | 3 | 3 | Equal |
| while_let_inner_break | 3 | 3 | 3 | 3 | 3 | Equal |
| for_inner_break | 6 | 6 | 6 | 6 | 6 | Equal |
| while_unknown_label | compile: missing label | compile: missing label | compile: missing label | E0426 | compile: missing label | Equal |
| while_let_unknown_label | compile: missing label | compile: missing label | compile: missing label | E0426 | compile: missing label | Equal |
| for_unknown_label | compile: missing label | compile: missing label | compile: missing label | E0426/E0268 | compile: missing label | Equal |
| body_unknown_label | compile: missing label | compile: missing label | compile: missing label | E0426 | compile: missing label | Equal |
| body_label_outer_valued_for | 7 | 7 | 7 | 7 | 7 | Equal |
| body_label_while_value | 7 | 7 | 7 | E0571 | 7 | Equal |

Target capability is compile-time metadata, separate from its output slot.
All three for variants reject a valued break to that target, whether the
result is used or discarded; a labeled valued break to an outer loop remains
valid. Loop and labeled-block targets keep their value capability.

In lower.rs::loop_ and lower/patterns.rs::pattern_loop, the target is pushed
before condition/scrutinee lowering, initially with the entry cleanup mark.
After the condition completes, its body cleanup mark is set to the original
post-condition mark. Conditions can therefore leave early while releasing
all their temporaries; ordinary body lowering retains the original shape.

The diagnostic bridge uses an explicit location cell, not message matching:
inner_break sets it before returning FrontendDiagnostic; Collector stores it
on the failed lower call and compile_error consumes it to construct the
existing BreakUnsupportedValue diagnostic at that exact span. Session
controls confirm compile category, legacy message and origin, including
for-result-discarded. Existing first-failure recording is retained.

Controls: 54 core tests PASS, including the six-function instruction-listing
fixture captured before production edits. Slots, parameters, argument words,
strings and encoded instructions match exactly for plain while, while-let,
nested while, ordinary for, adapted iterator for and text-pipeline for.
The snapshot exporter is opt-in via RNX_LOOP_LISTING; default execution asserts
the committed baseline. It is excluded from the one-compile Miri inventory.

Twenty cleanup tests each compile once and invoke three times: text, vector
and iterator-lease temporaries in condition break, continue, outer-label
break, nested-loop control and division-by-zero error; plus while-let pattern
leases through the same exits, including a continue after a previous binding.
Each invocation ends with zero live entries and zero payload bytes. Error
controls require DivisionByZero and no cleanup_error, not merely any failure.

Clippy with tests and warnings denied PASS; 26 session parity controls PASS.
All 30 new rows measure Equal; Equal rises from 202 to 232. Every old row and
other class is unchanged. The producer does not assert Equal. Unwrapped
while-let scrutinee loop remains a named admission gap; the nested-loop table
uses an admitted block wrapper and does not broaden expression admission.

No inventoried executor or policy item, opcode or constant evaluator changed.
Unrelated formatter changes in touched files were removed. Source review
before signing remains pending, followed by the new 64-test Miri inventory,
the full 52-test 0264 inventory, required final, preflight and commit checker.

Source review seq 31060 accepted with two additions, both installed. Eight
operand-temporary cleanup controls cover text-length arithmetic interrupted
by break, continue or an outer-label break, and abandoned partial aggregates
in while-let scrutinees, including nested control and runtime error. Each
compiles once, invokes three times, and checks zero live entries/payload.
All pass; no cleanup finding needed a semantic workaround.

The diagnostic marker now carries the full Location (source, revision and
span). Collector consumes it only for FrontendDiagnostic at that location.
A direct control rejects consumption by a different refusal or source, then
checks that only its matching error consumes it and receives the original
span. There is no driver retry after lowering a for body: selection happens
before entry and all body failures propagate with ?. Generic iterator driver
selection followed by a valued break and unrelated async refusal reports the
break error; reversing the order preserves AsyncBlock. Both controls pass.

Reviewed core controls: 64 PASS; direct bridge control: 1 PASS. Clippy with
execution and without execution PASS; 26 session parity controls PASS. The
new Miri inventory now selects 64 exact tests, including the direct bridge
control; the instruction-listing fixture remains native-only. Existing
constant evaluator source and its 52-test inventory are unchanged.
