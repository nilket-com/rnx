# ADR 0226: Packed Boolean XOR and equality

Status: Accepted

Pure-Boolean plans using `^`, `==` or `!=` still run through the generic
scalar executor at about 90 Ir/row flat, one lane at a time. That loop's
register allocation shifted under unrelated reservation-path changes in ADR
0225, which was withdrawn for it. The packed Boolean driver from ADR 0220
already evaluates `&`, `|` and `!` over 64-bit words at 0.74–2.5 Ir/row.

Extend the 0220 driver's eligibility and word kernels to Boolean `^`, `==`
and `!=`:
- XOR is `a ^ b`;
- inequality on Booleans is XOR;
- equality is `!(a ^ b)`, masked to the tail.

Plans that mix these with `&`, `|` and `!` qualify. Plans with numeric
comparisons stay with the predicate driver (ADR 0223), and adding these
operators to its Boolean combination step is out of scope here. Semantics
stay strict-null: the result is null when any input is null, the payload
under invalid rows is zero, and there is no Kleene or short-circuit
evaluation. Fuel, cancellation and observability are unchanged.

Edit only the Boolean driver module and the existing classification line.
The generic executor, the other drivers and the protected kernels gain no
edits and no callers.

Acceptance against published `b177e963` (runtime-equal to `45823dda`):
- exact protected program vectors for every non-qualifying shape, with
  reviewed fixed per-call pins only;
- `booleq`, `boolne` and `boolxor` become driver shapes, with targets set
  from the small diagnostic (expected near the 0220 pair costs);
- the 0220 AND, OR and chain targets are kept;
- controls cover truth tables with differing masks, offsets, chunks, tails,
  aliases, constants on either side, chains mixing all five operators, and
  exact equality with the generic executor.
