# ADR 0303: The interpreter loop is isolated from cold helpers

Status: Withdrawn

Base: published d49a8f19e06e58b4ead1fea7e1c0e5ecbaa19baf (ADR 0302, withdrawn).

## Problem

Edits outside the fixed interpreter loop keep changing its machine code.
ADR 0302's piece A added a three-line dispatch to a cold Bytes helper in
`fixed_intrinsic`. The loop source (`invoke_args` and its sliced copy)
was unchanged, and the new helper runs zero times in the fixed-route
matrix. Yet all seven `invoke_args` instances changed size, and the
native-off loop paid about 1.05 million extra mov, jmp, lea and movb
instructions in `loops` at size 5. That was +3.22%, failing both bands.

The ELF pair shows the cold chain is already out of line: each loop
closure makes one static call to the separate `Executor::intrinsic`,
which tail-calls `fixed_intrinsic`. Under the release profile's ThinLTO,
plausible couplings are shared callee decisions, optimization attributes
and cross-module importing. A Rust module split or a separate codegen
unit does not isolate machine code under ThinLTO.

The same coupling stopped 0236 to 0238 and 0301. Retiring the old core
needs more changes of exactly this kind. The user ruled: pause Bytes,
and give loop isolation one bounded record. If the isolation cannot
prove itself, park Bytes and continue old-core work outside the
interpreter.

## Decision

1. Put one compiler boundary on the loop's existing cold Call-miss edge,
   so that a change behind it cannot change the loop's machine code. The
   hot-loop ABI and every opcode's path stay as they are. No new check,
   load or branch runs per opcode.
   - The preferred boundary is a separately compiled object whose IR is
     absent from the final LTO link. It qualifies only if it keeps the
     typed `Executor`, `Store`, `Code` and `Function` contract, the
     error, unwind and drop behavior, the landed targets and the
     unmodified ordinary release profile. The package has no separate
     core Rust-object build today. If one would need a new crate, a
     per-package optimization flag or an unsafe private-layout ABI,
     the boundary is the fallback instead.
   - The fallback is a typed dispatch boundary on that edge, selected
     once if it needs an indirect target. It must be shown that LLVM
     cannot see through it; a constant function pointer can be folded
     back into a direct call.
   - One boundary is chosen at the design review and only it is
     measured. A failed screen does not authorize trying the other.
     The final ELF dispatch is shown in each of the seven
     specializations, with the link evidence for the chosen strategy.
2. The ordinary release recipe and the measurement recipe stay as they
   are. A different optimization flag would need its own approval and
   evidence.
3. A passing record publishes the boundary alone, with no Bytes code.
   Bytes then resumes on it as its own record. If the isolation fails
   its controls, the record is withdrawn and Bytes is parked.

## Constraints

- Semantic behavior unchanged; existing tests pass.
- The boundary's own cost is measured before anything is added behind
  it, and both fixed-route bands hold against published main.
- The loop and its helpers are protected execution code: a boundary
  review before production edits and a source review before the final.
  Any inventoried item the boundary changes needs an exact
  source-condition transition, reviewed before its screen.
- No new crate. Tabs.

## Controls

Under matched paths and the ordinary release recipe:
0. **The identity checker first:** the existing 0298 resolver reports
   an unreviewed RIP-relative LEA in all seven 0302 piece A loop bodies,
   on the baseline side too. Before any isolation performance run, the
   checker establishes canonical identity on a known-identical pair of
   all seven bodies.
   - Resolver work is limited to evidenced relocations and their
     targets: jump-table entries, exact data and location literals
     where needed.
   - Unresolved content still fails.
   - The loop-edit negative control must fail because a supported
     instruction changed, not because the checker could not read it.
   - If no known-identical pair can be established, the record stops
     and brings the tool gap back; it is not counted as a failed
     isolation.
   - Pre-link evidence is supplemental where it needs diagnostic build
     flags. The measured release binaries keep the current recipe.
1. **Isolation alone:** the boundary build against published main passes
   the semantic tests and both bands, and the cold-call cost is
   recorded.
2. **Planted cold edits:** each planted edit, and the held-out piece A,
   compares for the per-record band against the same isolation-only
   candidate. For the cumulative band it compares against the same
   approved cumulative anchor.
   - These are diagnostic comparisons through the retained paired-build,
     capture and scorer route, not publication finals, since the
     ordinary gate takes its baseline from the plan parent.
   - Each receipt is bound to its signed source and ELF pair, and to the
     unchanged toolchain, flags, environment and matched paths.
   - The enrolled cold Call-miss witness keeps its exact output, counts
     the calls in each build and reports the dispatch's instruction
     cost. `awfy_permute` already exercises `fixed_intrinsic`
     native-off and is the starting witness.

   Each planted edit is a cold change that survives optimization and is
   exercised by its own control.
   - Two edits: one shaped like 0302's piece A in the dispatcher, and one
     that adds a caller of a shared helper.
   - All seven `invoke_args` bodies must match after relocation targets
     are resolved, with the cold target's identity stable while its body
     changes.
   - The ELF is retained, and where needed the pre-link object text,
     relocations and jump tables.
   - An unknown instruction form leaves identity unestablished, which
     fails.
   - A negative control that edits a real loop instruction must fail
     identity.
   - Both bands pass.
3. **Held-out test:** replay the actual 0302 piece A on the boundary.
   Both bands, with per-function counts retained. An unchanged
   `invoke_args` does not guarantee unchanged costs in its callees.
4. A pass proves the boundary for these perturbations on this
   toolchain, not for every future edit; the Results say so.
5. One complete final.
