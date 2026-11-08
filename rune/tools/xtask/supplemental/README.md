# Supplemental Miri runner (ADR 0210)

This pool supplements the canonical verifier; it does not replace its 98 rows.
Run formatting and Clippy first, prepare both feature groups once, verify the
exact live test inventory, then admit memory-bounded exact-name jobs.

Launch a cold run from a clean, signed checkout:

```sh
python3 rune/tools/xtask/supplemental/run.py --root "$PWD" \
  --out "$PWD/target/xtask-verify/supplemental-cold" \
  --launch --unit rune-supplemental-cold --require-overlap
```

External launchers may notify on the terminal receipt. Do not poll.
Await the service exit directly. `--workers` only reduces the
resource-derived limit. Launch enforces MemoryMax, zero swap and group cleanup.

After an interruption, use the same output path and identity with `--resume`.
Completed PASS artifacts are rehashed; incomplete attempts remain in place and
receive new attempt directories. A completed FAIL requires investigation and a
new run; resume cannot retry it. No source/tool/manifest changes may reuse PASS.
Every invocation has a unique terminal receipt; no previous receipt is replaced.

Shared Cargo-home package-cache startup lock syscall times are recorded, not
forbidden. Prepared targets are copied per job; rebuilds or build-directory
lock waits fail. Actual interpreter lifetimes come from kernel traces. Full
coverage includes ignored tests and checks their status against the manifest.

N5, method closures and budget errors use singleton exact-name shards, since
each has few active tests and long total time. All remaining subsets retain
their exact-name batches. Singleton jobs expose per-test timings without guessing
unknown durations. Further balancing can use measured times; one indivisible
test remains a lower bound on elapsed time. Ignored tests remain named jobs.

The measured maximum individual-descendant RSS across seven subsets was
3,288,178,688 bytes. Round to 4 GiB and add 25%: reserve 5 GiB per interpreter.
This RSS statistic is not an aggregate cgroup peak; the margin covers process
overhead and variation. Admission uses CPU affinity and the smaller of
MemAvailable and all ancestor cgroup headrooms, less a 2 GiB reserve. Refuse to
launch when one interpreter cannot fit. The pinned reservation is included in
run identity; memory observations and selected workers are in invocation receipts.

Controls run with:

```sh
python3 -m unittest discover -s rune/tools/xtask/supplemental -p 'test_*.py'
```

## Proportional tooling acceptance

The default `--scope full` still runs all seven subsets. For runner-only changes,
`--scope tooling-short` prepares both configurations cold and executes the real
payload-scan and budget subsets concurrently. It requires actual interpreter
overlap, runs the same durability/kill-resume/identity/isolation controls, and
labels the receipt as short tooling acceptance, not full test coverage. Scope
and overlap policy are part of identity; short receipts cannot satisfy full runs.

Set `CARGO_INCREMENTAL=0` in preparation and execution: copied harnesses otherwise
retain absolute incremental-cache paths into the shared preparation. Preparation
rejects such captured paths, and final rehashing checks preparation immutability.
The initial full-run failure remains preserved as failed isolation evidence.
