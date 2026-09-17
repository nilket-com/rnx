# rnx-project (gate 4)

The independent project tool currently implements input validation and generation
as private library code exercised by tests. There is **no lock/build/run command
yet**, no Cargo build orchestration or build receipt. Those remain the later
gates of record 0057. Source fingerprints are now computed by the private core.

Implemented: bounded regular-file reads, strict TOML manifests, recursive graph
expansion, typed JSON lock documents and source maps, deterministic Cargo/main
wrapper generation, and a bounded capability exchange with an existing executable.
Syntax validation of a lock does not verify any recorded digest or artifact.

```sh
cargo test --locked --manifest-path tools/project/Cargo.toml
cargo clippy --locked --manifest-path tools/project/Cargo.toml --all-targets -- -D warnings
python3 tools/project/scripts/notices.py --check
```

The end-to-end handoff test is explicitly ignored in ordinary tool tests because
it needs a feature-enabled runner. Run from rnx, sequentially:

```sh
cargo build --locked --features project-sources
RNX_GATE2_BINARY="$PWD/target/debug/rnx" cargo test --locked --manifest-path tools/project/Cargo.toml manifest_to_runner_handoff -- --ignored --test-threads=1
```

It parses a real application and transitive source manifests, generates the map,
checks the actual runner's capability, and executes through `run --source-map`.
The runner inherits neither the manifest nor its map into any other context.

The one-second handshake owns one direct child, null stdin and capped stdout and
stderr (4096 bytes each). Both streams drain concurrently. Unsupported/malformed
replies, nonzero exit, any stderr, overflow or deadline expiry refuse naming the
executable, and refusal kills and reaps the direct child. This is trusted local
code, not containment for arbitrary descendants or a bound on OS spawn stalls.

TOML inputs are capped at 1 MiB; JSON documents at 16 MiB including escaping on
output. Each manifest permits at most 256 source and 256 native declarations.
The source graph remains 64 distinct manifests including the application, 256
mounts and depth 16. Native builder paths use ordinary ASCII Rust identifiers,
not expressions, raw identifiers, generics or crate-relative paths. Native module
names and source aliases are checked with pinned Rune's identifier parser.

Lock schema version 1 has declarations, an expanded source map, package file
inventories and a generated-or-executable identity. Serialization is deterministic;
root SHA-256 fields here are only syntax-checked. Toolchain versions are part of
the planned build identity, so upgrading the toolchain will require rebuilding.
Content inventories are computed separately from these staged wire documents;
writing and verifying a product lock/receipt remains the workflow gates.

Private modules retain staged unused-code allowances until the product commands
call them. Nothing here is a new supported public rnx Rust API. The resolved
all-platform dependency graph and notices are separate from stock rnx's graph;
Windows type-checking is not execution evidence.


## Content identity

The source inventory walks the application entry directory and each distinct
Rune source root, hashing working-tree bytes. External source manifests get their
own file identity. Only `.git` directories and `.rnx`, `rnx.lock` and
`rnx.Cargo.lock` at the application root are excluded. Files, not mtimes, decide
identity; symlinks, special files and non-Unicode names refuse.

The native inventory consumes bounded Cargo metadata for the generated assembly.
It retains each local package association and hashes each distinct package root
once, using Git-tracked working-tree files. Dirty edits count. Untracked,
non-ignored files, missing tracked files, unmerged index entries, submodules and
symlinks refuse. Ignored files remain outside this contract even if a trusted
build script reads them. Git inventory output has an additional 16 MiB cap.

Tree hashes use SHA-256 and `rnx-tree-v1` framing from the record: sorted UTF-8
slash paths, big-endian u64 path length, path, executable byte, big-endian u64
content length, contents. Unix executable bits count; other platforms use zero.
The inventory records OS and architecture. A shared allowance caps traversal at
100,000 entries (including walked source directories) and all fingerprint reads
at 512 MiB. Ancestor parsing rereads are charged too. Hashing streams through a
16 KiB buffer. Size changes detected during reading refuse; this is not an atomic
snapshot against a concurrent editor.

Ancestor Cargo.toml, both Cargo config spellings, and both toolchain spellings
are inventoried along package and invocation-directory ancestry. Cargo home's
config candidates and explicit `package.workspace` redirects are included.
Absent candidates are recorded so adding a file invalidates the inventory. This
is conservative: an inactive config spelling or ancestor may also invalidate it.
Cargo configuration follows the invocation directory, not each dependency root.
The workflow must retain that directory across lock/build checks.

A config containing `include` currently stops the audit with a named refusal:
this core does not claim to inventory its external include graph. It must not
proceed to a build. Build command/environment overrides and effective toolchain
identities still belong to the workflow gate; these file inventories alone are
not permission to build. Trusted build scripts, proc macros and compiler inputs
outside the declared trees remain the record's non-hermetic qualification.

The test-only repository audit writes a snapshot outside the repository, avoiding
an inventory that includes its own output. It is deliberately ignored in ordinary
tests because its native root must first have all new files tracked or staged.


A native package root covers every tracked file below that root, not only Rust
source files. For rnx as a path dependency that means the repository, including
plans and the nested adapter. Editing a plan therefore invalidates the build
identity. Narrowing the package input set would require a separate root decision.


## Generated assembly gate

With `test-support`, `rnx-project-assembly-probe` exposes only the private
assembly primitives to the integration fixture. It is **not** the planned
`lock/build/run` CLI. `prepare` validates a real manifest and source graph, then
writes Cargo.toml, main.rs and the handoff into a fresh directory directly below
`.rnx`; existing staging is never overwritten. The fixture lets Cargo resolve
from the accepted adapter lock, then builds with `--locked`.

The probe's `run` verifies an explicitly supplied executable digest before any
capability or script call. A nonempty map requires the capability handshake;
an empty map uses ordinary run so an older rnx-pg remains usable. On Unix it
execs the checked executable, preserving arguments, status and signal delivery.
On other platforms the test probe waits for the child; those paths have not been
executed and are not the product signal-handling contract.

The executable hash is a primitive, not a receipt. The probe accepts its expected
hash from the fixture. Published locks, build-environment policy, provenance,
pre/post checks and receipt validation remain gate 5. The fixture under
`rnx-bench/probes/package-assembly` exercises PostgreSQL on a private cluster,
a mapped Rune package, both builder kinds, arguments/status/interrupt cleanup,
prebuilt overrides and a native notebook across restart. It never grants source
loading to eval, sessions or workers.
