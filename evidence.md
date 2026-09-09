# Rune scripting spike: decision evidence

2026-09-08. Recommendation: proceed to design a dedicated scripting environment.
The evidence supports the repository's immediate orchestration needs and a
constrained persistent REPL. It does not establish Python ecosystem parity,
unrestricted incremental Rune semantics, or production readiness.

## What ran

The independent package pins upstream Rune 0.14.1, with no patches or compiler
fork. The Rust host adds seven generic functions: read text, create a new file,
create a directory, canonicalize a path, parse/stringify JSON, and execute an
argument-array subprocess. File creation refuses existing evidence paths. Native
string arguments borrow instead of consuming Rune values; a consuming initial
binding caused a real `Cannot take` failure when the script reused a path.

`journey.rn` completed the same 19 public agent actions as the former Python
harness in a real first-run window. This included the example, opening the
repository's Parquet fixture, SQL edits, new plot incarnations and publication.
The successful first package digest was
`643fd87e8807c6021636f8c59c06d8f59787be9c8c4fe2bea158f98ad032d9af`.
Receipts: `/tmp/polariton-rune-CuZS5U/borrowed-receipts`. No Python, jq, pointer
automation or product-private API was used by the Rune script. The host invokes
the existing Polariton binary; it does not reimplement the agent protocol server.

The final runner, with its two-million-instruction budget, repeated all 19
actions and produced the identical digest. Final receipts are in
`/tmp/polariton-rune-final-Ik310A/receipts`; both test windows were closed.

An unknown-answer fake actuator was invoked once and the script stopped with a
nonzero exit. A script returning `Err("explicit failure")` also exited nonzero.
Those controls caught the distinction between a VM execution error and a Rune
main function successfully returning an Err value: the runner must handle both.

Process assertions cover exit code 7, a 40 ms deadline against a process group
with a sleeping child, a 3 MB output bounded to 2 MiB with a truncation flag, and
SIGINT cancellation while waiting for a process. Arguments are never shell-parsed
by the host. `/bin/sh` is used explicitly only by those process fixtures.

## Persistent state: observations, not assumptions

Each accepted input is parsed using Rune's AST. Function/struct/enum declarations
are staged, and a new unit imports the retained bindings and returns their new
values. Previous statements are never executed again. Values remain Rune values;
they are not serialized between inputs. The interpreter and old VM can be dropped
while a retained closure keeps its needed compiled unit alive.

| Probe | Observed result |
|---|---|
| Retained closure calling `helper() = 10`, then new `helper() = 20` | Closure still calls 10; fresh direct call returns 20 |
| Retained function value | Still calls the old definition |
| Retained closure's captured vector changes | Closure observes the later mutation |
| Old struct instance, same declaration in new unit | Field access, type test and a new-unit method work |
| Same type name with a different struct shape | Old value still passes the type test; accessing the newly added field fails |
| Compile error with a candidate redefinition | Prior declarations and bindings remain usable |
| Runtime failure after vector mutation and scalar shadowing | Vector mutation survives; candidate scalar and new binding are not published |
| Runtime failure after creating a file | File remains; a later input does not replay or undo the write |
| Later evaluations after one effectful append | Effect count stays one; no transcript replay |
| Tuple/vector/object destructuring and shadowing | Bindings persist and resolve to the expected values |
| Infinite loop | Two-million-instruction budget halts it; previous state is still usable |
| Reset | Old binding names are unavailable |

The prototype therefore refuses a changed struct/enum declaration until reset.
Function redefinition has snapshot semantics for retained function values and
closures. Those are usable semantics, but they need to be explained rather than
advertised as equivalent to Python's name resolution.

The AST work mattered. In this version, a struct item's derived span omits its
closing delimiter. The adapter uses parsed statement starts and the enclosing
block's closing token to preserve complete source fragments. A line classifier
would also mishandle the multiline and destructuring examples. Rust-style
or-pattern match arms did not compile in the pinned Rune version; the script uses
separate arms. None of these required changing the compiler.

A local debug-host probe compiled and executed 100 successive inputs in about
21 ms with a small retained environment and optimized dependencies. This is only
a smoke measurement on this machine, not a benchmark against Python or a claim
about large sessions. The debug executable was approximately 9.3 MiB on disk.

## What this does not settle

- The adapter supports only a subset of declaration forms. It has not solved
  modules, imports, macros, arbitrary pattern-name resolution, impl replacement,
  source maps, or full-language incremental compilation. Identifiers used by the
  wrapper are reserved. Its handling of a failed input is not transactional.
- Closures retain old units intentionally. Retained-memory accounting, cycles,
  reclamation and long-lived-session growth were not measured. Resetting names
  is not a demonstrated guarantee that every allocation is reclaimed.
- Process handling is Unix-specific and proven for ordinary process groups.
  A descendant that deliberately escapes its group can retain a pipe and defeat
  the current reader join. Native file operations can block independently of VM
  instructions. Windows process trees, binary I/O, stdin streaming, environment,
  working directories and production cancellation need proper implementations.
- The prompt has no completion, history, automatic multiline detection or rich
  diagnostics. Output formatting supports JSON-compatible values, not every Rune
  value. An instruction budget is not a general sandbox or a memory limit.
- Existing Python scripts also need HTTP, TOML, dates, regular expressions and
  binary subprocess streaming. Those are inventoried, not migrated or supplied
  by this seven-function host. No third-party Python package appeared in the four
  older tracked scripts, but replacing their standard-library behavior still
  requires implementation and acceptance tests.

## Project implication

The compiler-fork question is answered narrowly: this runner and this explicit
REPL policy work as downstream code. That is enough to justify a public-project
design, not enough to promise an unrestricted Python replacement in a few days.

The public core should own the host interfaces, execution semantics, diagnostics
and interactive session. Product-specific integration should consume that core
or use the public agent protocol. The proof used generic process/JSON functions,
so it does not require Toron, an editor, a window or an account merely to run.
Reuse of upstream CLI/documentation/language-server facilities remains a design
choice; this prototype exercises the compiler/VM directly.

The next planning cut should specify a supported first release: file execution,
short evaluation, the stated REPL subset, process/filesystem/JSON APIs, stable
errors, installation and a three-platform ladder. It should treat retained-memory
behavior and process-tree cancellation as acceptance work, not polish. Naming,
license/provenance review and the public repository are still undecided.

## The brief this spike answered

Moved here from the README by record 0026, verbatim. It describes work in
the repository rnx came out of — an exercise that needs a program from
there to run — so it is not what someone arriving at rnx should read
first. It is kept because it is what the spike was asked, and the
sections above are the answers.

For the real-window exercise, start one fresh `polariton --agents`, take its id
from `polariton windows`, and provide a new output directory whose parent exists:

```sh
cargo run --locked -- \
  run journey.rn \
  /absolute/path/to/polariton WINDOW_ID /absolute/path/to/data.parquet \
  /tmp/new-rune-journey first-data first-plot
```

The `.rn` file uses only generic host modules and the public `polariton act`
protocol. It saves commands and answers, requires a new incarnation after each
plot run, and stops on unknown/failure without replay. Close the test window
afterward. Bash/jq remains the product acceptance harness until a supported Rune
runner is deliberately adopted; this spike is not a new install dependency.

The immediate Python harness replacement is independent and uses Bash/jq.
This spike asks whether a standalone Rune host can replace that orchestration,
and what persistent interactive evaluation can truthfully guarantee.

## Questions that must be executed

1. Compile independent units, transferring Rune values without replaying old
   statements. Retain a closure and a struct instance; redefine a function and
   record which definition each call uses. Change the struct shape as a negative
   control. Keep a closure alive after its original VM has been dropped.
2. Introduce a compile error, then a runtime error after a mutation. Observe
   binding publication, aliased state and external effects separately. Do not
   promise rollback of already executed work.
3. Use Rune's AST to separate declarations/statements and identify bindings,
   including shadowing and destructuring. An expression wrapper alone is not a
   persistent REPL.
4. Expose files, JSON, argument-array subprocesses, bounded capture and deadlines
   in a standalone host. Run the same 19-action notebook journey without Python.
   Exercise process failure, timeout and cancellation rather than only success.

The resulting report must separate demonstrated behavior, unsupported behavior,
and production work remaining. A runner may be viable even if the proposed REPL
state model fails. Public repository placement follows this evidence.

## Python inventory

All four older tracked Python files import only the standard library:

| File | Workload | Host facilities needed |
|---|---|---|
| `plans/product/migration/verify_nilket_graft.py` | Frozen Git migration audit | Paths, binary subprocess pipes, byte parsing, maps |
| `nilket/scripts/classify_fold_perf.py` | Perf-symbol classification | Process execution, JSON, regular expressions, counters |
| `nilket/crates/toron/scripts/bench/summarize_runtime_filter_perft.py` | Benchmark summaries and comparisons | File/text parsing, grouping, arithmetic, formatted output |
| `nilket/crates/wendelon/ci/seed_retention_advisory.py` | CI retention advisory | JSON, TOML, dates, environment, HTTP and URL handling |

This is not a migration of those files. Standard-library-only still includes
substantial behavior: streaming binary pipes, HTTP error handling and dates need
proper replacements. The new notebook harness is the initial, narrower workload.

Upstream baseline: the repository resolves Rune 0.14.1. Its `rune::cli::Entry`
supports custom contexts and script/check/test/format/documentation commands;
its CLI command enum contains no REPL. Existing Baryon job bindings are useful
reference material, but the standalone spike must not instantiate an editor.
