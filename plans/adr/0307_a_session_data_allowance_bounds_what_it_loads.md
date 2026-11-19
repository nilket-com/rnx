# ADR 0307: A session's data allowance bounds what it loads

Status: Proposed

Base: published 5cb40a4d139d9ff582c89024c7a8a1ed88e74296 (fix after ADR 0306).

## Problem

ADR 0306 brought `polars::read_csv(path, schema)` to new-engine sessions
with a temporary ceiling: 64 KiB of raw input, 2,048 physical lines and
16 schema columns. The ceiling is fixed because there was nothing finite
to derive a bound from:
- `DomainSession::for_frontend` and `reset_fallible` build their
  `ProgramDomain` with default Store limits, which are the representable
  maximum;
- `RNX_MEMORY_CEILING` bounds the process's live allocation, not a
  session's Store.

Measured peaks on four small fixtures (about 4.9 MB tracked for 2,000
rows by 16 columns) do not establish a bound for larger input. rnx's
stated target is gigabyte-scale analytics. A 64 KiB loader in the new
engine would break real data as sessions rely on it.

## Decision

1. A new-engine session has a finite, operator-chosen data allowance:
   `RNX_SESSION_STORE_BYTES`, default 512 MiB, independent of
   `RNX_MEMORY_CEILING`'s process live-allocation meaning.
   - It is parsed once, before `DomainSession` is created, at the common
     execution-session entry, so the terminal, the worker and Jupyter
     share one policy.
   - It is passed as `Limits.bytes` and reused across reset.
   - Invalid, zero and overflowing values are rejected, never silently
     replaced.
   - Unit fixtures pass explicit limits without process-global
     environment changes.
2. A checked producer receives the remaining allowance for its call
   explicitly, through the cold non-leaf owned input packet, not a global.
   `Store::owned_host_input` already has the Store and the scratch budget.
   It adds verified remaining credit to the private packet: the existing
   `host_remaining_bytes`, less the conservative live packet cost. The
   adapter then accounts for the path copy and the schema and preflight
   scratch. A scalar packet carries no credit and keeps the existing
   bad-schema `Err(String)` path. `read_csv` bounds itself by that credit:
   - nothing is reserved up front. The snapshot grows fallibly and
     boundedly up to the credit, with the existing limit+1 sentinel.
     File metadata is only a sizing hint;
   - physical lines and columns are counted from the snapshot before
     Polars;
   - a conservative parser preflight estimate is charged against the
     allowance before parsing. It includes the snapshot's allocated
     capacity while it is live alongside the parser buffers, a
     conservative width before header inference as well as the schema
     width, per-column and per-chunk allocations, and the enabled parser
     and concurrency configuration. Its arithmetic is checked before
     calling Polars, and parse-error offsets are preserved;
   - the retained frame's charge stays separate, as in 0306.
   A misfit is the existing terminal `StoreCapacity` at the call. The
   temporary 64 KiB, 2,048-line and 16-column ceiling is removed.
3. The preflight estimate is source-informed first: a model derived from
   the parser's allocation behavior. Measured peaks on fixtures across
   orders of magnitude of bytes, lines and width (wide, quoted, ragged
   and late-error inputs), with held-out sizes, can falsify and calibrate
   it, but a margin on observed peaks alone does not establish a bound at
   scale. If the source audit cannot justify large inputs, the record
   reports the resulting envelope before replacing the temporary
   ceiling. It bounds this parser's work over the admitted snapshot, not
   the process's memory.
4. The old route is unchanged. Outside sessions this producer keeps the
   old route; other file routes already support execution.

## Constraints

- Session construction, reset and the cold producer input only. No
  interpreter, opcode or Store layout change; any inventoried item
  touched gets an exact source-condition transition reviewed before its
  screen. The fixed-route band is screened early.
- The snapshot still holds the whole admitted file in memory beside the
  frame; streaming is a later question.
- No new crate. Tabs.

## Controls

- The allowance: `RNX_SESSION_STORE_BYTES` and its 512 MiB default;
  rejection of invalid, zero and overflowing values; terminal, worker and
  Jupyter sharing it; a witness where an existing frame reduces the room
  for a second load and reset restores it under the original limit.
- The packet credit: targeted controls for the other owned-tree
  consumers that share the packet layout and charge.
- Scaled loads, against explicit legacy, in the real
  `rnx-polars repl --engine=execution`: equal results at several sizes
  well beyond 64 KiB inside the allowance.
- Over the allowance: refused with `StoreCapacity` before parsing, with
  nothing retained, for an over-size file and for a parse whose estimate
  exceeds the remaining allowance.
- The estimate: measured peaks never exceed it on held-out fixtures,
  with the margin reported.
- `width()` after a large load, and release and live counts back to
  baseline after reset.
- A mutation per rule fails its control.
- One complete final.
