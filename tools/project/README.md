# rnx-project

Local source packages and compiled native extensions for rnx. This is a separate
Cargo workspace; none of its dependencies enters stock rnx's default graph.

```sh
cargo build --locked --release --manifest-path tools/project/Cargo.toml --bin rnx-project
rnx-project lock --manifest app/rnx.toml --offline
rnx-project build --manifest app/rnx.toml --offline
rnx-project run --manifest app/rnx.toml -- argument1 argument2
```

`--manifest` is required; there is no upward search. Paths inside a manifest are
relative to that manifest. Only lock resolves the Cargo graph. Build uses
`--locked`, the release profile and the compiler's host target; run never invokes
Cargo or rustc, builds anything, repairs a lock or requires a network connection.
Run passes everything after `--` as script arguments. Direct runner flags before
`--` are not exposed by this first project CLI. Build chatter goes to stderr.

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
Override run verifies its locked binary hash and needs no receipt or Cargo.

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
receipt.json atomically. The receipt contains its version, the exact project-lock
digest and the executable digest. Failed/interrupted builds publish no receipt.
Run rechecks the lock pair, declarations, source and native contents, generated
wrapper identity, receipt and executable before launch. Identical derived source
maps are compared and reused; missing or different regular maps are published
from the lock. Executable verification still reads and hashes the whole file.
The first 0059 optimisation reduced the measured tiny Polars project launch from
about 155 ms to 95 ms while retaining full hashing; the faster metadata default
is not implemented in this stage. These are trusted local
projects; verify-then-execute is not atomic against hostile replacement.

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
