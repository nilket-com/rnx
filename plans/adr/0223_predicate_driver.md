# ADR 0223: Filter predicates run in their own driver

Status: Accepted

Filter predicates are comparisons combined with Boolean operators, the core
analytics filter shape. They run through the generic scalar executor
(warm, 100M rows, flat / mixed nulls):

| Predicate | Flat | Mixed nulls |
|---|---:|---:|
| `x > 1.0` | 64.3 | 79.0 |
| I64 `x == c` | 59.3 | 74.1 |
| `x > 1.0 & y < 2.0` | 168.8 | 202.5 |
| that conjunction combined with a Bool column | 236.1 | 279.5 |
| `x*2.0+y > 3.0` | 167.8 | 201.5 |

They qualify for neither the pure-Boolean driver (ADR 0220) nor the pure-F64
driver (ADR 0221). Those records showed that a separate whole-plan driver,
selected once per call, isolates cleanly. A change to the shared generic
executor does not.

Route Bool-output predicate plans to a third driver in its own module,
selected through the existing out-of-line dispatcher, so `invoke` is
unchanged. A plan qualifies when:
- its result is Bool;
- it contains at least one numeric comparison (`==`, `!=`, `<`, `<=`, `>`,
  `>=` on two I64 operands or two F64 operands);
- its other computed nodes are F64 `+`, `-`, `*`, or Boolean `&`, `|`, `^`,
  `==`, `!=`, `!`.

Plans with I64 arithmetic stay on the generic path. Checked overflow brings
fault and replay ordering, which this record does not take on, so the driver
cannot fault. Mixed-type comparisons remain unsupported, as today.

The driver evaluates whole blocks. F64 intermediates stay typed (u64-bit
scratch, as in ADR 0221). Comparisons produce packed Bool words, and
Boolean nodes operate on packed words, as in ADR 0220. Results must equal the
generic executor exactly:
- IEEE comparison semantics: ordered comparisons and `==` are false with NaN;
  `!=` is true;
- the same F64 operation order, with no FMA;
- strict null intersection of all inputs;
- zero payload under invalid rows;
- the existing fuel, cancellation, refusal and observability contracts.

The new loops have their own reviewed block shapes. F64 comparisons use
64-lane word loops with packed operations; I64 comparisons retain the cheaper
bounded eight-lane form selected by executed instruction counts. Executed
specializations are pinned by instruction snapshot, with no per-lane bounds
checks. Arithmetic keeps packed primary loops with matched byte stride. Nothing calls the generic executor, the
protected SIMD kernels or the other two drivers' kernels. Shared bitmap
helpers may be reused under the 0220 rule.

Acceptance against published `e4e6f465`:
- every non-qualifying shape, including both existing drivers, keeps an
  exact program function vector, apart from reviewed fixed pins;
- protected and driver shapes are unchanged;
- predicate targets are set after the small diagnostic, then confirmed at
  10M/100M first and warm;
- NaN, null, offset, chunk, tail, constant-side and alias controls pass;
- results equal the generic path exactly.

Measured acceptance: all six predicates satisfy their approved whole-map
ceilings and at most 25% of published-baseline instructions at 10M and 100M
rows, first and warm, across four layouts. Protected paths retain exact
function vectors apart from reviewed per-call setup constants and absolute
call counts. Their program totals are non-increasing; unknown work is exact
and system-library work follows the approved ELF-object band. Original
failure receipts remain alongside approved rescores. No wall-time speedup
is inferred from these instruction counts.
