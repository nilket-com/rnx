# Implementation plan 0299: An async main runs on the new engine

Status: Proposed

Paired ADR: ../adr/0299_an_async_main_runs_on_the_new_engine.md.

1. Record the old engine's outputs for every witness first: value,
   located error, budget, interruption and arity.
2. Report the protected-source hooks for review before code changes: the
   file-entry compile policy, how a file caller learns the entry is
   async, and the sliced waiting invocation's completion and error
   conversion.
3. Admit a declared `pub async fn main` with direct awaits of enrolled
   waiting host functions, under the file-entry policy only; every other
   async form stays refused at compile time.
4. Run the admitted entry through `start_sliced` under
   `with_host_wait`; completion precedence, budget, interruption, located
   errors and exit codes as decided.
5. The witnesses, refusal and no-retry controls, census 0212 and its
   tables, mutations.
6. fmt, Clippy, the suites, the census, preflight and the commit
   checker; sign; source review of the protected boundary; one complete
   final; Results.
