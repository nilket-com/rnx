# Implementation plan 0302: Bytes values without hot-path cost

Status: Withdrawn

Paired ADR: ../adr/0302_bytes_values_without_hot_path_cost.md.

1. Boundary review of the three pieces and any inventoried item they
   touch, before production edits.
2. The type and payload base, re-screened on this baseline.
3. Pieces A, B and C, each built on the base and screened alone; stop on
   any failure.
4. The combination, screened.
5. The oracle within scope, the piece controls and mutations.
6. fmt, Clippy, the suites, preflight and the commit checker; sign;
   source review; one complete final; Results.

## Results

Withdrawn after piece A failed its bands. Nothing from it is published.
Candidates and receipts are retained under `target/0302/` in the
development checkout.

The type and payload base passed again on this baseline (d3b8be4d): both
bands 32/32, native `shared_rows` work +0.020%. `tuple_get` kept its
direct call from `dynamic_get` at 788 bytes. No standalone tag helper
appeared.

Piece A (49217c45) changed one inventoried item, `fixed_intrinsic`,
under an approved hash-bound transition. That item gained a three-line
dispatch to a cold literal materializer. Its controls passed. The
ordinary screen failed: per-record 14/32 and cumulative 24/32, with
every native-off kernel failing. `loops` rose 3.22%, `numeric_calls`
2.28%, `fib` 2.12%, `awfy_sieve` 2.13% and `awfy_permute` 0.64%; native
`shared_rows` rose 0.42%, with growth at 0.96%.

The unchanged-source `invoke_args` loop accounts for all of the
increase in `loops`, `fib` and `numeric_calls`.
- The native-off closure grew from 17,335 to 17,429 bytes, and all
  seven instances changed size.
- In `loops` the extra 1,050,216 instructions are mov, jmp, lea and movb
  on byte-free paths.
- The materializer runs zero times in the matrix, and the three largest
  increases do not execute `fixed_intrinsic` at all.

The ELF pair shows the cold chain already outlined: the loop makes one
call to `Executor::intrinsic`, which tail-calls `fixed_intrinsic`, which
calls `literal_bytes_call`. So the coupling is not the new path being
inlined. Under the release profile's ThinLTO it is plausibly a shared
callee decision, an optimization attribute or cross-module importing.
These builds do not isolate which.

The same coupling stopped 0236 to 0238 and 0301. On Codex's
recommendation, the user ruled: pause Bytes, and isolate the
interpreter loop from cold-helper changes as its own bounded record.
If that isolation cannot prove itself, Bytes is parked and old-core work
continues outside the interpreter.
