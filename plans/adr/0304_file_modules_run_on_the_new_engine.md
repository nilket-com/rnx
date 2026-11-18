# ADR 0304: File modules run on the new engine

Status: Proposed

Base: published 6cd3cfb5e53db4b571ba8d1bc8c4264ed0942c3e (ADR 0303, withdrawn).

## Problem

A file that declares a module (`mod name;`) never runs on the new engine.
In Auto mode, `src/runner.rs` checks `crate::engine::requires_loader` on
the entry text and selects the old engine with refusal `SourceLoader`
before trying execution. The new engine's frontend compiles with a
`NoopSourceLoader` (`experimental_core/frontend.rs`) and snapshots its
source records before the compiler discovers modules, so it cannot load
a module file. The old route already has the bounded file and
mapped-project `Loader` (`src/program.rs`).

This is milestone B1 of the old-core removal inventory: repository
workloads that need no fallback. Bytes (A1) is parked after ADRs 0301 to
0303 showed that its integration changes the interpreter loop's code
generation. B1 is outside the interpreter.

## Decision

1. The new engine's file compilation uses the existing bounded `Loader`,
   so a file program with module declarations compiles and runs on the
   new engine. Path resolution, read bounds and error wording are those
   of the old route.
2. Source identity is bound at insertion. The compiler inserts a loaded
   `Source` only after its duplicate-module check, so a duplicate can
   load bytes without getting an ID. The loader keeps the read bytes and
   path when it resolves, and the program's source records come from the
   admitted `Source`s and their assigned IDs. Located errors and
   debug-source output therefore name the right module file. The
   frontend's count, size and name limits apply to discovered modules as
   well as entry inputs, and the loader keeps its separate 8 MiB
   allowance.
3. One Auto attempt plus its fallback is distinct from distinct logical
   imports:
   - Two aliases of the same physical module are still charged
     separately, as the mapped-loader test pins.
   - A logical load already attempted is replayed from the same resolved
     outcome, lookup and read failures included, with no second
     resolution, open or charge. A previously unseen import does its
     first read under the carried allowance.
   - Identity is the loader request and its route, independent of either
     compiler's numeric `SourceId` or visitation order.
   - A fallback starts from the original entry `Source`s and replays
     imports through the loader as module requests. It never feeds
     completed module `Source`s to the old engine, which would present
     module bodies as top-level inputs.
   - Refusal and fallback happen before effects. The existing custom
     macro pre-effect policy holds. Debug output reflects the
     compilation that runs, without repeating the first attempt's
     output.
4. Modules on the new route support what the new engine already
   supports in a single file. A module form the new engine refuses keeps
   its typed compile-time refusal and its fallback.
5. Sessions, `rnx serve` and embedding keep their current module
   behavior; this record covers `rnx run` and the census.

## Constraints

- Changes stay in file routing, the loader and the shared compiler
  frontend, on the existing program, runner, invocation, context
  provenance and execution semantics. If the boundary review finds a
  protected executor or store change, the record splits or stops there.
- The fixed-route band holds, since compiler layout can move with source
  edits outside the loop.
- No new crate. Tabs.

## Controls

- The existing nested and mapped module fixtures run on both engines.
  Output, errors and their locations, path choice, read bounds and exit
  codes match, with `engine=execution` reported for the new route.
- Exactly-once effects: a module that prints or calls a counting host
  runs it once, including when a later refusal falls back.
- Path and error behavior as it really is today. The loader checks
  declared map inputs and `mod.rn` precedence; explicitly mounted
  dependencies may live outside the entry directory, and there is no
  general project-directory check. So:
  - a concrete currently rejected map or path case stays rejected, with
    its diagnostic;
  - for missing, read and allowance failures, the receipt states which
    route renders the existing diagnostic, and it points at the
    original module declaration's file and span;
  - the existing rule that a runtime error is terminal holds.
- A late refusal after a loaded file is changed or removed: the fallback
  uses the replayed result, with the open count and byte charge
  unchanged.
- A located runtime error inside a module names that module's file,
  line and column.
- A refused module form falls back before effects with its typed
  refusal.
- The census keeps its pinned single-source witness (244 cases,
  compile-only). Explicitly pinned module-tree cases are added, with
  entry and dependency bytes bound before loading and loads confined to
  the declared fixture tree. Engine selection is reported from real CLI
  witnesses or the routing inventory, with its own denominator; the
  census alone cannot measure a CLI fallback.
- A mutation per rule fails its control.
- One complete final.
