# Implementation plan 0308: A frame reports its height on the new engine

Status: Implemented

Paired ADR: ../adr/0308_a_frame_reports_its_height_on_the_new_engine.md.

1. Boundary note, read-only: enrolling `polars::Error` (charge, display,
   `kind()`), the checked `Result<S, E>` read through registration, the
   cold output and the frontend proof, how `height()` can fail, and any
   inventoried item touched.
2. Record the old engine's REPL outputs for every control first.
3. The error enrollment, the certified typed-result read metadata and
   cold output, and the admission rule (input-level `?` and direct
   `match` only); then `height()`; the early fixed-route screen.
4. Session, error, capacity, registration, helper-refusal, retained
   helper, escape, routing and inventory controls; mutations.
5. fmt, Clippy, the suites, preflight and the commit checker; sign;
   source review; one complete final; Results.

## Results

`DataFrame::height()` runs on the execution engine as a checked read
returning `Result<int, polars::Error>`, enrolled once through the
generator by exact type, path and `&self` receiver.

`polars::Error` is a checked native type:
- Its charge is its inline size plus both strings' capacity.
- Its display is the existing message, registered once with the type.
- `kind()` is a checked method. `message()` is unchanged.

The registration carries pre-erasure metadata for the method's error
type. The cold host output builds the `Ok` scalar, or imports the owned
error inside an `Err`. If the `Result` cannot be allocated, the import
rolls back: `StoreCapacity`, with no Store entries or bytes left and the
native dropped once. `StoreCapacity` stays distinct from `polars::Error`.
The interpreter (`execute.rs`) is untouched; the bridge change is one
cold output arm.

The frontend proof follows the error from the method call. The typed
error is admitted in two places only:
- an exhaustive, guard-free `Ok`/`Err` match directly on the call;
- `?` in the session input itself.

Every other use is refused before effects:
- a `?` in a script helper, including one that builds its own frame,
  is refused at definition, and a later call to it is a missing item;
- the wrapped result in a vector, a closure capture or a host argument;
- a non-exhaustive match.

A helper that catches the error and returns a plain value is admitted.
Files keep the old route.

What a session shows:
- A loaded CSV kept from an earlier input reports its height (3), and an
  empty frame after reset reports 0.
- A direct match on the error shows `ConversionError` and the real
  message, "height: 9223372036854775808 does not fit a script integer",
  through `kind()` and explicit display. These, and successful
  `height()` and reset, agreed with the old engine when measured once;
  the committed controls pin the execution values.
- A bare `Err` binding returned from a match shows the message. The old
  engine prints `<::polars::Error>`; this follows 0305's bare-native
  display.
- A failing top-level `?` shows today's execution diagnostic, a Store
  type error, without the contained error. It is pinned on the execution
  engine only, and the frame stays usable in the next input. The old
  engine also hides the contained error, with a tuple/result type
  mismatch. Neither is changed here. Showing the contained error is a
  later, execution-only record; the old engine is not maintained for it.

The error path uses the real method: a test-only, 64-bit zero-column
frame from `DataFrame::empty_with_height(i64::MAX as usize + 1)`.

Six mutations were each caught by their named control:
- wrong nominal type;
- missing rollback;
- undercharged error string;
- helper propagation;
- reversed `Result` tag;
- a bare wrapped return.

The early fixed-route screen passed against aae80905. The complete final
passed all 101 stages: 52.2 minutes of accepted run, 53.3 end to end.
Both fixed-route bands held, with no memory.high, memory.max or OOM
events.
