# Cross-repository correctness verification

The launcher builds a dev-only Rust controller against a selected Rune source
without changing rnx's production Rune pin or active lock. Rust performs the
observations, classifications, comparisons and source/binary checks.

```
python3 tools/verify.py cross --suite all \
  --rune-rev c7aee7e072dd84604fe41c6d94270942e49da4c8 \
  --fixtures-root /absolute/fixtures --out /absolute/new-output
```

Use `--rune-path /absolute/rune` instead of `--rune-rev` for an existing tree.
Revision exports use `../rune` by default; `--rune-repo` selects another local Git
repository. Every output directory must be new. Failed attempts retain their
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

Copy the checked-in `tools/verify/fixtures` tree to the chosen fixtures root.
Census inputs and per-case oracles are all checked in; no file exceeds 1 MB.
Enrollment additionally requires this frozen external artifact:

| Relative path | Bytes | SHA-256 |
|---|---:|---|
| `enrollment/stock/OBSERVATIONS.json` | 68,373,788 | `bc12127e43b753e15fe7402524a781e3c59dab8d218d42c619a231e0aeed9a05` |

The artifact is the preserved 0192 enrollment stock receipt, originally
`0192-rnx-enrollment4/out/stock/OBSERVATIONS.json` in the review archive.
Obtain those exact bytes from that archive's custodian and place them at the
indexed path. The Rust controller checks both size and digest before observing
anything. It generates the large repeat-byte stdin and filesystem content from
checked-in specifications and verifies their original digests.

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
