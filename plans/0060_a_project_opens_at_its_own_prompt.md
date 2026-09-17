# rnx 0060: a project opens at its own prompt

Status: proposed 2026-09-17. The sixtieth record follows the accepted Linux
closure of 0059. This is a draft for review, not implementation authority.

## Context

The Polars example builds a complete rnx executable containing its native
extension. That executable already supports run, eval, sessions and the worker.
The project tool exposes only run, so reaching the project's prompt requires
finding the content-addressed artifact and launching it by hand. The user's
next task is to keep a frame at that prompt and explore it across inputs.

0051 installs extension builders once per serving process. There is no supported
dynamic native-loading boundary. Rune being interpreted does not prohibit one;
designing one is simply outside this record. Opening an already-built project
session needs neither dynamic loading nor a restart of an existing session.

0057 supplies mapped Rune sources only to run. Sessions and eval refuse modules.
A project prompt therefore exposes its native extensions, not its mapped source
packages. The manifest entry is not a startup script and must not be evaluated
to create the prompt.

0059 made normal launch check source contents and trust a fully established
artifact metadata stamp, with --verify forcing a full artifact hash. Its measured
Polars pipeline launch is about 29 ms versus 11 ms direct on one host. These are
whole script timings, not a prediction of interactive readiness. The same launch
policy belongs to all project execution modes.

## Decisions

### 1. Two commands, with explicit argument boundaries

Add:

```sh
rnx-project session --manifest app/rnx.toml
rnx-project session --manifest app/rnx.toml --no-splash --color=never
rnx-project eval --manifest app/rnx.toml -- 'polars::lit(1)'
rnx-project eval --verify --manifest app/rnx.toml -- '1 + 1'
```

--manifest remains required, once, with no upward search. --verify is optional
once for run, session and eval, in either order with --manifest. Lock/build still
refuse it. Session and eval never resolve, build, invoke Cargo/rustc, repair public
locks or require network access. Missing/stale inputs give the existing recovery
instructions rather than triggering work implicitly.

Session accepts no positional arguments or trailing --. Eval requires -- followed
by exactly one source argument, including when that argument begins with a dash.
An empty string is source and reaches rnx unchanged; missing or multiple source
arguments refuse before project work. Do not join arguments into source, invoke
a shell, or turn an expression into a temporary file. Preserve rnx's existing
Unicode argument validation rather than adding lossy conversions.

Both new commands accept --color=auto|always|never once before the boundary.
Session additionally accepts --no-splash once. Duplicates, unknown options,
invalid colours, --offline and eval's --no-splash refuse. Translate presentation
flags to the leading position the existing rnx CLI expects, followed by repl or
eval and its source. Default colour/splash behaviour remains rnx's own. Do not
expand run's flag surface in this record: its existing -- boundary still forwards
script arguments verbatim, including an argument named --verify.

### 2. One checked artifact path for all three launch modes

Refactor the private project launch path so run, session and eval share lock-pair,
input, generated-wrapper, receipt and artifact verification. Command construction
must continue to require the checked-artifact value; no unchecked alternative or
new public tool API. Generated and executable-override projects both work.

All 0059 rules remain: matching stamps avoid payload reads, mismatch hashes
against the recorded digest, refresh happens only on a match, --verify always
hashes, valid legacy receipts migrate, malformed receipts refuse, and missing
generated receipts require build. Overrides establish a stamp from the locked
digest when permitted by 0059. Publication failures refuse the launch. Source
contents, including mapped sources unused by this execution mode, are still
checked: the command opens the declared project, not a reduced identity of it.

Run alone creates/validates its derived source map and performs the existing
map capability handshake at launch. Session and eval neither create nor read
derived maps, perform that launch handshake, nor pass --source-map or the entry
file. They still validate the public lock and source inventory. A broken unused
derived map cannot prevent these two modes; a stale source dependency must.
Lock-time capability requirements for an override with mounts remain unchanged.

No manifest, public lock, private receipt or generated wrapper format changes.
No dependency changes are needed. This refactor must preserve run's output,
arguments, source-map validation and refusal behaviour.

### 3. The assembled executable owns the session

Use the existing Unix exec boundary with inherited stdin, stdout, stderr and
environment. Retain the caller's working directory, as project run currently
does; --manifest selects the project, not a directory to chdir into. Document
that relative CSV/Parquet paths are relative to that working directory.

Release the advisory command lock through its existing close-on-exec behaviour.
The interactive process must not hold the project writer lock for its lifetime.
Do not introduce a parent process, stream proxy or terminal emulation. The
executable owns line editing, history, splash, title restoration, signals,
budgets, rendering, reset and exit status exactly as in a direct launch.

Session loads the ordinary session configuration; eval does not. Extensions
remain available after reset, with bindings cleared and builders not rerun.
Compile/runtime refusals and catchable native errors keep their existing meaning.
This record does not fix the known top-level ? presentation diagnostic or add
automatic DataFrame rendering: use the adapter's bounded preview.

Verification happens before exec. After opening a session there is no periodic
source revalidation, rebuild or extension replacement. A later project edit does
not mutate the running process. A new launch verifies the then-current project.
Windows project commands retain their explicit refusal until supervision exists;
type-checking is not execution evidence.

## Gates and stop points

1. Product CLI parsing and actual argv: both orders of manifest/verify, presentation
   flags, empty and multiline source, non-ASCII text, dash-prefixed source,
   missing/extra source and all duplicate/unknown options. Refusals before project
   work use a nonexistent manifest as a control. Compare eval output/status with
   direct execution for success, compile failure, native error and explicit exit.
   Keep run's existing argument-forwarding checks, including --verify after --.
2. Generated and override launches use the same verification path. Test-only read
   counters with positive full-read controls prove matching-stamp zero reads and
   --verify full reads in both new modes. Exercise legacy migration, missing and
   malformed receipt, stale lock, touch, differing replacement and source edits
   with restored mtime. Reuse the inert 0059 corruption gate; never execute a
   deliberately corrupted artifact. Failed checks start no user input. A PATH
   trap proves launch invokes neither Cargo nor rustc, with required Git available.
3. Source scope: a valid project with mapped sources opens a session and evaluates
   expressions without a source map. A missing or malformed derived map is left
   untouched. Editing a mapped source still refuses launch. Module declarations
   still refuse in eval/session; the entry file has an observable side effect
   which neither new command executes. From another directory, script filesystem
   access proves the inherited working directory. An open session does not keep
   the project advisory lock held; inspect/acquire that lock without modifying
   the live project's build or inputs.
4. Real user journey through the ordinary product binary and the Polars example:
   lock/build explicitly, then use a pseudo-terminal to open its project session.
   Create a tiny CSV in a temporary working directory, bind a frame, filter and
   aggregate across inputs, preview, provoke a missing-column error, reuse the
   original frame, write Parquet and verify the rows. Reset clears bindings but
   leaves the extension usable. Quit and EOF close cleanly; prompt Ctrl-C retains
   the existing session behaviour. Compare against direct artifact behaviour.
   Keep history/config/output fixtures isolated from the user's home. Also test
   piped session input. Extension-builder counters, where needed, belong only in
   the separate test-support fixture, not the measured ordinary executable.
5. Measure the user boundary: project eval versus direct artifact eval, and
   project session spawn-to-first-prompt versus direct session with identical
   config, splash and terminal settings. Use two interleaved repeats with twenty
   samples each, all retained, one pinned CPU and Polars thread. Build and initial
   receipt establishment are outside unchanged-launch timings and labelled.
   Report --verify separately. Require default median overhead within 25 ms of
   direct in both repeats on the existing host; if it misses, preserve evidence
   and return to review without weakening checks. Do not call prompt readiness
   a pipeline time or attribute input latency to launch verification.
6. Tool suites in both configurations, formatting, strict all-targets Clippy and
   notices pass. Rerun existing workflow and 0059 coverage fixtures to establish
   run compatibility. Type-check Windows separately. Root source, public API,
   dependency graph, kernel and adapters stay unchanged. Update tool help/README
   and the bench example with the commands a user can copy. Run packaged README
   checks if root README links change; no unrelated root tests or performance
   work are required for an isolated tool change.

## Guardrails and later work

No :dep, scratch-project generation, adapter registry, implicit build, live
session replacement, binding replay, dynamic library loading, mapped-source
imports at the prompt, notebook installation command or automatic frame display.
Those are separate user workflows and contracts. Native inventory caching and
context reuse remain outside this record.

The first review point is this draft. Implementation should keep a single shared
verification path and the existing CLI engine, with the fewest commits that
preserve meaningful measured checkpoints. Published measurement baselines stay.
