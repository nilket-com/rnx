# Cross-repository correctness verification

The launcher builds a dev-only Rust controller against rnx's locked Rune source
without changing the production pin or active lock. Rust performs live stock
and execution observations, classifications, comparisons and source/binary checks.

```
python3 tools/verify.py cross --suite all
```

Before launching, the runner checks every live product Cargo.lock, fork-Git
manifest pin, and both RUNE_VERSION constants against the root Rune revision.
It records the checked paths in `PIN-CONSISTENCY.json`. Historical `probes/`
graphs keep their independent source pins; generated `target/` files are excluded.
Run the guard's live-tree and drift controls with:

```
python3 -I -B tools/test_verify_pins.py
```

By default Cargo resolves the pinned Rune Git dependency, checked-in fixtures are
used, and output goes to a new directory under `target/rnx-verify`. For an explicit
cross-revision check, use `--rune-rev REV --rune-repo /absolute/rune-git`, or use
`--rune-path /absolute/rune` for an existing tree. `--fixtures-root` and `--out`
override the defaults. Every output directory must be new. Failed attempts retain their
raw files. The default eight build jobs are an allocation, not a scheduler:
callers must keep concurrent allocations within the correctness host's budget.

The default compiles the current repository's real registration. An explicit
`--registration-fixture /absolute/rnx-control-checkout` compiles that complete
control checkout and labels its receipts as a registration control. It must
contain the same checked-in verifier sources as this checkout. Registration
pins still apply; a changed registration cannot pass as equivalent. There is
no automatic copied-harness fallback.

This runner requires Linux, bubblewrap (`bwrap`), GNU `timeout`, `sha256sum`,
`readelf`, Git, Python 3.12 or later, Rust 1.98.1, and already available offline
Cargo dependencies. The runner records the bubblewrap version and fails if it
cannot create the required private namespace. Enrollment gives every child a
private filesystem tree at the historical logical path using a bind mount.
It never falls back to the host's historical tree.

The checked-in `tools/verify/fixtures` tree contains the census and enrollment
inputs and policy data; no external stock-observation archive is required. The
controller compares full compile-warning multisets for every admitted census
program, preserving source identity, spans, messages, auxiliary context and
multiplicity. It runs stock and execution children for each enrollment row. It generates
large repeat-byte stdin and filesystem content from checked-in specifications and
verifies their digests.

The controller and observer roles use the same dev executable and report that
identity explicitly. Its source is the actual `src/lib.rs`, compiled with
`cfg(test)` through a harness-free target defined only in the private manifest.
The normal Cargo manifest and lock remain unchanged. No observer API is added
to production rnx. The correctness runner does not measure performance; V1c
owns end-to-end timing acceptance and scheduling.

Cargo metadata and compilation run inside the private build namespace. The Rust
controller runs outside it and validates Cargo's complete artifact stream before
observing anything. Each enrollment child then uses one bubblewrap namespace;
the runner does not require nested user namespaces. Final acceptance is the
Rust-written `COMPLETE.json`, after suite, source, binary and host-file checks.
