# ADR 0212: Borrow column inputs through block execution

Status: Accepted

Remove redundant input staging in general column expressions while preserving the existing
borrowed Arrow/Polars adapter and owned-output contract. Represent input operands as checked
borrowed typed slices, constants as scalar operands, and computed intermediates as owned scratch.
Allocate scratch only for intermediates that need it, with liveness and alias checks. Preserve
specialized paths that already borrow inputs. Write final results directly into owned output
where semantics and disjointness permit; retain necessary intermediate storage.

Read validity through checked borrowed bit ranges, avoiding whole-block input mask copies where
word loads suffice. Support independent value/validity offsets, unaligned tails and differently
chunked inputs without rechunking. Retain owned output validity under the current output API;
borrowing input validity must not imply changing output lifetime or retaining engine borrows.
Return the already computed valid-row count from core completion and use it in the host.

Keep output zero-initialization, bounded scratch allocation, cancellation bounds, fuel charging,
first fault and row/PC ordering, integer overflow behavior, NaN semantics, null-payload rules,
output disposal on failure and scalar fallback unchanged. No interpreter hook changes, new
runtime dependency, SIMD tier, uninitialized output, or broad expression-language expansion.
Use safe borrowing in core and the existing audited architecture primitive boundary.

First gate is proportional: committed prerequisite checks, focused operand/bitmap/count tests,
scalar versus AVX2 equivalence, short Miri covering new borrow/alias paths, and a small durable
parallel Ir preflight. Include reused operands (x+x), constants on either side, identity outputs,
shared input owners, misaligned offsets, empty chunks, tails, all-null masks, overflow in invalid
versus valid rows, failure cleanup, and exact work/cancellation boundaries. Profile witnesses must
show payload staging removed, not merely inlined or renamed; source and pointer controls complement
symbol counts. Counts must match independent masked-popcount controls including padding.

After that passes, capture first/warm 10M/100M cases through the durable memory-admitted parallel
pool. Keep existing absolute ceilings and unchanged-path protections; apply reviewed system-library
rules only in their approved scope, with fresh pair-specific attribution for any exception. Report
full-map and kernel costs separately where measurable, memory capacities, and copies remaining.
Require a measured reduction on generic multiply3 and elimination of host recount; propose exact
numerical reduction targets for review after the first proportional diagnostic, not by subtracting
inclusive timings optimistically. Timing is a separate quiet report, never a parallel Ir gate.

Resolved design: plan operands distinguish typed borrowed input blocks, scalar constants,
and owned intermediate slots. The final node writes output directly; first-fault replay reads
original inputs, so it needs no result staging. Keep existing measured bitmap block specializations
where they win; select borrowed word traversal using the proportional diagnostic.
