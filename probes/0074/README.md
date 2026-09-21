# Probe 0074

The frozen 0073 generator (accepted at 98885e0) against the Rust workspace
sources at Polars's Python release tag `py-2.0.0-rc.2`, commit
`da47b7405f0e2d188e71cce7c2d588c695f4098d`, as a git source. Probe only:
`adapters/polars` stays on 0.55.2.

    probes/0074/run.sh                # replay: frozen inputs verified, committed locks required, fresh outputs
    probes/0074/run.sh report         # report only
    PROBE_INIT=1 probes/0074/run.sh   # first run, creates locks (never on replay)
    PROBE_EXPERIMENTAL=1 ...          # skips the frozen check; every output labeled experimental
    probes/0074/controls.sh           # negative controls on the runner
    python3 probes/0074/report.py --self-test   # identity controls on the delta

- `frozen.json`, `freeze.py`: digests of every input the probe depends on (generator, extractor, adapter, baseline inventory and results, locks); `run.sh` refuses to start if any differs.
- `doc.sh`: rustdoc JSON at the pinned commit, the adapter's feature set, lock in `locks/doc-rc2.lock`.
- `run.sh`: stages with statuses in `out/status.json`; an infrastructure failure invalidates the run, a compile failure or failing oracle cases are results.
- `report.py`: pins, denominators, identity-based API delta, accounting delta, compilation, oracle differences, excluded cases, fixture census, the join diagnostic.
- `diag_joins.rs`, `diag-join-order.rn`: the labeled join diagnostics outside the classifier.
