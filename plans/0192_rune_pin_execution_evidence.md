# 0192: Rune pin and execution API evidence

The root and compiler-only project tool now pin Rune
`f736a56bd2999bed1eb9fefd74e0ce9672f89c68` (R2), advancing from
`0dcbba74c8430e09befa768429f6342f337d5219`. The root enables `execution`
with `std` and `fmt`; the tool retains `std` and `anyhow`. The seven host
registration files use the renamed API. `json::parse` declares
`rune::host::OutputCertificate::JSON_RESULT`. The default runner remains the
stock VM, and no adapter gains execution-engine enrollment.

All external evidence paths below are under `/home/me/work/.reviews/`.
Correctness jobs ran serially on Strix; releases and measurements ran on
Nano through `gate`, with direct blocking `sluice` waits. No push endpoint
was registered. Chatd decisions in `rnx-0192` are seq22993 (project pin),
22998 (root diagnostic tests), 23003 (Polars diagnostics and inventories),
23008/23013 (pre-existing allocation failures), and 23018 (observer adaptation).

## Lock and notice delta

Eight active locks changed: root, `tools/project`, the Postgres/Polars/Candle
adapters, the Polars/Candle demos, and `servers/http-postgres`. Each changes
only the source identity of seven Rune workspace packages: `rune`,
`rune-alloc`, `rune-alloc-macros`, `rune-core`, `rune-macros`, `rune-tracing`,
and `rune-tracing-macros`, all still version 0.15.0. These are 56 source
identity changes total. No registry package versions, other dependencies,
or dependency edges moved. Locked metadata resolves one Rune source identity
per graph. Historical probe locks and Jupyter's independent lock are unchanged.

Evidence: `0192-rnx-live/LOCK-DELTA.json`, `METADATA2-RESULTS.json`, and the
per-workspace `*-2-metadata.json`. The earlier mixed-pin metadata attempt
is retained; aligning the project tool removed the duplicate old/new graph.

Root, Postgres, and Polars notice generators and checks passed with no tracked
notice changes. Jupyter's check passed (34 packages, 33 distinct texts).
Candle has no separate notice generator; the initial attempt to call one
failed as a packaging error and is retained in `NOTICES-RESULTS.json`.
Final check results and streams are in `0192-rnx-live/FINAL-CHECKS.json`.

## JSON output certificate

`parse` still returns `Result<rune::runtime::Value, String>`. Its exhaustive
`from_document` match consumes an owned serde JSON tree and creates only unit,
bool, i64, u64, f64, owned strings, vectors, and string-keyed objects. Children
are recursively converted; allocation/parser failures become owned error
strings. The outer result adds only `Ok` or `Err(String)`. The constructor
cannot produce nominal or opaque values, tuples, options, cycles, or retained
host aliases. The parser's existing recursion limit remains. This is exactly
the closed `JSON_RESULT` domain; `stringify` stays unenrolled.

## Real-registration census

`0192-rnx-census1/RESULT.json` reports **244 unchanged, zero changed**.
`OBSERVATIONS.json` is byte-identical both to the R2 certified copy control
and the original S7b1 certified copy control, including all nine JSON cases:

```
f5dccd6009e68ea051a33a74c5c674d8a821d776f8c79a4c23af1399dd530886
```

This is the SHA-256 of both complete observation files. All admission,
diagnostic, resolved-item, registry, and lifecycle fields are included.

The test export adds only the compile-only observer and its `#[cfg(test)]`
module hook. All seven registration files are the candidate's actual bytes;
there are no registration overrides or Rune path substitutions. It uses the
committed lock and the Git dependency at f736a56b. `REAL-REGISTRATION.json`,
`HARNESS-PINS.json`, compiler-artifact records, and `SOURCE-BINDING.json` bind
the export, binary, and Rune compilation units; DWARF source names resolve
to that Git checkout. Every input is hash-checked. No census program executes.

## Native suites and checked outputs

Counts include unit, integration, and documentation tests in each command;
rows overlap and are not summed as independent coverage.

| Check | Passed | Failed | Ignored | Evidence |
|---|---:|---:|---:|---|
| Root default | 478 | 0 | 2 | `0192-rnx-gates2` |
| Root project-sources + test-support | 543 | 0 | 3 | `0192-rnx-gates2` |
| Project tool | 105 | 0 | 2 | `0192-rnx-gates2` |
| Jupyter | 27 | 0 | 0 | `0192-rnx-gates2` |
| Postgres adapter | 5 | 0 | 0 | `0192-rnx-gates2` |
| Polars adapter | 284 | 1 pre-existing | 2 | `0192-rnx-gates3` |
| Candle adapter | 102 | 4 pre-existing | 8 | `0192-rnx-gates3` |
| Root MSRV 1.95.0, both features | 543 | 0 | 3 | `0192-rnx-gates4` |
| Final root library | 232 | 0 | 3 | `0192-rnx-final-gates1` |
| Final release metadata | 13 | 0 | 0 | `0192-rnx-final-gates1` |
| Final Polars clone contract | 7 | 0 | 0 | `0192-rnx-final-gates1` |

Rust 1.98.1 root clippy passed, retaining existing deprecation warnings.
Jupyter, both adapter demos, and the HTTP/Postgres server built successfully.
Formatting passed for root, project tool, Jupyter, and Polars. Model-dependent
Candle tests remain ignored; no model downloads or accelerator coverage is
claimed.

The three root demos pass their byte-exact transcript checks and independent
numerical controls. `0192-rnx-notebooks1` verifies the stored 01–03 notebooks
on plain and Polars workers and 04 on Polars, including repeated fresh kernels,
restart controls, terminal references, and independent CSV checks. All checker
controls pass. Notebook and demo goldens remain unchanged. The first notebook
launch lacked its Python environment on Strix; the continuation installs the
repository's exact pinned requirements, recorded in `pip-freeze.txt`.

Polars's generated tests initially lacked ignored 0072 inventory fixtures.
The existing files were copied unchanged with `INVENTORY-PINS.json`; all 18
generator checks then passed without regeneration. Its 3,997-case generated
oracle passes under existing policies. The report writer changes three rows
relative to the committed report: `unique` and `unique_generic` show permitted
unordered row-order differences; `get_concurrency_limit` is 30 rather than 28,
matching Strix's admitted CPU count. The other 3,994 rows are exact. See
`0192-rnx-gates3/ORACLE-DELTA.json` and the separately saved before/after reports.
No generated report is copied into the product worktree or committed.

## Changed diagnostic expectations

Every changed test expectation maps to an approved Rune record. Tests assert
specific diagnostics; no generic normalization or fallback was added.

| Consumer | Previous expectation | Current expectation and approved Rune record |
|---|---|---|
| `runner` unavailable origin | Missing-instance hash text | `Type std::i64 has no method named missing` (with Rune's backtick quoting), D1 a7d85572; retains broken.rn attribution, exact original diagnostic for absent/different origins, and structured method hash assertion |
| `session` unmapped retained origin | Numeric method hash in Display | Same exact D1 named diagnostic, origin None; caller uses `decoy`, which must not appear |
| Polars `arrow_values` | Old moved-value read wording | `Cannot read the value: it has been moved`, D1c 98a2da30 |
| Polars `clone_contract` | Old moved-value take wording | `Cannot take the value: it has been moved`, D1c 98a2da30 |
| Polars `json_boundary` | Missing-instance hash text | Exact named `to_json` diagnostic for DataFrame and Series, D1 a7d85572 |
| Polars `protocols` | Unsupported unary HASH wording | `Cannot apply HASH to polars::JoinType` (with Rune's backticks), D1d 1743ba88 |
| Polars `wide_shim` | Typed and raw displays equal | Exact typed `Cannot take argument 3: it is being read` and raw `Cannot take the value: it is being read`, D1c 98a2da30 |

Version identity constants and the release-metadata assertion now report the
new Rune pin. Ordinary instance-creation uses of “fresh,” script names,
upstream identifiers, and historical records stay unchanged. STYLE's engine
term is now “execution engine.” Touched files follow its American-spelling
rule, preserving public API names and established external diagnostic strings.

## Pre-existing allocation failures

All five tests below fail at the same assertion and exact byte count on the
pristine ea343850 baseline with its old Rune pin and on the candidate, on
Strix with the same compiler, features, and profile. Claude's seq23008/23013
disposition is **unchanged from baseline**, rather than a passing allocation
cap or an unexplained new regression. No cap or test body was relaxed.

| Test | Refusal peak bytes, both pins |
|---|---:|
| Polars `dense_alloc::refusals_come_before_the_buffer` | 2,097,363 |
| Candle `alloc::refusals_come_before_the_export` | 2,089,120 |
| Candle `count_alloc::an_oversized_list_is_refused_before_any_table_is_built` | 2,097,328 |
| Candle `cross_encoder_alloc::refusals_allocate_nothing_proportional` | 524,528 |
| Candle `nli_alloc::refusals_allocate_nothing_proportional` | 524,528 |

Raw evidence is in `0192-rnx-alloc1`, `0192-rnx-alloc2`,
`0192-rnx-candle-baseline1`, and `0192-rnx-gates3`. The first allocation
controller stopped after the baseline because an earlier generated-oracle
run had written its report inside the candidate export. That report and
failed preflight remain preserved; the pristine candidate rerun used a new
export and reproduced the exact failure independently.

A separately pinned, test-only counting-allocator trace locates the Polars
allocation in Rune's stock `Worklist::push -> Vec::dismantle -> Vm::pop_call_frame`
cleanup of the oversized script list, not in the dense export buffer. All five
cases are queued for one follow-up after 0192: determine whether Rune should
avoid proportional flat-vector cleanup worklists or Rnx should measure only
its own attributable allocations. That choice and any fix are outside 0192.

## Host-enrollment replay

Both ea343850 and the candidate pass the adapted, identical observer against
the frozen 0187 corpus: 110 stock rows match the old oracle; 110 execution
rows pass, with 107 exact rows, two retained compile-time arity refusals, and
one retained StoreCapacity witness. All five runtime-table/reuse controls,
four pure variant helper checks, and four direct projection crosschecks pass.
The complete fuel frontier is 0–66; 66 is the first successful budget with
all 64 writes. Two subject classifier rejects, six malformed fixed strings,
and two invalid chains pass their rejection controls. Clock values retain
the declared wall-clock envelope; stderr and filesystem effects remain exact.
The existing JSON tests from 0188 also pass in the root suite.

Evidence: `0192-rnx-enrollment-base4`, `0192-rnx-enrollment4`, and
`0192-rnx-live/ENROLLMENT-AUDIT.json`. Each run pins 2,855 raw files.
Baseline classifier fixtures needed a packaging continuation in
`0192-rnx-enrollment-cont4`; the candidate passes all three phases directly.

The H1a-era observer needed explicit C9 compatibility for `HostExpectedType`
and `HostExpectedNumber`. It preserves primary/argument/Display/capture/site
comparisons and verifies payload hashes against compiled native type hashes.
The added Ordering scalar arm rejects unreviewed output. Both runs use the
same observer SHA-256 `835d4e81ac4ae283a9ca300e8560b27e34cf45f91df5094bc3996652940251bd`.
Corpus bytes, frozen projections, and fail-closed defaults are unchanged.
The initial compile failures and the old classifier's row-097 refusal are
retained in enrollment attempts 1–3; no product fix was needed.

## Nano release and wall time

`codex-rnx0192-release1` built the old source-bound baseline and candidate
with committed locks; `codex-rnx0192-release2` built the final candidate export
with `cargo +1.98.1 build --locked --release --bin rnx`. The unchanged release
profile uses thin LTO. Both final comparison binaries have identical identities:

- rustc 1.98.1, commit `48a229cea`, dated 2026-09-01.
- LLD 22.1.8, LLVM commit `52ed14fcd56afc30f9cccd8ca8ce237c2eef7e04`.
- Baseline binary SHA-256: `821d5032046f80949362067eb13de5c489eb56388db5838897c79bcaa0a23e10`.
- Final candidate SHA-256: `858c1c84ca73f8fa7a8a69ea90a91a695778a6157d9aea2757df0d11d9a60027`.

The ELF `.comment` identity guard passes before measurement. Raw identities
and build records are in `0192-rnx-release2` (baseline origin: release1).
`codex-rnx0192-wall2` measures CPU 4 on Nano: two warmups followed by 20
samples per binary/workload, alternating baseline/candidate order. Every
sample exits successfully with empty stderr and exact expected stdout.

| Workload | Baseline median ms | Candidate median ms | Change |
|---|---:|---:|---:|
| Startup, `eval "1 + 2"` | 5.711530 | 5.613383 | -1.718% |
| 01_mortgage | 6.295293 | 6.311041 | +0.250% |
| 02_orders | 6.204930 | 6.218587 | +0.220% |
| 03_report | 6.371415 | 6.391221 | +0.311% |

This is a wall-time sanity report with no threshold or causal performance
claim. Raw samples, commands, stdout/stderr, hashes, and identity receipt are
in `0192-rnx-wall2`; the earlier source export's release/wall runs remain intact.

## Final source binding

`0192-rnx-src5/SOURCE-PINS.json` binds the final implementation source before
this evidence-only addition. Full native suites and the census/enrollment
exports used src2/src3. `0192-rnx-live/FINAL-VS-RELEASE.diff` records the later
style-only differences: comments and private test names/locals/fixtures.
Affected root and test targets pass again from src5 in `0192-rnx-final-gates1`;
the final release and wall-time check also use src5. No runtime expressions,
registrations, certificates, locks, inputs, or goldens changed after the census.
`0192-rnx-live/FINAL-SOURCE.json` verifies the worktree against every src5 pin;
the sole later addition is this evidence file. Gate receipts, including
preserved failed attempts, are copied into `0192-rnx-live/gate-receipts`.
