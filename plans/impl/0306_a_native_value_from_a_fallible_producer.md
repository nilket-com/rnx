# Implementation plan 0306: A native value from a fallible producer

Status: Implemented

Paired ADR: ../adr/0306_a_native_value_from_a_fallible_producer.md.

1. Boundary note, read-only: the widened signature rule, the host
   bridge's cold output path for `Result<T, String>`, the input
   contracts for `read_csv`, its bounds, and every inventoried item
   touched with its exact hashes.
2. Record the old engine's REPL outputs for every control first.
3. The signature rule and the cold output import; then the `read_csv`
   enrollment; the early fixed-route screen. Stop on a band failure.
4. Session, bound, charge, registration, refusal, routing and inventory
   controls; mutations.
5. fmt, Clippy, the suites, preflight and the commit checker; sign;
   source review; one complete final; Results.

## Results

A checked opaque producer may now take bounded owned inputs and return
`Result<T, String>` for an enrolled native `T`. `polars::read_csv(path,
schema)` is its first use. One builder registers the legacy raw
handler and binds a distinct checked arity-two alternate to the
installed legacy handler under the same public name.
- The one-argument form keeps the old route, with a located `Arity`
  refusal in sticky execution sessions.
- The cold `host_output` path imports `Ok` as a uniquely owned native
  value inside the existing `Result` parent, and `Err(String)` with the
  old engine's text. Disposal on failure happens exactly once, with
  destructor errors secondary.
- A narrow frontend proof extension lets only a direct `?` unwrap the
  native value into checked session state. Wrapped results, lists, the
  one-argument form and function values refuse before the producer
  runs.
- No interpreter, opcode or Store layout changed. The source guard
  matched 40/40, with `Executor::host_call` unchanged at `bf36f1ee…`, so
  no source-condition transition was needed.

In the real `rnx-polars` REPL against explicit legacy, these match: a
frame loaded in one input and read with `width()` in a later one, the
three error strings, and reset. A typed charge witness, a cleanup
control that panics a native destructor after a parent-capacity
failure, the registration and compile-fail controls, and eight planted
mutations all hold.


The checked CSV route has temporary first-slice limits of 64 KiB of
captured input, 2,048 physical lines and 16 schema columns. The width
precheck counts at most 15 comma bytes per logical record, including
quoted commas. The read checks for one extra byte, both parser passes
use the same captured bytes, and recognized compressed input refuses
before parsing. The parser keeps the legacy chunking options and
default of no rechunking so ordinary parse diagnostics agree. Limits
surface as a terminal `StoreCapacity` error at the call. Retained
DataFrame bytes are charged separately.

The REPL's `RNX_MEMORY_CEILING` is a process live-allocation ceiling.
It does not set the Store's byte limit. Using the current Store's
remaining allowance to size the snapshot would give it effectively
unbounded input. Scaling this checked route is the next record. It
must establish an operator-configurable allowance for the call and a
conservative parser peak bound over bytes, lines and width before
lifting these ceilings.

On the focused allocator observations, the peak tracked allocation
above a warmed baseline was 4,874,937 bytes for 2,000 rows with 16
columns, 189,965 bytes for a long quoted field, 67,297 bytes for a
ragged record and about 200,000 bytes for a late parse error. These
figures describe the fixtures on the test host.

The early fixed-route screen passed (both bands 32/32). Complete final
v1 failed in `async-test-support` only. In
`a_synchronous_session_input_in_a_loop_is_still_interrupted_within_a_slice`,
the old-engine REPL loop did not stop within five seconds of a SIGINT
sent on a fixed 400 ms timer. That is consistent with the signal
arriving before the loop started. The run did not record the signal's
ordering, but Codex reproduced that failure mode on the binaries before
and after this record. v2 failed in `final-controls` only. The
`compare.rs` planted finals run in `/tmp`, a 31 GB tmpfs, and their
disk admission needed about 29 GB free there. Leftover test scratch
directories, 23,297 of them totalling 2.8 GB, had brought `/tmp` below
that. After they were removed, the unchanged v3 passed all 101 stages:
34.4 minutes of accepted run, 35.1 end to end. Both fixed-route bands
held, with no memory.high, memory.max or OOM events. Test-only fixes
follow: a readiness-driven SIGINT, a stubbed `disk_free` in
`compare.rs`, and scoped cleanup in the fixtures that leak scratch
directories.
