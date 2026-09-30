# rnx 0122: a workflow probe, then the first workflow fix

Status: plan. Record 0121 is closed on `origin/main` at `fa3d2ca`.

**The direction changed on 2026-09-29.** The user, Codex and Claude agreed:
- Polars values (DataFrame, LazyFrame, Series, Expr) are the everyday language of rnx analytics;
- Arrow is an explicit exit and exchange route (0119–0120), not the default value model;
- the parity percentage is a diagnostic, not the goal.

Records now target the common analytics workflows. The previous 0122, Arrow builders and mutation, is parked until a workflow needs it.

Claude and Codex agreed this cut before the plan (to-codex-062 and Codex's refinements).

## Why a probe first

The unsupported rows on user-facing types (v2: LazyFrame 18, Expr 14, DataFrame 29, Column 22, Series 65, I/O 312) are a triage pool, not a target. Some have working equivalents elsewhere (lazy `group_by` for eager `group_by`). Others are internal plumbing (most of I/O). Which ones actually block an analyst is an empirical question, so this record measures it.

## Stage 1: the probe

**Eight workflows**, written in natural Rune the way a notebook user would write them:
1. **Load:** CSV, Parquet and JSON, each from a file and from bytes.
2. **Inspect:** schema, `head`, `describe`, null counts, `shape`.
3. **Clean:** cast, rename, fill and drop nulls, deduplicate, filter.
4. **Derive:** new columns from string, temporal and conditional expressions.
5. **Group and aggregate:** eager and lazy.
6. **Reshape:** join, then pivot and unpivot.
7. **Windows:** `over`, rolling, cumulative.
8. **Output:** write CSV, Parquet and JSON; present the result.

**Each step stands alone.** A step's script builds its own input from fixtures, so an early missing API cannot hide a later blocker. The workflow also runs end to end.

**Fixtures are deterministic and local.** A small committed CSV and JSON under `probes/0122/data/`, with nulls, dates, strings and a categorical-like column. The Parquet copy is written from them at test time by the Rust twin, so it is deterministic and not committed. The bytes variants are read from the same files.

**Each step has a Rust twin** doing the same thing in Polars' Rust API; the twin is the reference result.

**Each step's outcome is one of:**
- **works:** the script's result equals the twin's (shape, dtypes, values, nulls);
- **differs:** a result that differs from the twin's is a product finding, reported with its cause;
- **blocked:** the step cannot be written or fails, attributed to exactly one of:
  - the surface row or rows behind it, with the blocker family from their refusal reason;
  - a presentation or host gap (session, notebook or display, not a Polars surface row);
  - a missing capability with no surface row (for example, an argument shape the generator never models).

**Both pins.** v2 is the target. 0.55.2 is replayed where the same API exists; it is reported, not ranked.

## The ranking, fixed before probing

**Every blocker of a step is recorded.** A standalone step can have several independent blockers, and a family that blocks a step does not by itself enable it (review of the plan). Where a step's blockers can be separated, the step is split until each part has one blocker. A step that stays multiply blocked is listed with all of its families and earns no credit.

For each blocker family:
1. **Primary: marginal steps enabled.** The number of distinct workflow steps that would work after fixing only that family, meaning the steps it alone blocks.
2. **Tie-break one: marginal complete workflows.** The number of workflows that would then run end to end.
3. **Tie-break two: implementation risk:**
   - **low:** an existing family or mapping rule extends;
   - **medium:** a new family within the current ownership rules;
   - **high:** a new trait, lifetime or callback model.

The table also reports, as evidence only: each family's multiply blocked steps (visible, not credited) and its callable count.

## Stage 2: at most one fix, bounded

Only after the ranked table exists, fix the top-ranked blocker family if all of these hold:
- it needs no new trait model, lifetime model or callback model;
- its proposed emitted bindings number at most 40, an exact cutoff measured from a generation of the fix before admission;
- it stays within the existing ownership and safety rules (0109 moves, 0119–0120 guards and bounds).

Otherwise 0122 closes as the audit, and the fix is planned as 0123 from the table.

A fix, if taken, meets the usual gates:
- the freeze holds;
- the oracle has no mismatches;
- the fixed steps flip from blocked to works in the probe, with their Rust twins;
- the suites pass, debug last;
- launch is measured.

## Proof

- **Replayable probe:** `probes/0122/` holds the step and workflow scripts, the twins and fixtures, and the committed outcome table at both pins. A replay script re-runs them and compares with the table.
- **Every blocked step is attributed**, and every attribution cites its surface row's recorded reason or names the gap.
- **The ranked table** follows the rule above, with the callable counts beside it.
- **The census** is reported as a diagnostic.

## Stop rules

- A step differs from its twin: report it as a finding, and do not rewrite the step to hide it.
- Stage 2's family needs a new trait, lifetime or callback model, or exceeds the size bound: close 0122 as the audit and plan 0123.
- A fix moves a frozen binding: stop.
