# 0069 gate 2: the shared build directory in the product

Status: passes on Linux, ready for review; third revision after review. The product is `118864f` (the
gate 1 patch ported plus the shared directory, the coordination lock, the
executable scan, listing and removal, and the seed lock). Gates 3 and 4
remain. The probe is `rnx-bench/probes/shared-build-product/`; results in
`rnx-bench/results/shared-build-product-0069/`. Every build in it is the
stock `rnx` binary's own `project lock` and `project build` on Git-source
projects against a file-URL origin, with `shared_build = true` written by
hand in the declarations, since the catalogue writes it only after gate 3.

## What changed

All in `tools/project`:

- `cache_entry`: a declared assembly builds with `--config
  build.build-dir=<root>/build/<key>` (the key from its identity) while its
  `--target-dir` stays the entry's; the directory and `<root>/build` are
  created with the entry policy; the builder holds
  `<root>/locks/build-<key>.lock` *shared* from before Cargo until the ready
  document is renamed into place. After the build and before the artifact
  is copied, `references` scans the executable for the directory's canonical
  path and, if found, returns the refusal — nothing is copied, no ready
  document is written, the message names the assembly's natives. The lock
  lives in the existing private `locks/` directory rather than beside the
  directory as the plan provisionally wrote: outside the removable directory,
  as required, and where 0066 keeps every writer lock. The review's first
  finding — a pending removal renamed to a bare `<key>` lost its kind, so a
  bare resume deleted it under the entry lock while a builder held the build
  lock — is closed: the pending name is `removing/build-<key>`, listing
  prints it as `build-<key>` with build metadata, resume of `build-<key>`
  takes the build lock, and the bare spelling finds nothing to resume. The
  review's follow-up — resume still built `removing/<key>` for its
  containment check and report — is closed too: the path is built from the
  actual target component, so a resume run from inside the pending
  directory is refused, and a build directory's dry-run reports build
  metadata.
- `maintenance`: `rnx cache list` reports the `build/` location with
  `id: build-<key>`, size, and `referencing_entries` counted from local
  ready documents (scoped in the output). `rnx cache remove build-<key>`
  takes the coordination lock exclusively and refuses `busy` while a builder
  holds it; otherwise it follows 0066's path unchanged — preflight, dry-run,
  rename into `removing/<key>` with `renameat2(RENAME_NOREPLACE)`, sync,
  delete, `--resume`. `--quiescent` remains the acknowledgement.
- `git_sources` and `shared`: a project without a previous `rnx.Cargo.lock`
  resolves from the runtime's `Cargo.lock` (the Git checkout's after a first
  acquiring pass, or the path checkout's).
- The refusal of every user `CARGO_*` override and the 0067 configuration
  audit are untouched; the build-dir setting is passed on the tool's own
  command line.

## The matrix, through the product (`matrix.json`)

| case | lock | build | Compiling entries | kind | bare frame |
|---|---|---|---|---|---|
| a first Polars, declared | 4.9 s | 114.8 s | 308 | shared | presents |
| b same natives, plain wrapper | 5.1 s | **5.0 s** | **1** | shared | opaque |
| c Polars + PostgreSQL | 4.6 s | 39.9 s | 31 | shared; `postgres::query` exists | presents |
| d a runtime revision bump | 4.4 s | 9.3 s | 3 (`rnx`, `rnx-polars`, wrapper) | shared | presents |
| e the first declaration again | 4.2 s | 2.5 s | 0 (attached) | shared | presents |
| f undeclared control | 4.1 s | 113.5 s | 308 | private | presents |

Build times are the whole `project build`: verification, the coordination
lock, Cargo, the executable scan, the startup probe and publication. Every
executable's behaviour is verified through the notebook worker. The
revision bump adds only a directory to the tree, so its cost is the Git
package identities (`rnx`, `rnx-polars`) plus the wrapper; a change to the
runtime's sources would cost those crates' real compilation and their
dependents. All declared assemblies share one key on this machine. The
shared directory holds 2.0 GB after the matrix; each shared entry keeps
about 215 MB (its executable and final artifacts) against 1.7 GB for the
private control.

Seed lock: the first project's resolution shares 215 names with the
runtime's lock and matches 213 exact identities; the absent ones are the
management crate's (`sha2 0.10`, `digest 0.10`, `crypto-common`,
`block-buffer`, `cpufeatures`) and the runtime's own `rnx 0.0.0` record.

## The contract (`contract.json`)

- The 0061 retained-output native, declared `shared_build = true` at the
  origin's second revision: the executable references the directory, the
  tool refuses — "shared build refused: the executable references the shared
  build directory …; nothing was published … drop `shared_build = true` from
  its declaration (rnx-probe) and lock again". The entry the message names
  has no `ready.json`, no `ready.new` and no artifact, the project has no
  receipt, and a launch refuses and still writes none (the review's second
  finding: the earlier assertions were vacuous; these are exact).
- The same native undeclared: private, 40.7 s, and `probe::retained()`
  returns its value.
- Two builders on one directory at once, two new wrappers: both compile
  (3 and 1 entries), both publish, both behave as written; Cargo's build
  lock serializes their compilation under the shared coordination hold.
- A builder holding the lock: `rnx cache remove build-<key> --quiescent`
  refuses "busy: a builder holds the shared build directory build-<key> …
  wait for every build to finish, then retry"; the build completes and the
  directory stands. `rnx cache list` shows it with seven referencing
  entries and its size.
- An interrupted removal (a test-support build paused at `after-rename`,
  then killed): the pending directory is `removing/build-<key>`, listed as
  `build-<key>` with build metadata; `remove <key> --resume` (bare) finds
  nothing to resume; `remove build-<key> --resume` run from inside the
  pending directory is refused ("inside target"); its `--dry-run` reports
  `kind: build` (as does the live directory's); a runtime-only build at the
  same key creates a fresh
  `build/<key>`; `remove build-<key> --resume` while that builder holds the
  lock refuses `busy`; after the build, resume removes the pending
  directory (3,946 leaves, 2.6 GB) and leaves the rebuilt directory and its
  entry running (`42`).
- Overrides: `CARGO_BUILD_BUILD_DIR` in the environment is refused by
  name; a `[build] build-dir` key in Cargo home's configuration is refused
  by the 0067 audit.

## Checks (`checks.json`)

Root and tool formatting; tool strict clippy in both configurations; tool
suites 71 and 72 (new: the scan finds the directory's path and only that;
two builders hold the lock together and exclude an exclusive taker until the
last releases; the directory follows the recorded build kind and a retained
identity has none; `build-<key>` id syntax for the cache only); root suites
default 390, test-support 435, runner-only 403.

## Qualifications and next gate

Which shipped graphs trip the scan: neither Polars nor Polars + PostgreSQL
does on this toolchain (their executables published from shared builds);
the catalogue still writes `shared_build = false` until gate 3 runs the
lifetime sequence for a runtime-only executable, a Polars executable, a
PostgreSQL executable with a real query, and the combined one. Gate 3 also
exercises the removal semantics (a removal killed mid-way, a rebuild at the
same key, resume) and the retained-output consumer through later builds and
the same-revision environment-input rewrite. Costs are single runs on nano;
gate 4 samples. No Windows claim.
