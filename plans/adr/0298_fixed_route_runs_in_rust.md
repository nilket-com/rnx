# ADR 0298: Fixed-route runs in Rust

Status: Proposed

Base: published 7991e30de16863b07289a8423678e8831c7687f4 (ADR 0297).

## Problem

`fixed-route` is the final's last Python stage program:
`tools/column/fixed_route_gate.py`, over `drift_band.py` and
`anchor_cache.py`, which ADR 0296 ported to `tools/final/src/fixed_route.rs`
but nothing calls yet.

## Decision

1. `rnx-final stage fixed-route --out DIR [--cargo PATH] [--valgrind-root
   DIR] [--anchor-only] [--import-anchor PATH]` runs the gate's sequence
   over the 0296 port:
   - the valgrind root (`stage::valgrind_root`);
   - the record (`selection::record`) and a clean candidate;
   - the policy, validated and with its anchor's approval;
   - the `execute-guard` build and its location, and the source
     condition;
   - the mirror check;
   - the measurement recipe;
   - the anchor build, then the cache lookup, the legacy import from
     `target/fixed-route-anchors/<key>`, or a fresh capture, then
     publication, and the anchor's recipe check;
   - unless `--anchor-only`, the record's paired build, its captures and
     the score;
   - the unchanged source and head;
   - the retained semantic logs, failure retention through
     `failure::copy_bundle`, and the receipt published through the
     producer protocol.
   Inside a final, the payload is `RNX_FINAL_FIXED_ROOT`
   (`producer::assigned_root`), as now.
2. Measurement identity is preserved exactly:
   - the recipe (CPU lines, toolchain digests, run environment and its
     identity, the build environment's identity, keys, allowlist and
     prefixes, the policy subset, tooling and fixture digests);
   - `capture_key`, so the live cache's entries stay hits;
   - `reuse_anchor` and `verified`.
   `paired_build.build_environment` (with `BUILD_KEYS` and
   `BUILD_PREFIXES`) and `ir_pool.capture_environment` are ported for the
   recipe. Their Python stays for the children that still use it.
3. The children stay Python, run as the gate runs them: `paired_build.py`
   and `fixed_route_vectors.py` through the stage wrapper (log, receipt,
   stage row, retained failure), and `executor_identity.py` when the
   revisions differ (exit 0 or 1 an observation, its error fatal). They
   run under the controller's guardian, which drains each one's group.
4. Retired:
   - `fixed_route_gate.py`, `drift_band.py`, `anchor_cache.py`,
     `test_drift_band.py` and `test_anchor_cache.py`, whose remaining
     cases (the build environment, capture key, anchor reuse) become Rust
     witnesses;
   - the gate case of `test_executor_identity.py` (identity tool error
     fatal, observation continuing), which becomes a Rust control;
   - `test_valgrind_resolution.py`, whose cases `stage::valgrind_root`
     controls already cover (checked when retiring);
   - ADR 0296's two-way handoff control, whose Python side retires.
   The catalogue, scheduler lanes, producer lists, Python list, README and
   tombstones follow.

## Constraints

- The same commands, children, limits and resources; the anchor cache's
  root, journal and bundle; the 60-minute limit.
- No new crate, no new Python file; the Python list only shrinks.

## Controls

- Witnesses from the published Python at 7991e30d with controlled values:
  - `build_environment` (unrelated variables ignored, unknown overrides
    refused, path and locales stabilized), `capture_environment` and the
    recipe's identities;
  - `capture_key` over the test's cases, plus equality with the Python
    key for the same recipe;
  - `reuse_anchor` and `verified` over planted legacy captures;
  - the gate's sequence over intercepted children and git: a cache hit,
    a miss with capture and publication, a legacy import migrated and
    failed, `--anchor-only`, a stage failure retained, a dirty tree, an
    approval refusal, an identity tool error, and a source change during
    the gate.
- A mutation per refusal family fails its control.
- One complete Rust final, in which fixed-route runs for real against the
  live cache.
