# rnx 0187: implementation evidence

The root and project-tool Rune dependencies use
`27d8b6711fa15a20726ebefe4c6ed3934a56190b`. Thirty closed synchronous host
functions are enrolled through the reviewed admission API. Their Rust bodies,
signatures and existing stock registrations are preserved. Stock remains the
runtime route; admission does not introduce fallback or default fresh execution.

## Compatibility checks

All 24 initial checks and five follow-up checks pass. The root default suite
passes on Rust 1.98.1 (478 tests); the project-sources/test-support suite passes
on both 1.98.1 and the declared 1.95.0 floor (543 tests each). Jupyter passes
27 tests. Formatting, normal Clippy, locked resolution for all eight active
lockfiles, and checks of all seven standalone workspaces pass. Polars callbacks
pass 9/9. Stored plain and Polars notebook goldens and their negative controls
pass without golden changes. Candle coverage is compile-only.

The eight lockfiles change only Rune revision entries. No package or dependency
edge changes accompany enrollment. Existing Clippy deprecation warnings remain.

## Compile admission census

Before and after registrations were compared at the same Rune revision,
features, locks, context and toolchain. All 30 enrollment controls become
fresh-admitted. Real corpus admission remains 30/55 stock-compiling inputs:
no additional complete program is admitted. One primary refusal advances;
one overlapping wrapped input advances only a secondary refusal. These are
not independent complete-program unlocks.

Remaining primary refusals across the 25 stock-compiling, fresh-refused inputs:
FormatSpec 8; json::parse 6; powf 4; missing 2; Pattern 2; contains_key 1;
round 1; fs::write_new 1. Inputs overlap; counts are descriptive.

## Hermetic execution

The 110 stock rows are identical before and after enrollment in typed outcomes,
error projections, actual stderr bytes and filesystem effects. Clock values
use a bounded wall-clock envelope rather than equal timestamps.

Fresh execution passes all 110 comparison rows: 107 agree under the declared
projections, including the clock envelope, and three have reviewed retained
differences. The two wrong-arity calls refuse admission; the resource witness
succeeds on stock and fails cleanly with StoreCapacity on fresh.

All five runtime controls pass: absent, wrong, cleared and unused host tables
refuse before effects, and the same executor/store recover with the correct
runtime. Repeated valid invocation preserves reuse. Four pure variant helper
checks and four direct tuple projection crosschecks pass. Every fresh child
ends idle with zero live entries and zero payload bytes.

The fuel frontier covers every budget 0 through 66. Budgets 0 through 65
exhaust fuel with non-decreasing prefix effects; budget 66 first completes all
64 writes. This does not claim numerical fuel parity with stock. Error-classifier
controls reject six fixed malformed strings, two invalid chains, division by
zero and index-out-of-bounds subjects. Failed harness build attempts are
preserved separately; corrected builds pass.

Host duration and standard-library allocations remain outside script fuel and
Store budgets. Compile admission and these correctness checks establish no
benchmark speedup or complete-program coverage gain. Raw receipts, observer
harnesses and executables are excluded from this change.
