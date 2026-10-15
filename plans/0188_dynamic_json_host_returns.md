# rnx 0188: dynamic JSON host returns

Enrol `json::parse` as a fresh dynamic-return host by changing its registration
to `module.fresh_host_function("parse", parse)?`. Preserve its handler and
signature. Advance both Rune manifest pins from
`27d8b6711fa15a20726ebefe4c6ed3934a56190b` to
`0dcbba74c8430e09befa768429f6342f337d5219` (C10).

Stock remains the runtime route. This record adds neither a fresh default nor
fallback routing. `fs::metadata`, `time::parts` and `env::vars` remain unenrolled;
all other registrations and host bodies remain unchanged.

## Pin inventory

Synchronize the root and project-tool manifests, all eight active lockfiles
(root, tools/project, adapters/polars, adapters/candle, adapters/postgres,
demos/polars, demos/candle and servers/http-postgres), the root and Jupyter
worker RUNE_VERSION constants, and the release-metadata assertion. Lock changes
are Rune-family source revisions only: no package, version or dependency-edge
changes. Historical probe locks and the Jupyter lock remain unchanged.
The implementation contains exactly these 14 paths, including src/json.rs.

## Reviewed validation stages

1. Review this plan, then make the signed subject-only plan commit. Apply the
   reviewed 14-path package in one isolated worktree from d9e2be6b. The source
   package already passed locked metadata resolution in all eight workspaces.
2. Run the root suite on Rust 1.98.1. Validate project-sources/test-support on
   both 1.98.1 and the declared Rust 1.95 floor, the project tool and relevant
   standalone workspaces, and the Jupyter pin and worker tests.
3. Run formatting and normal Clippy, retaining the existing deprecation
   baseline. Run Polars callbacks, stored notebook goldens and negative controls
   without changing goldens. Candle is compile-only. Do not execute the live
   H0 corpus. Report any failure or output delta for review before a fixture
   edit or retry.
4. After gates, submit the implementation for verification as one signed
   subject-only commit. Gate documentation and reviewed fixes amend that
   implementation commit; no microcommits. Claude verifies and pushes both
   commits. Remove this record's worktree at close.

## Existing C10 evidence

The C10 compile-only census reports 41/55 corpus
admissions (previously 39/55; additions 0142 and 0143). The separately reported
H0 case 0207 advances from 4/7 to 5/7. Six other frontiers become sort_by method
refusals; the 30-function allowlist and script-shadow control are unchanged.
This is compile-admission evidence, not a runtime or performance claim.
Rune C10's strict 126-row contract, allocation-failure and Miri gates passed;
the real companion compatibility gates above remain required.
