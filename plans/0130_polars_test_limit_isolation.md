# rnx 0130: test limit isolation (the in_memory_io flake)

Status: plan, a small maintenance record before 0131. Recorded as a standing flake in 0129's evidence.

## The flake

**The symptom:** `tests/in_memory_io.rs` `csv_round_trip_with_a_separator` (0118) fails intermittently in debug with "Missing interface environment": 1 of 12 runs at `65d32e6`, 3 of 12 at 0129's head.

**The cause, established:**
- **The real error is hidden.** That text is how a `polars::Error` debug-prints outside a VM. Formatting it inside the script shows nothing wrong with the round trip itself: 300 of 300 serial runs succeed.
- **The race:** two sibling tests in the same binary, `a_sink_over_its_limit_commits_nothing_and_is_reusable` and `reader_bytes_are_bounded`, call `polars::set_materialize_limit(n)`. That is a test-support override held in a process-global atomic (`TEST_LIMIT`, `src/generated/support.rs`). A round trip running in parallel inside that window materializes against the tiny bound and fails.

**The same latent race elsewhere:**
- **Lowered with no lock:** `tests/arrow_values.rs` (limit 12) and `tests/concrete_arrays.rs` (limit 3) lower the global limit while their siblings run unlocked.
- **Serialized by hand:** the other files use a per-file `SERIAL` mutex, which works only when every test in the file remembers to take it.
- **The JSON bound:** `TEST_JSON_LIMIT` has the same shape.

## The fix

**Every test in a binary that lowers the limit holds that binary's lock.** That is the convention 15 of the 19 limit-lowering test files already follow, now completed in the four that don't:
- `in_memory_io.rs`: 0 of 5 tests held a lock;
- `arrow_values.rs`: 0 of 6;
- `concrete_arrays.rs`: 0 of 11;
- `generic_impls.rs`: only its bound test held `LIMIT`, so the others could run inside its window.

**The lock is poison-tolerant** (`unwrap_or_else(|e| e.into_inner())`), so one failing test doesn't cascade into its siblings. Production code is untouched.

**Rejected: a thread-local override.** It was tried first and disproven: `borrowed_slices` lowers the bound and expects a refusal inside a byte callback, and 0080's bridge runs that callback on another thread. A per-thread override was invisible there, and all 6 of its tests failed deterministically. The bound has to stay process-wide, so serializing per binary is the correct scope.

**A convention check keeps it true:** a test binary that calls `set_materialize_limit` or `set_json_limit` takes its lock in every `#[test]` (19 of 19 files after this record).

## Gates

- **The flake:** `in_memory_io` passes 50 of 50 debug runs, at the same parallelism that failed 3 of 12.
- **The other unlocked files:** `arrow_values` and `concrete_arrays` pass 20 of 20 each.
- **Suites:** Polars' release default, release `test-support` and debug `test-support` all pass.
- **Unchanged:** the freeze (`generated/` bindings and `polars-gen`), and the oracle tally.
