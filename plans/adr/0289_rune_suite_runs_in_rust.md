# ADR 0289: rune-suite runs in Rust

Status: Proposed

Base: published 837f2b5d4fbd23f123d4a0a607872b21bf584012 (ADR 0288).

## Problem

`rune-suite` is the last suite stage still run by Python:
`tools/column/suite_stage.py` runs its test recipes and API-boundary
twins, and `tools/column/native_contracts.py` builds the Rune test
binary and runs every native-manifest case, with `stage_scheduler.py`
as its jobserver client. It is also the final's longest stage (19.2
minutes in ADR 0288's final), so its token use is the one most worth
stating exactly. The finalizer already checks its catalogue in Rust,
against a receipt Python writes.

## Decision

1. `rune-suite` runs as `rnx-final stage suite --stage rune-suite`,
   bound by `stage::bound` like the other 55 Rust stages, and writes the
   same receipt the finalizer reads: its checks, its `contracts` with
   the manifest digest, binary digest, cases and catalogue.
2. Its recipes stay the native manifest's: each library recipe, and the
   six API-boundary twins, each with exactly its declared count of
   discovered and passed doc tests. Discovery and execution must be
   nonempty, as for every suite stage.
3. The contract binary is built once, from the one declared
   execution-and-serde library recipe, and must be the one Rune test
   executable Cargo reports; its compilation units must name this
   checkout. Every case checks the binary's digest before it runs.
4. Each case keeps its rules: selector discovery (exact when declared,
   ignored cases run), only the declared fixture environment, the
   sandbox when declared, one result line with all selected tests
   passed, the declared row total, empty stderr when declared, and no
   row differences. Every case runs even when another fails, and the
   catalogue check, the finalizer's own, refuses a missing, extra,
   changed, partial or failed case.
5. Tokens, under the final's jobserver: an exclusive case runs alone,
   holding every token its stage does not; the other cases then run
   concurrently, each on one of the stage's own tokens or one more from
   the pool, at most the case width. Outside a final the memory
   headroom check stays.
6. Retired: `suite_stage.py`, `native_contracts.py`, `stage_scheduler.py`
   and their tests `test_suite_stage.py`, `test_native_contracts.py` and
   `test_stage_scheduler.py`; their controls become Rust controls, and
   the one budget control that tests `host_budget.py`, which stays,
   moves to `test_host_budget.py`. The two Rust tests that drive the
   Python client instead drive the Rust one.

## Dispositions

- `child_result.py` imports `discovered` and `executed` from
  `suite_stage.py` for its Cargo-test children. Those two parsers move
  into `child_result.py` while it stays Python, with a control holding
  them to the Rust stage's parsers over the same logs, nonempty rules
  included. No Rust bootstrap is added to parse a log.
- `test_anchor_cache.py`'s assignment control keeps its assigned-root
  checks on the retained Python helper; its baseline-invoke and
  early-deletion assertion moves to a control on the Rust stage's
  `Live::invoke`: a command reads its baseline before deletion and
  cannot after.
- The jobserver client is a port of the Python client's transport into
  the existing budget crate's `fifo` module: a joining client found in
  `MAKEFLAGS`, which acquires a count of tokens, or one token from the
  stage's own slots or the pool, whichever frees first. The controller's
  `Pool` stays the sole owner of the pool; this is a permit client, not
  a second pool or scheduler. A token taken from the stage's own slots
  returns there, and one borrowed from the FIFO returns to it, on
  success, error, cancellation and worker failure alike.
- The contract build target is released before the cases run, as
  `OwnedBuild` does: the copied binary and `BINARY.json` are kept, the
  target is removed once no build process remains, and its release is
  journaled. It is neither kept until the final's cleanup nor removed
  while its build runs.
- The discovery and execution environments stay distinct: discovery
  and the binary's selection run without the case's fixture
  environment, and execution receives only the declared allowlist, the
  case's own variables and the receipt and source bindings.

## Constraints

- The same cases, recipes, counts and selection; the shared budget, the
  isolated environment and the 60-minute limit are unchanged.
- No new third-party crate, no new Python file; the Python file count
  only falls.
- The scheduler's admission does not change here; whether returned
  tokens should wake it stays a separate, reviewed question.

## Controls

- Witnesses from the published Python: the recipes, the twins, the
  case arguments and fixture environment, and the catalogue verdicts
  over planted rows.
- The ported controls: a missing, extra or changed case; a partial,
  failed or differently selected one; an exclusive case holding every
  token its stage does not while it runs; shared cases overlapping
  within the width; every case running when one fails; a changed
  binary refused before a case.
- The client: a one-token pool finishing every case on the stage's own
  slot; exclusive acquisition; overlap bounded by the width; every
  token back where it came from after completion and after a failed
  case. Readiness-driven, with no sleeps or progress polling.
- The build: a missing or second declared recipe, an ambiguous
  executable, compilation units naming another checkout, and a target
  released before the cases with the binary and its receipt kept.
- One complete Rust final, whose schedule record shows `rune-suite`'s
  admission and duration.
