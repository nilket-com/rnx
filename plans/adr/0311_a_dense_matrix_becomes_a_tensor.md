# ADR 0311: A dense matrix becomes a tensor

Status: Proposed

Base: published 53de671215e30845ca6915c9ce00ad32adcbca9e (ADR 0310).

## Problem

ADR 0310 lets an execution session turn a retained CSV frame into an
`interchange::Dense` with `df.to_dense(names, dtype)?`. The next step of
a model workflow hands that matrix to Candle with
`candle::Tensor::from_dense(dense)?`, then reads `rank()` and
`elem_count()`. All three still take the old route.

What `from_dense` does (`adapters/candle/src/lib.rs`):
- It borrows `&Dense`.
- It clones the matrix's `Vec<f32>` or `Vec<f64>` once. Pinned
  candle-core 0.11.0 moves that `Vec` into CPU storage.
- It checks the element count again.
- It returns `Result<Tensor, String>`.

`Dense::new` already guarantees `rows × columns == len`, finite values,
at most 2^24 cells, and f32 or f64 only. The copy's size is therefore
exact: rows × columns × 4 or 8 bytes, at most 64 or 128 MiB.

`Tensor` is registered with `ty` plus a standalone `DISPLAY_FMT`
handler, and `rank` and `elem_count` sit in the bulk associated-function
list.

Two parts of the cold proof do not fit this call:
- It admits a shared native only as a method receiver, not as the
  argument of an associated function.
- `opaque_result_function` (0306) is a free-name, arity-two shape over
  an owned tree.

## Decision

1. The qualified call `candle::Tensor::from_dense(dense)`, with exactly
   one direct `&Dense` argument, is a checked producer of a uniquely
   owned `Tensor`.
   - The session alternate is bound to the hash a qualified call
     actually resolves to. The original handler is verified at
     registration and the active alternate at dispatch. Both stock
     lookups stay intact.
   - Method-call syntax, function values, indirect calls and
     non-session execution are refused, and the receiver-call form is
     not enrolled by accident.
   - It is sealed by registration-time metadata: the input `Dense` and
     the distinct output `Tensor`, each verified by installed-context
     hash and Rust `TypeId`. The two types come from different modules.
   - The cold proof admits this one associated producer with
     `Fact::Exact(Dense)` as its sole argument, and a direct `?` that
     yields exactly `Tensor`.
   - It reuses 0310's dormant rule, with three distinct cases:
     - Candle installed without the external `Dense` keeps the stock
       symbol, with the alternate dormant and refused at admission.
     - A missing stock `Tensor` container is the existing install-time
       `MissingContainer` error.
     - A context without Candle has no such symbol.
     A conflicting hash and `TypeId` pair fails in either install order.
   - A `Dense` argument that is unreadable but still statically typed
     reports the existing moved-argument access error at execution,
     before the typed callback or any copy.
   - The legacy route keeps the same symbol.
2. It runs on 0310's shared-producer carrier in
   `Executor::budgeted_sliced_intrinsic`. No inventoried item changes,
   so `Executor::host_call` stays at `bf36f1ee`.
   - The carrier gains this shape: one shared native argument, no
     projected inputs.
   - It authenticates the handler, the type pair and the slot.
   - It retains the `Dense` with one reference-count increment, never a
     buffer copy.
   - The cold materializer re-reads the allowance and charges the copy
     before it happens.
3. The charge covers rows × columns × element size (checked), plus the
   metadata pinned candle-core 0.11.0 allocates in `from_storage`: the
   shared storage lock, the tensor object, the shape and the contiguous
   strides, with allocation headers where relevant. It also covers the
   Store and `Result` wrappers. The live packet and wrapper allowances
   are subtracted once. The transient peak is documented separately from
   the retained Store payload. Arithmetic
   is checked. The copy is charged once, Store admission and release
   stay authoritative, and a miss is `StoreCapacity` with the `Dense`
   unchanged. The exact-fit claim is scoped to this fresh contiguous CPU
   allocation; a view could retain more backing storage than its logical
   element count. Registering `Tensor`'s display makes no other `Tensor`
   producer eligible.
4. `Tensor` moves to one `opaque_type` registration with the bounded
   Candle display, replacing `ty` plus the standalone `DISPLAY_FMT`.
   `rank()` and `elem_count()` become checked `opaque_method(&Tensor)
   -> i64` reads, out of the bulk list. Every other Candle operation
   stays on the old route.
5. Scope: the `read_csv` → `to_dense` → `from_dense` session segment,
   then `rank()` and `elem_count()`. `demos/candle/model.rn` is a file,
   and its later steps (`Mlp` and others) stay on the old route; nothing
   more is claimed. No old-engine output is pinned.

## Constraints

- All 40 inventoried items stay at their published source, and the
  published guard runs on all 40 first.
- An early complete fixed-route screen runs on the carrier change
  before Candle integration. Stop on either band failure, and never
  widen a band.
- rnx root-tests must pass on the signed commit before the complete
  final, because frontend proof changes are involved.
- No interpreter, opcode or Store layout change. No new crate. Tabs.

## Controls

- Real execution REPL through the combined adapter entry point in
  `demos/candle/main.rs`, with both install orders:
  - `read_csv`, `to_dense(names, "f32")?` and `Tensor::from_dense(d)?`
    in separate inputs;
  - `t.rank()` is 2 and `t.elem_count()` is rows × columns;
  - tensor values checked on the host side against the `Dense`;
  - f64 too;
  - reset;
  - the tensor's bounded display.
- Credit:
  - at the documented charge minus one byte, `StoreCapacity` with zero
    payload copies, and the same `Dense` still readable and retryable;
  - exactly at the charge, admitted with correct contents, and the
    `Dense` stays charged independently;
  - the output charge returns once the last `Tensor` owner goes away.
  - Isolated f32 and f64 cases at different sizes show the payload
    growing by exactly one output buffer.
  - Ordinary conversion errors stay a script `Err`; exhaustion is
    `StoreCapacity`.
- Refusals, with call counts:
  - method syntax, an indirect call;
  - a non-`Dense` argument, a moved `Dense`, a wrong arity;
  - a function value, a static or a direct non-session ingress;
  - a missing or conflicting `Dense` or `Tensor` registration.
  Each is refused before any call or copy.
- `width`, `height` and `to_dense` still pass, and file routing is
  unchanged.
- The inventory admits exactly the new sites, each with a live witness.
- A mutation per rule fails its control.
- One complete final.
