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

A file may load other Rune files with `mod name;` and use their public
items. Lookup follows Rune 0.14.2's layout: start at the entry file's
directory, append the module's full item path, then prefer `mod.rn` over
`.rn`. For example:

```text
app/main.rn                  mod a; mod inline { pub mod deep; }
app/a.rn                     pub mod b;
app/a/b.rn                   // a::b, not app/b.rn
app/inline/deep.rn            pub mod leaf;
app/inline/deep/leaf.rn       // inline::deep::leaf
app/c/mod.rn                 // preferred to app/c.rn for mod c;
```

The executable fixture at `tests/fixtures/modules/mixed` checks this layout
against Rune's loader. A relative entry path resolves against the working
directory; module lookup adds no working-directory search. Symlinks are
followed. This is local source loading, with no package search path or
manifest in an ordinary run. Eval, sessions and notebook cells still refuse module declarations;
settings still cannot load files.

With an executable built using the optional `project-sources` feature,
`project-source-version` returns `{"format":1}` without constructing a context.
`run --source-map MAP ENTRY` accepts an explicit version-one JSON map with
`format`, an absolute `entry`, and `mounts` containing identifier-array `prefix`
and absolute `root` fields. The entry must match. The map is capped at 16 MiB,
256 mounts and 16 prefix components. Unknown or duplicate fields refuse.
An exact mount loads its root's `mod.rn`; descendants use the layout above,
with the longest mounted prefix winning and no fallback to a local file.
No map is read by eval, sessions, notebooks, config or the server entry.
This is explicit file selection, not package-lock verification or a sandbox.
The separate project tool's lock/build/run commands are still under development.

The entry and loaded modules share a new **8 MiB source allowance**, measured
in bytes. Reads stop at the remaining allowance plus one detection byte,
which is discarded on refusal; the error names the crossing file. This is a
source-size limit, not a bound on compilation time or total process memory.
Compile and runtime errors name the file that failed, and `--debug-source`
prints each loaded source under its own path header. Returned structs and
named variants retain their field names across files.

`process::run(program, args, options)` runs a child and refuses output that is not UTF-8, naming the
stream rather than handing back a plausible string with the evidence
replaced. `process::run_bytes(program, args, options)` runs the same child and returns its streams as
byte strings, for a script whose child speaks bytes.

Both forms report three things about a
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

Pass `#{}` for default options, or choose `timeout_ms` (default 30000,
1 through 90000), `input` (String or Bytes), `cwd`, `env` and `env_clear`.
Input strings are sent as UTF-8 without adding a newline; both forms accept
bytes too. Delivery closes stdin and shares the existing deadline. A child
may stop reading early, so success does not certify consumption. Without
input, stdin is null. Capture retains at most 2 MiB per stream.

```rune
let result = process::run("git", ["status", "--porcelain"], #{
    cwd: "checkout",
    env: #{GIT_PAGER: "cat", GIT_DIR: None},
})?;
```

An environment string sets a variable (including an empty string), `None`
removes it, and `env_clear: true` disables inheritance before applying the
object. These settings change only the child. Unknown options, invalid
names and NUL-containing arguments or environment values are refused before
launch. Nonzero exit is an `Ok` reply; launching failure is `Err`.

Explicit relative program paths always resolve against rnx's directory,
even without `cwd`; `./build.sh` with `cwd: "out"` runs the parent's script
inside `out`. Relative `cwd` also starts at rnx's directory. Windows
drive-relative paths such as `C:build.exe` are refused; use an absolute path.
Bare names retain the platform's executable lookup, including its limitations
on `PATH` overrides. Supply an explicit path to select an exact executable.

These calls are synchronous, including inside async code. The deadline
starts after spawn, so it does not bound validation, lookup or launch.
Record 0049 removes the old `host::` namespace, including the compatibility
process names. Existing scripts and notebook cells need these replacements:

| old call | replacement |
| --- | --- |
| `host::json_parse(text)` | `json::parse(text)` |
| `host::json_stringify(value)` | `json::stringify(value)` |
| `host::stdin()` | `io::stdin()` |
| `host::eprint(text)` | `io::eprint(text)` |
| `host::exit(code)` | `process::exit(code)` |
| `host::process(p, args, ms)` | `process::run(p, args, #{timeout_ms: ms})` |
| `host::process_bytes(p, args, ms)` | `process::run_bytes(p, args, #{timeout_ms: ms})` |
| `host::process_bytes_input(p, args, data, ms)` | `process::run_bytes(p, args, #{input: data, timeout_ms: ms})` |

Keep the explicit timeout when migrating. The process facade also validates
options, NULs and paths before launch and resolves explicit relative program
paths against rnx's working directory, as described above; it does not retain
the legacy entry points' validation or relative-path behavior. There are no
aliases or runtime deprecation warnings.

`io` is rnx's module; `use std::io;` shadows it with Rune's module. Use
`::io::stdin()` if that import is present. Rune's print/println/dbg functions
and macros are unchanged. `io::stdin()` is a once-per-process stream read,
not Jupyter notebook input; a session reset does not make it readable again.
`process::exit` is refused catchably in sessions and notebook workers.

A script chooses its own exit status with `process::exit(code)`, and can say
something on the way out with `io::eprint(text)`, so it can fail quietly
with its report on standard output or exit 2 for a usage error the way a
command-line tool is expected to. A status outside 0 to 255 is refused rather
than truncated, because 256 would reach the shell as 0.

A script given no path can read `io::stdin()`, so it can sit in a pipeline
like any other filter. It reads the stream once, under the same eight
mebibyte limit as `fs::read`, and refuses a terminal rather than waiting
for an end-of-file nobody is going to send.

Scripts and sessions also have `text::`: `find`, `split_max`, and
`group_digits`, each added because one real script needed it. Domain modules
complete and describe themselves at the prompt.

`rnx eval <source>` evaluates one expression and exits. It reads what the
expression returned the way `run` reads what a script returned, so an
expression that fails exits nonzero and says so on standard error rather than
printing an error and reporting success.

Both render a value the same way, and both render it whole: a script's
returned value is what a caller reads, so nothing is elided from it. The
prompt previews instead, marking what it cut, because a person is reading
that. A value too deeply nested to render is reported on standard error with a
nonzero status rather than printed in part.

JSON is something a script asks for, with `json::stringify(value)`, which
refuses what JSON cannot represent and names where in the value it gave up.
There is no flag: a script that wants JSON on standard output prints it.

`json::parse(text)` returns a `Result` containing the parsed value.
Integers from `i64::MIN` through `i64::MAX` become signed integers; larger
ones through `u64::MAX` become unsigned integers, without losing digits.
Integers outside that range, fractions, and exponent notation use
serde_json's approximate double conversion, which does not promise correct
rounding. Overflow to infinity is refused; `-0` becomes `-0.0`. JSON `null`
becomes `()`, which writes back as `null`. Strings preserve Unicode and
escaped characters. Repeated object keys keep the last value; key order is
not preserved. Parse errors identify a position in the JSON document.

The reader accepts 127 nested arrays or objects and refuses the 128th under
serde_json's recursion guard. The writer retains its 256-level bound shared
with the value renderer, so deeply nested JSON can be written but not read
back. Neither bound is disabled by parsing or serialization.

HTTP is available as `http::get(url).await?` and `http::get_bytes(url).await?`.
Both return an object with `status`, `headers`, `body`, and the final `url`.
Headers have lowercase names and lists of values, so
`response.headers["content-type"][0]` reads the first value. HTTP 4xx/5xx
statuses are ordinary responses. Transport errors return `Err`.

`http::request(method, url, options).await?` and `http::request_bytes`
accept `headers` (string-to-string object), `body` (String or Bytes),
`timeout_ms`, and `body_limit`. Unknown options are refused.
The default deadline is 30,000 ms (allowed: 1–90,000), shared by connecting,
redirects, headers, and body reads. The default body limit is 8 MiB
(allowed: 1 byte–64 MiB), counted after gzip decoding. Oversized and
incomplete bodies are refused. Text must be UTF-8; other encodings need
the bytes form. Gzip decoding removes the wire Content-Encoding and
Content-Length headers; returned headers describe the decoded response.

JSON still uses `json::parse(response.body)?`; request JSON uses
`json::stringify(value)?` and an explicit Content-Type header.
Redirects are followed at most ten times, with sensitive headers removed
on host/port changes. TLS verification uses bundled roots, which require
updating the binary to refresh. Proxy environment variables are honored.
A client is created lazily and reused across session inputs. HTTP requests use
execution-scoped cancellation: an interrupted, failed or budget-exhausted input
revokes requests it polled, while unrelated retained requests and pooled
connections survive. Ctrl-C while editing abandons only the edit. `:reset`
revokes all requests and releases the cached client; transport tasks and their
allocations finish on subsequent runtime progress, not necessarily before the
next prompt. Session exit and worker shutdown drain their stopped runtime.
A retained request selected away from can hold its socket until repoll, reset
or retirement; repoll observes its original deadline rather than starting over.
All four calls borrow their arguments and snapshot options at creation; network
I/O and the deadline start only when the returned future is first polled.
Name lookup uses the system resolver. A lookup already running there cannot
be cancelled and may continue after the request times out or is interrupted.
Session reset does not wait for it, and runtime shutdown does not delay
process exit for it. Its OS resources remain until it returns or the process
exits; rnx does not impose a separate deadline on that background lookup.
Version and help do not initialize networking.

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
A file's `main` may be `async` and `.await` at the top level; so may an
`eval` expression and a session input (record 0032). Ctrl-C ends a run that
is waiting on a future: it says `interrupted` and exits 130, as a shell
reports a process Ctrl-C ended. A run that is looping is ended by its
budget, as it always was.

`rnx` on its own is a session, which is `rnx repl` — the thing most often
wanted needs no word after it. `rnx help` lists the commands, and a word that
is not one of them is refused rather than doing something else. `rnx
selfcheck` asserts this build's own invariants and reports what it saw; it is
what the bare command did before record 0024.

Colour follows each output stream: a terminal gets a bold prompt, syntax
highlighting, coloured values and diagnostic markers; a pipe stays plain.
`NO_COLOR` with a nonempty value or `TERM=dumb` disables automatic colour.
Use `rnx --color=always eval 'Some(42)'` to force it, or `--color=never`
to disable it. The flag goes before the command; after a script path it
belongs to the script. Styling preserves rendered text and its bounds;
script `print!` / `println!` and `io::eprint` remain untouched.
Input highlighting requires the line editor: terminal stdin and a supported
terminal type. Redirecting stdout leaves that editor and its redraws active,
with highlighting off in auto mode and on under always.

A visible prompt is numbered, `[1] > `, and a printed result carries the
same marker, `[1] 42`. Unit results stay silent. `:renumber` starts at `[1]`
again while keeping bindings, declarations, retained source and history;
`:reset` clears the session and starts numbering over too. Errors from older
code say, for example, `input 4 of numbering 1`; current-source errors name
only their position. Recognised commands, empty or abandoned inputs and
Ctrl-C while editing do not count. Admitted inputs count even when they fail
or are interrupted; allocation-ceiling and input-cap refusals do not.
With ordinary piped input there are no prompts or result markers. Rustyline's
unsupported-terminal modes (`dumb`, `cons25`, `emacs`, case-insensitively)
write prompts even for pipes, and now write numbered results there too.
Script output, colon-command output, and diagnostics do not get result markers.

`rnx repl` is a line-edited session: history with the arrow keys and
incremental search, an input that continues on the next line while Rune's
parser says it is unfinished (two blank lines abandon it), values rendered
within bounds, diagnostics at the line and column typed, Ctrl-C to clear an
input or stop a running one, Tab to complete a binding, a declaration, a
registered function path such as `json::parse`, or a command, Ctrl-D or `:quit` to leave. `:reset` empties
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

## Files and directories

Record 0035 deliberately moves `host::read`, `host::write_new`,
`host::mkdir`, and `host::absolute` to the same names under `fs::`.
The old names are no longer registered. `fs::read` now accepts only regular
files and its UTF-8 refusal points to `fs::read_bytes`.

```rune
let text = fs::read("input.txt")?;
fs::write_new("output.txt", text)?;
for name in fs::read_dir(".")? { println(name); }
```

All eighteen functions return `Result` and name paths in refusals:

- `read` and `read_bytes`: regular files, at most 8 MiB; text is strict UTF-8.
- `write_new`, `write`, `append`: accept String or Bytes. Only `write`
  truncates an existing file, in place; writes are not crash-atomic.
- `exists`, `metadata`, `read_dir`, `absolute`, `cwd`, `temp_dir`:
  queries use Unicode paths without substitution. `exists` preserves errors;
  missing paths and dangling links return false. `metadata` follows links and
  reports kind, size, readonly, symlink and modified_ms (epoch milliseconds,
  floored before 1970); its underlying observations are not a snapshot.
  Listings return byte-sorted names, at most 100,000.
- `mkdir`, `mkdir_all`, `copy`, `rename`, `remove_file`, `remove_dir`,
  `remove_dir_all`: copy and rename refuse existing destinations atomically.
  Copy validates its source first; failure after destination creation leaves
  the partial or complete file and says so. Unsupported exclusive rename is
  an error, never a check-then-act fallback.

Recursive removal does not follow symlinks. A top-level link is removed as
such; otherwise root, cwd and its ancestors are refused. This mistake guard
is not atomic against concurrent path changes. All filesystem calls are
synchronous: there is no deadline or interruption within a call. Unix FIFO
opens are non-blocking and refused; other opens can still block. A path such
as `/dev/stdin` is accepted when its opened target is a regular file.

## Path helpers

`path::` operates on Unicode strings using the running platform's Rust path
rules. The helpers do no filesystem I/O. Non-Unicode path values remain deferred.

| function | result |
| --- | --- |
| `join(base, part)` | String; an absolute part replaces the base |
| `parent(p)` | Option<String>; `parent("a")` is `Some("")`, then `parent("")` is `None` |
| `file_name(p)` | Option<String>; final name, ignoring trailing separators |
| `file_stem(p)` | Option<String>; name without its last extension |
| `extension(p)` | Option<String>; last extension without the dot |
| `with_extension(p, ext)` | Result<String>; replace or remove the last extension |
| `is_absolute(p)` | bool; the platform's absolute-path rules |
| `separator()` | String; the platform's main separator |

```rune
let output = path::join("output", path::with_extension("report.csv", "json")?);
```

`.bashrc` has no extension; `a.` has an empty one. Removing the last extension
from `a.tar.gz` gives `a.tar`, which still has an extension. An extension
containing a separator is a catchable refusal: `/` on Unix, `/` or `\` on
Windows. Other strings, including NUL, are handled lexically; filesystem
functions validate them when used.

Reassembling parent and filename can change spelling (`a//b`, `a/./b`,
and `a/b/` reassemble alike), without asserting that they identify the same
file. In particular, a trailing separator requires a directory.
Unix join preserves `..`. Windows join normalizes `.` and `..` when its base
has a verbatim prefix and the added part is nonempty. Windows joins insert
`\` but can retain existing `/` separators. Rooted parts retain the base's
drive prefix; a drive-relative part replaces the base. Windows absolute paths
need both a prefix and a root: `C:\a` is absolute, `C:a` and `\a` are not.
Use `fs::absolute` to canonicalize an existing path through the filesystem.

## Time and dates

Moments are signed integer milliseconds since the Unix epoch, matching
`fs::metadata(path)?.modified_ms`. Fractions floor toward negative infinity:
parsing one nanosecond before 1970 gives `-1`. Calendar operations accept
`-377705023201000` through `253402207200999`, with catchable refusals outside
that range. No native time value is needed.

| function | result |
| --- | --- |
| `time::now_ms()` | i64; wall clock, for timestamps, not elapsed time |
| `time::monotonic_ms()` | i64; elapsed milliseconds since the first call in this process |
| `time::format(ms, pattern, zone)` | Result<String>; Jiff's strftime dialect |
| `time::rfc3339(ms, zone)` | Result<String>; strict RFC 3339 with three fractional digits |
| `time::parse(text)` | Result<i64>; offset or Z required, fractions floored |
| `time::parts(ms, zone)` | Result<Object>; calendar fields, ISO weekday, offset and zone |
| `time::from_parts(object, zone)` | Result<i64>; calendar fields to a moment |
| `time::sleep(ms).await` | Result<()>; interruptible sleep |

```rune
let stamp = time::rfc3339(time::now_ms(), "UTC")?;
println(stamp);
let start = time::monotonic_ms();
time::sleep(500).await?;
println(time::monotonic_ms() - start);
```

The monotonic clock never decreases and is not an epoch timestamp. Both clocks
return integers, so rnx cannot prevent a script from mixing their meanings.
Its origin survives session resets and means nothing in another process.

Zones are `"UTC"`, IANA names such as `"Europe/Paris"`, fixed offsets in
`±HH:MM` with hours 00–23 and minutes 00–59, or `"local"`. Local lookup uses
`TZ` or the system configuration and refuses failure instead of substituting
UTC. Unix uses system zoneinfo; Windows uses a bundled database which ages
with the binary. Zone lookup can perform blocking filesystem operations.
Jiff caches zone data; these calls do not promise immediate observation of
an external database change. `TZDIR` selects a database when valid; an invalid
one causes Jiff to search system directories.

`parts` returns year, month, day, hour, minute, second, millisecond, weekday
(Monday=1 through Sunday=7), offset_seconds and zone. An unnamed local zone
retains `"local"` as its label. `from_parts` requires year/month/day; clock
fields default to zero. Extra fields, including weekday and zone, are
informational and ignored; the zone argument is authoritative. A fold needs
an explicit, valid offset_seconds. A gap is always refused. A supplied offset
that disagrees with the zone is refused even on an ordinary date.

`parse` accepts Jiff's ISO/Temporal date-time forms with an offset spelled
`±HH:MM` or Z. Leap seconds and non-zone annotations are refused. Named zone
annotations are checked against numeric offsets exactly, including historical
seconds. `Z[Europe/Paris]` instead specifies an exact UTC instant with Paris
as its display zone; it is accepted, while a conflicting numeric offset is not.

`rfc3339` refuses negative years and offsets that are not whole minutes within
±23:59. The maximum supported moment fits; the minimum does not. `format`
can represent those values with `%Y` and `%:z`. It uses Jiff's fallible
formatter: invalid directives are catchable errors. Its precision follows
the pattern (`%.3f` for milliseconds), with no fixed precision promise.

Sleep accepts 0–68719476735 milliseconds. It is inert until awaited, and
awaiting at the prompt promotes the input to the async driver. Pending sleep
is interruptible; this does not change the driver's CPU-loop limitation.

## Environment and script arguments

`env::args()` returns a fresh vector of fresh strings: the arguments after
`rnx run FILE`, excluding the command and file path. It initially agrees with
`main`'s argument; mutating either value cannot change later calls. Eval and
the session receive an empty list. No new eval argument syntax is added.
All command-line words after the executable name must be Unicode. The first
invalid word is refused before dispatch, with its position (command = 1),
escaped data and exit status 2. The executable name itself is not decoded.

`env::var(name)?` returns `None` for missing and `Some("")` for empty.
Empty names, `=` or NUL in a name, and non-Unicode values are errors.
`env::vars()?` returns an object containing the environment or refuses the
whole call on any non-Unicode name or value, including a shadowed duplicate.
Unix duplicates retain their first value, as a lookup does. Windows lookups
are case-insensitive. The module has no setter or child-environment override.

`env::home_dir()?` uses Rust's platform lookup: nonempty HOME on Unix or
nonempty USERPROFILE on Windows, otherwise the account database or profile
API. A missing answer is None; a non-Unicode answer is an error naming its
source. The account lookup can block; use `env::var("HOME")` when that
fallback is unwanted.

Variables read directly by rnx are `TERM` and `NO_COLOR` (automatic colour),
`RNX_CONFIG` (config file), `XDG_CONFIG_HOME`, `APPDATA`,
`RNX_HISTORY` (history path),
`RNX_MEMORY_CEILING` (session allocation ceiling), and `HOME`,
`XDG_STATE_HOME`, `LOCALAPPDATA` (state-directory selection). The
`RNX_TEST_*` family is reserved for test-support builds. The new environment
functions also read the requested variables and the home-directory variable
above. Time-zone refusal messages read `TZ` and `TZDIR`; Jiff also reads
them on rnx's behalf for zone discovery. Separately, reqwest reads `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`,
`NO_PROXY` and their lowercase forms on rnx's behalf. This list documents
these interfaces, not every environment read inside the platform or its
libraries; maintain it manually when dependencies change. Reading a variable
through `env::` does not change how rnx or its children use it.

Record 0042 adds `:q` as an alias for `:quit`, `:clear` to clear the visible
screen without clearing bindings or numbering, and `--no-splash` before the
command to omit the session greeting. Global flags can appear in either order.
The compact `[n] > ` prompt and result marker share an accent. Clearing and
window titles require a supported terminal on stdout independently of colour;
`--color=never` still permits them and `always` never sends them into a pipe.
Titles are restored on return and explicit exit where the terminal supports a
title stack; unsupported stacks and abnormal termination may leave the title set.

Presentation settings live in `config.rn`: use an absolute `RNX_CONFIG` path,
otherwise `$XDG_CONFIG_HOME/rnx/config.rn` or `$HOME/.config/rnx/config.rn`
on Unix, and `%APPDATA%/rnx/config.rn` (falling back to `%LOCALAPPDATA%`) on Windows.
Empty directory variables count as absent; relative XDG_CONFIG_HOME is ignored.
Other relative or non-Unicode configuration paths warn. No account database is
queried to find HOME. Missing or blank files are silent.

```rune
#{
    color: "auto",
    splash: false,
    palette: #{
        keyword: "#c678dd", string: "#98c379", number: "#61afef",
        comment: "bright-black", prompt_number: "#98c379", error: "bright-red",
        result_number: "blue", prompt_frame: "bright-black",
    },
}
```

Every key is optional. Six-digit hex colours send RGB requests; the sixteen ANSI
names (`black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`,
with optional `bright-`) use the terminal palette. Foreground changes preserve
bold/dim attributes; the error colour also applies to its caret. Terminal support
and display settings still affect appearance. Without configuration, the prompt
digits are bold green, result digits bold blue, and their brackets and the
prompt arrow use dim default foreground. Value numbers remain cyan.
`prompt_number` now colours only input digits; existing configs keep working,
with the new `result_number` and `prompt_frame` keys defaulted independently.
Choose `bright-blue` if your terminal’s blue is too dark. Faint intensity is
a terminal request, not a guaranteed shade of grey.

Flags override saved settings. `--color=never` wins over saved `always`,
`NO_COLOR` disables `auto` when nonempty, and `--no-splash` overrides `splash: true`.
Config is read only for session entry points (bare `rnx` and `rnx repl`),
including piped sessions. `run`, `eval`, `version`, `help` and `selfcheck` never
read it; run and eval take colour from flags and defaults only.
An invalid field warns and keeps its default; valid neighbours still apply.
A parse, compile, evaluation or top-level shape failure warns and uses defaults.
Warnings name the file and are escaped plain text, before palette initialization.

Config is a synchronous function body returning the object. Local helper functions
and computed settings work, but no host modules, printing, external modules or
async execution are provided. Reads accept regular files only (non-blocking open
on Unix), capped at 64 KiB; evaluation has a 100,000-instruction budget. This
bounds VM instructions, not native-call time, filesystem stalls or process-wide
resource exhaustion. The evaluator is discarded after extracting settings.

## Evaluation worker (advanced)

Record 0046 adds `rnx worker --control-read N --control-write N` for a parent
that owns two inherited one-way control pipes and captures stdout/stderr
concurrently. It is the foundation for a future notebook kernel, not a Jupyter
kernel or a command to type into the REPL. Stdin must be the null device.
Worker startup never reads personal config/history or emits a prompt, splash,
title or presentation colour. Script printing remains raw.

Control is UTF-8 JSON lines (256 KiB maximum), starting with `ready` protocol 1.
An operation has an increasing positive `id` (at most 2^53−1), `op` (`execute`,
`reset`, `shutdown`) and a fresh parent-generated `nonce` (64 lowercase hex
characters). Execute also carries `source`, limited to 32 KiB. Admitted execution
emits `armed` after clearing the interrupt flag. `settled` carries the request
ID, reset epoch, admitted input index or null, bounded `text_plain` (null for
unit), and a structured failure or null. Source origins name the defining input.

The parent must collect both stream barriers and hand off retained output before
sending `{"op":"ack","id":1}`. A barrier is the byte sequence
`\x1eRNX-WORKER-1:<id>:<stdout|stderr>:<nonce>\x1f`; it is not a line and
must be recognized before decoding text. Output collection is capped at 2 MiB
per stream but continues draining discarded bytes until the barrier. Wrong or
missing acknowledgements cannot admit another operation. Reset clears session
state and advances the epoch; request IDs never reset. Cleanup failure retires
the worker. A complete acknowledged shutdown exits zero; broken control or
incomplete boundaries are failures, never fabricated successful cells.

The parent supplies independent cancellation/kill supervision. It waits for
`armed` before delivering a queued interrupt. Synchronous execution remains
interruptible; async CPU loops retain the existing budget-only limitation.
A hard restart loses bindings. Partial stdout lines may remain buffered until
the operation's flush. Background output has interval attribution, not guaranteed
causal ownership: between operations it is unassociated, but an old writer
running during the next operation cannot be identified from a shared pipe.

The bounded parent fixture is in `tests/worker_parent.py`; the integration gate
requires Python 3 in addition to Rust. Unix transport has executed on nano.
Windows transport type-checks in the standalone probe and awaits execution.
The full wire contract, bounds and failure policy are in record 0046.


## Caller-owned handler execution (optional)

The `server-runtime` feature exposes `rnx::server::{Program, Invocation, Failure}`.
Compile an entry file once with `Program::compile(path, schema_extensions)`;
create fresh, identically registered extensions on each worker and call
`program.prepare(extensions, "module::handler", request_value, budget)`.
The handler takes one Rune value. Construct and inspect values through
`rnx::rune`; no private renderer or serializer is needed. `Program` is shareable;
an `Invocation` and its values stay on their owner thread.

Await `invocation.run()` once on the caller's runtime, then explicitly call
`invocation.close()`, including after dropping a polled run. Close repeats a
remembered execution failure (`cancelled` or `vm`), with `cleanup` taking
precedence for state loss. Success means execution and retirement succeeded;
it does not certify that the runtime's network tasks have finished. Zero and
`usize::MAX` budgets are refused. This entry installs no signal handler, reads
no colour config and always refuses script `process::exit` catchably. Native
extensions remain trusted. No listener, pool, worker threads or process-kill
policy is supplied by this feature. The separate Linux example application in
[servers/http-postgres](https://github.com/nilket-com/rnx/blob/main/servers/http-postgres/README.md) assembles that policy
through this public API, with its own workspace, lockfile and executable. Record 0056 documents its execution and ownership contracts.

## Assemble an executable with native extensions

An external Rust application can depend on this checkout and run rnx with
its own Rune modules. No package manager or dynamic plugin loading is involved.
With default features the library exposes only `main_with`, `Extensions`, `Scope`, and its pinned `rune`
re-export. Keep the normal `main` return type so startup errors retain rnx's
exit status and diagnostics:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let extensions = rnx::Extensions::none().with("example", |module| {
        module.function("answer", || 42i64)
            .build().map_err(|error| error.to_string())?;
        Ok(vec![("example::answer".into(), "answer() -> 42")])
    });
    rnx::main_with(extensions)
}
```

Use `rnx::rune` rather than a separately versioned Rune dependency. An
incompatible Rune version has different Rust types; conflicting exact pins
within the same compatible version line can instead fail Cargo resolution.
Call `main_with` once, from the main thread. Builders may capture owned
non-`Send` state. They run in order after the batteries and, for sessions,
after settings evaluation, before user input. Run, eval, the session and the
worker all receive extensions; reset retains them without rebuilding.
Version, help, selfcheck and the pure settings evaluator do not run builders.
An assembled application works with `rnx-jupyter install --rnx /absolute/path/to/app`.

Adapters are trusted Rust code. Each owns its declared namespace. rnx checks
the declared name against reserved and earlier extension names, checks help
paths under `name::`, and reports Rune's installation errors. These checks
catch mistakes, not misbehaviour. They cannot verify that help describes a
registered function or that a builder kept its module in that namespace.
An adapter can replace the lent module, and the fixture demonstrates that
replacing it with a module named `fs` adds a function to `fs`. Do not do that.

Registration uses Rune's full API. A plain function name is one component:
`function(["nested", "answer"], ...)` does not compile, and the string
`"nested::answer"` does not create a callable nested path. Native type item
attributes carry their own path: use, for example,
`#[derive(rnx::rune::Any)]` with `#[rune(crate = rnx::rune, item = ::example)]`
for a type owned by `example`. A mismatched `item = ::other` is not rewritten
to `example`; the probe resolves it as `::other::Elsewhere` even before declaring an
`other` crate, and then as `other::Elsewhere` after declaring that crate. The checked-in assembly fixture in rnx-bench records these cases.

Builder errors and unwinding panics refuse startup once, naming the
extension, before the prompt or worker ready message. During a builder,
rnx suppresses the panic hook only on that thread and forwards other threads'
panics to the previous hook, restoring it afterwards. Aborts, panic-abort
builds, explicit process exits and panics on spawned threads are not converted.
Native code can perform I/O, block, allocate, or change process state; the
registration checks do not bound that work.

The default `count-allocations` feature supplies the executable's global
allocator, allocation reports and ceiling. An application cannot declare a
second global allocator while it is enabled. Turning off rnx's default
features disables accounting and its ceiling, just as in the stock binary.
Extensions' retained allocations count toward that ceiling.

An extension whose pending futures hold resources can opt into cancellation:

```rust
let extensions = rnx::Extensions::none().with_lifecycle("example", |module, scope| {
    module.function("answer", move || scope.track(async { Ok(42i64) }))
        .build().map_err(|e| e.to_string())?;
    Ok(vec![("example::answer".into(), "answer() -> future<Result<i64>>")])
});
```

`Scope::track` accepts a `'static` future returning `Result<T, String>`. Neither
that future nor `T` needs `Send` or `Sync`. The clonable scope is safe to capture
in Rune's `Send + Sync` function closure, but its operations belong to the
serving context's thread. Wrong-thread and retired-context calls return distinct
errors without polling the supplied future.

A failed execution revokes pending tracked operations it polled, even if an
older binding retains the future. Revocation drops the inner future synchronously,
without another runtime turn. A retained wrapper subsequently returns
`Err("operation cancelled")`; this does not change Rune's consuming `await`
semantics. Use `select` when the Rune future must remain available. Successful
selection keeps the losing operation, and failures leave unrelated operations
alone. Reset and normal teardown revoke all tracked operations. Renumber does
not. A destructor panic retires the context; a worker reports state loss.

This primitive requires resources that close on drop. It does not drain spawned
tasks, stop blocking calls, roll back server-side effects, or retry work. HTTP
uses this same execution-scoped ownership, including in stock rnx contexts.
`process::exit` invokes cleanup
for registered owners explicitly; a native future currently on the poll stack
ends with the process, since there is no subsequent return from that call.

The first external capability is [rnx-postgres](https://github.com/nilket-com/rnx/blob/main/adapters/postgres/README.md).
It builds its own `rnx-pg` executable with `postgres::query`, using typed
parameters and tracked per-call connections. It is an independent workspace;
stock rnx's dependency graph and batteries do not include the database driver.
