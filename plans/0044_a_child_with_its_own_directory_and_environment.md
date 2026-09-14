# rnx 0044: a child with its own directory and environment

Status: implemented 2026-09-14; Linux gates pass, with evidence beside
this record. Windows facade/unit-test type-check passes; Windows execution
remains unverified. The forty-fourth record of rnx. It gives
the existing process supervisor a `process::` facade, text input, a
working directory and child-only environment changes. It takes up the
child-environment work deferred by records 0031 and 0036; it does not
close record 0031's separate async CPU-interruption gate.

## Context

Source inspection on 2026-09-14 finds three registered functions in
`host.rs`: `process`, `process_bytes` and `process_bytes_input`. All reach
`run_child`. Records 0016 through 0023 own its concurrent pipe draining,
capture limits, deadlines, cancellation, descendant cleanup and reply.
None accepts a working directory or environment changes, and only the
bytes-output form accepts input. Those are the gaps, not a missing
process engine. This record reports no new performance measurements.

Three scripts motivate the additions: capture Git's status in a selected
checkout, feed text to a program that reads until end-of-input, and run
a build in another directory with variables that must not change rnx's
environment or a later child. Each should use the existing supervisor.

One standard-library caveat matters before writing the facade. Rust's
`Command::current_dir` documentation calls a relative executable path
with a child directory platform-specific and unstable. The contract here
must choose an origin for that path instead of exposing that ambiguity.
Bare-name search is a separate operation and remains the standard
library's. These statements come from the installed Rust documentation
for `Command::new`, `current_dir`, `env`, `env_remove` and `env_clear`;
they are not claims of new Windows execution evidence.

## Decision

### 1. Two names, one supervisor, the existing reply

```rune
process::run(program, args, options)?
process::run_bytes(program, args, options)?
```

Both functions are synchronous and return `Result<Object>`. `program` is
a string, `args` a vector of strings and `options` an object. `#{}` asks
for the defaults. The options-object convention is record 0034's: known
optional keys, absent keys defaulted, present wrong types and unknown
keys refused by name. There is no arity overload or new native result
type. `run_bytes` differs only in the output representation.

Both use `run_child`, extended with prepared launch settings. The old
callers pass inherited-directory, inherited-environment settings. No
second capture loop, timeout loop, pipe writer, process group or job
object path is introduced. Reply construction and decoding are shared
with the existing entry points.

| field | meaning, unchanged from the host functions |
| --- | --- |
| `code` | integer exit code, or unit when no code is available |
| `stdout`, `stderr` | strings for `run`, `Bytes` for `run_bytes` |
| `timed_out`, `cancelled` | why rnx ended the child |
| `truncated` | a capture exceeded its retained-byte cap |
| `cut_short` | capture ended before the streams reached their end |
| `unreadable` | a capture could not be read completely |

A nonzero exit is an `Ok` reply, including its output. Callers inspect
`timed_out` and `cancelled` before interpreting `code`, then the three
capture flags before treating output as complete. A killed child can
have no exit code on Unix and code 1 on Windows, as before. Validation,
launch and existing supervisor failures are `Err`. The text form retains
the existing strict UTF-8 rules, including the exception for an incomplete
trailing character when rnx itself stopped that stream. No lossy decoding
or new interpretation of failure is added.

### 2. Five options, validated before the child is started

| key | accepted value | absent |
| --- | --- | --- |
| `timeout_ms` | integer in `1..=90000` | `30000` |
| `input` | `String` or `Bytes` | closed/null standard input |
| `cwd` | nonempty path string | inherit rnx's working directory |
| `env` | object whose values are strings or `None` | no overrides |
| `env_clear` | boolean | `false` |

The 30-second default is this facade's choice, matching HTTP's default;
the old functions still require their explicit deadline argument. An
empty options object and one with every default spelled out agree.
Unit is not a substitute for an absent option, and environment removal
uses `None`, not unit or an empty string.

Validate the complete call before spawning: options, argument elements,
environment entries and path inputs. Refuse an empty program and NUL in
the program, arguments or `cwd`; name the argument's one-based position
in `args` when it is invalid. Environment errors name the variable and
the reason, without echoing its value. Existing diagnostic presentation
continues to escape user text. No refusal prints a warning to stderr.

An input string is sent as its UTF-8 bytes, without adding a newline. A
`Bytes` input is sent unchanged. Both output forms accept either input
kind. Empty input is a supplied empty stream; absent input uses the old
null-stdin behaviour. Delivery uses the existing concurrent writer and
closes stdin when finished. An `Ok` reply does not certify that the child
consumed every supplied byte: record 0017's early-reader-exit rule stays.
No new stdin-size cap is introduced; copying the supplied input retains
the existing allocation and memory-ceiling behaviour.

### 3. A directory changes the child, not the meaning of an explicit program path

The new functions resolve an explicit relative executable path against
rnx's working directory at the call, before applying `cwd`. An explicit
path contains a separator recognised by the running platform, is
absolute, has a Windows prefix/root, or is exactly `.` or `..`. Unix
recognises `/`; Windows recognises `/` and `\`. An absolute program stays
absolute. Bare names such as `git` are passed to `Command` for lookup.

Relative `cwd` is also based on rnx's directory, never on the executable's
directory. Resolve both from one captured parent directory when either
needs it. Joining that directory to an ordinary relative path must not
collapse `..` across a possible symlink. There is no requirement to
canonicalize an executable, read it, or prove it exists before spawning.
The operating system still decides whether it can be launched.

On Windows, a rooted path without a prefix takes the captured parent
directory's prefix. A drive-relative path such as `C:tool.exe` or `C:work`
is refused for either program or `cwd`, asking for an absolute path;
this avoids consulting an implicit per-drive working directory. Windows
verbatim paths keep their native semantics. These cases receive
Windows-specific gates rather than invented Unix equivalents.

For example, with rnx in `/project`, program `./build.sh` and
`cwd: "out"` select `/project/build.sh` with child directory
`/project/out`, not `/project/out/build.sh`. To select the latter, the
script names `./out/build.sh`. Gates place different executables at the
two locations so the wrong origin cannot pass.

Bare-name lookup retains `Command`'s platform rules, including its
documented limitations concerning `PATH` changes. This record promises
that overrides reach the child, not that setting `PATH` supplies an
identical executable-search algorithm on every platform. A script that
needs an exact executable supplies an explicit path. The help and
evidence state this distinction. A failed launch with `cwd` specified
names both the supplied program and the directory, without guessing
which caused an ambiguous operating-system error.

No call changes rnx's own working directory. The compatibility functions
keep their existing executable lookup behaviour.

### 4. Environment changes belong to this child alone

By default the child inherits the parent's native environment. Do not
round-trip that environment through `env::vars` or Rune strings: an
inherited non-Unicode entry is not a reason to refuse a launch.

With `env_clear: true`, disable inheritance first. Then apply `env`:
a string sets or replaces the named variable, including an empty string;
`None` removes it and prevents inheritance. A removal after clearing is
a harmless no-op. Validate every entry before applying any to `Command`.
Use `Command`'s child-environment operations, never global `set_var` or
`remove_var`. A later child and `env::var` still see the parent environment.

Names must be nonempty and contain neither `=` nor NUL. Values must
contain no NUL. A wrong type is refused naming its key. Names are
case-sensitive on Unix and case-insensitive on Windows. Reject two
Windows override keys that name the same variable under the comparison
used by `Command`, rather than letting object iteration choose the
winner; Unicode lowercasing is not a substitute for that comparison.
The platform implementation and its gate establish the comparison used.
Duplicate identical object keys have already been resolved by Rune
before the call; this facade cannot recover their source spelling.

### 5. Existing bounds stay where they are

The supervisor starts the deadline after a successful spawn. Validation,
path preparation and the operating system's launch are outside it; this
record does not promise a wall-clock bound for those operations. The
existing per-stream retained capture cap is 2 MiB. Cleanup allowances,
interrupt handling and delivery behaviour remain records 0016–0023's,
not new options on this facade.

These are blocking host calls. They occupy the executor thread, even
when called by an async script. VM instruction budgets cannot interrupt
a native call midway; the supervisor itself handles its deadline and
the interrupt flag. There is no promise of parallel subprocess calls,
and the module does not add a Tokio process path.

### 6. Migration changes documentation before removing names

Register exactly the two new `process::` functions. Keep all three
`host::process*` functions with their existing signatures, defaults,
reply fields and failure wording. Their descriptions point to the new
module; they emit no runtime deprecation warnings, respecting record
0018's stderr contract. Pass the sibling byte-function name into the shared decoder: a new
`process::run` refusal points to `process::run_bytes`, while old callers
keep `host::process_bytes` and their existing wording.

README examples, completion and session help document `process::` as
the preferred surface. Keep compatibility coverage, including the
record 0017 port and `journey.rn`, rather than deleting the evidence
that existing scripts still work. Removal of old names is another
record. No shell helper, pipeline, persistent child handle, background
job, streaming output or error-on-nonzero helper is added here. A script
may explicitly name a shell as its program, as it already can.

## Gates

1. **Surface and compatibility.** Exactly two new names are registered
   and described, alongside the old three. Exercise text and bytes
   replies from both surfaces against one controlled child, asserting
   all eight fields and their types. A nonzero child writes stderr and
   returns `Ok` with its code; a missing executable returns `Err`.
   Compatibility tests and the existing examples retain their output.
2. **Inputs and validation.** Echo non-ASCII text without an added newline
   and arbitrary bytes through both applicable output forms. Exercise
   empty and absent input and a child that exits before reading it all.
   Check every unknown key, wrong option type, deadline endpoint and
   invalid argument/environment spelling. A child-side marker proves
   invalid calls never launch; NUL in stdin remains valid data.
3. **Directory and executable origin.** Two distinct fixture executables
   at parent-relative and child-relative paths distinguish the rule.
   The child reports its directory and rnx's directory remains unchanged.
   Cover absolute and relative `cwd`, missing/non-directory `cwd`, and
   spaces and non-ASCII characters in paths. Gate Windows rooted,
   drive-relative and separator cases on Windows. Exercise a bare-name
   lookup in a controlled search environment and report platform limits
   separately from explicit-path guarantees.
4. **Child environment.** Use an absolute fixture executable so search
   does not decide the outcome. Cover inheritance, replacement, empty
   value, removal, clear followed by setting, and a following unchanged
   child. Assert `env::var` is unchanged. Gate Windows case collisions
   and Unix non-Unicode inherited values with platform fixtures. Clear
   tests assert absence of a parent sentinel and presence of explicit
   settings, not an exact environment a runtime could itself augment.
5. **The supervisor still owns the work.** Exercise the new text and
   bytes paths through deadline, cancellation, oversized capture,
   undecodable output and descendant-held-pipe fixtures. Reuse existing
   deterministic phase hooks for delivery/cleanup interruption instead
   of replacing them with sleeps. Existing process suites remain green.
   Source review establishes that all five entry points reach the same
   supervisor and platform spawn, with no second lifecycle implementation.
6. **The motivating scripts.** In fresh temporary directories, capture
   `git status --porcelain` from a temporary repository with fixture-owned
   Git configuration, pipe text into an EOF-reading fixture, and run a
   build-shaped fixture under `cwd` and an environment override. Use no
   public network or developer checkout. The nonzero/stderr case in
   gate 1 keeps the meaning of `Ok` visible beside these examples.
7. **Integration and cost.** Run default and test-support suites
   sequentially, including help/completion across a reset and the memory
   ceiling with the module installed. Preserve the previous release
   binary. Measure bare `run`, `eval`, the existing JSON workload and a
   short supervised child before/after under matching conditions; report
   binary size and session baseline too. Commit raw measurements to
   rnx-bench and link them from this record's evidence. No numerical
   speedup or zero-cost claim precedes those measurements.

## Guardrails and risks

- There is one supervisor and no global directory or environment mutation.
- Validation refuses before spawn; it neither prints nor launches a child
  to discover whether options are valid.
- Old signatures and runtime messages stay compatible. No new dependency
  or process implementation is justified merely by the new namespace.
- Program lookup and child launch can block outside the deadline. Bare
  lookup is platform-shaped; an explicit path is the portable origin rule.
- Input is copied and output is bounded as before. This record does not
  turn the supervisor into an unbounded streaming or concurrency API.
- Windows gates require actual Windows execution. A cross-check or source
  review is recorded as such and cannot mark those gates passed.
- Stop implementation for review if keeping one supervisor requires
  weakening a cleanup invariant or silently changing an old call. New
  launch settings do not license a rewrite of records 0016–0023.

## Forward

The child-environment use case is addressed here. Shell syntax, pipelines,
spawned handles and asynchronous process calls remain separate decisions
if scripts need them. None is required to give a child a directory, input
and an environment today.
