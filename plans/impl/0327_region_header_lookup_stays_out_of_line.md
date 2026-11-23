# Implementation plan 0327: Region header lookup stays out of line

Status: Proposed

Paired ADR: ../adr/0327_region_header_lookup_stays_out_of_line.md.

1. Boundary: the scratch candidate 59c65bc1 on fef4fcb1, reconciled
   against published df42ef8c. Record any deviation before editing.
2. Keep `ProgramRegions::header` out of line, with the caller's empty,
   resumed and armed checks unchanged.
3. Add the profile control for the empty-table path and the nonempty
   header call count.
4. Score the file gate and both project routes per Decision 3 before
   signing. Check every delta against its band's allowance.
5. fmt, Clippy, preflight and the targeted checks under the final's real
   recipe. Sign. Source review, one complete final, Results.
