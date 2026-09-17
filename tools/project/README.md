# rnx-project (gate 1)

Record 0057's internal source-graph expansion is implemented here. There is no
command-line executable, manifest parser, lock/build/run workflow or source-tree
fingerprinter yet. This independent workspace currently has no dependencies.

Run `cargo test --locked --manifest-path tools/project/Cargo.toml` from rnx.
The tests supply parsed graph nodes; they do not claim to exercise TOML input.
Expansion canonicalizes manifest paths, caches each manifest once, detects cycles
on the active ancestry, and emits a mount per importing edge. A diamond produces
two prefixes. Bounds are 64 distinct manifests including the application, 256
expanded mounts and 16 dependency edges of depth. Every limit refusal names the
edge, and the next manifest is not read after the limit is reached.

The graph is private implementation, not another supported rnx library interface.
The forthcoming parser must validate aliases and bound manifest reads before
constructing these nodes. Until it exists the unused-code allowance identifies
this staged core explicitly. Root loader tests live in src/program/project_tests.rs
and run with the project's `project-sources` feature.
