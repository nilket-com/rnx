# Implementation plan 0314: A script runs a model forward on the new engine

Status: Implemented

Paired ADR: ../adr/0314_a_script_runs_a_model_forward_on_the_new_engine.md.

1. Boundary note, read-only. Cover:
   - the sealed registration with receiver, argument and success
     descriptors;
   - the two-native case in the cold shared-producer branch;
   - the frontend and lowering distinction from 0310's column arguments;
   - the file enrollment ahead of the `producer_outputs` skip;
   - the left-to-right borrows and the error precedence;
   - the retained charge.
2. Restore the published source guard on all 40 items. Build the cold
   branch and registration with a synthetic two-native control, then run
   the complete early fixed-route screen. On a band failure, stop.
3. Integrate Candle. Add the file, value, error-order, moved-operand,
   lifetime, accounting, session, `model.rn`, inventory and mutation
   controls.
4. fmt, Clippy and preflight. Sign. Then run `final.sh --required` on the
   signed commit. Source review, one complete final, Results.

## Results

`model.forward(x)?` runs on the execution engine in a checked script
file's `main`. It is registered once, before erasure, with sealed `Mlp`,
`Tensor` and `Tensor` descriptors and its real handler. It goes through
the cold shared-producer branch, extended for exactly two shared native
inputs. The branch's request is now one enum: receiver only, receiver
with 0310's projected columns, or receiver with one native argument.

The receiver must be readable before the argument is read. A moved
argument is reported as argument 1, and a wrong type as
`HostExpectedType`. Neither calls the worker. Both inputs travel as
stock handles, never deep copies. A conflicting second borrow was held
in a real control: the worker did not run, the first borrow was
released, and both operands were acquired again successfully.

`Executor::host_call` still hashes to its published `bf36f1ee`.
`execute.rs`, `sliced.rs` and the tools are unchanged. The Rune edits
made after the early screen are test-only.

The checked synchronous file entry now runs the model demo through
`model.forward(x)?`. The real file prefix printed rank 2 and element count
64. A test-only reader obtained all 64 output scores on this route. Each
matched the scalar calculation from the feature CSV and bundled weights.

The full demo still selected the old engine at admission with
`OpaqueReceiver`. It printed its marker once. On an exact tensor,
`y.to_dense(["score"])` independently produced the same refusal.
A session with independently supplied native inputs refused the method
before effects and continued to use its inputs.

The fixed-route screen ran on the sealed registration and synthetic
producer before the Candle integration. All 32 workload and mode cells
passed both bands. The 40 protected source items passed their guard.
The finite-credit check covers the overlapping tensor buffers and their
metadata; backend and worker-thread scratch are outside that estimate.

Under a finite credit, the reservation is checked before the worker is
entered. One byte short gives `StoreCapacity` with zero worker calls.
Exact credit succeeds. The retained output is charged through 0311's
`tensor_bytes`, reconciled against a drop measurement on the checked
path with the native box counted once.

Twelve planted rule violations (six core, one session, five adapter)
were each caught, with their unmodified controls passing. Signed
root-tests passed 719 to 0. A required final on 761dd538 passed all 17
selected stages.

The first complete final on that commit failed one controller test.
In `tools/final/tests/scheduling.rs`,
`capacity_returned_while_an_event_is_outstanding_is_found_by_its_recheck`
hit its fake's 20-second readiness wait, "the waiter started", while
`final-controls` ran beside heavy stages. 0314 does not change
`tools/`, and that failure is the only one of this test in the 20
recorded complete finals. It is logged for a separate fix.

The rerun on the unchanged commit passed all 101 stages: 34.2 minutes
of accepted run, 34.7 end to end. Both fixed-route bands held, with no
memory.high, memory.max or OOM events.

Scope: the file prefix through `forward`. Sessions refuse it. The full
`model.rn` still falls back before effects. Its next steps,
`y.to_dense(names)`, `with_dense`, sort and typed column reads, are
later records. No old-engine output is pinned.
