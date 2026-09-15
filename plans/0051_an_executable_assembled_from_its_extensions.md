# rnx 0051: an executable assembled from its extensions

Status: implemented 2026-09-15, drafted by Claude; revised the same day after
Codex's review (name before builder, trusted adapters rather than structural
namespace confinement, per-thread panic conversion, settings ordering,
exit contract, allocator baseline gate). Implementation and evidence by Codex.
The fifty-first record is step two of the extensibility
sequence agreed on 2026-09-15: a small supported library interface through
which an application executable assembles rnx with external native modules.
It adds no package discovery, no plugin loading and no database. See the
companion evidence for measurements and platform limits.

## Context

At `b775a40`, rnx is a binary-only crate. `main.rs` parses version, help
and the worker argument, loads settings, builds one `rune::Context` with
`install_core`, `fs`, `path`, `time`, `text`, `http` and, outside `run`,
`env`, and hands that single context to whichever entry point the
arguments select: the worker, `eval`, the session or `run`. A session's
`:reset` clears bindings and state and keeps its context, so whatever the
context holds at construction is present for the process's whole life. The
pure settings evaluator (`config.rs`) builds its own bare context and never
touches these installers. Version and help exit before any context exists.

Every battery is a Rune native module installed by Rust code the binary
owns. A Rust crate outside this repository, a PostgreSQL adapter for
instance, can build a `rune::Module` in the same way but has no way to put
it into an rnx context, because nothing of rnx is a library. Record 0049
left `install_core` and the battery installers crate-private on purpose:
making them public would turn today's internals into a promise.

Three facts shape the boundary. First, `rune::Module` is a Rune type, so
any interface that touches one exposes Rune's version; Cargo will resolve
incompatible Rune versions side by side if an adapter names its own, with a
type error where the two `Module` types meet. Conflicting exact pins within
a compatible release line can instead fail dependency resolution. Second, Rune 0.14.2
gives a host no public way to enumerate what a module or a context
contains: `Context::iter_functions`, `contains_crate` and the module's item
are all crate-private. A label an adapter attaches to a module it built
cannot be checked against the module's real namespace, and a module built
with `Module::with_crate("fs")` would install into the battery. Third,
help and completion are driven by rnx's own inventory (`host::HostFunction`
path and doc strings collected from each installer), so an extension is
invisible to `:help` unless it describes itself, and a described path is
only as true as the namespace check behind it.

Startup is the measured property this project defends. The single-file
comparison cases and the matched hyperfine procedure from records 0049 and
0050 exist to prove that a change to how the executable is put together
costs nothing a person can see or time.

## Decision

### 1. rnx becomes a library with one execution entry point

The crate gains a library target beside the binary. The library's public
surface is exactly:

- `rnx::main_with(extensions) -> Result<(), Box<dyn std::error::Error>>`,
  the whole of today's `main` behind one function with today's signature:
  argument dispatch, settings, context construction, the worker, `eval`,
  the session, `run`, `selfcheck`. It returns the way `main` returns today,
  so the ordinary stack unwinds, session resources and terminal guards
  drop, and a propagated error reaches Rust's `Termination` reporting with
  the same bytes. Explicit exits inside dispatch (`process::exit`, the
  worker's failure exit, terminal exit) stay where they are. Today's `rnx`
  binary becomes `fn main() -> Result<(), Box<dyn Error>> { rnx::main_with(rnx::Extensions::none()) }`.
- `pub use rune;`, the pinned Rune crate re-exported, so an adapter writes
  `rnx::rune::Module` and cannot compile against a different Rune.
- `Extensions`, the type of decision 2.

Nothing else is public: not the context builder, not the batteries, not the
session, runner or worker. A later record may widen this; this one may not.
The library declares the counting allocator behind `count-allocations`
exactly as the binary does today, so an application executable built on
the library has the same allocation ceiling and reporting, and both
features (`count-allocations`, `test-support`) live on the library with the
same defaults and meanings.

### 2. Adapters are trusted; rnx checks what it can

```rust
pub struct Extensions { /* private */ }
impl Extensions {
    pub fn none() -> Self;
    /// `name` is the Rune crate the extension owns. `build` receives a module
    /// rnx created with `Module::with_crate(name)` and returns help entries.
    pub fn with(
        self,
        name: &'static str,
        build: impl FnOnce(&mut rnx::rune::Module) -> Result<Vec<(String, &'static str)>, String> + 'static,
    ) -> Self;
}
```

An adapter is Rust code running inside the process with everything that
implies; rnx does not pretend to confine it. Two designs were considered
and rejected before this one. Lending the module does not fix its
namespace, because `*module = Module::with_crate("fs")?` is safe Rust. A
registrar that forwards a subset of Rune's registration methods does not
either: `ty::<T>()` calls the public `InstallWith::install_with(&mut
Module)` on the adapter's own type, which is exactly the reference the
registrar meant to hide, and omitting `ty` would leave native types for a
later design while maintaining a parallel registration API. Rune 0.14.2
also gives a host no public way to enumerate what a module contains, so
no after-the-fact check is possible. The honest contract is therefore:
the adapter owns its namespace and is responsible for registering only
under `name::`; rnx creates the module under the declared name, checks
the declared name, the help paths and installation errors, and states in
the README that these checks catch mistakes, not misbehaviour. Rune's
full registration API stays available to adapters.

The name is registered before any builder runs, so a builder that returns
`Err` or panics is still attributed. `'static` is required of the closure
because it is stored until the context is built; `Send` is not, because
builders run on the main thread before the session, runner or worker
starts.

Before a builder runs, rnx refuses a name that is empty, is not a Rune
identifier, is `std` or a battery crate (`json`, `io`, `process`, `fs`,
`path`, `time`, `text`, `http`, `env`, `rnx_test`), or repeats an earlier
extension's name. Builders run after rnx's own batteries, in the order
given, each at most once per process. A builder that returns `Err`, a
module that fails to install, or a help path that does not begin with
`name::` refuses startup with:

    error: extension `postgres` could not be installed: <reason>

and exit status 1, before any prompt, script or worker protocol begins.

Panics are a stated policy, not a side effect. Each builder runs under
`catch_unwind`. For its duration rnx installs a panic hook that, for the
builder's own thread only, records the message and prints nothing, and for
every other thread calls the previously installed hook unchanged; the
previous hook is restored immediately after. Only a panic unwinding
through the builder call is converted, into the refusal above reading
`panicked: <message>`, the sole diagnostic. A panic on a thread the builder
spawned, an abort, a `panic = "abort"` build of the adapter, or an
explicit `process::exit` inside a builder is not converted, and the README
says so.

What Rune's registration APIs do with paths is recorded, not promised.
They accept paths: a
function registered under a multi-segment name, or a type whose `item`
attribute is absolute, might place an item outside `name::`. The
implementation first probes each such attempt against the pinned Rune and
records what lands where, so the README can tell adapter authors which
registrations stay under `name::` and which do not. The help path check is
the only namespace check rnx can perform, so it is strict: every entry
starts with `name::` and names a single item.

### 3. Where the hook applies

Extensions are installed wherever rnx builds its serving context: the
worker, `eval`, the session (and therefore after `:reset`, which keeps the
context) and `run`. `env`'s run-time snapshot behaviour is unchanged.
Version and help exit before the builders are called; the builders are not
called for `selfcheck` either, which checks rnx's own invariants. The pure
settings evaluator never sees extensions: it keeps its bare context and its
no-op source loader. The record states this as a guarantee an adapter can
rely on: settings are loaded first and cannot resolve an extension
function, then every builder runs exactly once, then user input is read;
`:reset` keeps the context and runs no builder again.

The assembled executable is a worker. The existing rnx-jupyter kernel
launches whatever executable its kernelspec names through `--rnx`, checks
the worker's ready message for protocol 1 and the pinned versions, and
speaks the 0046 protocol; an application executable answers identically
because it is rnx. Installing that executable as a kernel is the existing
`rnx-jupyter install --rnx /path/to/app`. The kernel package does not
change in this record.

### 4. What stays the same

The `rnx` binary's behaviour is unchanged in every case the single-file
comparison suite covers, byte for byte on stdout, stderr and status. Its
startup is unchanged within the matched procedure's drift. No new
dependency enters either the library or the binary. The dependency graph
of an application executable is the adapter's business; rnx's notices
describe rnx.

### 5. The fixture adapter

A crate `rnx-extension-fixture` under rnx-bench, outside rnx's workspace,
depends on rnx by path and on nothing else, and builds two executables:

- `app`: `rnx::main_with(Extensions::none().with("fixture", fixture::build))`, where
  the module `fixture` registers `fixture::answer() -> 42`, an async
  `fixture::later(ms)` that sleeps and returns `ms`, and a
  `fixture::fail() -> Result` that returns `Err("fixture failure")`, each
  with a help string.
- `broken`: the same plus a second extension `sibling` whose builder
  returns `Err("deliberately broken")`; variants selected by an environment
  variable the fixture alone reads register an extension named `json`, one
  named `std`, one named `fixture` twice, one whose builder panics, one
  whose help path is `other::thing`, and the namespace-escape attempts of
  decision 2.
- The builder appends one line to the file named by `RNX_FIXTURE_MARKER` when it
  runs, so call counts are observable from outside.

The fixture proves the contract; it is not shipped and not installed
anywhere but the bench's temporary directories.

## Acceptance gates

1. **Stock behaviour through the library.** The `rnx` binary built from the
   library passes all root suites and the single-file comparison cases byte
   for byte against the accepted binary: the twelve existing cases plus
   `eval` with no source, `run` with no path, `run` with a missing file, and
   `--budget` with a non-number, zero and `usize::MAX`, so the exit contract
   of decision 1 is proved on the paths that return an error rather than
   exit. `app` passes the same cases with identical output, since a script
   that uses no extension cannot tell. Matched hyperfine on version,
   `eval 42`, bare run and the JSON workload for the accepted binary, the
   new `rnx` and `app`, with hashes and drift stated; no speedup is claimed.
2. **The extension is present everywhere.** Through `app`: `run` of a file
   calling `fixture::answer()`; `eval`; a session before and after
   `:reset`, including `:help fixture::answer` and completion of
   `fixture::`; `fixture::later(20).await?` returning 20 under the async
   foundation; `fixture::fail()?` reported as an ordinary script error
   with the fixture's message. The stock `rnx` refuses `fixture::answer`
   as a missing item.
3. **Refusals name the extension.** `broken` exits 1 with the message of
   decision 2 naming `sibling` and its reason, prints no prompt and no
   splash, and `broken worker --control-read N --control-write N` exits 1
   before sending `ready`. The `json`, `std`, duplicate `fixture` and
   `other::thing` variants refuse the same way naming the extension and the
   reason, before their builders run where the check precedes the builder.
   The panicking builder produces exactly one diagnostic, the refusal, and
   no Rust panic text on stderr. Each namespace-escape attempt is recorded
   with its outcome in the evidence and the README, as adapter guidance.
   A builder that replaces its module with `Module::with_crate("fs")` is
   run once to show the observed consequence, which the README states is
   the adapter's responsibility to avoid.
4. **Ordering, not absence.** With `RNX_FIXTURE_MARKER` set: `app version`,
   `app --help` and `app selfcheck` leave no line; `app eval 1` leaves
   exactly one; a session that starts, evaluates `fixture::answer()`,
   runs `:reset`, evaluates it again and quits leaves exactly one; a
   settings file that references `fixture::answer` is refused by the pure
   evaluator with today's wording and the prompt still appears, and the
   marker shows the builder ran after that refusal, once. The config
   evaluator's unit test asserts extensions are not reachable from it.
4b. **The allocator is inherited.** Under `test-support`, the evidence
   first records the baseline difference between `app` and `rnx` in
   `rnx_test::test_allocation_peak` for an empty input, since extension
   modules, help entries and stored closures allocate what stock rnx does
   not. Then a known allocation (a script building a 1 MiB string) raises
   both peaks by the same amount (the fixture states a 4 KiB tolerance for
   surrounding evaluation allocations), and the existing over-ceiling fixture
   is refused by `app` with the existing `over_ceiling` wire category and
   the same message template. The live-byte count in that message is each
   process's measured count, not a requirement that different baselines match. Without
   `count-allocations`, `app` says so in the same words `rnx` does. The
   README states that an application cannot declare its own global
   allocator while rnx's counting feature is enabled, because Rust admits
   one per executable.
5. **The notebook route.** `rnx-jupyter install --rnx <app>` into a
   temporary Jupyter data directory, then jupyter_client executes
   `fixture::answer()`, restarts, executes it again, and after a worker
   `:reset`-equivalent (a new kernel) the binding is fresh but the
   extension is present. The kernel's ready check passes unchanged. The
   saved notebook validates.
6. **The boundary is the boundary.** A compile-time test in the fixture
   crate confirms that only `main_with`, `Extensions` and `rune` are nameable
   from `rnx`; the `cargo doc` item list is recorded in
   the evidence. An adapter that depends on a different `rune` version
   from an incompatible release line fails to build with a type mismatch at
   the builder's `&mut Module` parameter. The evidence also records the
   resolution failure for conflicting compatible exact pins.
7. **Regression and checks.** Both root suites, formatting, clippy at the
   inherited baseline, notices unchanged, kernel suites unchanged. Attempt the Windows type-check of the library and binary. If the existing
   native-toolchain blocker persists, record it and type-check the changed
   extension assembly code separately; neither is Windows execution.

## Guardrails and stop conditions

- Nothing beyond the three names is `pub` from the library. Stop if an entry
  point cannot be moved behind `main_with` without exposing more.
- A declared-name collision is refused before the builder runs. rnx
  claims no check beyond the declared name, the help paths and Rune's own
  installation errors; the README says so in those words.
- The settings evaluator, version and help must not construct extensions.
  Stop if the dispatch cannot keep that order.
- Stop if the library split changes any byte of the twelve comparison
  cases or moves startup beyond the matched procedure's drift.
- The fixture crate and any future adapter stay outside rnx's workspace
  and lockfile; rnx's dependency graph is unchanged by this record.

## Risks and forward

A builder runs arbitrary adapter code before the prompt, so a slow
extension is a slow startup; the record measures the fixture, not a real
adapter, and step three will measure PostgreSQL's cost honestly. The
re-exported Rune fixes adapters to rnx's pin, which is the point, and means
an rnx Rune upgrade is an adapter rebuild. The help inventory trusts an
extension's own doc strings. Step three, the PostgreSQL adapter, follows:
it supplies a name and a builder and nothing else.
