# rnx-project

Local source packages and compiled native extensions for rnx. This is a separate
Cargo workspace; none of its dependencies enters stock rnx's default graph.

```sh
cargo build --locked --release --manifest-path tools/project/Cargo.toml --bin rnx-project
rnx-project lock --manifest app/rnx.toml --offline
rnx-project build --manifest app/rnx.toml --offline
rnx-project session --manifest app/rnx.toml
rnx-project eval --manifest app/rnx.toml -- '1 + 1'
rnx-project run --manifest app/rnx.toml -- argument1 argument2
rnx-project run --manifest app/rnx.toml --verify -- argument1 argument2
```

`--manifest` is required; there is no upward search. Paths inside a manifest are
relative to that manifest. Only lock resolves the Cargo graph. Build uses
`--locked`, the release profile and the compiler's host target; run never invokes
Cargo or rustc, builds anything, repairs a lock or requires a network connection.
**Launch checks your sources, trusts your build output unless you ask it to verify.**

By default, matching artifact size, modification time, executable-bit state and
Unix device/inode avoid rereading the executable. A mismatch triggers a full hash
against the known digest; matching contents refresh the receipt, different
contents refuse. `run --verify` always hashes the artifact. It is accepted once,
in either order with `--manifest`, before `--`; lock/build do not accept it.
Run passes everything after `--` as script arguments. Direct runner flags before
`--` are not exposed by this first project CLI. Build chatter goes to stderr.

## Open the project's prompt

After the explicit lock and build above, `session` opens the assembled executable's
REPL with its native extensions, including Polars. `eval` takes exactly one source
argument after `--`; quote it in your shell. Neither command runs the manifest's
entry file, builds anything, or imports mapped Rune packages. Module declarations
remain unavailable in eval and sessions. Source dependencies are still checked
for changes even though these modes do not use a source map.

```sh
rnx-project session --manifest app/rnx.toml --no-splash --color=never
rnx-project eval --verify --manifest app/rnx.toml -- '1 + 1'
```

Session and eval accept `--verify` and `--color=auto|always|never` once, before
any `--` boundary, in either order with `--manifest`. Only session accepts
`--no-splash`. Session takes no positional arguments or trailing `--`; eval takes
exactly one source argument, including an empty string. `--offline` belongs only
to lock/build. Run's existing script-argument boundary is unchanged.

All three launch modes share the receipt and source checks described below.
They inherit your working directory: relative CSV and Parquet paths refer to
where you launched the command, not the manifest directory. Session loads your
usual rnx settings; eval does not. The executable owns the terminal, history,
signals and exit status, with no proxy process. Reset clears bindings and retains
extensions. Use a frame's `preview()` for bounded display; bare frames remain
opaque. Opening a session releases the project command lock, and verification
runs once before launch, not again between inputs. Rebuilding a project does not
change an already-running session.

## Files you commit

The generated form writes exactly **rnx.lock** and **rnx.Cargo.lock** beside
rnx.toml. The former is bounded, deterministic, readable JSON; the latter is
Cargo's own lockfile. Source and native inventories are separate, with package
associations separated from shared tree hashes. No generated wrapper, receipt,
source map or executable is written beside the manifest.

Everything else is under `.rnx/`, which contains its own `.gitignore` with `*`.
That keeps it ignored without modifying your repository's .gitignore. It holds
assembly sources, Cargo's target cache, content-addressed executable artifacts
and maps, the command lock and receipt.json. An executable override writes only
rnx.lock and removes an obsolete generated-form rnx.Cargo.lock after publication.
An override's first run verifies its locked binary hash and establishes a private
receipt. Later runs use the same metadata/`--verify` policy; no Cargo is needed.

The command lock is an OS advisory file lock, released even if a process dies.
The two public files cannot be replaced with one filesystem rename. Publication
stages and syncs the bytes, replaces Cargo's file first and rnx.lock last; the
JSON lock commits the exact Cargo digest. Cooperating commands serialize. An
interruption between replacements can leave a mismatched pair, which build/run
**refuse**, rather than accepting mixed generations. Run lock explicitly to
recover. A normal publication failure attempts to restore the old JSON first and then
the old Cargo bytes. There is no claim of an atomic snapshot against an editor or git.

Build removes an old receipt before attempting work. After Cargo succeeds it
rechecks inputs and the lock, copies and verifies the executable, and publishes
receipt.json atomically. The version-2 receipt contains the exact project-lock digest, executable digest
and artifact metadata stamp. Build always fully hashes and verifies the installed
artifact before publishing. Failed/interrupted builds publish no receipt.
Run still rechecks the lock pair, declarations, source/native contents and
generated wrapper identity on every launch. Identical derived maps are compared
and reused; missing or different regular maps are published from the lock.

A valid old version-1 receipt is fully checked once and migrated before launch.
A missing generated-build receipt asks for build. An override without a receipt
can establish one by checking its already-locked digest. Malformed receipts
refuse; default run never makes changed contents trusted by changing the digest.
Receipt refresh is atomic and an interrupted or failed refresh does not publish
partial metadata. A harmless touch or identical replacement costs one full check,
then subsequent launches become fast again.

This is trusted local build output, not a content-authentication boundary. The
default can miss a same-size in-place edit if its mtime is restored or the
filesystem cannot distinguish the timestamps. Identity reuse or manipulated
metadata/receipts can also defeat metadata checks. Use `--verify` for a full
content check. Neither mode makes verify-then-execute atomic against concurrent
replacement. Source edits, including same-size/restored-mtime edits, still use
content fingerprints and are refused until the project is refreshed.

On the measured small Polars application, the old project launch took about
155 ms, the new default 29 ms, explicit `--verify` 95 ms, and generated-direct
11 ms. These are whole CLI runs on one pinned Linux host, not notebook timings
or universal latency bounds. Native input checks remain about 17 ms. Legacy
migration and metadata-mismatch refresh each took about 98 ms on this example;
they are separate first-use costs, not part of the warm default number.

## Inputs and build policy

Source trees include regular files recursively, excluding `.git` directories and
only the project's own outputs at the application root. Native path packages use
Git-tracked **working-tree bytes**, so dirty edits count. Missing tracked files,
untracked non-ignored files, submodules, symlinks, special files and non-Unicode
names refuse. Ancestor workspace, config and toolchain candidates are recorded,
including absence. Shared native roots are hashed once with each association kept.

A native rnx path dependency covers the whole tracked repository, including plans
and nested adapters. A plan edit therefore invalidates a project's build identity.
Narrowing that input set would be a separate root decision. A project whose
lockfiles are inside a native package root refuses: hashing a lock into its own
native input digest would be self-referential. Put the project outside that root;
the tool does not invent an exclusion from the native contract. A source root
containing the project similarly refuses unless it is the application entry root
where the explicit output exclusions apply. Moving a project
preserves content hashes where bytes and modes match, but absolute paths in the
map, inventories and generated assembly change: relock and rebuild explicitly.

Configuration includes stop the audit. Build-affecting Cargo config overrides
are refused: only http, net, registry, registries and term tables are allowed.
CARGO_* environment overrides except absolute CARGO_HOME, and RUST* overrides
except RUSTUP_HOME/RUSTUP_TOOLCHAIN, are refused by name without printing values.
Toolchain selection is recorded through actual rustc -Vv and cargo -V output;
build compares it with the lock. Run needs no installed compiler and validates
the provenance of its already-built artifact instead. Arbitrary trusted build
scripts/proc macros can still read ignored files, environment and external inputs.
This is stale-build detection, not hermetic or bit-reproducible compilation.

Manifests are capped at 1 MiB, JSON control files and captured tool output at
16 MiB. Fingerprinting has a shared per-snapshot allowance of 100,000 entries and
512 MiB; compilation retains its separate 8 MiB source allowance. Native Git
inventory output also has a 16 MiB cap. Tree hashes use the record's versioned
SHA-256 framing with sorted paths, content lengths, bytes and executable mode.

## Platform and validation

Linux commands are exercised. On Unix, Cargo owns a separate process group;
SIGINT/SIGTERM kill that group and reap the direct child before returning 130/143.
Run execs the verified executable, preserving its process status and signal
semantics. Trusted programs that deliberately leave the group are not contained.

**Windows project commands currently refuse at startup.** The code type-checks,
but owned Windows subprocess supervision is not implemented; this is stronger
than simply saying execution is unverified. No orphan-prevention claim is made
for Windows. The root project-source loader remains separately type-checked.

```sh
cargo test --locked --manifest-path tools/project/Cargo.toml -- --test-threads=1
cargo test --locked --manifest-path tools/project/Cargo.toml --features test-support -- --test-threads=1
cargo clippy --locked --manifest-path tools/project/Cargo.toml --all-targets --features test-support -- -D warnings
python3 tools/project/scripts/notices.py --check
```

Test-support adds pause points for publication and post-build edits; they are
absent from the ordinary executable. The assembly probe remains a test-only
adapter over private functions, not a second product CLI. Full workflow fixtures
are in rnx-bench/probes/project-workflow. No tool implementation module is public.
