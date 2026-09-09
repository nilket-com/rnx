# rnx: a scripting environment for Rune

A runner for `.rn` files, an expression evaluator, and an interactive
session, on upstream Rune with no compiler fork. Records: the first release
([plans/0001](plans/0001_the_first_release.md)) and the interactive session
([plans/0002](plans/0002_the_interactive_session.md)); evidence for each
sits beside it. The original feasibility spike and its outcome are in
[evidence.md](evidence.md). This is not yet a release.

## Reproduce

This is an independent, unpublished Cargo package pinned to Rune 0.14.1, with
its own lockfile. It uses the compiler/VM APIs directly; it does not instantiate
Baryon, Polariton or Toron and does not yet wrap upstream `rune::cli::Entry`.
The host process implementation is Linux/Unix only.

```sh
cargo run --locked
cargo run --locked -- eval 'let x = 4; x + 3'
cargo run --locked -- repl
```

The default command runs the spike's assertions and prints observations.

`rnx run <file.rn> [args]` executes a file's `main`. A compile or runtime
error names the file, the line, the column, the source line, and marks the
column, including for an error inside a called function; a call to a method
that does not exist names the method rather than the hash Rune reports; every diagnostic
goes to standard error and the script's own output to standard output.
`--debug-source`, before the path, also prints the compiled source; anything
after the path is the script's argument.

`host::process` runs a child and refuses output that is not UTF-8, naming the
stream rather than handing back a plausible string with the evidence
replaced. `host::process_bytes` runs the same child and returns its streams as
byte strings, for a script whose child speaks bytes.

`host::process_bytes_input` also tells the child what to do: the byte string
it is given is written to the child's standard input, which is then closed,
because a child like `git cat-file --batch` needs the end of its input to
finish. All three streams move at once, so a large input cannot wedge against
a large reply, and delivery gives up at the same deadline the call has. A
success means the child answered, not that it read everything: a child may
stop reading, and that is its prerogative.

A script chooses its own exit status with `host::exit(code)`, and can say
something on the way out with `host::eprint(text)`, so it can fail quietly
with its report on standard output or exit 2 for a usage error the way a
command-line tool is expected to. A status outside 0 to 255 is refused rather
than truncated, because 256 would reach the shell as 0.

A script given no path can read `host::stdin()`, so it can sit in a pipeline
like any other filter. It reads the stream once, under the same eight
mebibyte limit as `host::read`, and refuses a terminal rather than waiting
for an end-of-file nobody is going to send.

Besides `host::`, scripts and sessions have `text::`: `find`, `split_max`,
and `group_digits`, each added because one real script needed it. Both
modules complete and describe themselves at the prompt.

`rnx eval <source>` evaluates one expression and exits. It reads what the
expression returned the way `run` reads what a script returned, so an
expression that fails exits nonzero and says so on standard error rather than
printing an error and reporting success.

Both render a value the same way, and both render it whole: a script's
returned value is what a caller reads, so nothing is elided from it. The
prompt previews instead, marking what it cut, because a person is reading
that. A value too deeply nested to render is reported on standard error with a
nonzero status rather than printed in part.

JSON is something a script asks for, with `host::json_stringify(value)`, which
refuses what JSON cannot represent and names where in the value it gave up.
There is no flag: a script that wants JSON on standard output prints it.

Everything rnx prints itself is escaped — a diagnostic, a source excerpt, a
file path, an error a script returned — so nothing can move the cursor of
whoever ran it, and a caret is placed by the columns the escaped line actually
occupies.

`rnx run --budget N file.rn`, with the flag before the path, sets how many
instructions the script may spend; without it the limit is two million, as it
has always been. `N` runs from 1 to one less than the largest `usize`, that
last value being Rune's way of saying "no budget" and so refused. The count
comes from the command line and nowhere else: no host function, environment
variable or file directive sets it, and there is no way to remove the bound.

`rnx repl` is a line-edited session: history with the arrow keys and
incremental search, an input that continues on the next line while Rune's
parser says it is unfinished (two blank lines abandon it), values rendered
within bounds, diagnostics at the line and column typed, Ctrl-C to clear an
input or stop a running one, Tab to complete a binding, a declaration, a
`host::` function path, or a command, Ctrl-D or `:quit` to leave. `:reset` empties
the session, `:memory` reports tracked live allocation request bytes against
a ceiling, `:debug` shows the source generated for the last input, and
`:help` lists them all. The ceiling is `RNX_MEMORY_CEILING` bytes if set,
else 512 MiB; once a sample between inputs finds the figure at or above it,
evaluation is refused until a reset samples below it, while inspection keeps
answering. Building with `--no-default-features` compiles the accounting out,
and the build says so rather than reporting a figure. History is text only, kept in `$RNX_HISTORY`,
else `$XDG_STATE_HOME/rnx/history`, else `~/.local/state/rnx/history`, and
nothing in it runs on restore. `:vars` lists the bindings with their types
and values, and `:help <name>` describes one binding, declaration, host
function, or command; both are bounded reads that run no Rune code, so they
answer even when the session is over its memory bound. The session supports a deliberately limited
set of persistent declarations and bindings; unsupported declarations refuse
rather than pretending to survive. Successful inputs publish bindings and
declarations; failed inputs can still mutate shared values and perform
external effects. Tests: `cargo test --locked` (the session gates run through
a pseudo-terminal on Linux).

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
