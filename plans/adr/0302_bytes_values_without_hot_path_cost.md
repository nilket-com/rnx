# ADR 0302: Bytes values without hot-path cost

Status: Withdrawn

Base: published 4447af3351d110d4c2fccd29793da436cef4c92e (ADR 0301, withdrawn).

## Problem

ADR 0301's integrated Bytes failed both fixed-route bands: per-record
14/32 and cumulative 22/32. `shared_rows` rose 13.8%, `awfy_sieve` 3.8%,
and the loop and call kernels 0.8 to 2.0%, though none of them runs
Bytes code. Its type and payload step alone passed both bands, with
`shared_rows` up 0.020%. The profiles located the extra cost in three
places:
- code-generation drift in the integrated fixed loop (the profiles
  located the extra instructions but did not isolate one source edit);
- Bytes integer subscripts changed the inlining of `tuple_get` in
  `dynamic_get`;
- unproved `push` receivers moved from the direct Vec push to the
  method-lookup path.
The user ruled: redesign now, piece by piece, each piece screened alone,
with any failure stopping for the user's ruling. The integrated 0301
work is archived outside git as reference (`INTEGRATED-v3.patch`,
sha256 2dca0ca9…).

## Decision

1. The type and payload step (an eighteenth outer `Payload` variant and
   `Type::Bytes`, a leaf with no children, capacity charged) is the
   base for every piece. It is re-screened on this record's baseline.
2. Three pieces, each built from that base and screened alone against
   the same published baseline and cumulative anchor:
   - **A, literals:** byte-string literals and the session's constant
     byte string reach a private, certified materializer through the
     existing `Op::Call`, with a checked descriptor in the existing
     `Function::args` pool. The `Op` enum, the instruction and
     `Function` layouts, and both the fixed and sliced opcode matches
     stay unchanged. Native code declines this call. Each use
     materializes a fresh leaf. Literal byte work has an operation-local
     cap on top of the ordinary charged call. Per-byte invocation fuel
     is not promised; it would need its own metering design.
   - **B, push:** `Op::Push` stays exactly as today for every receiver
     not proven to be Bytes. A receiver proven to be Bytes takes a
     certified cold call. A program in which Bytes could reach a push
     whose receiver is unproved is refused at compile time, before
     effects, with a located refusal. Every existing Vec site is not
     required to prove Vec. Each local oracle case the proof cannot
     show is pinned as such a refusal, with its stock outcome and the
     core refusal's location, or the proof covers it; this includes
     `observation/capture`, `chain/mutation` and `identity/container`.
     Only frontend modes that already fall back do so. Forced execution
     and sticky sessions return the refusal before effects.
   - **C, indexing:** `tuple_get` and its direct call from `dynamic_get`
     stay unchanged. An integer subscript on Bytes enters a cold helper
     from the existing error path; ranges get a Bytes arm in the
     existing range branch; writes get a cold arm beside the Vec body.
     The evaluation order of `Get` and `Set` is unchanged.
3. If all three pass alone, the combination is screened, since the
   inliner sees them together. The combined candidate carries the rest
   of the core surface the local oracle needs (comparisons, formatting,
   constructors and methods besides push, tombstones, entry refusals)
   and the ingress enforcement, so nothing is added after its
   performance check.
4. Scope of Bytes ingress in this record: Bytes values come only from
   this program's own literals and constants and from operations on
   them. Bytes arriving from outside is refused with a typed refusal
   before effects:
   - host outputs stay rejected;
   - an embedding caller's returned Bytes root, directly or nested with
     the existing root constructors, fed into another invocation is
     refused at the entry boundary;
   - sessions refuse Bytes producers outright in this record:
     `ProgramDomain` registration (`compile_insert` and
     `compile_session`) refuses at the first producer with the existing
     located `ByteStr` refusal, before the lifecycle starts. So the
     session parity classes are unchanged, and a host effect earlier in
     a script cannot run before the refusal. `ProgramDomain::start`
     refuses direct and nested Bytes argument roots before charges or
     effects, and publication fails closed for an injected Bytes delta.
     Admitting Bytes in sessions, which needs Bytes in session state
     predicted before effects, is the next record's work.
   The boundary review names each route and bounds any graph work it
   adds. The byte-free route keeps its existing behavior, and any new
   admission cost is measured in the screen that adds it. The host
   boundary (`web::response`), session state and the session parity
   rows follow in the next record. So do the four byte host functions,
   which stay their own cuts.
5. A cut that changes an inventoried protected item uses an exact
   source-condition transition: fresh, bound to this record's baseline
   and to that item's canonical hashes. The fixed-route bands are never
   relaxed.

## Constraints

- No change to existing types' behavior. Each screen keeps both bands
  and their limits.
- Each screen keeps ELF evidence for the functions the 0301 attribution
  named (`invoke_args`, `dynamic_get`, `tuple_get`, `push_call`,
  `Store::tag`) and the profiled self-Ir for `loops`, `fib`,
  `awfy_sieve`, `awfy_permute` and `shared_rows`.
- The protected store, value and run paths get a boundary review before
  edits and a source review before the final. The user's 0301
  authorization of the protected run edit carries to this redesign, on
  the same stop-on-failure terms.
- No new crate. Tabs.

## Controls

- The C16g oracle enforced, through the 0301 projection, for the cases
  inside decision 4's scope. Cases needing host or session ingress stay
  observed, with their reason recorded.
- A: malformed descriptor, exhausted store, independent repeated
  constants, the byte-work cap, and native declining the call.
- B: the lowered code for the existing `shared_rows` and cloned-Vec
  push sites is unchanged; a proven Bytes push; a custom-provider
  refusal; the same unknown-Vec call shape admitted without a possible
  Bytes source and refused with one; the three named oracle cases.
- Ingress: direct and nested returned-root re-entry and retained
  session state refused before effects, through the actual fixed and
  sliced and domain entry APIs, including a case where a host effect
  would otherwise run first.
- C: integer, range, write, alias, invalid key and moved-value controls,
  a Bytes function parameter, the direct `tuple_get` symbol and call
  check, and cold diagnostic recovery for non-Bytes errors.
- Each piece's screen, then the combined screen: both bands and the ELF
  evidence above. Any failure stops for the user's ruling.
- A mutation per rule fails its control.
- One complete final.
