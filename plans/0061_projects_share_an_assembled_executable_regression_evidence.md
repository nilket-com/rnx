# 0061 gate 6: regression and Linux closure

Status: ready for closing review. Gates 1–5 are accepted and pushed; gate 5 is
rnx `b96d784` / rnx-bench `c3eb0d8`. All checks below pass on that published
implementation. This gate changes only the plan and this evidence in rnx, plus
bench drivers and results. No product correction or dependency change was needed.

## Suites, boundaries and platform

All root configurations ran serially with one test thread:

| Configuration | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| Default | 375 | 0 | 0 |
| test-support | 418 | 0 | 0 |
| test-support + server-runtime + project-sources | 437 | 0 | 0 |
| Project tool, default | 40 | 0 | 2 |
| Project tool, test-support | 40 | 0 | 2 |
| Explicit opt-in tool integrations | 2 | 0 | 0 |

The root combined count includes its five compile-fail documentation tests.
The two normally ignored tool integrations were explicitly supplied a fresh
project-sources binary and the real rnx repository. Both ran: manifest-to-runner
source-map handoff, and the native inventory audit (two package associations,
two native roots, 35 external candidates, two present external files). The
inventory snapshot is retained outside the fingerprinted repository.

Root and tool formatting, both notice inventories, a fresh ordinary release
build and root selfcheck pass. Strict **tool** Clippy passes across all targets
in both configurations. This is not a new root all-target Clippy claim.

The tool's all-target, test-support MSVC Windows type-check passes. It retains
the known test-only unused `UNIX_EPOCH` import warning in `artifact.rs`. Windows
was not executed: `commands::install_signals` still refuses project commands on
non-Unix platforms rather than offering unimplemented subprocess supervision.
No Windows execution or cleanup guarantee is inferred from type-checking.

The published pre-cache root is `83398d9`. Its default Cargo tree and the current
tree are byte-identical after replacing only the checkout path. An archived
baseline directory, not a Git worktree, supplies that comparison. The root
workspace still has one member. Git comparison confirms unchanged root source,
manifest, lockfile and notices; jupyter, adapters and servers; and the tool's own
manifest, lockfile and notices. Every change since that baseline is confined to
plans or the project tool. No new stock startup measurement is claimed for this
unchanged root implementation; gate 5 retains the actual shared-launch evidence.

## External regressions

`rnx-bench/probes/cache-regression/README.md` contains the serial reproduction
commands. `results/cache-regression-0061/` retains every command/status, raw logs,
source snapshots and transformations, inventory, default graphs and binary
identities. The published earlier results were left untouched.

| Fixture | Result and scope |
| --- | --- |
| Gate 4 real commands | All 13 groups pass: legacy preservation, explicit migration/no promotion, second-consumer attachment, launch without compiler, artifact and source checks, receipt/map scope, corruption/refusal, failure and override paths. |
| Gate 3 publication/concurrency | All 37 cases pass against imported current product sources, including separately interrupted builders/waiters, retry, unrelated keys, publication injections, corruption and special-file refusals. |
| 0059 verification | All seven contract groups pass, including zero-read default/full-read verify, legacy receipt migration, replacement and touch, same-size/restored-mtime source checks, refresh failures and overrides. |
| 0060 command contracts | All four groups pass: generated/override checks, argv/refusals and compiler-free launch. |
| 0060 real Polars override journey | Frames and lazy plans survive catchable errors, Parquet roundtrips, reset, Ctrl-C and EOF; working directory/settings/source-map scope and nine byte-identical eval comparisons pass. |
| 0057 local workflow | All 16 groups pass, including lock-pair failures, concurrent edits, Cargo interruption with a build-script child, relocation and self-reference refusals. |
| Ordinary command build | Shared lock/build/eval succeeds with both fault-hook families requested but compiled out. |
| Real PostgreSQL workflow | Current shared lock/build/run, plain adapter and mapped Rune source return the bound text and 42; v3 receipt published and private postmaster independently observed reaped. |

The legacy 0057/0059/0060 fixtures expressly assert local paths and old formats.
Their successful lock creation uses a real frozen tool built from published
`814d20f`; builds and launches use the current tool. The old workflow's lock
failure paths likewise exercise that frozen lock implementation. These are
legacy-compatibility tests, not substituted evidence for new shared publication.
New lock/publication/migration semantics are exercised separately through the
current gate-4 command fixture and gate-3 entry fixture. Original script hashes
and exact transformed copies are saved; assertions were not weakened.

The real PostgreSQL fixture uses **current** commands throughout with a private
shared cache. Its obsolete per-project target-seeding block is removed because
shared builds cannot use it. The cluster is a temporary Unix-socket instance,
never the system server. A payload containing quotes, SQL punctuation, a
backslash and an emoji returns exactly alongside the mapped/adapter-computed
value. Only the manifest, entry and two public lockfiles are visible beside
the ignored dot-directory. The recorded postmaster PID is absent after cleanup.

The gate-3 isolated driver hash-asserts every imported product source against
the real tool. Replay conditions name published `b96d784`; the archived patch
from `814d20f` preserves the same implementation relative to the frozen baseline.
No fixture startup, compile time or regression duration is presented as a new
performance measurement. All drivers complete successfully and no fixture
executable remains running.

## Completion and retained qualifications

Gate 6 passes. The record is ready to close on Linux: newly locked generated
projects reuse a cache-owned assembly, legacy local projects remain local until
explicit relock, and ordinary launch keeps source checks and the metadata-default
artifact policy. The accepted Polars gate demonstrates two real consumers,
zero-compilation attachment, approximately 31 ms pipeline launch versus 11 ms
direct, and a 1.46 GiB retained entry. This gate does not replace those samples.

The README covers lock/build/session, cache selection and canonicalization,
changed Cargo context, explicit migration/recovery, retained whole entries,
measured disk growth and non-hermetic reuse. Its qualifications remain:

- Canonical native roots and cache location are identity inputs; relocation is
  not transparent and arbitrary build-script observations are not fully keyed.
- Attachment fully hashes the executable. Everyday metadata checks retain the
  documented restored-mtime in-place-edit miss; `--verify` retains full hashing.
- Runtime auxiliary build outputs are retained, not recursively authenticated
  on each launch. There is no automatic eviction or live-entry tracking.
- SIGINT/SIGTERM owned Cargo cleanup and hard-killed waiter recovery are tested;
  safe SIGKILL of an active builder is not promised.
- Windows execution, an adapter catalogue, `:dep`, startup probing and replacing
  a live session remain later records. No current operation transfers bindings.
