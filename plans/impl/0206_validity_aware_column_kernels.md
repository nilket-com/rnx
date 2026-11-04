# ADR 0206 implementation: validity-aware column primitives

Approved for implementation: rune-native 26505, following read-only Step 0.
Base: Rune 1b4b66bd3657868ecb1d49f8b3f355585b3ec98d; rnx 0205.
Signed plan precedes signed implementation. Rune review/gates/publication first;
then a separately reviewed rnx pin-only record, actual CLI rerun, and benchmark
report update. Monorepo migration remains a separate subsequent record. N7/N8
remain parked. No interpreter hot-loop changes or new dependencies.

Replace per-row validity intersection with bounded word reads, intersect all
argument masks, write packed output validity and count valid rows with count_ones.
Keep maximum 1024-row blocks, shortening only at physical input chunk boundaries,
not validity runs. All-zero blocks skip arithmetic; all-one blocks use unmasked
primitives; mixed blocks mask both output and every overflow contribution.
Neutralizing null payloads alone is insufficient: constant subexpressions can
overflow. Preserve exact valid-row fuel, original row/operation first fault,
one replay from original inputs and allowance, cancellation bounds, fallible
allocation and actual-capacity memory accounting. All null payloads remain zero.

Packed Bool NOT uses word operations with independent bit offsets. Add typed
safe-slice two-input F64 add/subtract/multiply primitives, runtime AVX2 dispatch
and portable fallback, with no FMA or reassociation. General checked I64 multiply
uses a typed scalar primitive. Full-width AVX2 multiply and the i32-range fast
path are deferred: Step 0 found them worse on general wide inputs. No new special
fusion for x*3+y; improve the general typed primitive chain. Broader dispatcher
tuning is deferred. Keep std/core-only primitives and the committed dependency
set guard; exceptions require explicit review.

## Performance acceptance (rune-native 26505)

Measure the actual production CLI whole map, 10M and 100M logical rows, complete
outputs plus actual path and work witnesses. Numeric nullable x+1, x*2+y,
F64 x+y and x*3+y must cost <=6 times their corresponding NEW flat cost per
logical row and strictly improve their existing nullable cost. Include long-run,
alternating and deterministic-random masks, independent chunks, offsets, tails.
Nullable Bool NOT <=5 Ir/logical row. Flat F64 x+y <=4.0 Ir/logical row.
Flat checked x*3+y <=12.0 Ir/logical row. Existing flat x+1/x*2+y/Bool NOT and
all old-path controls require zero unreviewed growth. Do not silently inherit
historical attributed exceptions as new allowances. Separately measure mask
expansion and improve Step 0's 9.5 Ir/element masked-add prototype.

Published 0205 100M whole-map baselines, flat / sliced-chunked half-null Ir/row:
x+1 2.22797007 / 80.8622571; x*2+y 3.78764385 / 111.65973137;
Bool NOT 0.06479742 / 108.55504969; F64 x+y 66.55904763 / 143.04538324;
x*3+y 91.06205902 / 155.29694058. The old nullable fixture has long runs;
it is not evidence for alternating/random layouts. Primitive-only Step 0 is
supporting design evidence, not a substitute for these whole-map gates.

## Verification and orchestration

Committed fail-fast prerequisite command: formatting, dependency guard, isolated
std/core primitive compile/tests, Clippy, focused controls before long suites.
Differential forced-portable/AVX2 controls cover every fuel stop; null overflow
at every SIMD lane and tail (including overflowing constant subexpressions);
first-fault order, independent bit offsets 0..7, word/chunk boundaries, all-null,
alternating/random/long-run masks, zero rows, empty chunks, memory limits,
cancellation, no partial publication and exactly one scalar replay. Miri checks
safe leases/buffers/portable execution; native tests check SIMD separately.
Committed thresholds must reject vacuous profiles and wrong outputs/paths/work.
Use exact source/build/tool identities, bounded host memory, lock/affinity and
thermal/counter controls, unique artifact directories, blocking exit notices
and terminal receipts. Never poll long jobs. Attribute failures read-only before
source/threshold changes. Fresh cold-all/Miri and existing-path controls remain
required. Bring exact source and receipts for publication review; approval of
this plan does not waive those gates.

## Record convention and sequence

This plan implements [ADR 0206](../adr/0206_validity_aware_column_kernels.md).
Signed subject-only commits use `plan: adr 0206 ...` and `impl: adr 0206 ...`;
one plan plus one implementation per repository record, amending local fixes.
The rnx pin and benchmark report use implementation plans under the same 0206
number. After CK2, ADR 0207 addresses the nominal-limit concurrency defect and removes
the retry exception; ADR 0208 performs the monorepo migration, dry-run first;
ADR 0209 ports the full ket checker once at the monorepo root. Before migration,
rnx and benchmark follower implementation plans cite this Rune decision at its
exact published commit, without duplicating the ADR; the rnx reference equals
the Rune revision pinned in Cargo.lock. Do not move old plans or rewrite
published history. Add a committed preflight guard for subject, signature,
ADR existence and plan-only paths; negative controls must reject malformed
subjects, trailers, missing decisions and implementation changes in plan commits.
