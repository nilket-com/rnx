# rnx-project (gate 2)

The independent project tool currently implements input validation and generation
as private library code exercised by tests. There is **no lock/build/run command
yet**, no source fingerprinting, Cargo build orchestration or build receipt.
These remain the later gates of record 0057.

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
Content comparisons and ancestor Cargo-input accounting remain gate 3.

Private modules retain staged unused-code allowances until the product commands
call them. Nothing here is a new supported public rnx Rust API. The resolved
all-platform dependency graph and notices are separate from stock rnx's graph;
Windows type-checking is not execution evidence.
