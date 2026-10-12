# rnx 0185: migrate to the Rune fork, stock execution only

**Status:** plan. Authority: Codex GO seq 8593, after the read-only integration study (seq 8545). Local only:
nothing is pushed and `main` does not move. Codex implements; Claude authors plans and reviews code; Codex reviews plans.

## 1. Scope

rnx moves from crates.io Rune 0.14.2 to the Rune fork at rev `55d501332c0b8f9aa04695453e7a567b83819a6c`
(version 0.15.0, upstream `main` plus the fresh-core work behind off-by-default features). Execution stays
on the stock Vm. The record is an API port: behaviour is kept, apart from the upstream changes listed in §4,
each of which is tested rather than assumed.

Not in this record: the `fresh-core` and `fresh-core-native` features, any routing between engines, the
fork-side convergence records (F1–F3), and any unreviewed semantic change. Findings during the port are reported
and resolved jointly before implementation. The amendments in §8 record the reviewed exceptions.

## 2. Dependency

- Root `Cargo.toml` and `tools/project/Cargo.toml`: `rune` from the fork's Git repository at the full rev,
  `default-features = false`. The root keeps `["std", "fmt"]`. `tools/project` adds `"anyhow"`: the fork's
  `std` without `anyhow` does not compile.
- The companion crates (rune-core, rune-alloc, rune-macros, rune-tracing and their macro crates) come from
  the same Git source through the fork's own path dependencies. rnx names none of them directly; the
  adapters, servers and demos reach Rune only through `rnx::rune`.
- If the Git fetch is unavailable, a local `[patch]` to a checkout of the same rev is used for validation
  and recorded as such. It is never committed.
- `rust-version` stays 1.95: the fork declares 1.95 and the gate test checks rnx's own field.
- Lockfiles that move: the root, `tools/project`, and every separate workspace that builds rnx
  (`adapters/polars`, `adapters/candle`, `adapters/postgres`, `servers/http-postgres`, `demos/polars`,
  `demos/candle`). `jupyter` and `tools/polars-gen` have no Rune edge. Historical `probes/*` keep 0.14.2.

## 3. API port

The inventory is the study's (`api-delta`). Classes:

| Change | Where |
|---|---|
| `rune::sync::Arc` for `Unit`/`RuntimeContext` (`Arc::try_new`) | root, adapter tests, polars-gen harness templates |
| `FatalDiagnosticKind` private: `as_compile_error()` | five root sites |
| `SourceLoader::load(&Sources, SourceId, ..)` | program.rs |
| `resume()` returns `VmResume`; `VmOutcome { Complete, Yielded, Limited }`; `async_resume` removed | execute.rs, server.rs |
| `Vm::without_runtime` fallible | server.rs |
| `VmResult::panic`/`::err` removed, `InstAddress`→`Address`, `ToReturn`→`IntoReturn`, `Output::store`→`Memory::store`, `raw_function` generics | web.rs, polars lib.rs, candle signal.rs, polars-gen templates |

Generated code is never hand-edited. The polars-gen templates are corrected, the generator first reproduces
the committed files at the old templates (`--check`, a determinism baseline), and the bindings and oracle are
then regenerated from the same inventory and release file. The regeneration is run twice and must be
byte-identical. Deprecations (`VmResult`, `vm_try!`, `vm_write!`) are left as warnings for a later record.

## 4. Behaviour changes the fork brings, tested here

1. **Budget halts.** The fork's `resume` reports an instruction-budget halt as `Ok(VmOutcome::Limited)`,
   which is resumable. rnx's sliced path resumes only on that outcome; every `Err` ends the execution and is
   never retried.
2. **Native loop permits.** Upstream Rune now takes a budget permit per step in native iterator loops
   (`budget::permit`), with no feature gate, and raises a non-resumable `Halted { Limited }` error when the
   budget runs out inside one. The study's claim to check: a session or `eval` input that spends more than
   one 10,000-instruction slice inside one native call fails where 0.14.2 completed. This is reproduced
   locally before anything is changed. If it is real, the record reports concrete source options that keep
   total-budget enforcement; it does not install an unlimited permit budget, and it does not work around it
   in rnx.
3. **Halt text.** "Halted for unexpected reason `limited`" is now "Exhausted the budget it was given". The
   Polars callback bridge's `HALT_LIMITED` match is corrected, with a test that a callback's budget halt is
   still reported as one.
4. **Wording drift.** Compiler, VM and `rnx fmt` output changed upstream. Goldens and transcripts are
   re-baselined only where the diff is wording, and each diff is listed in the evidence.

## 5. Version and docs

`rnx version` names the Rune it embeds as the fork's crate version and rev, and the gate test ties it to the
manifest's `rev`. The Jupyter worker handshake and kernel info follow. The description and README lose
the "no compiler fork" claim.

## 6. Validation

Local only, in bounded batches, each with its wall time and target-directory size recorded, in a new target
directory, with a free-disk floor of 30 GiB watched throughout:

1. The root crate: build, lib and integration tests.
2. The transcript and demo checks the repository has.
3. `run`, `eval` and session smoke.
4. The notebook kernel tests, if cheap.
5. The adapters (Polars, Candle, Postgres) and the HTTP server, staged: each one's actual build cost is
   recorded. A batch that exceeds its bound is stopped and reported with its cost; coverage is not removed.

## 7. Commits

One plan commit (this file) and one impl commit; gates, docs and fixes amend the impl commit. Evidence
goes in `0185_rune_fork_migration_evidence.md` in the impl commit.

## 8. Reviewed amendments during implementation

The initial migration reproduced three fork differences that required source corrections,
reviewed jointly with Claude (chatd 8648–8689):

- Native iterator work must charge the exact total without aborting at a soft
  session slice. A std-only synchronous `budget::with_slice` separates the outer
  VM yield from hard exhaustion. Native work may delay cancellation until it
  returns or spends the remaining total; errors are never retried. The no-std ABI
  stays unchanged. rnx clamps the unlimited sentinel to a finite total.
- Empty parser recovery nodes must point at the next significant token rather
  than dropped whitespace. The fork corrects the span; rnx caret tests stay intact.
- Recursive value walks use the fork’s public `MAX_VALUE_DEPTH` bound, counting
  the root and scalar leaves. rnx adopts its 64-level contract, with exact
  acceptance/refusal checks for arrays, tuples, objects, options and mixed values.
  Raising the limit requires stack-safety evidence or iterative walks.

The fork corrections live in an isolated local branch off the pinned revision,
receive Claude’s code review, and are pinned in rnx after local validation.
No main update or push is included. The original 55d50133 probe and every failed
validation attempt remain in local evidence.

The API port also updates the notices generator for Git source annotations and
records the fork’s licence texts from the exact resolved revision. Network-failure
controls use the already locked registry package clipboard-win 5.4.1 because Git
Rune has no published-package `.cargo_vcs_info.json`.

Final reviewed fork revision: `03b1aa4dc29e4125392d0e03dab12bc408bf701d`
(parent `55d501332c0b8f9aa04695453e7a567b83819a6c`). All current
workspace lockfiles resolve that revision; historical probes remain unchanged.
Interactive preview retains its previous visible depth: its presentation limit
moves from 8 to 9 to compensate for counting the root at level 1. This does not
change the shared complete-output bound of 64.
