# ADR 0339: A match that can miss proves no receiver

Status: Proposed

Base: published 9f9930882e432a338dccdf3b9c40d272b8bb97b5 (ADR 0338).

## Problem

The new engine runs a script method on a value that isn't of the
method's type. In

    let x = match v { 1 => S {} };
    x.m()

the match has no arm for other values of `v`. On a miss its value
is unit. The compiler reads the arms alone, proves that `x` is an
`S`, and emits a direct call of `S::m` with no check of the
receiver. On a miss the body of `S::m` runs with unit as `self`.
The old core reports that unit has no such method.

Two proofs make the same mistake independently. The lowering in
`lower/static_methods.rs` deduces a nominal type from the arm
bodies. The proof graph in `frontend/receiver_proofs.rs` reaches a
call through a local, a parameter or a script function's return.
It already marks a match that can miss, but it loses the unit in
two places. Its solver weakens the proof for `Result` alone. And
the join of a possible unit with a proved `S` gives `S`.

Both were measured on the published 509fb093 with native code off.
A local `x.m()` printed 1 on a miss. A script-defined `capacity()`
called on a function parameter printed 1 on a miss. The old core
reported the missing method both times, and both hits printed 1 on
both engines. An ordinary `x.m()` on a parameter already took
dynamic dispatch and gave the right error.

ADR 0337 treated a possible miss as unknown for `Result` receivers
only and left the nominal rule as it was.

## Decision

1. A direct call of a script method is emitted only where every
   path that reaches the call yields a receiver of the same proved
   type. A match that can miss yields unit on that path. A path
   that leaves the function yields no receiver and isn't counted.
   The two are different and stay different in both proofs.
2. One predicate says whether a match covers its input, and both
   proofs use it. In this record a match covers its input when it
   has an unguarded wildcard arm or an unguarded arm that is a
   plain binding. A guarded catch-all doesn't count. Literal and
   destructuring arms alone don't count, whatever they list. The
   `Result` rule of ADR 0337 uses the same predicate.
3. The lowering asks the predicate before it deduces a nominal type
   from arm bodies. Without the proof the call goes through the
   ordinary method resolution, as for any receiver of unknown type.
4. The proof graph adds unit to the join of a match that can miss,
   for every nominal receiver. The possible unit survives joins
   through locals, parameters, script returns and nested matches.
   It is there when every explicit arm leaves the function, since
   then the miss is the only value. The existing obligations for
   `Result`, fields and recursion stay.
   A reachable miss may be treated as an unknown receiver. That is
   enough. A match that covers its input and whose arms all leave
   the function still yields no value. The lowering gains no
   general analysis of which paths leave the function.
5. Where the exact proof is lost, the call takes one of two routes,
   fixed at compile time. A call on a receiver still proved an `S`
   keeps its direct call.
   - Where certified dynamic dispatch is available for the name,
     the call uses it. On a miss it reports a missing method at the
     call, at run time, and the method body doesn't run. On a hit
     it calls the script body once.
   - Where it isn't, the call keeps the existing compile-time
     refusal, with the call's position, before any effect of the
     program. It refuses for every input, since the proof is made
     before the input is known. Through the public file entry the
     program then runs once on the legacy route.
6. The old core's measured outcome is the reference for what a
   program prints and reports, on a hit and on a miss. The route a
   call took is recorded apart from that output. No new test pins
   how the old core is built.
7. So a match that can miss but hits in practice loses its direct
   call too. Where dynamic dispatch isn't available it now runs on
   the legacy route. That loss is accepted here and is counted,
   source by source. Return to review before signing if it removes
   working behavior from a public example, demo or adapter test.
8. The same invariant holds for every other expression whose value
   can be the implicit unit on some path: an `if` without `else`,
   a labeled block left by a bare `break`, the value of a `loop`,
   `while` or `for`, and a block with no tail expression. These
   are believed conservative today. The boundary step probes a
   representative of each. One that proves a receiver wrongly is
   repaired here when the repair is this same rule. Anything larger
   returns to review.
9. Scope: the new engine's front end and lowering. No change to the
   legacy engine. No analysis of whether literal or variant arms
   are exhaustive. No new opcode, and the bytecode format, argument
   passing and host ABI don't change. Record the corpus, the command
   and the count of legacy-only cases on the base and on the
   candidate. The base count is 83. A changed count is a finding to
   explain.

## Controls

1. Hit and miss pairs for each proof: the local form through the
   lowering, and the parameter and script-return forms through the
   graph. Each runs with native code on and off, with a counted
   side effect in the method body, and asserts the route it took.
   With dynamic dispatch: on a miss the body runs zero times and
   the missing-method error is reported at the call; on a hit the
   body runs once. Without it: when only the new engine may run,
   both inputs refuse at compile time at the call's position with
   no effect; through the public file entry the program runs once
   and its output equals the old core's on the hit and on the miss.
2. A match with an unguarded wildcard and one with an unguarded
   binding keep the direct call, asserted by route.
3. A guarded catch-all that misses. A nested match whose inner
   match can miss. Each through a local, a parameter and a script
   return. And a helper function whose last statement is a match
   that can miss, ended by a semicolon, whose one arm returns an
   `S`: the hit returns `S` and the miss returns the implicit unit,
   so the caller's receiver isn't proved. Beside it, the same
   helper with a covering match keeps its proof.
4. A call refused for want of a proof names the call's position,
   on a hit input and on a miss input alike.
5. The `Result` controls of ADR 0337 are unchanged.
6. Each old proof, planted back alone, breaks its own witness while
   the other repair stays in place.
7. The retained nominal, field and self proofs and the public
   admission corpus are replayed against the published base. Every
   source that changes is listed with its old and new value or
   error, its route and its position. Corrected output is listed
   apart from a hit that was admitted and now falls back.
8. A representative of each form of Decision 8, with its measured
   outcome and route on both engines.
9. The file gate and both project screens, image to image against
   the published base.
10. Before signing, the whole Rune library test binary in separate
    build state. Then one complete final on the signed candidate,
    under the limits of ADR 0338.

## Constraints

- No new third-party crate. Tabs.
- No refusal at run time.
- A program whose receivers are proved today by a covering match,
  a constructor or a field keeps its results, effects and route.
