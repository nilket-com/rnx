# rnx 0061: gate 1 cache context and ownership evidence

Status: ready for Linux review, 2026-09-17. Plan 3c27365 incorporates the accepted
root-symlink clarification. Gates 2–6 remain open; no product cache is implemented.
The prototype and results are in rnx-bench `probes/cache-context/` and
`results/cache-context-0061/`.

## Boundary measured

An isolated tool copy imports the existing manifest parser, generator, bounded
fingerprinter, Cargo/native inventory and serializers unchanged. A small Rust
prototype canonicalizes declared native roots, guards managed directory/input
paths and applies the existing configuration allowlist. Python orchestrates real
Cargo and constructs a diagnostic identity from the resulting records. That
identity is a gate-one fixture, not the final product lock/key implementation.
The imported-source manifest records hashes against 3c27365, and the build driver
asserts byte equality. The dependency lock is unchanged; the isolated manifest
adds only its probe binary. Root source, manifest, lockfile, notices and every
application package remain unchanged.

The cache root has an allowed offline Cargo config and an installed toolchain
selection. A private Cargo home contains its own allowed configuration. The user
selects the root through a symlink; the Rust boundary canonicalizes it, then
checks managed paths below it. Native package locations are retained. Projects
contain different application text but the same native declarations.

Resolution occurs in cache/resolve/one/assembly. A preflight inventory checks the
cache Cargo context before metadata. The resolved inventory includes native
package ancestry. The canonical wrapper, Cargo lock, inventory, versions and
context give a key; final metadata/build use cache/entries/<key>/assembly and its
sibling target directory. The identity contains neither the scratch path nor its
own resulting key/final entry path. A second consumer resolves independently in
a second temporary workspace and produces the same inputs.

## Observations

- Temporary resolution, final metadata and post-build inventory agree exactly,
  as do Cargo/rustc and selected toolchain observations. Canonical wrapper/main
  bytes and locked graph match at both positions and for the second consumer.
- Strace records Cargo opening the same cache-root and Cargo-home configuration
  files. A deliberately unavailable dependency produces an offline failure at
  both positions without passing --offline. This proves the configured setting
  is acted on; the ordinary fixture has only local dependencies and downloads
  nothing. The control never attempts online resolution.
- The native adapter writes a file to OUT_DIR during compilation and reads that
  exact file at runtime. Two consumers return the same native manifest directory,
  retained cache path and value. Removing temporary resolver directories does
  not change their output. Removing only the retained file makes execution fail;
  restoring it restores the original output. The final build directory never
  moves, which closes the accepted probe's OUT_DIR counterexample for this shape.
- Eighteen late candidate injections cover managed ancestor/stage Cargo and
  toolchain files plus project-local settings outside the cache search chain.
  They refuse before Cargo. Changing an audited allowed config or creating a
  previously absent candidate changes inventory; an unsupported build key is
  also refused by preflight before resolution.
- A user symlink selecting the root succeeds. Symlinks in managed directory and
  generated-input positions refuse, as do a FIFO directory candidate and a FIFO
  config candidate without blocking. These are trusted local path checks, not
  a race-free filesystem snapshot or a sandbox against arbitrary native code.

The fixture executable is 474896 bytes in the measured build; the exact retained
entry byte count is in observations.json. This is a tiny wrapper-compatible
Rust fixture, not Polars and not Rune execution. Gate 5 still owes actual Polars
assembly-plus-target disk bytes and separate full artifact-hash attachment cost.
No cache-hit timing is inferred from these fixture observations.

## Validation, scope and next step

The isolated prototype formats and passes strict Clippy with warnings denied.
All final behavioural controls pass; commands, syscall traces, bounded inventory
records, diagnostic identity, generated and native sources, configuration and
output are archived. Fixture subprocesses complete before temporary directories
are removed. No user's cache, Cargo config, history, database or kernelspec is
modified. Whole process-group interruption/publication is gate 3, not proven by
this controlled gate-one run.

The result supports proceeding to the production cache boundary with the selected
build location and retained entry lifetime. It does not implement shared readiness,
locking, project attachment, migration, eviction or startup probing. No gate is
waived. Gates 2–6 must still establish the production identity matrix, concurrency
and failure policy, legacy/receipt behaviour, real two-project Polars costs and
regressions. Windows is not executed by this prototype.
