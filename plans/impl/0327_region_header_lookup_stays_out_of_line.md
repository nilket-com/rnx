# Implementation plan 0327: Region header lookup stays out of line

Status: Implemented

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

## Results

The only production edit changes `ProgramRegions::header` from
`#[inline]` to `#[inline(never)]`. Removing the old attribute would
still let ThinLTO move scan setup into the dispatcher. The caller
retains its checks for an empty table, a resumed frame and an armed
cancel hook.

The baseline was published df42ef8c. The file gate passed all 32
per-record and 32 cumulative comparisons. The empty project and the
Polars/Candle project each passed 64 per-record comparisons. Each side
used its own locked and verified app, helper and engine. The adapter
builds used the same published project Cargo lock as their dependency
seed. Project fixture outputs matched on every baseline/candidate pair.

The table gives native-on execution instruction deltas on all three
routes. W (excluding libc and the loader) and T (raw total) had identical
deltas. Growth is the size-5 count minus the size-1 count. With native
code off, every level and growth delta was zero.

| Kernel | Size 1 | Size 5 | Growth |
|---|---:|---:|---:|
| Fib | −602,185 | −602,365 | −180 |
| Scalar loops | −11 | −23 | −12 |
| Numeric calls | −13 | −33 | −20 |
| Shared rows | 0 | 0 | 0 |
| AWFY Sieve | +25 | +133 | +108 |
| AWFY Permute | −766,619 | −766,971 | −352 |
| Integer vector | +490 | +2,458 | +1,968 |
| Boolean vector | +490 | +2,458 | +1,968 |

On Permute, the baseline executed the header scan-length calculation
220,480 times at size 1 and 220,596 times at size 5. The candidate
executed neither a scan-length calculation nor a header call at either
size. Both images executed the dispatcher jump-table address calculation
404,414 and 404,642 times respectively and entered the native method
region once and five times. On the nonempty loops fixture, the baseline
started the inline lookup twice and ten times, measured at the cursor
reset immediately before the eight-byte search loop. The candidate made
exactly two and ten out-of-line calls and preserved the native entry
counts. All increases in the table fit the existing bands.

Formatting, the four Rune Clippy feature sets and preflight passed.
The native-feature library run under the engine-dev profile passed
2,212 tests, including region, cancellation, resume and budget controls.

The complete final on 6379dd94 passed all 107 stages, with 54.6 minutes
of accepted run and 55.1 minutes end to end. Both fixed-route bands
held, there were no memory.high, memory.max or OOM events, and the host
was in the performance profile.
