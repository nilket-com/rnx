# ADR 0213: Optimistic mixed-mask checked integer SIMD

Status: Accepted

Extend generic column I64 Add/Sub and supported constant-Mul nodes to checked
AVX2 with mixed validity. Keep the existing portable execution, unsupported
multiply fallback, initialized output, borrowed operands and direct output.
No new dependency, interpreter hook, F64/Bool expansion or unsafe storage API.

Use CK2-style optimistic validity per node: run checked arithmetic across all
lanes, including null payloads. With no overflow flag, retain computed words
without a validity test per scalar lane. With a flag, rerun only that node using
the existing typed masked scalar kernel; only a valid-lane overflow may contribute
to the chain's failure flag. This is necessary because first_fault assumes a valid
scalar witness and is unreachable if fed a null-only overflow flag.

Intermediate invalid words can remain arbitrary but initialized. They may cause
later speculative overflow, handled by the same bounded masked rescan. Valid
lanes are independent, so null values cannot change valid results. Keep original
node order and aggregate valid faults before existing original-input replay,
which retains row-first then PC-first fault ordering. No retries of an entire
call or chain: at most one scalar rescan per flagged node/block. Record adversarial
null-overflow costs explicitly, not only the common non-overflow path.

Zero invalid final I64 output lanes once after the node chain, using the existing
zero_i64 mechanism. A returned Output node may be consumed by later nodes; its
valid values remain available and its invalid values may remain initialized
garbage until chain completion. Portable/unhandled operations retain their safe
current behavior; identity and constant returns retain existing masking. No
changes to F64/Bool execution or public null payload semantics.

Reuse add_pair and mul_constant where their supported shapes apply. Checked Sub
has no existing primitive: add a bounded checked subtraction wrapper and AVX2
implementation at the established primitive boundary, with scalar equivalence.
Resolve slice/constant shapes once per node/block; avoid materializing constant
columns. Constant MIN edge cases must not be transformed via overflowing negation.
Do not silently enable scalar execution while reporting AVX2; preserve truthful
path observability and forced-portable controls.

Tests: masks all-zero/all-one/alternating/random, offset and tail lengths through
64 and block boundaries, constant operands on either side, reused inputs,
Slot/Output reuse, extrema and multiply constants0,+/-1,MIN/MAX, valid overflow,
null-only overflow, null garbage overflowing only in a later node, and competing
row/PC faults. Compare Auto/Portable/reference output values, validity, exact
work, faults and cancellation. Include mixed generic chains containing fallback
nodes, final output read later, and adversarial null-only flagged blocks. Targeted
Miri covers safe borrows, boundaries and scalar paths; real AVX2 controls cover
architecture execution. Preserve first-fault replay from original input.

Proportional sequence: signed plan, reviewed implementation; format/Clippy plus
focused functional/Miri/host checks; small durable paired instruction diagnostic;
review numerical targets before first/warm10M/100M captures. Preserve existing
ceilings and flat/sliced/protected paths. The47.51Ir/row masked0212 total and41.94
executor self cost are attribution, not predicted savings. Baseline is published
0212 commit0f95d955. Select an initial target only after the diagnostic. Keep
output-zero-init as a separate ownership-design follow-up.
