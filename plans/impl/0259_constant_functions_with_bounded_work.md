# Implementation plan 0259: constant functions in an execution session, with bounded work

Status: Proposed

Paired ADR: ../adr/0259_constant_functions_with_bounded_work.md.

1. Pin a4dcb2d8. Read the collection of constant functions in the query
   layer and its recursion bookkeeping, the bounded evaluator and what its
   node count charges today, the legacy call path that must stay
   unreachable from collecting mode, and the fixed-route inventory.
2. Before editing, send a note with measurements: units spent beside
   legacy instructions for the reference functions and the proposed
   default; time to exhaust it; stack bytes per evaluator level, the
   proposed live-call and nesting limits with their margin, and the
   deepest simple recursion they admit beside legacy; how work is
   separated from the node count if they overlap today; how a callee
   diagnostic keeps its origin. If any inventoried item would change,
   send that instead and stop.
3. Implement collection, the read-only lookup, calls with isolated
   frames, return, the work counter and the depth limits.
4. Add the controls of the ADR and regenerate the pinned table.
5. Run Miri, the required stages, preflight and the commit checker. Bring
   the packet with the measurements, the class of each new row, the
   totals before and after, the selected stages and the per-stage
   durations. The candidate is frozen once the packet is sent.

Results are added here by the implementer.

## Implementation and measured bounds

Constant-function HIR is collected without using the stock assembler or VM.
Each call evaluates its arguments once in order, moves them into an isolated
frame, and consumes only its own `Return`. The query owns a shared admission
work counter; no initializer or nested call resets it. Ordinary run-time calls
remain refused at the call expression (previously the declaration was refused).

Work is charged on expression, block and function entry, and each reached
condition. The default is 2,000,000 units. These are not legacy instructions:

| Reference | Legacy instructions | Work units |
|---|---:|---:|
| scalar `f(x)=x+1` | 8 | 7 |
| linear recursion, n=16 | 202 | 235 |
| branching recursion, n=12 | 81,910 | 106,481 |
| Fibonacci, n=18 | 83,610 | 108,691 |
| branching recursion, n=14 | 327,670 | 425,969 |
| three sibling branching n=14 calls | 983,007 | 1,277,908 |

Isolated calibration exhausted 2M units in roughly 540 ms unoptimized / 67 ms
optimized; the restructured Fibonacci(25) probe exhausted them in roughly
650 ms / 65 ms. Single samples include compilation and instrumentation; they
are sizing observations, not a universal worst-case time guarantee. Fibonacci
(25) would require about 3.16M units and correctly refuses on work, not depth.
The committed branching n=14 control passes and asserts 425,969 work units
with zero data-node charge.

The cumulative data budget is **65,536 nodes beyond roots**, per evaluation:
constructing or copying N nodes charges N−1; work pays for the root. Scalar
results and scalar local reads therefore consume work but no data budget.
Moves into bindings and back to callers do not charge a copy. Bytes remain
cumulative at 16 MiB for text/key construction and copying. Aggregate-heavy
recursion can correctly exhaust data before work. The former expression/block
node charges are removed; this changes the meaning of the 0257/0258 node bound.
Their copy/Option and structural limit controls pass, including the existing
named expansion failures. Aggregate expansion is checked before construction,
so the lower-level ConstValue representation limit does not replace the named
admission refusal. Temporary vector children are bounded by the item count and
retain their cumulative construction/copy charges until assembly.

The stack bounds are independent: expression nesting 128 per function body,
combined evaluator nesting 896 across all live bodies, and 224 live calls.
Named non-inlined traversal and construction helpers keep unoptimized frames
small; the large match's original calibration used 47,712 bytes per linear
recursive call, versus 5,632 after restructuring. Production `Flow` and origin
bookkeeping are measured again, not assumed to have the scratch frame size.

Committed `constant_function_stack_boundary_2mib` explicitly creates a 2 MiB
thread. On the final development source, Rust 1.98.1, x86-64:

| Case | Unoptimized evaluator span (bytes) | Optimized span (bytes) |
|---|---:|---:|
| pending addition, n=170, admitted | 1,080,160 | 382,784 |
| tail recursion, n=200, admitted | 927,360 | 328,384 |
| pending addition, nesting refusal | 1,131,280 | 400,960 |
| shortest recursion, 224-call refusal | 508,928 | 179,200 |
| 16 nested objects per call, nesting refusal | 1,271,280 | 485,280 |
| 16 nested vectors per call, nesting refusal | 1,191,200 | 496,720 |
| body failure at n=170, unwound | 1,080,720 | 383,040 |

The test asserts measured evaluator span plus a 512 KiB reserve remains below
2 MiB. The largest unoptimized span plus reserve is 1,795,568 bytes, leaving
301,584 bytes. The separately observed pre-evaluator span is 125,832 bytes
unoptimized / 20,376 optimized. This is a compiler/profile-dependent measured
envelope with instrumentation and margin, not a proof for all platforms or
arbitrarily small embedding stacks. These tests must remain when frame shape
changes. No stack enlargement was used. Linear pending-addition n=200 exceeds
the combined bound; legacy n=3000 was admitted in calibration. This is bounded
recursion, not legacy depth parity; an explicit-frame evaluator is a follow-up.

Same-source errors retain body spans. Across sources, the compile-error API
cannot carry a different SourceId, so the error is at the caller's span and
preserves ExecutionRefusal and its exact construct for admission refusals.
Other errors use a non-panicking named-callee fallback: `in constant function \`f\`: ...`. A two-source control
checks that association. Self/mutual constant dependencies still report the
existing missing-item failure; no new cycle detector is claimed. Session
failures commit nothing and preserve prior bindings.

## Development validation

`target/0259-boundary/DEV-RESULT.json`: PASS. Rune all-feature clippy (11.30 s),
48 constant-related core controls including 0257/0258 (10.90 s including build),
optimized constant controls and 2 MiB measurements (81.23 s including build),
session constant controls (47.66 s), full session parity tests (44.54 s).
The generated pin changes only the constant-function row to Equal and adds
three Equal rows: recursion, early return and wildcard parameters. Equal rises
180 → 184; admission refusals fall 2 → 1. Other rows/classes are unchanged.
`target/0259-boundary/PIN-DELTA.json` records that comparison.

Proposed exact Miri inventory: `tools/column/constant_function_miri.json`.
Large work and stack fixtures run natively; Miri uses small limits and covers
isolated frames, Return/arguments, shallow mutual recursion, independent owned
results, failure cleanup, and the retained Option/data controls. Stack address
instrumentation is disabled under Miri's strict-provenance mode.

Disk cleanup preceded implementation: `target/0259-boundary/RELEASE.json`
records 35.09 GiB released from approved stale Miri targets and engine-dev
incremental output. Original receipts remain archived and released keys are
marked non-reusable; the current preparation still hits. Generated-evidence
retention is deferred to a tooling record.

Source review accepted with refusal-kind preservation and no user-reachable
expect. Both are implemented. Two-input controls compare refusal constructs
for work, reached loops and live-call depth against single-source admission,
check the legacy remedy, and verify earlier bindings survive. The initial
control compared input-location suffixes too; corrected to compare the construct
and remedy while allowing the deliberate call-site relocation.

Review core clippy and both stack profiles passed (`REVIEW-RESULT.json`);
corrected session and parity controls passed (`REVIEW-v2-RESULT.json`).
The stack table above records the post-review source measurements.
Miri and the complete final remain required before publication. No inventoried executor or policy item is edited.
