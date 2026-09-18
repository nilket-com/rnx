# rnx 0064 gate 5: installation and matched project launch costs

Gate 5 passes on Linux, ready for review. Gate 6 remains open, including the
review's cosmetic fsck diagnostic-prefix follow-up. This checkpoint changes only
documentation. All ordinary products and native sources measured here come from
the published `c10dca7` snapshot. The isolated timing tool's two modified source
files are archived alongside that baseline; no instrumentation enters the product.

Evidence: `rnx-bench/probes/runtime-cost` and `results/runtime-cost-0064`.
The ordinary tool SHA-256 is
`69c3a991d5cab37389861007c56f5a5890cb423c99dd6e9260ce7632cc26578d`,
identical to gate 4's ordinary tool. Scripts retain all samples and validate output.

## Installation and retained storage

The snapshot is 420 tracked files, 6,453,273 source bytes. Initial installation
at setup took 2.36 seconds, a single unrestricted-CPU observation. The subsequent
experiment uses fresh stores with warm source/filesystem caches, pinned to CPU 0:

| Fresh-store installation | Repeat 1 median | Repeat 2 median |
| --- | ---: | ---: |
| Ordinary tool | 1,522.5 ms | 1,543.5 ms |
| Isolated timed tool | 1,531.4 ms | 1,522.2 ms |

Each repeat interleaves ten ordinary and ten timed installations (40 retained
samples). Instrumentation totals bracket the ordinary medians rather than showing
a consistent added delay. Its monotonic boundary intervals are disjoint; medians
of intervals need not add to the median of the total.

| Instrumented interval | Median |
| --- | ---: |
| Initial validation, before copy | 108.6 ms |
| Copy and per-file sync | 331.6 ms |
| Independent Git index | 258.7 ms |
| Copied-source validation, metadata and tree sync | 560.7 ms |
| Entry rename and parent sync | 0.85 ms |
| Final validation before selection | 249.5 ms |
| Selection publication and sync | 7.38 ms |

The process-wall total also includes CLI startup and final output. The timed copy
adds start/end observations plus timestamps at existing installer boundaries.
Both binaries install the same source identity. No cached install is substituted
for a fresh-store sample. Timing starts only after the four assembly builds finish.

Installed source files consume 6,453,273 logical bytes; independent Git files
consume 1,994,854. Including metadata, the entry holds 8,448,696 logical file bytes.
Counting filesystem blocks for files **and directories**, it occupies 11,472,896
bytes on this filesystem. This is the additional retained source installation,
separate from compiled assemblies.

| Retained assembly | Allocated bytes, including directories |
| --- | ---: |
| Checkout, Polars | 1,721,565,184 |
| Installed, Polars | 1,721,663,488 |
| Checkout, Polars + PostgreSQL | 1,752,289,280 |
| Installed, Polars + PostgreSQL | 1,752,301,568 |

These are whole entries, including their retained build directories, not just
executable sizes. A new source identity/location can retain another assembly;
there is no eviction in this record. Byte totals are host/build observations,
not a fixed storage requirement for every toolchain.

## First compilation and ready attachment

Four source-identical project recipes use the same ordinary tool, cache root,
features and locked native graph. The generated main and Cargo-lock digest match
between each checkout/installed pair; canonical native paths and keys differ.
Each assembly entry/target was absent before its build. Cargo was offline with
registry sources already present. Cold builds use the available CPUs, separately
from all pinned measurements:

| Recipe | Checkout cold build | Installed cold build |
| --- | ---: | ---: |
| Polars | 115.7 s | 111.3 s |
| Polars + PostgreSQL | 112.2 s | 114.8 s |

These are single observations, with no speed comparison inferred. The first
checkout build overlapped compilation of the isolated timing tool during setup;
that qualification is recorded, and the documented sequential rerun avoids it.
All cold builds finish before installation or launch sampling begins.

Forty ready attachments (five per origin/recipe/repeat) include native/context
validation, **full executable hashing** and receipt publication. Compiler traps
with positive controls prove zero compilation; the ready document and artifact
inode/mtime stay unchanged. The traps' command-wrapper overhead is included.

| Recipe | Checkout attachment medians | Installed attachment medians |
| --- | ---: | ---: |
| Polars | 297.5 / 298.2 ms | 303.3 / 303.8 ms |
| Polars + PostgreSQL | 326.6 / 326.8 ms | 331.6 / 333.4 ms |

## Everyday launch against direct

One Polars thread is set before spawn; samples are pinned to CPU 0 and interleaved
with a fixed seed. The three modes are file run, eval and spawn-to-first-prompt.
Run/eval execute `polars::lit(1).is_ok()` and return true. Every stdout, stderr,
exit or prompt is checked. Per-sample cwd/history/config preparation lies outside
the measured interval. The full CSV/Parquet behavior is gate 4's journey.

There are two repeats, four project shapes, three modes, three launch routes and
20 samples per cell: all **1,440 samples** remain, with no outlier exclusion.
Representative file-run medians (repeat 1 / repeat 2):

| Source / recipe | Everyday | Explicit verify | Direct |
| --- | ---: | ---: | ---: |
| Checkout, Polars | 28.08 / 27.92 ms | 94.20 / 94.05 ms | 6.28 / 6.20 ms |
| Installed, Polars | 28.89 / 28.83 ms | 95.38 / 95.38 ms | 6.21 / 6.17 ms |
| Checkout, both | 32.06 / 32.12 ms | 99.16 / 99.20 ms | 6.26 / 6.25 ms |
| Installed, both | 33.10 / 32.99 ms | 100.13 / 100.19 ms | 6.23 / 6.30 ms |

Across every mode and both repeats, everyday minus direct spans:

- Checkout Polars: 21.72–21.80 ms; installed Polars: 22.50–22.89 ms.
- Checkout both: 25.76–26.01 ms; installed both: 26.70–26.97 ms.

Installed launches are consistently roughly 1 ms above their matched checkout
controls. This is recorded as an observed difference, not dismissed as noise.
Its cause is not isolated; native paths, path depth and inventories differ.
File traces for ordinary installed launches with one and two natives show no
current.json or installation.json lookup. Product launch still uses the same
0059/0061 input checks; there is no runtime-discovery or installation-validation
call on that path. Source content checking remains in force.

Both two-native controls exceed the old fixed 25 ms-over-direct target. Gate 5
explicitly compares matching adapter counts instead. These observations support
the planned native-inventory record; they do not justify reducing coverage here
or extrapolating a constant cost to arbitrary adapters.

## Discovery and scope

The 160 private-protocol describe samples measure tool spawn to validated notice,
then decline and reap. They are tool-endpoint costs, not terminal-rendering or
session-startup times. Checkout uses the explicit override; installed uses the
selected default. Repeated medians are 0.65–0.69 ms for checkout and 0.71–0.76 ms
for installed, across one/two requested adapters. No source compilation or
scratch publication occurs on decline.

All fixture processes ended. No user installation, cache or source was modified.
The initial scaffolding run assumed the lock had a separate key field; it failed
before compilation. Its log is retained. The final driver hashes the persisted
canonical identity and checks absence at the corresponding ready path.

Root/tool product code, dependencies and public API are unchanged since gate 4.
The changed probe scripts parse, root/tool formatting checks pass, and the raw
journals, conditions, file traces, binaries' hashes and exact instrumented source
are retained. Full regression and the diagnostic wording nit remain gate 6.
