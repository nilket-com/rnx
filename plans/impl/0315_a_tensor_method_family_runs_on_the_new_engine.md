# Implementation plan 0315: A tensor method family runs on the new engine

Status: Implemented

Paired ADR: ../adr/0315_a_tensor_method_family_runs_on_the_new_engine.md.

1. Boundary note, read-only. Cover:
   - the file-only unary producer descriptor;
   - receiver-to-output admission and cold dispatch;
   - the file-entry binding;
   - turning `unary!` into the single declaration and removing the names
     from `assoc!`;
   - the adoption charge over the stated domain.
2. Restore the published source guard on all 40 items. Build the core
   change with a synthetic unary control, then run the complete early
   fixed-route screen. On a band failure, stop.
3. Integrate the Candle family. Add the per-member route and value,
   chain, error, accounting, extension, session, inventory and mutation
   controls.
4. fmt, Clippy and preflight. Sign. Then run `final.sh --required` on the
   signed commit. Source review, one complete final, Results.

## Results

One adapter declaration now brings the whole Candle unary family onto
the execution engine. `unary!(...)` generates the 19 methods and a
`register_unary` that calls `Module::file_unary_producer` once per name.
The 19 names left the parallel `assoc!` list.

The engine side is generic and names no Candle method:
- a `file_unary` table keyed by the binding hash, requiring the bound
  handler's own allocation;
- one-slot dispatch that selects method metadata for that table and
  constructor metadata otherwise;
- an equal input and output type accepted only for an authenticated
  unary method;
- zero explicit arguments admitted only in a checked file `main`, and
  refused elsewhere as `OpaqueEntry`.

`Executor::host_call` still hashes to its published `bf36f1ee`.
`execute.rs`, `sliced.rs` and the tools are unchanged.

All 19 unary methods now select execution in checked script files. All
19 previously fell back on the same CSV-to-tensor prefix. The CLI also
ran the three-method chain and read its rank and element count.
The checked adapter tests dropped the input before reading each output
and compared its cells at F32 and F64 with the corresponding Candle
operation. A twentieth, fixture-only method entered
through the same macro. It required no change to the engine.

On non-empty CPU tensors, independently dropping each boxed input and
output released the bytes predicted by the retained charge at both
dtypes. Sessions refused the method before the observable prefix ran.
This family advances neither demo past its next fallback.

The guard matched all 40 published source items. Before Candle was
integrated, the complete fixed-route screen passed all 32 cells in both
the per-record and cumulative bands.

The retained charge is exact over the stated domain: non-empty,
rank-two F32 and F64 CPU tensors, which is everything that reaches the
checked path. In pinned Candle 0.11.0, all 76 non-empty unary outputs
had new storage. All 38 empty outputs aliased the receiver, so admitting
empty tensors or views needs a shared-storage charging rule first.

Eleven planted defects were each caught by their control, in a fresh,
isolated build:
- a swapped backend operation;
- an unenrolled extension;
- a one-byte retained-charge error;
- a missing descriptor and a mismatched descriptor;
- a session entry.

Signed root-tests passed 719. A required final on ae355363 passed all 17
selected stages. The complete final on that commit then passed all 101
stages on its first attempt: 41.8 minutes of accepted run, 42.8 end to
end. Both fixed-route bands held, with no memory.high, memory.max or OOM
events.

Scope: the 19 unary methods in checked file `main`s. The binary family
(`add`, `sub`, `mul`, `div`, `maximum`, `minimum`) needs same-type pairs
with both borrows shared, including `x.add(x)`, and is a later record.
No old-engine output is pinned.
