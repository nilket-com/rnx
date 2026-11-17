# Column adapter checks

The column adapter's checks run as stages of the Rust final (its format,
Clippy, suite and CLI stages); run them with the final's entry, inside its
own systemd unit:

```sh
sh tools/final/final.sh --complete --out target/RECORD/FINAL
```

ADR 0205's ordered prerequisite workflow was
retired in ADR 0287. Development runs are not final signed acceptance or
performance evidence.

For other long jobs use the durable launcher with the declared exact memory
ceiling, as `tools/column/launch.py --unit NAME --cwd ROOT --hosts HOSTS --
COMMAND`.

The launcher propagates PATH and SSH_AUTH_SOCK, sets MemoryMax to the JSON
`measurement.memory_bytes` value, disables swap, and kills the service on OOM.
Receipts are read after exit; no polling or progress watcher is needed. Source
and tools must remain fixed for the entire run.

Remote acceptance on a separate verifier host is retired (ADR 0284); a new
launcher needs its own record and an end-to-end check on that host.
`child_result.validate_remote` still validates the historical remote finals.

The stage list belongs to the Rust final controller
(`tools/final/src/catalogue.rs`). All selected Clippy checks precede tests and
release builds; any failure stops admission. A final starts from its entry,
inside its own systemd unit:

```sh
sh tools/final/final.sh --complete --out target/RECORD/FINAL
```

Clippy compares diagnostics against published rnx `c32a6ba` with the same recipe.
The reviewed existing `erasing_op` diagnostic remains counted at warning level
in both adapter recipes; new or changed diagnostics fail. New column modules
deny all warnings, apart from their reviewed, narrow error-size allowances.

Performance evidence additionally needs the declared measurement lease,
affinity, readiness, thermal and PMU checks; a successful functional receipt
alone makes no performance claim.

## Existing-path instruction checks

The repository-owned controller measures the first outer execution invocation,
arming the legacy VM boundary only after `drive_async` begins, after compilation.
Global collection includes joined-worker callbacks. Startup and compilation are
outside this interval. Supply a receipt binding both production binaries to their
source revisions, build commands and SHA256 values:

```
python3 -B tools/column/old_path.py --mode preflight \
    --build-receipt /absolute/build/RESULT.json --rune-root /absolute/rune \
    --out /absolute/new/preflight
python3 -B tools/column/old_path.py --mode full \
    --build-receipt /absolute/build/RESULT.json --rune-root /absolute/rune \
    --out /absolute/new/full
```

Run preflight first, then full only after reviewing its terminal receipt. Both
commands require the unchanged measurement lease/readiness limits and a unique
output directory; launch with `launch.py` under the declared memory ceiling.
They emit terminal notifications and do not poll jobs. A later source amendment
is accepted against a build receipt only when its entire diff is confined to
`tools/column/` or the inventory producer `probes/0072/inventory/doc.sh`;
all product changes require new build evidence.

Every callback capture must contain at least one nested VM invocation per row,
nonzero callback-bridge instructions and nonzero joined-worker instructions.
Both binaries must show positive instruction growth as work increases before
the candidate's extra growth is compared against the unchanged `<= 0` rule.
The negative controls reject the former vacuous 1,514-instruction capture,
missing callback or worker work, insufficient callback calls, and constant-cost
profiles. All `test_*.py` controls run before expensive acceptance stages.

Fresh acceptance generates the three Polars 0.55.2 documentation inventories
(`adapter-json`, `adapter`, `adapter-narrow`) immediately before the full Polars
suite. Install the pinned `nightly-2026-09-20` toolchain in the verifier
environment first. The `polars-inventories` stage (`rnx-final stage
inventories`) uses the existing producer's exact feature selection and
committed locks for explicit fetches, then documents, extracts and checks
generated bindings with Cargo offline. Missing or incomplete
documentation, provenance/feature mismatches, changed locks and binding drift
fail the stage; ignored inventories from another checkout are never copied.

Record 0205 review seq26301 accepts the observed legacy callback inlining drift
as `ACCEPTED_ATTRIBUTED`, not a raw PASS. Raw comparisons still use `<= 0` and
remain in the receipt. The publication review must separately bind this sole
exception to the disassembly and observed function costs; no other path is
exempt. Explicit inlining of the small integer-conversion helper is deferred to
the next Rune runtime record, with its own existing-path checks.

The full Polars suite currently writes a tracked oracle report. Only that
stage may produce `adapters/polars/oracle-results.json`: the controller archives
its full bytes and hash, verifies the case IDs, paths, policies, flags, counts,
tally and approved outcomes, then restores the committed report before checking
source identity. Every other mutation remains a failure; test exit codes remain
unchanged. The shared helper has negative controls for source edits, wrong
stages and invalid outcomes.

Gate tools write terminal receipts and exit nonzero on failure. External launchers
may notify after exit; no messaging service is required by these tools.

## Record acceptance (ADR 0232)

The Rust controller (`tools/final`) selects a record's stages from its plan
and implementation commits, computing the whole record diff from the plan's
parent; callers cannot choose a narrower baseline. Changes under `src/`,
`rune/crates/`, `tools/project/` or `tools/verify/`, or to root or Rune Cargo
manifests and locks or `tools/final/src/cross.rs`, require `cross`; renames count both
names. It runs each stage through the child protocol below and finalizes the
candidate, current source, policy, canonical commands, required stages, log
hashes and child receipts. `gate_policy.record` remains only as the record
identity that stage programs read. Development controls alone are not
publication evidence, and the final PASS is the documented publication gate,
not a Git push hook.

Receipt-producing Python children are explicitly registered in `child_result.py`.
They must publish their semantic result with `publish`, or bind an existing
controller-owned result with `bind` (the Rust cross controller). Raw command
adapters have an explicit allowlist and retain exit status plus durable logs;
an unknown producer cannot silently become a raw command. Producers require an
explicit `--out`.
A fresh invocation context binds source content, executable bits, candidate,
command and declared recipe environment. Only allowlisted key names and a digest
are stored, never environment values; terminal/session/SSH variables are excluded.
Success requires both exit zero and a fresh bound PASS.
Missing, malformed, stale and failed receipts fail closed. Artifacts are synced
before terminal receipts. Existing attempt directories are never overwritten.

An explicit producer `--resume` may replace a prior PASS only after archiving
its exact original bytes; a completed FAIL is not reusable evidence. The resumed
producer remains responsible for validating each reused stage. Parent evidence
also verifies the original logs and context. Historical receipts without this
binding cannot be retroactively promoted to final acceptance. Historical remote
acceptances bound a fresh invocation, candidate, source and bootstrap recipe
and required remote finalization. Existing measurement-specific binary/profile checks remain
in force alongside these controller contracts.

The aggregate column tool-test command is:
`python3 -B -m unittest discover -s tools/column -p 'test_*.py'`.
Its protocol test has a unique module name so imports from the measurement
suite cannot shadow it.
