# Implementation plan 0266: loop control in the execution engine belongs to its loop

Status: Proposed

Paired ADR: ../adr/0266_loop_control_at_run_time_matches_its_loop.md.

1. Pin b32bbaa6. Read how the execution engine's lowering keeps its
   loop targets and where a `while` condition, a `while let` scrutinee
   and a `for` iterator are lowered relative to pushing the target;
   the legacy compiler's handling of the same; the loop targets of the
   constant evaluator.
2. Before editing, send a note: the table of the ADR, measured, one
   program per row, for legacy run time, execution run time, the
   constant evaluator and Rust; the rows where legacy looks wrong; the
   cause of each execution difference in the lowering; the proposed
   change; whether any inventoried item would change. If one would,
   send that and stop.
3. Correct the lowering, and the constant evaluator where the ruling
   says so.
4. Add the controls and the parity rows; regenerate the pinned table.
5. Run Miri, a final in required mode, preflight and the commit
   checker. Bring the packet with the table before and after, the class
   of each new row, the totals, and the per-stage durations. The
   candidate is frozen once the packet is sent.

Results are added here by the implementer.
