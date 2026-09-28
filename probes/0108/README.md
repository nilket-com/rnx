# Probe 0108

The Polars v2 Rust API baseline. v2 means the Rust workspace at Polars's Python
`py-2.0.0-rc.2` tag, commit `da47b7405f0e2d188e71cce7c2d588c695f4098d`, built with the
feature set of the Python wheel. The production adapter stays on 0.55.2.

    probes/0108/features.sh        # the wheel's polars features (polars-runtime-32) -> features.txt
    probes/0108/doc.sh <rev> v2py  # rustdoc JSON at those features (lock: locks/doc-v2py.lock)
    target/0072/extract/debug/surface probes/0108/out/v2py polars probes/0108/out/v2py/result
    probes/0108/build.sh           # current generator + v2.toml -> scratch adapter, build, direct oracle,
                                   #   then the --no-default-features build and tests (PROBE_STAGE=no_default: that stage only)
    python3 probes/0108/census.py  # three denominators, root causes, 0.55.2 diff, migration
    python3 probes/0108/batches.py # every missing callable under one proposed rule
    probes/0108/python.sh          # hash-pinned wheel; asserts its _BUILD_COMMIT is da47b74 (provenance only)
    probes/0108/controls.sh        # the no-default stage fails the run on a failing or silent test command (fake cargo, temp outputs)
    probes/0108/verify.sh          # replay the committed evidence: digests, then census and batches byte for byte

`out/` is not tracked (107 MB of rustdoc JSON). The outputs the evidence quotes are
committed compressed in `evidence/` (1.9 MB): the v2 inventory, pins, surface, oracle
results, census and batches, and the two hash-pinned 0.55.2 baselines (the 0106
production surface and the adapter-narrow inventory). `digests.txt` pins every one.
`census.py` never reads the moving worktree: the baseline surface comes from commit
1eecbf1 (or the bundle), and it checks the inventory's feature provenance against
features.txt and the surface's release hash against v2.toml before counting.
