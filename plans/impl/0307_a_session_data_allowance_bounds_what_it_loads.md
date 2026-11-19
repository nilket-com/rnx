# Implementation plan 0307: A session's data allowance bounds what it loads

Status: Implemented

Paired ADR: ../adr/0307_a_session_data_allowance_bounds_what_it_loads.md.

1. Boundary note, read-only: which operator setting supplies a finite
   Store allowance and its default, how reset keeps it, how the cold
   owned input carries the remaining budget to the producer, and any
   inventoried item touched.
2. The scaled measurement design for the parser estimate (sizes, shapes,
   margin, held-out validation), reviewed before measuring.
3. The allowance and its carriage; then `read_csv`'s derived bounds; the
   early fixed-route screen.
4. Scaled parity, over-allowance, estimate, reset and charge controls;
   mutations.
5. fmt, Clippy, the suites, preflight and the commit checker; sign;
   source review; one complete final; Results.

## Results

Execution REPL, worker and Jupyter sessions have a data allowance,
`RNX_SESSION_STORE_BYTES`, default 512 MiB, separate from
`RNX_MEMORY_CEILING`.
- It is read once in `InteractiveSession::new`'s execution arm, passed
  as the Store's `Limits.bytes`, and reused by `reset_fallible`.
- A value must be ASCII decimal digits, greater than zero and no greater
  than `isize::MAX`. Empty, signed, spaced, suffixed, overflowing and
  non-UTF-8 values are rejected.
- Fixtures pass explicit limits without the process environment, and
  the old engine ignores the setting.
- The README documents it.

The final owned-input packet carries the Store's remaining allowance:
`host_remaining_bytes()` less the packet's charged scratch, or `None`
for a scalar. Other owned-tree consumers are unchanged. The
`execute.rs` and `execute/` inventoried sources are untouched.

The checked `read_csv` route works as follows:
- It recognizes a scalar or wrong-shaped schema first, then charges the
  path copy and schema planning and drops the packet.
- It grows its snapshot fallibly up to the credit, with a one-byte stack
  sentinel at the limit.
- It charges a source-derived parser estimate before calling Polars. A
  miss is `StoreCapacity` at the call.
- Polars' opt-in panic and symbolized-backtrace error modes are refused
  on this route with named errors, because their allocation bypasses the
  bounded formatter.

The temporary 64 KiB, 2,048-line and 16-column ceiling from 0306 is
removed.

The estimate was derived from Polars 0.55.2's CSV reader and checked on
isolated, warmed processes with pools of 1 and 28 threads.
- **First round:** 54 calibration and 17 held-out fixtures all stayed
  within it, the tightest gap 0.577 MiB. A held-out 207.98 MiB, 16-column
  load peaked at 355.95 MiB against an estimate of 480.09 MiB.
- **Wide-input fix:** that first formula over-estimated wide inputs. It
  charged the 8 KiB minimum string block for every column in every
  chunk. The term now counts fields longer than 12 bytes, by column
  position, on unquoted input, and keeps the conservative treatment for
  quoted and malformed records.
- **Second round:** all 16 fresh held-outs stayed within the frozen
  revised formula, the tightest gap 3.75 MiB. With the default pool, all four
  tested unquoted rectangular fixtures (64 or 97 columns, up to 31.8 MiB)
  are admitted, at estimates of 215 to 381 MiB. A 205.6 MiB 16-column load estimated
  462.57 MiB and peaked at 458.82 MiB with one thread, and estimated
  495.59 MiB and peaked at 351.92 MiB with 28.

These bound this parser's work over the admitted snapshot, not the
process's memory. Some shapes remain refused under the 512 MiB default
with 28 workers: wide quoted inputs at 2.04 and 7.79 MiB, and a ragged
97-column input at 8.81 MiB. The 2.04 MiB quoted and 8.81 MiB ragged
inputs fit with one worker. The 7.79 MiB quoted input does not; its
one-worker estimate is 613.50 MiB.

Controls: 52 adapter unit tests, two real-session tests and nine core
controls. In the real REPL a scaled CSV loads, a second retained load
that exceeds the allowance is refused, and the load is admitted again
after reset. The early fixed-route screen passed with the source
condition and both bands 32/32. The complete final passed all 101
stages: 47.3 minutes of accepted run, 48.3 end to end. Both fixed-route
bands held, with no memory.high, memory.max or OOM events.
