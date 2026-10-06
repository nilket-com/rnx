# 0179 tests-first and source checkpoint

This is an untimed checkpoint, not a performance result or adoption. Fork main
and the shipped rnx dependency remain unchanged.

## Frozen source and fixtures

The tests-only base is Rune `4e84cfac`, following `58ba84f1` (the unchanged 0172
inventory test cherry-picked onto `bb8e6937`). The candidate is `b28f8cb3`.
Bench fixture/base receipts are `c259eb0c`, following the accepted read-only
audit `0e416308`; candidate receipts are `20b29d85`.

Before any candidate edit, the base froze Item display expectations for 19
cases, exhaustive context contents, deterministic registration events, custom
associated-name constants and context isolation, plus complete first-use
scripts for both stdio settings. Inventory and event goldens have separate
doc and no-doc paths. Corruption checks remove a function, change a constant,
remove metadata or a name, and reorder an event. Handler identity remains
structural/behavioral, not pointer equality.

The first-use mapping names all 34 constructor factories. Namespace-only,
reexport and native-type probes are distinguished from runtime operations;
it does not claim coverage of every standard-library method. The scripts
compile and execute to returned integer 42 and produce no script output.
Their byte hashes are:

- `first-use-stdio.rn`: `a6cb670304ed84b8e8b192ce2a10947fd1bbbe3ee44401b5459bcdadecd949dd`.
- `first-use.rn`: `db032dd5b7e3e370abf509a66b628e1ae597d1fa8ca95f1d49f202a1fdec8042`.

The base commit and fixture hashes were sent to Claude before candidate edits.

## Reviewed test-method amendment

The original exact-production-feature focused unit-test command failed with
32 compilation errors in Rune's pre-existing root test module: emit/workspace
APIs and diagnostic fields/dependencies were unavailable. The command, errors
and preceding fixture preparation failures are retained in bench's
`results/startup-0179/base-contract/`; none was a performance sample.

Claude approved the amendment in chatd
`01a110da-baa2-7138-921a-50abdcf5efbe`: only the root `mod tests` is excluded
under `rune_startup_inventory`. The cfg is declared in Rune's existing Cargo
check-cfg list and set only on the diagnostic production-feature unit-test
command. It appears in neither deciding builds nor production execution.
All-feature tests run without it, retaining the root module. The focused
first-use test uses public Sources/prepare/Vm operations directly rather than
root-test helpers. No other module was excluded and no feature was added.

Both of these test-only changes belong to the base commit. Claude's prep must
build the unchanged historical harness against that base at the historical
path and require byte equality with 0178's primary before proceeding. A failed
neutrality check stops preparation; equivalence is not assumed from the diff.

## Candidate scope and correctness

Production changes are confined to `compile/context.rs`: a private direct
name helper, its private original-formatter fallback wrapper, and one use in
named associated Function alias registration. The helper counts bytes with
checked arithmetic before allocating; Crate/Str spelling matches Item display,
including later Crate components, empty strings, Unicode and deep/long paths.
Root and any numeric Id take the original whole-item formatter before string
allocation. The immutable item is traversed again to write the reserved string.
Hashing, constant conversion/insertion, handler preparation, Item display,
VM behavior, public APIs and build profiles are unchanged.

The candidate adds a test that checks both bytes and the direct-versus-fallback
choice against the base's frozen display cases. Focused release results:

| Configuration | Base | Candidate |
| --- | --- | --- |
| Exact production features, diagnostic cfg | 6 passed | 7 passed |
| All features, diagnostic cfg absent | 6 passed | 7 passed |

Candidate alloc-only no-std cargo check passed. Full suites, the standing
correctness corpus and fresh controlled builds remain the reviewed preparation
phase; these focused results do not substitute for them. No timing has run.

## Reviewed first-use harness adaptation

Claude agreed in chatd `01a110dc-1db5-715d-8d0e-b39314a671eb` that ordinary
`run` would leave the async main future uncompleted, and existing `async` would
discard its result. The deciding harness therefore adds
`first-use <true|false> <script>`: fresh context/runtime, compile with default
Options/Diagnostics, main argument `((),)`, async completion under the existing
1e9 budget, require returned i64 equal to 42, then emit exactly
`FIRST-USE 42\n`. Both variants are deciding cold workloads. The in-crate
correctness test uses the same executor/completion/argument but no VM budget;
that difference is disclosed, not credited as a measured budget test.

The unchanged historical-harness neutrality build is separate from adapted
base/candidate deciding binaries. Historical reproduction gates remain;
new first-use workloads have contemporary paired baselines. The adapted driver
and untimed controls still require Codex review before official measurement.
