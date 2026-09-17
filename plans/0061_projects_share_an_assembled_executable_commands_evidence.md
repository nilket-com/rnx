# rnx 0061 gate 4: commands and explicit migration

Status: ready for Linux review. Gate 3 is accepted/pushed at rnx 814d20f /
rnx-bench 4c8256f. Gates 5 (real shared Polars costs) and 6 remain open.

## Product change

Explicit generated-project `lock` now resolves under a private cache-root
workspace, using canonical wrapper paths and the gate-one Cargo search context.
It writes format 2 with the canonical shared identity and the same atomic
Cargo/project lock-pair protocol. Temporary resolvers are removed on ordinary
success/error/unwinding. SIGINT/SIGTERM uses the existing owned-child cleanup.
Override locks stay format 1.

`build` dispatches from the persisted assembly kind, not the current default:

- A format-1 generated lock uses the existing project-local assembly/target/artifact
  and version-2 receipt path. Nothing promotes or rewrites it implicitly.
- A shared lock acquires the gate-three entry while holding the project lock,
  rechecks real project declarations/source/native inventory and the lock pair,
  then publishes a version-3 receipt binding project digest, assembly key,
  executable digest and stamp. A hit validates/attaches, not compiles.

Run/session/eval require the matching v3 receipt before using a shared entry. They
obtain the fixed artifact path from the lock identity plus validated ready document;
a receipt cannot select an arbitrary path. They keep content checks for sources,
metadata-default/--verify artifact checks, run-only map handling and direct exec.
No Cargo or rustc is invoked by any launch mode. Missing readiness or receipt
refuses rather than repairing/building. A stamp refresh stays project-local.

Local v1/v2 receipts keep their existing migration rules. A shared receipt cannot
be interpreted as a local generated receipt. After an explicit switch to an
override, its old shared receipt is ignored as a stamp: the newly locked override
digest receives a full check and a normal v2 receipt. This preserves the existing
override recovery policy without trusting the old shared binding.

The new private `workflow/shared.rs` contains resolution and shared input replay.
There are no new dependencies, command options or public APIs. Root source,
manifest/lockfile/notices, kernel, adapters and server are unchanged.

## One integration correction

The first real-command fixture used the host's ordinary group-writable umask.
Cargo made `target/release` group-writable, which correctly failed the cache's
private-directory rule. Gate 3's fixture had set 077 and did not expose this.
Shared builds now set umask 077 **only in the Cargo child pre-exec hook**, so Cargo
and its children produce private outputs. The parent mask and the ownership rule
are unchanged. The normal-umask command fixture and all 37 gate-three cases pass.

## Evidence and reproduction

`rnx-bench/probes/cache-commands/README.md` gives complete commands.
`results/cache-commands-0061` retains old/new lock and receipt documents, readiness,
Cargo lock, tiny native sources, build-script output-directory log, binary hashes,
raw outcomes, transformed legacy contract scripts and a source patch from 814d20f.
The baseline command is built from an archive of published 814d20f, not simulated
by editing a new lock's version number.

The 13 real-command groups pass:

| Group | Observation |
| --- | --- |
| Legacy launch/build | Actual old-tool format-1 lock/v2 receipt. Current run/session/eval preserve lock pair, receipt and artifact bytes; current build stays local; no shared cache exists. |
| Explicit relock | Writes format 2; old receipt cannot launch. Build-script counter gains one compilation in the shared target; new path/inode, old artifact retained. No binary promotion. |
| Second consumer | Explicit lock/build attaches the same artifact with compilation/metadata trapped; no extra build-script call. |
| Launch without toolchain processes | Run/session/eval succeed with both compilation and version queries trapped. |
| Verification | Default reads zero artifact bytes, --verify reads the full artifact, touch and identical replacement refresh. Documented same-size/restored-mtime artifact edit passes default but fails --verify. |
| Bindings | Wrong receipt version/key/project digest/artifact digest refuses. |
| Refresh failure | Old receipt remains intact; changed stamp is not published. |
| Source checks | Same-size/restored-mtime source edit refuses before artifact bytes are read. |
| Interactive scope | Broken unused map does not block session/eval; run owns its repair. |
| Context/cache refusals | Different explicit root, newly present project Cargo config, corrupt readiness and deleted entry refuse without repair or compiler invocation. |
| Attachment failure | Ready entry remains valid; failed project has no receipt; explicit build reattaches. |
| Pair publication | Normal failure restores the old pair; signal between publication steps leaves a refused mismatch; explicit relock recovers. |
| Overrides | Stay format 1/v2, including a switch from shared; legacy v1 receipt migration remains available. |

The native fixture reports argv and records OUT_DIR; it is intentionally not a
Rune/Polars implementation. Its counter records exactly a legacy target build and
a shared target build. The second consumer adds no native compilation.

The accepted 0059 stamp-contract and 0060 interactive-contract fixtures are replayed
with their assertions intact. Successful lock creation alone uses the frozen tool
to preserve those fixtures' explicit local/v1/v2 assumptions; all builds and
launches use the current tool. Original source hashes and the transformed scripts
are archived. New shared lock publication is independently tested above.

The unchanged real 0060 Polars PTY journey also passes using a fresh private
**override** project and a copy of the accepted Polars executable: frame retention,
filter/group/aggregate, error recovery, Parquet round-trip, reset, Ctrl-C, EOF,
settings, caller working directory, stale mapped source refusal, and nine
byte-identical eval comparisons against direct execution. This proves override
and terminal compatibility, not the fresh shared Polars build reserved for gate 5.
The ordinary release command passes shared lock/build/eval with both project and
cache fault hooks set but compiled out.

A fixture bootstrap initially shared a target between two same-name tool packages,
leaving the old command at the current output path. Its format assertion caught
that mistake. The baseline now has its own target, and the bootstrap cleans only
the current tool package before rebuilding it; the complete command and contract
replays pass with verified current/baseline hashes. No product policy changed for
this fixture correction.

## Checks and qualifications

- Default and test-support tool suites: 40 passed, 2 ignored each; zero failures.
- Formatting and strict all-target Clippy pass in both configurations; notices current.
- Windows MSVC all-target type-check passes with the existing test-only UNIX_EPOCH
  warning. Windows commands still refuse; no execution claim.
- All 37 publication/concurrency cases pass against current product sources.
- No fixture Cargo/build script/session remains running; private directories are
  cleaned and no user cache or kernelspec is used.

Format compatibility does not waive source freshness. Updating a fingerprinted
rnx repository can invalidate an old project's inputs regardless of its lock
format. The unchanged-input legacy case stays local until explicit relock.

This gate makes no shared-cache launch timing or disk-growth claim. The README
labels the older 0059 figures as per-project measurements. Gate 5 must measure
two real Polars consumers, cold shared compilation, full-hash attachment, retained
disk size and the <=25 ms ordinary-launch overhead without weakening input checks.
