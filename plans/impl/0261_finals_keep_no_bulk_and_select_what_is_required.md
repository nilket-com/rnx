# Implementation plan 0261: a final keeps no bulk of its own and can run exactly what is required

Status: Proposed

Paired ADR: ../adr/0261_finals_keep_no_bulk_and_select_what_is_required.md.

1. Pin 66e72772. Read gate selection and finalization, the driver, the
   Clippy comparison and suite stage producers and their baseline
   handling, the fixed-route gate with its anchor reuse and vector pool,
   and the fixed-route policy.
2. Before editing, send a note: how the required set is computed and
   whether policy can state when a complete final is needed; every
   directory a complete final leaves in its own directory with sizes and
   readers; how an anchor cache entry is keyed, what it refers to, how
   many entries exist and what they hold; what a miss costs; the proposed
   cache layout and bound; anything in the ADR the code makes wrong. If
   the fixed-route policy file, a band or an inventoried item would
   change, send that instead and stop.
3. Implement the required mode, ownership of baseline checkouts, the
   self-contained anchor cache and the release of fixed-route payload.
4. Add the controls of the ADR.
5. Run a final in required mode and a complete final, preflight and the
   commit checker. Bring the packet with both results, the size of each
   final's directory afterwards, free space before and after, and the
   per-stage durations. The candidate is frozen once the packet is sent.

Results are added here by the implementer.
