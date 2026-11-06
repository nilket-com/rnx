# ADR 0220: Pure-Boolean column plans run in their own driver

Status: Accepted

Combining Boolean predicates with `&` and `|` is the core of analytics
filters, but it currently runs through the generic scalar executor. At 100M
rows, warm whole-map cost is 93.39 Ir/row with no nulls and 108.80 Ir/row
with mixed nulls; packed Boolean NOT costs 1.35. ADRs 0218 and 0219
established two facts. Packed word execution delivers the gain: 1.2 Ir/row
flat and 14 mixed in prototypes. But any change to the shared generic
executor, including a second instance of it, changes how its existing paths
compile.

Route pure-Boolean plans to a separate whole-plan driver, chosen once per
call. A plan qualifies when every computed node is Boolean `&`, `|` or `!`
over Boolean inputs and constants, and it contains at least one `&` or `|`.
NOT-only plans keep their existing path. The driver lives in its own module.
It evaluates the whole plan over packed words, including intermediates, so
chains stay packed. It reuses the existing bitmap readers and writers and the
validity intersection. Semantics stay strict-null: a row is valid only when
all of its inputs are valid. Short-circuit and Kleene logic stay out of scope.
Fuel, cancellation, faults, payload and padding rules, and path observability
match the existing contracts.

Do not edit the generic executor, `Step`, `operation`, or any function the
generic path calls, and add no new callers to them. Classification happens
inside the existing preparation pass. Every other plan takes the unchanged
existing path.

Controls must cover the following:
- asymmetric and offset validity;
- misaligned slice and chunk boundaries;
- empty, all-valid, all-null and tail-only inputs;
- the same column on both sides;
- multi-node chains, with NOT inside a chain;
- constants;
- exact output, validity, padding and valid-count equivalence against the
  reference;
- fuel and cancellation.

Acceptance against published `e05ce43c`:
- every non-qualifying shape (I64 and F64 specialized and generic paths,
  Bool NOT and Bool comparisons) has an exact program function vector
  (Ir and calls) at 4096 and 65536 rows, first and warm;
- the only exceptions are reviewed fixed per-call constants (the invoke
  branch and classification), pinned by exact vector with a +1 control;
- the protected SIMD shape and hot-loop counts are unchanged;
- unknown objects are compared by totals.

Set Boolean targets after the small diagnostic. Then measure first and warm
whole-map at 10M/100M under the existing program and ceiling rules.
