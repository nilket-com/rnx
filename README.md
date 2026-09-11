# rnx: a scripting environment for Rune

A runner for `.rn` files, an expression evaluator, and an interactive
session, on upstream Rune with no compiler fork.

**This is not a release.** The version is `0.0.0`, the package is not
published, and it carries no license yet — record 0026 explains what each of
those is waiting for. The behaviour below is what runs on Linux; macOS
type-checks and Windows compiles, and neither has been run (records 0025 and
0001's gate 4).

The reasoning is in `plans/`, one numbered record per decision, with evidence
beside the records that have it: the first release is
[plans/0001](https://github.com/nilket-com/rnx/blob/main/plans/0001_the_first_release.md) and the interactive
session is [plans/0002](https://github.com/nilket-com/rnx/blob/main/plans/0002_the_interactive_session.md). The
original feasibility spike and its outcome are in
[evidence.md](https://github.com/nilket-com/rnx/blob/main/evidence.md). Those files are in the repository and not in
the published package — record 0026 says why — so these point there, where
they work from either copy.

## Reproduce

This is an independent, unpublished Cargo package pinned to Rune 0.14.2, with
its own lockfile. It uses the compiler/VM APIs directly and does not yet wrap
upstream `rune::cli::Entry`. `rnx version` reports this build and the Rune it
is pinned to. It is built and tested with Rust 1.95; 1.88 is measured not to
work, in a dependency.

```sh
cargo run --locked
cargo run --locked -- eval 'let x = 4; x + 3'
cargo run --locked -- repl
```

`rnx` on its own opens a session. `rnx selfcheck` is what runs this build's
own assertions and prints what it saw, which is what the bare command did
before record 0024.

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

`host::process` and its two byte-returning forms report three things about a
capture, independently: `truncated` if the size cap was reached, `cut_short`
if rnx stopped reading before the stream ended, and `unreadable` if a read
failed. A caller that needs everything the child produced checks all three,
and checks `timed_out` and `cancelled` first, because both of those leave a
capture cut short and each says more about why.

`code` on its own does not establish that the child exited normally, and what
it holds for a child **rnx** ended differs by platform: on Unix a child killed
at its deadline has no exit status at all, so `code` is empty, while on
Windows ending the job **is** an exit status and `code` is 1 — indistinguishable
from a child that chose to exit 1. Both mean the same thing about the child,
which is that it did not choose how it ended, and `timed_out` and `cancelled`
are where that is said unambiguously on either platform. Read them first.

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

`rnx` on its own is a session, which is `rnx repl` — the thing most often
wanted needs no word after it. `rnx help` lists the commands, and a word that
is not one of them is refused rather than doing something else. `rnx
selfcheck` asserts this build's own invariants and reports what it saw; it is
what the bare command did before record 0024.

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
