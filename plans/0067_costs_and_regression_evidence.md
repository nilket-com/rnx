# 0067 gate 5: one-install costs and regression

Ready for closing review on Linux. This checkpoint changes READMEs and
plan/evidence text only. Product code is `1b894e0`, unchanged from accepted
Gate 3 `7ae8863`; the stock baseline is `94f5f3f`. No product correction was needed.

Bench: [`d18907a`](https://github.com/nilket-com/rnx-bench/tree/d18907a/results/one-install-final-0067),
with replay instructions in `probes/one-install-final/README.md`. Published
commits, the roster Git bundles and an isolated profiling patch preserve the
measured sources. All samples and fixture corrections are retained.

## User launch

The ordinary product measurement passes every matched Git-versus-path cell.
Compact medians across run, eval and first prompt, both repeats, in milliseconds
**over the same executable launched directly**:

| Adapters | Git source | Path source |
|---:|---:|---:|
| 0 | 1.89 | 13.87 |
| 1 | 2.19 | 14.87 |
| 2 | 2.35 | 15.22 |
| 3 | 2.53 | 15.56 |

The 1–2 ms prediction does not hold at every roster size; improvement over the
matched path workflow does. The zero-adapter row still includes the runtime.
That frozen tree has 481 tracked files / 7,488,049 bytes. Polars contributes
23 files / 984,868 bytes, PostgreSQL 21 / 517,390, and the complete renamed
PostgreSQL copy 21 / 517,384. These files overlap the runtime inventory; they
are not four independent byte totals.

The six path first-adapter increments are 1.015, 0.991, 0.904 ms in repeat 0
and 1.065, 1.053, 0.864 ms in repeat 1 (run/eval/prompt). They remain visible,
with 0065's qualification unchanged and its threshold neither relaxed nor
reopened. This is the current source snapshot, not the old 443-file fixture.

The journal retains 3,360 validated samples: 48 launch cells plus eight explicit
cost cells, two repeats of 30, fixed interleaving seed, one pinned core, one
Polars thread and two warmups. Per-cell medians and p10/p90 spread are retained.
No builds ran alongside the measurement. Every Git roster trace has exactly two
successful execs, manager and artifact, no native-source opens and no Internet
connections. Every attachment also passes positive-controlled compilation traps.

The isolated profiler is additional evidence, never the headline product. Its
unchanged standalone control, profiled counterpart and stock frontend are
compared in 720 separate samples. Inside `git_launch`, lock/pair decoding costs
about 0.42–0.63 ms, input/context checks 0.56–0.88 ms, receipt/ready/artifact checks
0.14–0.19 ms and command construction around 0.001 ms. Total is 1.1–1.7 ms.
CLI dispatch, project opening, envelope detection and process/exec costs are
outside those disjoint intervals. Medians need not add, and the full profiler
and control process clocks are retained.

## Stock startup and explicit costs

Against `94f5f3f`, all five stock workloads remain below the 5% stop in both
repeats. Version changes +0.033/+0.020 ms; eval 42 +0.097/+0.092 ms;
JSON 10k +0.026/+0.025 ms; first prompt +0.057/+0.037 ms; ordinary cells
+0.009/+0.003 ms. These are 2,400 retained, output-verified samples, with
100 per process cell/repeat and 200 per persistent-cell/repeat. The largest
percentage is 2.54%. Python spawn/capture/wait or PTY creation is included;
cell timing excludes startup. No speedup is claimed. Stock executable size
increases from 15,288,760 to 18,670,080 bytes, measured separately.

One-native full-command medians, with both repeats agreeing closely:

| Operation | Git source | Path source |
|---|---:|---:|
| eval with `--verify` | 320 ms | 57 ms |
| offline lock | 1,303 ms | 433 ms |
| attachment to ready assembly | 2,025 ms | 178 ms |
| startup-probe child | 6.57 ms | 6.54 ms |

Faster everyday launches do **not** make full authentication or preparation
cheap. A separate ten-sample diagnostic of Git `--verify` attributes about
272 ms to input verification, 40 ms to artifact checking and 0.56 ms to lock
reading. This is not mixed into the uninstrumented product medians. Repeated
full checks in preparation remain a possible follow-up, not a hidden optimization
or changed coverage in this checkpoint.

An isolated offline cold Polars target builds in 114.6 seconds, with Git/registry
sources cached and no competing fixture build. Roster setup times are separate;
some overlapped checks and are not advertised as cold-build comparisons.
A real published-origin Cargo fetch takes 4.73 seconds untraced into a fresh Git
home. A separate trace records 2,226,360 received TCP bytes and a 2,176,658-byte
retained Git pack. Transport includes protocol overhead; registry sources were
cached. The traced time is not substituted for the untraced observation.

Cargo Git storage for the fixture retains 11.30 MB allocated / 10.12 MB logical.
Whole assembly entries retain about 0.56 GB for zero adapters, 1.58 GB for Polars
and 1.61 GB for two/three adapters. Exact unique-inode/byte totals are reported
per owner. Registry storage and benchmark targets are excluded. The default
creates no runtime store; 0066 removal remains explicit and does not delete
Cargo-owned sources.

## Regression and scope

Root suites pass serially at 376 default, 419 test-support and 389 runner-only;
management passes at 51/52 with two ignored integration tests each. Root/tool
formatting, management strict all-target clippy in both configurations, combined
notices, selfcheck and final packaged-manifest checks pass. The package check
continues to state the unpublished internal-dependency limitation; it is not a
claim of crates.io packaging or release eligibility.

Feature graphs and actual generated/embedding artifacts omit management where
required. The external consumer passes assembly, failure, reset and worker
contracts. The ordinary HTTP/PostgreSQL server passes echo, real commit/rollback,
ignored test-only injection and clean shutdown with zero client backends. Native
unit tests and notices pass; Gate 4's accepted notebook and interactive journeys
remain at their measured checkpoint.

Root strict clippy retains the baseline's 13 production and two test-only
findings, with no additions in a real baseline comparison. PostgreSQL all-target
clippy reports one test-module placement lint in source byte-identical to the
baseline. Polars/server strict all-target clippy passes. These qualifications
are recorded rather than reported as clean checks.

Three harness corrections are explicit: the copied third adapter's help namespace,
an adapter notices-script filename, and Git/path attachment-success wording in
warm-up. The initial roster bundle/logs and observed output are retained. No
headline samples were discarded and no product defect was found. The temporary
baseline worktree is removed; historical fixture worktrees remain untouched.
No fixture processes remain.

The READMEs now begin with the tested one-binary Git install and `:dep polars`,
without a companion or runtime-install prerequisite. The reviewer's missing-Cargo
wording nit is retained for later, without expanding closure scope. Remaining
limits include explicit full-check cost, trusted Cargo checkout contents on the
default path, developer-path Git-variable fallback/depth costs, publication and
Windows execution. This is a Linux closure candidate, not those later claims.
