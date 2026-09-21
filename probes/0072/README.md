# Probe 0072: binding surface

Measures how much of the Rust Polars API a generator could bind for Rune,
and proves the rules on compiled, executed samples. Evidence and the
answer are in `plans/0072_binding_surface_evidence.md`; this directory is
the re-runnable tool.

    probes/0072/run.sh            # everything: docs, control, inventory, samples, tables
    probes/0072/run.sh report     # tables only, from existing output

Requires the pinned nightly (`rustup toolchain install nightly-2026-09-20`)
for rustdoc JSON; everything else is stable. `run.sh` fails on any failing
step: an incomplete documentation set, a control mismatch, a sample that
does not execute and match, a callback control with the wrong outcome, or
a self-check in which an injected wrong oracle fails to fail the run.

| part | what it does |
|---|---|
| `inventory/doc.sh <release> <adapter\|full>` | rustdoc JSON for `polars` and every `polars-*` crate it activates, into `out/<release>-<cfg>/` (cleared first, ignored by git) with `pins.json`; `full` is every declared feature; locks kept in `inventory/locks/` |
| `control/` | extraction control: a facade over two dependencies with every re-export and item shape, `expected.json` written first, `check.py` compares |
| `extract/` | `surface <docs-dir> polars <out>`: reachability walk, deduplication, classification by the rule table in `classify.rs`; writes `inventory.json`, `summary.json`, `summary.md` |
| `samples/gen.py <inventory> --build [--release R] [--inject-failure sNNN] [--inject-broken-reuse sNNN]` | deterministic sample selection, generated Rune wrappers plus Rust oracle tests in `samples/harness`, compile loop, `results-<R>.json` with generator identity; callbacks go through `Function::into_sync` inside the wrapper, with constant-capture, native-capture and error-propagation controls; `--inject-failure` and `--inject-broken-reuse` are the self-checks; builds run `--locked` against `samples/locks/<release>.lock` |
| `report.py` | the Markdown tables the evidence quotes |

Rules are judgments; the program applies them. Counts they produce are
predictions. Only `samples/results*.json` is demonstrated.
