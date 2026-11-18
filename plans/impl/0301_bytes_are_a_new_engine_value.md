# Implementation plan 0301: Bytes are a new-engine value

Status: Withdrawn

Paired ADR: ../adr/0301_bytes_are_a_new_engine_value.md.

1. Read the store, value and lowering code and the C16g oracle. Report
   the representation choice and every protected hook and executor arm
   for review, and have the user's authorization for the protected run
   edit, before production edits.
2. The type and the store payload alone; the fixed-route band. If it
   fails, stop for a ruling (variant-count-independent tag dispatch
   first, or park again).
   Then byte-string lowering, constants and the receiver certification.
3. The oracle's families: compare, index, methods, mutation, display,
   boundary and budget.
4. The host boundary in both directions, and session state, import and
   export.
5. Promote the oracle to enforced; session parity rows, census and
   inventory; store controls; mutations.
6. fmt, Clippy, the suites, preflight and the commit checker; sign;
   source review; one complete final; Results.

## Results

Withdrawn after the integrated screen failed its bands. Nothing from it
is published. The integrated work is archived outside git
(`target/0301/INTEGRATED-v3.patch`, sha256 2dca0ca9…) for the redesign.

The type and payload step passed alone: an eighteenth outer `Payload`
variant and `Type::Bytes`, with no producer. Both fixed-route bands
passed 32/32; native `shared_rows` rose 0.020% in work. No standalone
`Payload::tag`, `Entry::tag`, `index_tag` or `binary_heap_tag` symbol or
call appeared, and `Store::tag` stayed outlined (529 to 551 bytes). The
mechanism that parked C16g did not recur.

The integrated implementation matched all 109 frozen C16g observations.
To reach measurement it needed a hash-bound source-condition transition
(decision 7) for the three authorized items. The screen then failed:
per-record 14/32 and cumulative 22/32, against limits of 0.5% and 1.0%.
`shared_rows` rose 13.8% (native off) and 11.1% (native on), `awfy_sieve`
3.8%, and `loops`, `fib`, `numeric_calls`, `awfy_permute` and the vector
iterations 0.8 to 2.0%. None of these kernels runs Bytes code. Codex
attributed it, from the retained profiles, to three side effects:
- the integrated build changed `invoke_args`' code generation, adding about 500k mov and 150k jmp instructions in
  `loops` at size 5;
- Bytes integer subscripts replaced the call to `tuple_get` in
  `dynamic_get` with an inlined `index_integer::<true>` (`dynamic_get`
  grew from 4,208 to 5,361 bytes), which moved the sieve and permute
  cost;
- once Bytes had `push`, `rows.push` and `copy.push` on receivers the
  compiler had not proven took the method-lookup path instead of the
  direct Vec push.
No new tag-call boundary was observed.

The user ruled: redesign now, piece by piece, as the next record. Each
piece is screened alone before they are combined: literals with no new
arm in the hot match, Vec push lowering unchanged, and Bytes indexing
that leaves the existing inlining unchanged. Any failure stops for the
user's ruling.
