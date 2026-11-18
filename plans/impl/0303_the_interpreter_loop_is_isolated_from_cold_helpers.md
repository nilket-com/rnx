# Implementation plan 0303: The interpreter loop is isolated from cold helpers

Status: Withdrawn

Paired ADR: ../adr/0303_the_interpreter_loop_is_isolated_from_cold_helpers.md.

1. Boundary note, read-only: the fixed Call-miss site, the one chosen
   boundary and its link evidence, any inventoried item touched, and
   the exact measurement pair scopes.
2. The identity checker established on a known-identical seven-body
   pair, with its negative control; stop on a tool gap.
3. The boundary alone; screen it against published main and record the
   cold-call cost. Stop on failure.
4. The two planted cold edits; identity and both bands. Stop on failure.
5. Replay 0302 piece A on the boundary; both bands.
6. fmt, Clippy, the suites, preflight and the commit checker; sign;
   source review; one complete final; Results.

## Results

Withdrawn after the held-out Bytes patch failed loop identity and both
instruction-count bands. Bytes is parked. The boundary and diagnostic
patches are preserved in the local experiment archive.

The boundary alone passed 32/32 per-record and 32/32 cumulative on
`ba55b7f4`. All seven `invoke_args` instances called the same outlined
wrapper, which used a typed indirect call to `fixed_intrinsic`. In the
native-off `awfy_permute` witness the wrapper added four self instructions
per call and `fixed_intrinsic` used four fewer, for a combined 110.

Both independent plants passed both bands and their execution witnesses.
The capacity dispatcher plant (`265cf28d`) matched all seven loop bodies
after 62 source locations were proved against the exact build exports. The
shared `Store::tag` caller plant (`8fec2a07`) matched all seven directly.

The held-out replay (`fc812d38` against isolation-only `aa852d54`) retained
the indirect call and changed all seven loop bodies. Extent changes ranged
from -306 to +1,257 bytes. The resolved code showed changed branch targets
and stack frames. The loop source and the checked dispatch edge matched.

The user authorized a single diagnostic using those preserved ELFs, the
existing scorer, and the retained cumulative anchor. All 64 captures
validated with matching output. The per-record band passed 14/32 and the
cumulative band 22/32. At size 5, native-off work rose 3.22% on `loops`,
2.28% on `numeric_calls`, and 2.12% on `fib`. In each of those three cases
the interpreter loop accounts for the entire increase. The `loops` delta
was +1,050,216 instructions, the same as the unisolated 0302 screen.

Native-off `awfy_permute` retained 93,700 calls at each dispatch edge at
size 5. The wrapper stayed at 47 self instructions per call and
`fixed_intrinsic` dropped from 63 to 60. The loop gained 2,703,086
instructions, a net increase of 2,234,596 instructions of work after
the savings in `fixed_intrinsic` and intrinsic certification.

The unchanged release recipe establishes where the extra instructions
ran. It leaves the precise compiler decision behind the change unresolved.
Raw evidence is retained under `target/0303/`. The signed experiment
commits are in the local archive bundle with SHA-256
`2c9437d175580097399ba3fffde35a8cfb016442bcedb80b79480ae563404150`.
