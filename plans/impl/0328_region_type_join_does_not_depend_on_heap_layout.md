# Implementation plan 0328: Region type join does not depend on heap layout

Status: Implemented

Paired ADR: ../adr/0328_region_type_join_does_not_depend_on_heap_layout.md.

1. Boundary: published aa5af565 and the calibration receipt for the
   `dbg!` candidate. Record any deviation before editing.
2. Extract the successor slot join into the non-inlined helper. Keep
   scheduling, cutoff and error paths unchanged.
3. Add the pair-table control and the machine-code check on the
   production image.
4. Run the placement witnesses, the file gate and both project screens
   before signing. Check every delta against its band.
5. fmt, Clippy, preflight and the targeted checks under the final's real
   recipe. Sign. Source review, one complete final, Results.

## Results

Compared with published aa5af565, the optimized engine calls
`join_slot_types` from both successor paths. It selects scalar, 8-slot
and 32-slot loops by length. The caller checks the target slice bounds.
Neither the caller nor the helper selects a loop by comparing the two
buffer addresses. All 25 ordered slot-type pairs produced the expected
joined value and changed flag. Empty, equal and mixed rows passed.

The table gives setup instructions in `function_types_in` and its
extracted helper combined, including call and return. Both fixture
sizes gave the same counts. The address-order column counts joins
where the target preceded the input, out of 123 vector-length joins.
The baseline and candidate observed the same address orders.

| Heap placement | Target first | Published | Candidate |
|---|---:|---:|---:|
| Plain | 44 | 33,883 | 31,260 |
| Inert preload | 44 | 33,883 | 31,260 |
| Hold 16 bytes and a 32-byte guard | 44 | 33,883 | 31,260 |
| Hold 96 bytes and a 32-byte guard | 123 | 34,199 | 31,260 |

The candidate spent 17,395 instructions in the caller and 13,865 in the
helper under every placement. The 96-byte placement still added 18
calls to the memcpy forwarding stub, or 36 setup instructions.
All cells matched the baseline output and execution work at their size.
The file gate passed 32 per-record and 32 cumulative comparisons. On
native-on Boolean-vector setup the plain candidate saved 2,623 W at
each size, including the helper calls.

The empty project and the Polars/Candle project each passed 64
comparisons against their published-source builds. Every execution
level and growth delta was zero for native-on W and T, and every
whole-invocation level and growth delta was zero with native code off.
Paired fixture outputs matched. The first project lock attempt used a
symlink spelling for the baseline source and failed path binding before
measurement. Both scored project declarations use canonical source paths.

An initial file-screen invocation used the parked `dbg!` controller.
Its build receipt identified fd7ec908 as the candidate. The file scores
above come from the controller built in the join snapshot, with its
build receipts checked against aa5af565 and e9043218.

Formatting, the four Rune Clippy feature sets and preflight passed.
The engine-dev native-feature library run passed 2,212 tests, with
246 ignored.

Two earlier complete finals on 2bfff7bf stopped in final-controls. The
bootstrap tests' nested disk check measures /tmp, a tmpfs that leaked
test directories had left short of 24 GiB. The candidate was unchanged.
After those directories were removed, the complete final on 2bfff7bf
passed all 107 stages, with 37.7 minutes of accepted run and 37.9
minutes end to end. Both fixed-route bands held, there were no
memory.high, memory.max or OOM events, and the host was in the
performance profile.
