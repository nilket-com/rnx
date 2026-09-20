# 0069 probe: which compilation can a shared build directory avoid

Status: probe evidence for a record not yet drafted; second revision after
review (exact package identities, one canonical build directory, the wrapper
remedy measured, the retained-output consumer exercised). Product `c6d8a1c`
(0068 closed; the checkout is ahead by plan text only), nano, Rust 1.98.1.
The probe is `rnx-bench/probes/shared-build/`; results and every Cargo log
are in `rnx-bench/results/shared-build-0069/`. No product change: every
build is the tool's exact command (`cargo build --locked --release
--target-dir …`) on the wrapper and lock the tool generated, taken verbatim
from the lock's identity, with Cargo's stable `build.build-dir` (1.91+;
slim 1.95, nano 1.98) either private or pointed at one canonical shared
directory. Alternatives that need the same starting state are restored from
a snapshot *into that same path*; one relocation control copies the state
elsewhere. Every executable's behaviour is verified through the notebook
worker: what a bare frame shows, whether `postgres::query` exists, and what
the retained-output native returns. Counts are Cargo `Compiling` log
entries, not rustc invocations.

## 1. Lock overlap, by exact package identity

The runtime's `Cargo.lock` has 228 records (223 names). A fresh Polars
assembly resolution (386 records) shares 214 names with it and matches 192
exact `(name, version, source)` identities; 26 runtime identities under
shared names are absent, mostly newer patch releases (`rustls`, `zerocopy`,
`cc`, `smallvec`, …). Seeding the resolution with the runtime's lock — the
tool already starts from a project's previous `rnx.Cargo.lock`, so the probe
pre-placed the runtime's — gives 215 shared names, 214 exact matches, and
5 absent: `sha2 0.10.9`, `digest 0.10.7`, `crypto-common 0.1.7`,
`block-buffer 0.10.4`, `cpufeatures 0.2.17`. Those five belong to the
management crate, which the generated runtime disables; the seeded assembly
holds only the 0.11 line that Polars requires. Polars + PostgreSQL: 192
exact unseeded, 214 seeded. Seeding is a working preference; it is not
where the time is (§4, §5).

## 2. Today, and the wrong-binary control

| case | seconds | entries | bare frame |
|---|---|---|---|
| a Polars, private, cold | 112.5 | 308 | presents |
| b same natives, plain wrapper, private | 111.7 | 308 | opaque |
| c Polars, shared, cold | 110.7 | 308 | presents |
| d plain wrapper, shared, warm after c | **0.16** | **0** | **presents** |
| d2 the same, warm state copied to another path | 0.17 | 0 | presents |

Case d is byte-identical to c and presents frames although its `main.rs`
has no `.present(...)` call: with the same package name, version and
dependency set, Cargo judged the workspace package fresh by mtime against
the fingerprint the other assembly left. Relocating the directory does not
change that verdict. This is the stop any shared build directory must
design around, and it is preserved as a control.

## 3. The remedy, measured

The wrapper package name carries a digest of the wrapper body and its
dependency table, computed before the name exists — a nonrecursive
discriminator, so the assembly key that later hashes the wrapper is not an
input to it (`rnx-app-<16 hex>`; the executable is named the same way).

| case | seconds | entries | bare frame | bytes |
|---|---|---|---|---|
| w1 presenting, cold | 111.9 | 308 | presents | |
| w2 plain, after w1 | **0.61** | **1** (the wrapper) | opaque | |
| w3 presenting again | 0.17 | 0 | presents | = w1 |
| w4 plain again | 0.17 | 0 | opaque | = w2 |
| r1 plain first, cold | 111.8 | 308 | opaque | |
| r2 presenting second | 0.64 | 1 | presents | |
| w5 + PostgreSQL, unseeded | 38.1 | 30 | presents; `postgres::query` exists | |
| s1 seeded Polars, cold | 110.9 | 308 | presents | |
| s2 seeded + PostgreSQL | 37.3 | 31 | presents; `postgres::query` exists | |
| w6 seeded combined into the unseeded directory | 106.4 | 173 | presents | |

Both orders, both repeats and both seeded and unseeded combined builds behave
as written; repeats produce the same bytes as their first build. Adding
PostgreSQL compiles its graph (`tokio-postgres`, `postgres-protocol`, `phf`,
`md-5`, `sha2`, …), the wrapper, and the `polars*` crates, which the
combined graph's feature and version unification rebuilds; seeded and
unseeded cost the same. Mixing a seeded combined lock into a directory
filled by unseeded builds rebuilt 173 entries: that is the cost of mixing
resolutions in one directory for this graph, not a prohibition — a
directory may hold several resolutions; each unshared one costs its own
compile.

## 4. Install → first Polars

`cargo install --git file://… --rev c6d8a1c rnx --locked` into a fresh
root: 39.9 s, 163 entries; with `build.build-dir` at the canonical shared
path: 40.6 s, 163 entries, 225 fingerprints retained (0.62 GB). A Polars
assembly built at that same path afterwards: 110.8 s with 250 entries
unseeded, 110.4 s with 225 entries seeded — against 110.7 s cold. The 58–83
reused entries are the launcher's small crates; the Polars-side crates are
the minutes. This is a `--git` install of the local repository, not a
network fetch, and a path-runtime assembly; registry crates are the same
either way. Retaining installation build output is not worth a mechanism.

## 5. The retained-output consumer (0061) in shared storage

A path native whose build script writes `OUT_DIR/retained.txt` and whose
extension reads that exact file at runtime, beside Polars, with the
discriminated wrapper:

- l1 cold: 110.4 s, 309 entries; `probe::retained()` returns the value. The
  executable embeds the shared path (it is the `OUT_DIR` path, by `env!`).
- l2 the plain twin: 0.63 s, 1 entry; l1 still reads its value.
- l3 the native's library changed: 0.9 s, 2 entries, the same `OUT_DIR`
  directory; l1 still reads its value.
- l4 the native's **build script** changed: it re-ran into the **same**
  `OUT_DIR` and rewrote the file — and the unchanged, older executable l1
  now returns `REWRITTEN by a later build`. A later build in shared storage
  silently changed an existing executable's behaviour.
- Deleting the shared directory: l1 and l3 fail with `Err(<path>: No such
  file or directory)`.

So the shipped adapters' executables do not depend on the directory (0068
gate 4's and the first revision's Polars reads after deletion stand), but
the general obligation is real in both directions: shared outputs must not
be removed while any executable may read them, and must not be *rewritten*
under an existing executable either. Separate removal commands do not
address the second; the record must decide it (immutability of a retained
`OUT_DIR` once an executable depends on it — for example by giving each
native version its own directory, or by refusing build-script outputs that
are read at runtime, which the tool cannot detect in general).

## 6. Storage

Two Polars assemblies in one shared directory: 1.60 GB, the same as one
private directory (1.60 GB); the installation's retained directory 0.62 GB.

## What the record should decide

A tool-owned shared build directory under the cache root, keyed by
toolchain, target and profile; wrapper packages named by a content
discriminator computed before the assembly key; a seed lock per root and
runtime revision as a preference, with the cost of divergent resolutions
reported, not forbidden; the directory's own listing and removal under `rnx
cache`, never through entry removal, and a stated rule for retained build
outputs under existing executables (§5), which is the open design question.
Expected from the measurements: a second wrapper over the same natives in
under a second, a new native's graph in tens of seconds. Out of scope:
retaining `cargo install` output; the repeated-preparation path (its own
record).
