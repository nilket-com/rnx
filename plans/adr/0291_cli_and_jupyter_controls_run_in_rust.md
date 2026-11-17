# ADR 0291: CLI and Jupyter controls run in Rust

Status: Proposed

Base: published 427d0e3242b794698957773c421970905f68a6c7 (ADR 0290).

## Problem

Two of the final's stages still run Python stage programs that only
start other programs and check what they print. `production-cli-controls`
runs `tools/column/cli.py`: the release CLI over two fixtures, two
engines and two budgets, each output held to the routing and fuel
contract, the checker first tested on planted evidence. `jupyter-kernel`
runs `tools/column/jupyter_controls.py`: two Cargo builds, then the
pinned external Jupyter client's `tests/test_jupyter_execution.py`
against them, each child under the Python stage protocol.

## Decision

1. Both run as stage modes of the controller's own executable, bound by
   `stage::bound` as the others are: `rnx-final stage cli --binary B
   --out DIR` and `rnx-final stage jupyter --out DIR`. Each writes the
   receipt its Python program wrote.
2. The CLI stage keeps its contract exactly: the eight cases, their
   commands, the stock and execution routing markers, the fuel diagnostic,
   the row and path markers, the binary's digest before and after, and the
   checker's planted controls, every one of which must still be refused.
3. The Jupyter stage keeps its three children, commands, environment
   (`PYTHONDONTWRITEBYTECODE`, the build width), limits and order, and runs
   each through the controller's Rust child runner, so each child keeps
   its own receipt directory and validation. Its build width is the
   existing `stage::build_jobs` rule, shared, not copied: the jobserver's
   width inside a final, the CPU and memory observation outside one.
   `tests/test_jupyter_execution.py` stays Python: it is the contract the
   pinned Jupyter client runs.
4. Its route becomes `final jupyter-kernel`, still a checked execution
   route. A Rust stage mode declares its children as typed commands, the
   same declaration its runner iterates; the route checker reads that
   declaration with inert paths, spawning nothing and observing no host
   budget, and holds the route only when a declared child runs the file.
   For a Rust stage, the stage's own arguments are never a route.
   Preflight's, the measurement suite's and the Python stage scripts'
   route checks are unchanged.
5. Retired: `tools/column/cli.py` and `tools/column/jupyter_controls.py`,
   and `tools/column/test_column_protocol.py`, whose one control loaded
   `cli.py` by path to run its checker's planted evidence, now the Rust
   stage's own control, with tombstones; the route registry, the producer lists, the scheduler's
   resources for both scripts, and the Jupyter README follow.

## Constraints

- The same commands, fixtures, checks, limits and resources; the shared
  budget, isolation and the 60-minute limit are unchanged.
- No new crate, no new Python file; the Python list only shrinks.

## Controls

- Witnesses from the published Python at 427d0e32: the CLI checker's
  verdicts over its planted evidence and the cases' commands and receipt
  over intercepted runs; the Jupyter stage's commands, environment and
  receipt over intercepted children.
- A failed case or child fails the stage with its receipt kept; a
  changed binary is refused.
- Route controls: the test child removed or replaced, the stage's command
  removed or changed, or the path present only in the stage's own
  arguments, each breaks the route; the runner's intercepted children are
  the declaration's, and the witness's.
- One complete Rust final.
