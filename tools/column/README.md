# Column adapter checks

Run the ordered local checks with:

```sh
python3 -B tools/column/verify.py --scope adapter --cli --out target/0205/dev-unique
```

Every output directory must be new. The command pins source files, checks the
complete Rune pin graph, controller controls, formatting and Clippy before
tests, then checks host/Arrow behavior and routing inventory. With `--cli`, it
builds the actual adapter executable and verifies default/legacy routing, full
output equality and fuel exhaustion. Add `--release` for the production recipe;
the CLI receipt records the binary hash. Development runs are not final signed
acceptance or performance evidence.

For long jobs use the durable launcher with the declared exact memory ceiling:

```sh
python3 -B tools/column/launch.py --unit rnx-0205-unique --cwd /absolute/rnx --hosts /absolute/hosts.json -- python3 -B tools/column/verify.py --scope adapter --cli --release --out target/0205/release-unique
```

The launcher propagates PATH and SSH_AUTH_SOCK, sets MemoryMax to the JSON
`measurement.memory_bytes` value, disables swap, and kills the service on OOM.
Receipts are read after exit; no polling or progress watcher is needed. Source
and tools must remain fixed for the entire run.

After source review and signed implementation, launch fresh acceptance:

```sh
python3 -B tools/column/accept_remote.py --host "$VERIFIER_HOST" --remote-root "$VERIFIER_ROOT" --memory-bytes 25769803776 --out target/0205/fresh-unique
```

This requires a clean signed checkout, its signed parent, SSH agent and BatchMode
access. It bundles the actual commit, verifies signatures under `rnxverify`,
checks fresh workspace target directories, and blocks on the remote service's
exit. `ACCEPTANCE.json` is the remote terminal receipt; `RESULT.json` records its
hash, candidate, memory ceiling and terminal state. Failed runs retain their
logs and artifacts and require a new output name.

`accept.py` owns the gate list. It preserves the prior 23 acceptance gates and
adds the column controls, adapter Clippy, full Polars suite and production CLI
controls. All selected Clippy checks precede tests and release builds; any
failure stops subsequent stages. Inspect the exact plan without starting jobs:

```sh
python3 -B tools/column/accept.py --candidate COMMIT --out /unused/acceptance --plan
```

Clippy compares diagnostics against published rnx `c32a6ba` with the same recipe.
The reviewed existing `erasing_op` diagnostic remains counted at warning level
in both adapter recipes; new or changed diagnostics fail. New column modules
deny all warnings, apart from their reviewed, narrow error-size allowances.

`chain.py` remains the driver for reproducing the Rune prerequisite's pinned
four-build comparison (used for B2's passing existing-path chain). It is retained
for that workflow and is not invoked by rnx's `accept.py`; its manifest is not a
replacement for rnx's fresh acceptance list.
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
environment first. `inventories.py` uses the existing producer's exact feature
selection and committed locks for explicit fetches, then documents, extracts
and checks generated bindings with Cargo offline. Missing or incomplete
documentation, provenance/feature mismatches, changed locks and binding drift
fail the stage; ignored inventories from another checkout are never copied.

`resume.py --root ORIGINAL_FRESH_SOURCE --prior ORIGINAL_RESULT --inventories
INVENTORY_RESULT --out NEW_OUTPUT` supports the reviewed missing-inventory
recovery only. It verifies the unchanged original source/controller and command
plan, hashes the earlier passed logs, verifies the generated inventories, then
reruns the failed full Polars suite and every remaining gate. The original FAIL
receipt is preserved. A changed product requires a new fresh acceptance run.

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

For the reviewed successful-suite recovery, `resume.py --completed-suite
PREVIOUS_RESUME_RESULT --reviewed-oracle-sha256 EXACT_REVIEWED_HASH` additionally
binds the suite's exit-zero result, source and prior receipt; archives its report;
hashes its logs; and resumes at the next stage. It cannot reuse a failing suite.
Backlog: the generated oracle should write reports under `target/` or an explicit
regeneration flag, and unordered reports should not be committed as runtime
snapshots. That generator change is outside record 0205.

The reviewed cross-harness recovery uses `recover_cross.py --prior
FAILED_CROSS_RESUME --completed-suite REVIEWED_POLARS_RESULT --identity-pair
SAME_DIRECTORY_PAIR_RESULT --out NEW_OUTPUT`.
It checks the exact one-line `cfg(test)` amendment, hashes the earlier passed
logs and declared oracle output, runs current prerequisites and ordinary column
unit tests, and verifies the saved paired binaries against their hashes and
identical nonempty `.text` and `.rodata` sections. The baseline and candidate
must be built sequentially in one directory on one host with the same locked
production recipe. Cross-host whole-file hash equality is not a sound identity
test. It then
runs the full cross suite and the clean-clone inventory producer. No failed
cross result is reused. Ordinary unit tests remain enabled; the custom
`harness = false` cross executable excludes only their otherwise-unused helpers.
For a subsequent tooling-only amendment, `--verified-prerequisites RESULT` can
reuse that paired candidate's successful prerequisites and production CLI run;
the receipt and every stage log are hashed, and current tooling controls still
run in the final clean clone. Any product change invalidates this reuse.

Gate tools write terminal receipts and exit nonzero on failure. External launchers
may notify after exit; no messaging service is required by these tools.
