# ADR 0224 column comparison

Tools-only harness. No runtime changes. Python dependencies are pinned in the
ADR; `semantics.py` checks installed semantics before measurement. Run with the
approved virtual-environment Python, a pinned production `rnx-polars` binary,
and an operator-selected exclusive lease path. No package installs occur here.

```
python -B tools/column/compare/matrix.py --out VALIDATION_DIR --rnx RNX_BINARY --lease QUIET_LEASE --install-receipt INSTALL_RECEIPT --validate-only
python -B tools/column/compare/matrix.py --out TIMING_DIR --rnx RNX_BINARY --lease QUIET_LEASE --install-receipt INSTALL_RECEIPT --validation VALIDATION_DIR/RESULT.json --build-receipt BUILD_RECEIPT
```

Full-size validation precedes any timed matrix. Each worker is a fresh process;
preparation and validation are outside the call. It materializes the complete
output before stopping the timer. DuckDB input is copied to a table before
preparation and result export is included. Polars output conversion to Arrow is
only for validation, outside timing. Arrow calls return arrays directly.

Primary equivalent semantics include strict validity and computed-NaN guards.
Secondary native semantics are limited to seven Boolean/predicate shapes for
Polars and DuckDB, and are **not semantically equal to rnx**. Arrow is serial
direct kernels, with pool settings recorded but no threading claim. rnx column
execution is single-threaded. rnx and Arrow are each measured once and reused
with disclosure in both thread-mode panels, rather than duplicated.

Timing uses monotonic nanosecond timestamps in Python and the ADR0224 production
rnx clock. Record clock sources and reported resolution (unknown for Rust
Instant), rather than equating timestamp units with hardware resolution.
First means first kernel invocation after preparation,
not process startup. One first plus five repeated calls, five independent
process samples. Results must remain descriptive if resolution limits them.

The suite rotates engine order by replicate, runs only one process at a time,
and sleeps a fixed 120 seconds before each size/thread/replicate round. This
lease cannot prevent unrelated processes ignoring it; before/after thermal,
frequency and load snapshots are recorded. No readiness watchers or retries
select cooler samples. Peak RSS includes construction and validation.

`RESULT.json` is written atomically after fsync of worker artifacts. A successful
case resumes only with matching identity and hashes. Interrupted attempts are
retained and a new attempt fills a missing terminal result. A terminal FAIL
blocks automatic retry. Source, fixture-source, binary, package and environment
changes invalidate reuse. Raw samples are never overwritten by a renderer.

Small controls:
```
python -B -m unittest discover -s tools/column/compare -p 'test_*.py'
```

Do not launch timing until source review and full validation PASS. No aggregate
score; report each expression/layout/size/panel/thread mode independently.

Full-size validation can use `--validation-workers N`; admission bounds the pool
by available memory and a conservative per-worker estimate. Timing stays serial.
The trimmed matrix has 2,640 primary and 336 secondary process samples.
Each receipt records process wall time separately from kernel-call time.

Secondary native measurements use 100M rows only and three process samples.
Primary measurements retain both sizes and five samples. The matrix binds the
signed implementation commit and verified production binary hash. Reviewed
policy-only validation reuse permits changes only to scheduling, reporting and
control files; worker/fixture/expression/oracle identities must match exactly.
