# ADR 0218: Packed two-input Boolean AND/OR

Status: Withdrawn

Boolean performance gains held, but the generic executor's code generation
could not be isolated under the protected-path performance rule. This record
is withdrawn; the Boolean work will return after the generic executor's shape
is stabilized in a separate record.

Combining two predicates with `&` or `|` is one of the most common column
operations in analytics filters. It currently scalarizes. At 100M rows, warm
whole-map cost is 93.39 Ir/row with no nulls and 108.80 Ir/row with
alternating or random validity. Packed nullable Boolean NOT already costs
1.35 Ir/row. Of the mixed-null AND/OR cost, 85.23 Ir/row is generic executor
work and 19.00 is scalar arithmetic and type dispatch. The operation itself
needs only word-wise logic. These figures come from the read-only attribution
at `e05ce43c`.

Execute two-input Boolean `&` and `|` over packed bit words, for flat and
nullable chunked columns, through the existing Bool NOT bitmap machinery. That
machinery already handles arbitrary bit offsets, tails and validity
intersection. Semantics stay strict-null: a result row is valid only when both
inputs are valid. Short-circuit and Kleene three-valued logic stay out of scope.
Value bits under invalid rows follow the established Boolean payload and
padding rules exactly.

Leave the generic scalar frames and every other operation unchanged. In
particular, protect all published I64 paths, F64 constant and pair kernels
(flat and nullable) and Bool NOT. No new dependency, unsafe storage API or
interpreter change. Preserve fuel accounting, cancellation, forced-portable
equivalence and truthful path observability.

Controls must cover the following:
- asymmetric input nulls;
- misaligned slice and chunk boundaries;
- empty, all-valid, all-null and tail-only inputs;
- the same column on both sides;
- results consumed by a later node;
- exact output, validity, padding and valid-count equivalence against the
  reference.

Choose numeric targets after a small paired diagnostic. Then measure first
and warm whole-map at 10M/100M against published `e05ce43c` under the existing
program and ceiling rules, with nonpositive protected growth.

Typed block primitives for generic F64 expression chains (126.55 Ir/row
mixed-null for `x*2.0+y`) are the next separate record. They are not coupled
to this one.
