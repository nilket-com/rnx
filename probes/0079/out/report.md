# Record 0079 gate 2: contract probe

## Costs (release build; medians with min..max over interleaved samples; instrumentation off)

| measurement | Rune ms | Rust ms | note |
|---|---:|---:|---|
| apply_mut, 1 calls, in place, end to end | 0.027 (0.027..0.184) | 0.0001 | end to end: the binding, the engine thread, one VM per element; not a separation of VM and conversion costs |
| apply_mut, 1 calls, commit on success | 0.028 (0.026..0.153) | 0.0002 | paired commit minus in place: median 0.000 ms (-0.031..0.025) |
| apply_mut, 1000 calls, in place, end to end | 0.389 (0.292..0.562) | 0.0010 | end to end: the binding, the engine thread, one VM per element; not a separation of VM and conversion costs |
| apply_mut, 1000 calls, commit on success | 0.347 (0.296..0.498) | 0.0006 | paired commit minus in place: median -0.010 ms (-0.069..0.015) |
| apply_mut, 1000000 calls, in place, end to end | 219.745 (217.197..225.016) | 1.2843 | end to end: the binding, the engine thread, one VM per element; not a separation of VM and conversion costs |
| apply_mut, 1000000 calls, commit on success | 222.297 (216.520..238.148) | 0.9073 | paired commit minus in place: median 0.301 ms (-3.224..17.220) |
| bridge alone, one thread, 100000 calls, i64 in and out | 23.94 (23.912..24.155) | | 0.239 us per call: one VM per call |
| bridge alone, one thread, 100000 calls, Series in and out | 25.03 (25.012..25.084) | | 0.250 us per call; conversion over scalar 0.011 us |
| apply_columns_par, 1000 one-row columns, parallel wall time | 1.160 (0.942..2.109) | 0.653 | parallel wall time over the pool: throughput, not per-invocation latency |
| apply_unary_elementwise, one call on 1 000 000 rows | 1.379 (1.237..3.265) | 0.888 | one bridge call on the whole series plus the fixture, the engine thread and Polars's own work; end to end |
| budget wrapper, 100000 calls, paired difference | | | median 2.9 ns per call (-136.1..84.7) |

## Concurrency

{"calls": 64, "columns": 64, "distinct_threads": 28, "max_simultaneous": 28, "pool_threads": 28}

## Deferred journey

- budget_exhaustion: ExprContext: callback Expr::map: instruction budget 500 exhausted calls=5
- describe_sink: calls=3
- entry_points_invoking_callbacks: ['LazyFrame::collect (function and output_type)', 'DslPlan::compute_schema (output_type)', 'LazyFrame::describe_plan (output_type)']
- retry: [2, 4, 6] calls=5
- schema_sink: [x2] calls=3
- value: [2, 4, 6] calls=5
- vm_failure: ExprContext: callback Expr::map: call failed: Panicked: later failure calls=5
- wrong_type_at_collect: Context: callback Expr::map: wrong result type: got ::std::i64 (Expected type `::polars::Field` but found `::std::i64`)

Resolved plan until failure:

	---> FAILED HERE RESOLVING 'select' <---
DF ["x", "y", "z"]; PROJECT */3 COLUMNS: 'select' calls=1
- wrong_type_at_schema: Context: callback Expr::map: wrong result type: got ::std::i64 (Expected type `::polars::Field` but found `::std::i64`)

Resolved plan until failure:

	---> FAILED HERE RESOLVING 'select' <---
DF ["x", "y", "z"]; PROJECT */3 COLUMNS: 'select' calls=1

## CSV, budget discrimination, restoration, re-entry, mutation, errors, captures, budget

- csv.with_schema_modify: [a, b] f64 f64 calls=1 height=2 a=2
- csv.without: i64
- budget_discrimination.genuine: CallbackError: callback Column::apply_unary_elementwise: instruction budget 1000 exhausted
- budget_discrimination.user_panic_deferred: ExprContext: callback Expr::map: call failed: Panicked: limited user message
- budget_discrimination.user_panic_immediate: CallbackError: callback Column::apply_unary_elementwise: call failed: Panicked: limited user message
- restoration.calling_thread: ['exhaustion under an inner budget of 100: got Exhausted (wanted Exhausted), guard clear true, outer allowance intact: small runs true, large halts true', 'success under an inner budget of 100 000: got Value(2) (wanted Value(2)), guard clear true, outer allowance intact: small runs true, large halts true', 'vm failure under an inner budget of 100 000: got VmFailure (wanted VmFailure), guard clear true, outer allowance intact: small runs true, large halts true', 'native panic inside the callback under an inner budget of 100 000: got NativePanic (wanted NativePanic), guard clear true, outer allowance intact: small runs true, large halts true', 'typed unwind under an inner budget of 100 000: got TypedUnwind (wanted TypedUnwind), guard clear true, outer allowance intact: small runs true, large halts true']
- restoration.failing_invocations_observed: 63
- restoration.negative_control: negative control: no native panic: got Value(1) (wanted NativePanic), guard clear true, outer allowance intact: small runs true, large halts true
- restoration.probe_failures: 0
- restoration.worker_reuse: CallbackError: callback DataFrame::apply_columns_par: call failed: Panicked: fail on a worker|failing_calls=63|again=64 calls=64 sum=128|0
- reentry.nested_via_routed_path: CallbackError: callback Column::apply_unary_elementwise: call failed: Panicked: CallbackError|callback: `PolarsError::wrap_msg` is a routed binding and may not be called from a callback
- reentry.nested_via_unrouted_path: CallbackError: callback PolarsError::wrap_msg: nested callback: a callback invoked while another is running on this thread
- reentry.routed_from_callback: CallbackError: callback Column::apply_unary_elementwise: call failed: Panicked: CallbackError|callback: `Series::sum` is a routed binding and may not be called from a callback
- reentry.unrouted_from_callback: [4, 6, 8]
- mutation.commit_on_success: [1, null, 3, 4, 5] nulls=1 len=5 Ascending|CallbackError: callback Int64Chunked::apply_mut: call failed: Panicked: at four|[1, null, 3, 4, 5] nulls=1 len=5 Ascending|[2, null, 4, 5, 6]
- mutation.in_place_partial_write: CallbackError: callback Int64Chunked::apply_mut: call failed: Panicked: at four|[10, null, 30, 4, 5] Ascending
- mutation.mutated_argument: [1, 2, 3]|[0, 0, 0]
- mutation.result_buffer: [v0, v1, v2]
- mutation.vector_argument: [10, 20, 30]
- errors.fallible_vm_error: ExprContext: callback Expr::map: call failed: Panicked: vm error
- errors.fallible_wrong_type: ExprContext: callback Expr::map: wrong result type: got ::std::string::String (Expected type `::polars::Column` but found `::std::string::String`)
- errors.infallible_vm_error: CallbackError: callback Column::apply_unary_elementwise: call failed: Panicked: vm error|[1, 2, 3]|[2, 3, 4]
- errors.infallible_wrong_type: CallbackError: callback Column::apply_unary_elementwise: wrong result type: got ::std::i64 (Expected type `::polars::Series` but found `::std::i64`)
- errors.unrouted_leaks_panic: True
- errors.unrouted_wrap_msg_leaks_panic: True
- errors.wrap_msg_fail: CallbackError: callback PolarsError::wrap_msg: call failed: Panicked: inside
- errors.wrap_msg_ok: [base]
- captures.constant_then_rebound: [2, 3, 4]
- captures.function: CallbackCapture: callback Column::apply_unary_elementwise: a captured value is not a constant: Type `::std::ops::Function` can't be converted to a constant value
- captures.nested_object: CallbackCapture: callback Column::apply_unary_elementwise: a captured value is not a constant: Type `::polars::Series` can't be converted to a constant value
- captures.wrapped: CallbackCapture: callback Expr::map: a captured value is not a constant: Type `::polars::Series` can't be converted to a constant value
- budget.blocking_ms: 300.612211
- budget.blocking_native_call: [1, 2, 3]
- budget.nontermination: CallbackError: callback Column::apply_unary_elementwise: instruction budget 1000 exhausted
- budget.nontermination_ms: 0.12183100000000001
- budget.stored_callback_reads_setting: ['ok', 'ExprContext: callback Expr::map: instruction budget 100 exhausted', 'ok']

## Subprocess controls (watchdog; survivors are group members alive after exit)

- contract: ok (expected completed, exit 0, survivors []) ""
- starve_pool1: ok (expected deadlock, deadlock, survivors ['2592176']) ""
- starve_pool2: ok (expected deadlock, deadlock, survivors ['2592306']) ""
- starve_nested_callback_pool2: ok (expected completed, exit 0, survivors []) {"elapsed_ms": 0.7670739999999999, "interrupted": false, "interrupted_at_ms": 0, "pool_threads": 2, "result": "completed 4 columns on 2 pool threads"}
- starve_denied_pool2: ok (expected refused, exit 0, survivors []) {"elapsed_ms": 1.515071, "interrupted": false, "interrupted_at_ms": 0, "pool_threads": 2, "result": "CallbackError: callback DataFrame::apply_columns_par: call failed: Panicked: CallbackError|callback
- ctrlc: ok (expected interrupted, exit 0, survivors []) {"elapsed_ms": 4164.785003999999, "interrupted": true, "interrupted_at_ms": 786, "pool_threads": 28, "result": "completed len=20000 first=1"}
- watchdog_control: ok (expected deadlock, deadlock, survivors ['2592504']) ""
- survivor_control: ok (expected survivor, exit 0, survivors ['2592531']) {"elapsed_ms": 2.8563970000000003, "interrupted": false, "interrupted_at_ms": 0, "pool_threads": 28, "result": "CallbackError: callback DataFrame::apply_columns_par: call failed: Panicked: CallbackErr

failures: []
