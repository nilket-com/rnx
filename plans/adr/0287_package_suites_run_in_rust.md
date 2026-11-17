# ADR 0287: Package suites run in Rust

Status: Proposed

Base: published f0a6663697538651432537c442ed6c60791b424a (ADR 0286).

## Problem

Of the complete final's 103 stages, 54 run `tools/column/suite_stage.py`
and two run `tools/column/clippy.py`. Their recipes come from
`tools/column/package_gates.py`, a second copy of the Rust catalogue's
format, lint and package tables, and their Clippy comparison is written
once in Python and nowhere in Rust. Each stage is a Python program
between the Rust controller and Cargo. ADR 0284 left them for this
record.

## Decision

1. The existing `rnx-final` binary gains a stage mode, `rnx-final stage
   suite --stage NAME --out DIR` and `rnx-final stage clippy [--adapter]
   --out DIR`. 55 stages move to it: the 53 `suite_stage.py` stages
   other than `rune-suite` (18 format: `project-format`, two adapter,
   twelve package and three demo or server formats; 15 lint; 12 package;
   `rune-clippy`; the project suite and Clippy; the Candle and Postgres
   suites; and the three adapter Clippy stages) and the two `clippy.py` stages (`clippy`,
   `adapter-clippy`). The catalogue writes their commands with the
   logical program `rnx-final`; the controller runs them with its own
   executable, the artifact its bootstrap bound, and records that
   executable's digest in the stage's context. The stage mode neither
   reads a bootstrap record nor requires its own unit: it is a
   supervised child of the final.
2. One recipe table. The stage mode takes each stage's commands from the
   catalogue's own format, lint and package tables, and `package_gates.py`
   is deleted. A witness fixture generated once from the Python at the
   base holds every converted stage's commands to `suite_stage.recipes`
   and `clippy.py`'s command construction, with placeholders, by exact
   stage name, so that no stage can be missing from a category count.
3. The Clippy comparisons keep their baselines and rule.
   `clippy` and `adapter-clippy` compare against `clippy.py`'s fixed
   published commit (`0873610926649d8c79094c1aa346fa6728c5251a`); the
   adapter and project Clippy stages compare against the record's
   baseline. Each extracts its baseline from `git archive` into the
   final's owned baseline root (`RNX_FINAL_BASELINE_ROOT`) and keeps its
   target directories, features and the Polars lint flag. The diagnostic
   identity is the same: target, level, lint code, message and the
   normalized primary files, line movement ignored, counted with
   multiplicity; a new identity or a higher count fails. A witness
   fixture of `clippy.diagnostics` on planted Cargo message lines holds
   the Rust rule to it.
4. The producer side of the stage protocol moves to Rust with it:
   publishing a semantic receipt binds the validation context
   (`RNX_VALIDATION_CONTEXT`: candidate, source, environment), keeps the
   existing-result and resume rules (a prior PASS archived before it is
   replaced, a completed FAIL never resumed), writes the receipt
   durably before its bound envelope, and writes one terminal envelope.
   The child contract recognizes a Rust producer only as the logical
   `rnx-final stage suite` or `rnx-final stage clippy` run with the
   controller's own executable; any other executable or mode is not a
   producer. Unknown stages and modes, a mismatched executable or
   context, a missing or failed semantic receipt and a stale or resumed
   failure each fail. A stage run outside a final writes an unbound
   receipt that no final accepts.
5. Scheduling does not change. Each converted stage keeps the Cargo
   lanes, weight, bulk roots and environment it has today, derived from
   the same stage classification and recipes rather than from a script
   name; a witness comparison holds every converted stage to its row in
   the 0284 scheduling fixture. The stage mode starts no scheduler and
   bootstraps no Cargo of its own; it runs inside the stage's guardian,
   jobserver and job limits.
6. `rune-suite` stays Python until its native contract cases move:
   `suite_stage.py` keeps only that stage's recipe (from the native
   manifest), its API-boundary twin checks, its `invoke`, and its
   `native_contracts` call. The column probe's `VALGRIND_INCLUDE`
   resolution moves to the Rust mode, checked before the Cargo recipe
   runs.
7. Two finished-record workflows that started `clippy.py` are retired:
   `tools/column/verify.py`, ADR 0205's prerequisite workflow, which
   `tools/column/README.md` still documents as a local command and a
   durable-launcher example; and `tools/column/borrowed_controls.py`, ADR
   0212's proportional controls, with `test_borrowed_controls.py`. Their
   acceptances are published, no stage runs them, and their Clippy step
   has no Python program left to call. The README's commands for them
   are replaced by the Rust final's workflow. Their producer
   registrations stay, for the receipts they wrote, and they join the
   tombstone check. `tools/verify.py`, the live cross-stage tool, is a
   different program and is unchanged.

## Tests and controls

| Control | After this record |
| --- | --- |
| `clippy.controls()` (path and line normalization, a second identical diagnostic, a changed message) | Rust tests over the diagnostics fixture |
| `test_suite_stage.test_polars_clippy_keeps_existing_diagnostic_as_warning` | Rust test |
| `test_suite_stage.test_probe_build_and_lint_use_configured_headers_and_refuse_missing_headers` | Rust test, with a missing header and a format-only stage |
| `test_suite_stage.test_every_registered_recipe_propagates_package_build_failure` | Rust test |
| `test_suite_stage.test_api_twins_are_required_and_cannot_be_vacuous` | stays Python, with `rune-suite` |
| `test_anchor_cache` baseline-lifetime controls (use `suite_stage.invoke`) | stay; `invoke` stays |
| `test_borrowed_controls` | retired with its workflow |
| discovery nonempty, executed count nonzero | the Rust child helpers, reused |

## Constraints

- No new third-party crate, no new Python file, no new scheduler or
  bootstrap. The complete final stays under 60 minutes.
- No change to any baseline, target directory, feature set or lint
  allowance.

## Controls

- The recipe and diagnostics witness fixtures, each with a planted
  change caught.
- Producer controls: the binding written and validated; resume and a
  completed FAIL; an unknown mode, another executable and a mismatched
  context refused; an unbound run never publication evidence.
- Scheduling: every converted stage's lanes, weight, bulk roots and
  environment equal to the 0284 fixture; a stage stopped mid-Cargo is
  drained by its guardian.
- The witness fixtures for the catalogue, scheduling and selection carry
  the command change as a reviewed successor.
- One complete Rust final.

## Out of scope

`rune-suite`, `native_contracts.py` and the remaining stage programs
(`inventories`, `cli`, `jupyter_controls`, `fixed_route_gate`,
`tools/verify.py`), each in its own record.
