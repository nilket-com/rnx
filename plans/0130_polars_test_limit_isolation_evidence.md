# rnx 0130 evidence: test limit isolation

**The flake is fixed.** `in_memory_io` `csv_round_trip_with_a_separator` failed 3 of 12 debug runs at 0129's head; it now passes 50 of 50 at the same parallelism.

## The cause, established

**The real error was hidden.** "Missing interface environment" is how a `polars::Error` debug-prints outside a VM. Formatting the error inside the script showed the round trip itself is sound: 300 of 300 serial runs succeed.

**The race:** two sibling tests in the binary lower the process-wide test materialize limit (`polars::set_materialize_limit`, `test-support` only). A round trip running inside that window materializes against the tiny bound and fails.

## The fix: every test in a limit-lowering binary holds the binary's lock

**Completed in the four files that didn't follow the convention:**
- `in_memory_io.rs`: 0 of 5 tests held a lock;
- `arrow_values.rs`: 0 of 6;
- `concrete_arrays.rs`: 0 of 11;
- `generic_impls.rs`: only 1 of 6 held its `LIMIT` lock, and not as its first statement.

**Two details:**
- The lock is poison-tolerant, so one failure doesn't cascade.
- `generic_impls`' bound test previously took `LIMIT` mid-test. That inner lock was removed, because a second acquisition of the same `std::sync::Mutex` would deadlock.

**The convention check** (`tests/limit_locks.rs`) keeps the convention true: every test file that calls `set_materialize_limit(` or `set_json_limit(` (19 files) must open each `#[test]` with a `let _… = ….lock()` statement; leading `use` lines and comments are allowed. It is proven to bite: with one lock removed from `in_memory_io.rs`, it fails and names the test's first line.

**What it doesn't prove:** it is lexical. It checks that each test's first statement takes *a* lock, not that every test in a file takes the *same* mutex; a different `.lock()` would pass. Today each of the 19 files has exactly one lock static, which every test takes.

**Production code is untouched.**

## Rejected: a per-thread override

Tried first, and disproven by the suites. `borrowed_slices` lowers the bound and expects a refusal inside a byte callback, and 0080's bridge runs that callback on another thread. With a thread-local override, all 6 of its tests failed deterministically. The bound must stay process-wide, so serializing per binary is the correct scope.

## Gates

**Repeat runs (debug, `test-support`):**

| binary | runs | failures |
|---|---:|---:|
| `in_memory_io` | 50 | 0 |
| `arrow_values` | 20 | 0 |
| `concrete_arrays` | 20 | 0 |
| `generic_impls` | 20 | 0 |

**Polars suites:**

| suite | passed |
|---|---:|
| debug `test-support` | 264 |
| release `test-support` | 263 |
| release default | 40 |

All three include the convention check.

**The oracle:** `oracle-results.json` was restored after the runs; it showed only the standing `LazyFrame::unique` row-order flip.

**Freeze:** unchanged. No file under `src/` or `tools/` changed.
