# Implementation plan 0304: File modules run on the new engine

Status: Implemented

Paired ADR: ../adr/0304_file_modules_run_on_the_new_engine.md.

1. Boundary note, read-only: a file-only frontend compile entry taking
   the existing bounded loader; source identity at insertion; the
   logical-load replay for fallback; the census module-tree cases;
   confirmation that no opcode, invocation, store or protected
   inventory item changes. Then the early fixed-route screen.
2. Record the old engine's outputs for every module fixture first.
3. The loader in new-engine file compilation; late-loaded source
   records; runner routing.
4. Fallback reuse of the read snapshot; exactly-once effects.
5. Controls, census and inventory refresh, mutations.
6. fmt, Clippy, the suites, preflight and the commit checker; sign;
   source review; one complete final; Results.

## Results

In Auto mode, `rnx run` gives the new engine's file frontend the existing
bounded file and mapped-project `Loader`. A file declaring `mod name;` no
longer selects the old engine up front with refusal `SourceLoader`. The
program keeps source text indexed by the IDs Rune actually inserted. The
frontend checks the count, aggregate bytes and name limits on discovered
sources, and the loader keeps its separate 8 MiB read allowance. Callers
with no loader (sessions, the census's single-file route) keep the
existing compile entry. The executor and host-bridge files are
byte-identical.

A typed pre-effect refusal restarts from the original entry sources.
Replay reuses each logical load's recorded outcome, keyed by root path
and module item, failures included. Duplicate load occurrences keep
their order within a request, and an unseen alias still makes its first
charged read. Debug output comes from the route that runs, and a runtime
error stays terminal.

Controls:
- The 14 existing module and path tests were frozen on the old engine
  before the change.
- In the live CLI module cases, six Auto witnesses select execution and
  eight select the old engine, across native on and off. Execution
  covers a nested layout, an external mapped import and a terminal
  module panic.
- The old route renders the missing, invalid-UTF-8 and
  exhausted-allowance diagnostics at the declaring module. A `ByteStr`
  refusal falls back before effects. A runtime error points at the
  module's line and column, and its output occurs once. A bad source map
  fails before a route is selected.
- Eight mutations were rejected: replay success, cached failures,
  logical identity, layout precedence, source limits, source retention,
  terminal failure and pre-effect macro refusal.
- The census keeps its 244 admission and 71 executed pins, and adds two
  pinned compile-only module trees. They show inserted source IDs 0, 1
  and 2 on nested admission, and a located `ByteStr` refusal in a nested
  leaf.

The early fixed-route screen passed with the source condition. The
complete final passed all 101 stages: 49.2 minutes of accepted run, 50.3
end to end. Both fixed-route bands held, with no memory.high,
memory.max or OOM events. Before the final, 16.35 GB of verified Cargo
output (experiment checkout builds, old 0072 and 0073 build children and
the ordinary clone's debug intermediates) was removed by agreement to
meet the disk admission.
