# rnx 0064 gate 3: the tool discovers the selected installation

Gate 3 passes on Linux, ready for review. Gates 4–6 remain open.
Measured source is `c6fc230` plus the exact `source.patch` archived in
`rnx-bench/results/runtime-discovery-0064`. This evidence and the plan status
were written afterwards. No root Rust source, dependency, manifest, lockfile,
notice, kernel, adapter or server changed. The product changes are in the
project tool; both READMEs now describe installed discovery.

## Discovery and consent

An unassociated stock request selects RNX_DEP_RUNTIME when present. Invalid
absolute-path overrides refuse without reading the default as a fallback.
Otherwise the tool reads the bounded strict current and installation documents,
checks managed directories, and lets the existing catalogue validate the requested
runtime/adapter layout. No Git, writer lock, source scan, publication or build
occurs during describe. Missing selection creates nothing and prints:

```text
rnx-project runtime install --from /path/to/rnx
```

The existing notice field names the canonical installed source, installation ID,
original source path, commit/uncommitted provenance, clean/dirty state and UTC
installation time, or the explicit override. The existing REPL terminal-safe
output path handles it. There is no protocol or association-format change.

Consent re-description compares the same fields and candidate bytes before
mutation. A changed default refuses and requires fresh consent. For an installed
selection, prepare then runs the installer's full administration, inventory,
layout and recorded-content validation before creating the scratch. The scratch
stores that installed source path as an ordinary declaration. Associated sessions
keep their existing validated project path and never consult discovery.

F2 is closed: runtime show with no store/selection says no runtime is installed
and gives the install command and project-session alternative; selecting an unknown
full ID says unknown installation ID. The README explains why the independent
installed index has an unborn HEAD and why no synthetic commit is needed.

## Product fixtures

`probes/runtime-discovery/check.py` freezes the built product tool and uses private
store, state and cache roots. It records 24 checks plus a summary row:

- No store, a complete but unselected entry, clean and dirty defaults, multiple
  selections, valid override, invalid override with a valid default, and F2's two
  CLI refusals.
- A default change between describe and consent refuses before scratch creation.
  Corrupt installed bytes refuse at the recorded fingerprint; deleted entries
  refuse. Missing Git is actionable and leaves no new scratch. Describe itself
  succeeds with Git absent.
- Wrong-version, unknown-field and oversized current documents refuse. Managed
  entry/source symlinks refuse. A conflicting working-directory project is ignored.
- Successful validation reaches the existing injected author refusal only after
  creating a scratch whose declaration names the selected installed source.
- Six real stock PTYs retain a binding and start an HTTP future through Rune
  select. Selection change, corrupt source, deleted entry, missing Git, invalid
  override and decline all leave the original request usable. The fixture checks
  the held server connection before further input, then reads the binding and
  obtains the original HTTP result. Every session and hold-server thread is joined.

`probes/runtime-discovery/reopen.py` uses the real rnx library, tool, Cargo build,
startup probe and association carrier. It copies the tracked working snapshot
and substitutes tiny named functions for the two adapter engines. It installs
that source and a second snapshot, renames the fixture checkout, then prepares a
scratch through the product protocol against the first installation. After the
second is selected, the first scratch still opens and executes its native function.
Its associated describe ignores an invalid override; ordinary eval also ignores
an invalid default document. Its manifest and receipt remain unchanged.

These tiny native bodies are explicitly ownership controls, not evidence for
Polars or PostgreSQL execution. Full engine journeys from a single launcher/tool/
source snapshot remain gate 4. No cache, installation or user checkout outside
the fixture is changed. The original fixture path is absent during preparation
and reopen, and no fixture executable remains running.

The first matrix attempt recreated current.json with an ordinary host mode and
correctly failed private-file validation. The driver now restores mode 0600.
The first reopen run also passed; its build stderr is retained separately as
`preliminary-reopen-tool.stderr`. The final run uses the archived source patch.

## Checks and reproducibility

`checks.json` and check logs record root fmt, default root tests serially (376
passed, zero failures), tool fmt, strict all-target clippy in both configurations,
and tool tests in both configurations (40 passed, two existing ignored tests each).
The tool notices check remains 100 packages, 63 texts, six recorded unavailable.
Windows GNU cargo check also passed; Unix-only transition dead-code/unused warnings
remain. No Windows execution claim is made. The other root configurations and
broader transition/adapter replays remain gate 6.

The measured test-support tool SHA-256 is
`79897c52cf27dd665ec81aa9e84c61b99fe80ec2b1798e5b0bc448093496dd7d`.
`conditions.json` records the root executable, patch digest and toolchain.
All measured source is recoverable from the published baseline plus the archived
patch; the fixture scripts describe the tiny adapter substitutions explicitly.
The matrix and reopen drivers require their entire ignored target directory to
be absent before a fresh pair of runs, as their README states.
