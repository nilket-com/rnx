# ADR 0314: A script runs a model forward on the new engine

Status: Proposed

Base: published 5a33c085c4d1b528f46feaf5814f799add0c01ae (ADR 0313).

## Problem

A checked script file can now load a CSV, turn it into a tensor (0310 to
0312) and load the demo model (0313) on the new engine. The demo's next
line, `let y = model.forward(x)?;`, still sends the file to the old engine
with the refusal `OpaqueReceiver`.

`forward(&Mlp, &Tensor) -> Result<Tensor, String>` (`adapters/candle/src/lib.rs`)
borrows two live natives at once: the model as the receiver and the input
tensor as the argument. It produces a new, independent tensor.

No existing shape admits a method with a second certified native operand:
- 0311 has one shared native input.
- 0310 has one native receiver plus projected column names and dtype.
- 0313 loads from a `String` through the ordinary bridge.

Relaxing `HostSignature::valid` and using the ordinary bridge is not
enough. Its opaque prefix-conversion logic assumes a sole receiver and can
lose a moved second argument.

The old route was measured at 5a33c085:
- The real CSV, the bundled weights, both `Linear` layers with bias and
  ReLU produced 64 scores. Each matched a separate scalar calculation from
  the CSV and safetensors bytes exactly.
- The errors are ordered: dtype, then rank (even for an F64 rank-one
  input), then column count, then the size limits.
- A batch of 65,537 rows with hidden and output widths of 256 fails the
  hidden check first. A batch of 4,097 rows with widths 1 and 4,096
  reaches the output check. Both report the existing 16,777,216-element
  limit.
- Validation and worker or Candle failures are script strings. A moved
  operand and Store capacity are execution failures.

Memory, measured as Rust heap allocations above the live inputs:
- At batch 64 (3/8/1) the peak was 8,248 bytes, and dropping the output
  freed 520.
- At batch 257 (3/16/128) the peak was 280,859, and the drop freed
  131,848.
- At batch 4,097 (3/256/1) the first and repeat peaks were 8,424,795 and
  8,391,635, and the drop freed 16,652.
- Each output freed exactly 4 × O + 264 bytes, with O = batch × output
  width, excluding the Rune box, on these shapes and this build. For
  example, 257 × 128 gives 131,848 bytes.

The tensor-data envelope is 4 × max(2H, H + 2O), with H = batch × hidden
and O = batch × out. It describes overlapping outputs only. It is not a
process memory cap.

## Decision

1. `model.forward(x)` is a checked producer of a uniquely owned `Tensor`,
   in a checked synchronous file `main` only.
   - The actual associated method is registered once before erasure, with
     sealed receiver, argument and success descriptors (`Mlp`, `Tensor`,
     `Tensor`) and its real handler identity.
   - The `Result<_, String>` importer and the existing `Tensor` descriptor
     are kept. The argument and the result share a nominal type, and the
     result has independent storage.
2. The cold shared-producer branch is extended for exactly two shared
   native inputs. `Executor::host_call`, the fixed executor, the sliced
   dispatch source and the interpreter loops are unchanged.
   - `frontend/opaque.rs` and `lower.rs` tell one explicit native argument
     apart from 0310's two column arguments.
   - Admission requires exact matching facts for both inputs, and
     propagates `ResultOfNative(Tensor)` through success.
   - Receiver, argument and result descriptors, Rust `TypeId`s and the
     bound handler are checked.
3. Activation is restricted to the sealed checked-file context and an
   ordinary synchronous `main`.
   - `domain/file.rs` handles this enrollment before its current general
     skip for `producer_outputs`.
   - Sessions are refused even when a model is provided independently.
   - Installation-order and wrong-handler refusals are preserved.
4. The bridge snapshots only the two stock handles, read-checks them left
   to right, and holds two typed shared borrows across the joined call.
   - A moved receiver is argument 0, and a moved tensor is argument 1. An
     earlier access or type failure wins, and neither failure calls the
     worker.
   - An `Mlp` and a `Tensor` cannot be the same object.
     `model.forward(model)` is declined before effects, and a forged or
     mismatched descriptor fails closed.
   - There is no mutable borrow, deep input clone, unchecked downcast or
     persistent host handle.
5. Memory:
   - The adapter's ordered preflight and checked multiplication run before
     any matmul. The 0129 size checks stay.
   - The retained output is charged through 0311's `tensor_bytes`, checked
     against a drop measurement of the actual checked output with its
     native box counted once.
   - Under a finite credit, an operation-level reservation is checked
     before the worker is entered. It uses checked arithmetic over the
     known overlapping tensor payloads plus supported metadata and
     wrappers, counting each live allocation once. Transient backend and
     thread allocations are outside the guarantee. The final retained
     Store accounting stays separate.
   - A checked file keeps the unbounded file allowance. If an embedder
     supplies finite `Limits` to `CheckedMain::prepare`, they govern
     retained Store bytes plus that known reservation, not a transient
     cap, consistent with 0312 and 0313.
   - No guessed physical ceiling is applied to a default file that works
     today.
   - Sessions stay excluded until a transient bound can be defended.
6. Scope: the file prefix through `forward`, ending with the checked reads
   `rank()` and `elem_count()`. No production readback API is added; the
   values are checked by a host-side test of the real checked bridge.
   - In evaluation order, the next missing operation is
     `y.to_dense(names)` (`OpaqueReceiver` on an isolated tensor),
     followed by `with_dense`, sort and typed column reads in later
     records.
   - The full demo nests that call inside `df.with_dense(...)`, and the
     frontend walks the HIR. So the full demo's own diagnostic is recorded
     as observed, and the traversal is not changed to select the inner
     one.
   - No old-engine output is pinned.

## Constraints

- All 40 inventoried items stay at their published source, and the
  published guard runs on all 40 first.
- An early complete fixed-route screen runs on the cold-path and
  registration changes, with a synthetic two-native control, before the
  Candle integration. Stop on either band failure, and never widen a
  band.
- `final.sh --required` passes on the signed candidate before the
  complete final.
- No new crate. Tabs.

## Controls

- A file through `read_csv`, `to_dense`, `from_dense`, `Mlp::load` and
  `forward` selects the execution engine exactly once and prints
  `rank()` 2 and `elem_count()` 64.
- A test-only, host-side score collector on the actual checked return
  matches all 64 scores to the independent scalar calculation. No
  production readback API is added.
- The six measured error cases produce the same script strings on the
  checked route: dtype, rank, columns, the hidden-size limit and the
  output-size limit. A rank-one input comes from a test-only typed
  factory that admits an actual `Tensor`; no production tensor
  constructor is enrolled for it.
- Moved receiver (argument 0) and moved tensor (argument 1) are each
  reported, earliest first, without calling the worker.
  `model.forward(model)` and a forged descriptor are refused before
  effects.
- Both inputs are reused after success; both are dropped after success,
  with the result still readable; and scratch is released when the
  second borrow, the worker or the result adoption fails.
- Finite credit:
  - one byte short of the reservation gives `StoreCapacity` with zero
    worker calls;
  - exact credit on a valid fixture succeeds.
- The output's retained charge equals a drop measurement on the checked
  path, with the native box counted once.
- Sessions refuse `forward` before effects. Installation order and a
  wrong handler are refused.
- The unchanged full `model.rn` falls back before effects: exactly one
  route selection, the marker printed once on the old engine, and its
  actual diagnostic recorded. Separately, the isolated `y.to_dense(names)`
  refusal is pinned, and the isolated forward prefix selects execution.
- The inventory admits exactly the new sites, each with a live witness.
- A mutation per rule fails its control.
- One complete final.
