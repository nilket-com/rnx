# ADR 0315: A tensor method family runs on the new engine

Status: Proposed

Base: published 7507d17cd097bb30d694cf41a7e88e910ab4061b (fix after ADR 0314).

## Problem

Records 0305 to 0314 brought 12 native calls onto the new engine, one at
a time. Each needed its own registration, proof and hand-derived memory
charge. The interpreter is general, but this boundary has grown one
function per record. The question for this record is whether one
adapter declaration can bring a whole family onto the new engine, so
that the next member needs no compiler or VM change.

Candle defines 19 unary tensor methods in one macro,
`unary!` in `adapters/candle/src/tensor_ops.rs`:
`exp`, `log`, `sqrt`, `sqr`, `abs`, `neg`, `recip`, `tanh`, `relu`,
`gelu`, `gelu_erf`, `silu`, `sin`, `cos`, `floor`, `ceil`, `round`,
`sign`, `erf`. Each one:
- borrows one tensor;
- applies `float_only`;
- runs the same `wrap` worker;
- returns `Result<Tensor, String>`.

A read-only audit at 7507d17c found:
- All 19 direct calls `x.OP()?` in a checked file fall back today, with
  the refusal `OpaqueReceiver`. The probe used a 64 × 3 F32 tensor from
  the admitted CSV, `Dense` and `Tensor` prefix.
- In pinned candle-core 0.11.0, every member goes through `unary_op!`.
  For a non-empty CPU tensor, the result is computed into new output
  storage. For a zero-length tensor, the receiver is returned as is.
- Backing addresses were probed for each member: F32 and F64, full, a
  contiguous row-range slice with an offset, and empty. For the 19 unary
  members, all 76 non-empty outputs had distinct backing storage, and all
  38 empty outputs reused the receiver's tensor and its backing.
- Tensors that reach the new engine today are non-empty, rank-two and
  float. `Dense` rejects empty dimensions, and `forward` keeps a positive
  batch.
- Neither the model demo's next line nor the sales demo's uses this
  family.

## Decision

1. The existing `unary!(...)` list is the single declaration. It
   generates both the 19 methods and their enrollment.
   - It registers each name through one typed, file-only unary producer
     hook: `&Tensor -> Result<Tensor, String>`.
   - The names leave the parallel `assoc!` list, so nothing is
     registered twice.
   - Each method keeps its real typed handler, its `float_only` check,
     the joined worker and its `Result` diagnostics.
   - Adding an equivalent member is one name in the list plus adapter
     tests. The compiler and VM contain no Candle method names.
2. One-time core work adds a typed `Module` registration for a
   synchronous associated method that borrows `T` and returns
   `Result<T, String>`. It has a checked file alternate and an inactive
   session alternate, and keeps the registered original's identity.
   - It reuses the existing one-`OpaqueShared`-to-`OpaqueResultString`
     signature and the private `ProducerReceiver::produce`.
   - An explicit unary-method classification sits in the opaque
     registration metadata, tied to the method key, the receiver and
     output hash, and the Rust `TypeId`. It is cloned, merged and
     validated with the other method registrations. Inconsistent or
     overlapping producer classifications are rejected.
   - Arity alone cannot select this route, because 0311's `from_dense` is
     an arity-one qualified constructor. The cold `sliced_shared_producer`
     entry selects method metadata for this descriptor and constructor
     metadata for the other one-slot shape. An equal input and output
     type is permitted only for this authenticated unary case.
   - `frontend/opaque.rs` admits zero explicit arguments on an exact
     receiver, only in synchronous checked files. The producer result
     projections and the qualified-call and function-value refusals are
     kept. `lower.rs` already emits the receiver-only `Op::Call`.
   - `domain/file.rs` authenticates the original, the alternate, the
     context and the registered type before replacement. The path
     constructor's extra witness requirement stays scoped to its own
     shape.
   - The host-call opcode, the execution loops, authentication and the
     `Result` adoption path are unchanged. `Executor::host_call`,
     `execute.rs` and `sliced.rs` stay at their published source.
3. Memory follows the file reader's precedent: the retained output is
   charged when it is adopted on success, through 0311's `tensor_bytes`.
   - This is retained Store accounting. It does not predict temporary
     allocations or the OS allocation.
   - Error propagation, wrapper reservation and checked arithmetic are
     kept.
   - The domain is stated explicitly: non-empty, rank-two F32 and F64 CPU
     tensors, which is everything that reaches the checked path today.
     Over that domain every member allocates new storage, so the family
     closes with no exclusions.
   - Admitting empty tensors or views as general input first needs a
     rule for charging shared storage while both handles live and after
     either drops. That is a later record.
4. Sessions decline the family before invocation, as for 0313 and 0314.
   They need their separate bounded-temporary contract.
5. Scope: the 19 unary methods in checked file `main`s. The binary family
   (`add`, `sub`, `mul`, `div`, `maximum`, `minimum`) is a later record.
   It needs same-type pairs with both borrows shared, including
   `x.add(x)`. No demo line is claimed. No old-engine output is pinned.

## Constraints

- All 40 inventoried items stay at their published source, and the
  published guard runs on all 40 first.
- An early complete fixed-route screen runs on the core change, with a
  synthetic unary control, before the Candle integration. Stop on either
  band failure, and never widen a band.
- `final.sh --required` passes on the signed candidate before the
  complete final.
- No new crate. Tabs.

## Controls

- Every one of the 19 calls `x.OP()?` selects the execution engine in a
  checked file, against the audit's 19 measured fallbacks.
- Values match the existing numeric oracles for each member, F32 and
  F64.
- An in-family chain, `x.abs()?.sqrt()?.tanh()?`, runs end to end on the
  execution engine.
- All 19 members' values are compared at both dtypes on the checked path.
  Where the adapter tests have no per-member reference, the matching
  pinned Candle operation is called directly on the same values. The
  input is chosen so that an accidentally swapped operation is caught.
- An integer tensor cannot reach the checked file route. So `float_only`
  is exercised at the typed adapter boundary, and a script `Err` and a
  moved receiver are exercised through an authenticated synthetic
  checked-file method with a call counter. A moved receiver is an access
  error with no worker call. The real CLI cases stay within the
  published file domain.
- The retained bytes are checked by dropping each independently owned
  input and output, the output's Rune box counted exactly once, at both
  dtypes.
- A second member is enrolled through the actual production macro and
  registrar, with no engine edit. Two file-only methods of the same shape
  coexist.
- Sessions refuse a member before effects, with an observable prefix.
  A forged or missing marker, a wrong handler, a swapped or missing
  registration, and the same name on a foreign receiver each fail before
  effects. A unary member and an existing constructor work together, and
  the qualified constructor is unchanged.
- The inventory admits exactly the new sites, each with a live witness.
- A mutation per rule fails its control.
- One complete final.
