# Implementation plan 0325: Proven self calls lower directly

Status: Implemented

Paired ADR: ../adr/0325_proven_self_calls_lower_directly.md.

1. Boundary: Codex's lowering proposal (seq 35303), reconciled against
   published 5530079d. Record any deviation before editing.
2. Split the self binding and target proof from the dynamic gate in
   `self_receiver.rs`. Order the specialization in `lower.rs` before the
   general closed-name dynamic route.
3. Add the guarded direct call with receiver and operands saved once,
   and the unconditional form only where ingress proves the receiver.
4. Fold the proven self nominal test in the native recursive-method
   analysis under a validated concrete receiver at region entry, as in
   Decision 4.
5. Extend the contract with the 0325 overlay and add the file witnesses
   on both engines.
6. Score Permute, the file gate and both project routes per Decision 8
   before signing, and report the guard cost and any residual.
7. fmt, Clippy, preflight and the targeted checks under the final's real
   recipe. Sign. Source review, one complete final, Results.

## Results

The first file screen stopped on native-on `shared_rows`. Execution was
unchanged. The per-record setup increase was 2,265 total instructions.
Of these, 2,010 came from the standard library's main-thread stack query
before the installed CLI executed its helper. Under Valgrind, glibc read
53 mapping rows on the candidate and 52 on the baseline. Three native
launches of each binary found 32 mappings before the stack in every case.
Another 252 instructions came from 21 additional ELF relative relocations
for Rust bounds-check panic locations. Three instructions were in inner
setup.

The approved measurement change applies only to the outer pre-exec window.
A query-only Callgrind run measures the work and total instructions in
`pthread_getattr_np`. Its total must equal the inclusive cost of the
single call from `std::rt::lang_start_internal` in the original profile.
The receipt keeps the raw, excluded and native-map evidence. Baseline,
candidate and frozen reference use the same deduction. The frozen 0324
binary reproduced the original whole-window counts in both modes. The
deduction there is 366 work instructions and 114,438 total instructions.
Native mapping counts must match the frozen reference on both sides.
The execution windows and setup allowances retain their original values.

The checked-access candidate eliminated all 21 new panic-location
relocations. The installed ELF contained 31,591 relative relocations
against 31,592 in published 5530079d. The additional removal was the
existing receiver lookup at `lower/dynamic_methods.rs:79` in 5530079d.
The malformed-guard control, the other method and receiver controls, and
the isolated-window scoring controls passed.

The new file screen passed all 32 per-record comparisons and 31 of 32
cumulative comparisons. At both sizes, native-on `shared_rows`
gave the following total instruction counts.

| Setup component | Original 0324 freeze | Published 5530079d | 0325 candidate |
| --- | ---: | ---: | ---: |
| Raw outer window | 862,165 | 860,914 | 862,912 |
| Excluded stack query | 114,438 | 112,368 | 114,378 |
| Adjusted outer window | 747,727 | 748,546 | 748,534 |
| Inner setup | 127,758 | 127,774 | 127,777 |
| Total scored setup | 875,485 | 876,320 | 876,311 |

The published build already carried **835 additional setup instructions**,
819 in the adjusted outer window and 16 inside the helper. This is the
measured startup cost across 0324's memfd fallback change and subsequent
repairs. Its original freeze preceded the introduction of the
`memfd_create@GLIBC_2.27` import. The largest attributable outer
increases were in the loader's `strcmp` (+832), `do_lookup_x` (+338)
and `_dl_lookup_symbol_x` (+170), with offsets elsewhere. The
function counts measure the aggregate change between those builds.

The cumulative reference for the installed outer window now comes from
published 5530079d, measured with the stack-query exclusion in both
native modes. Each kernel keeps its frozen inner setup reference. For
`shared_rows`, the cumulative reference is therefore 748,546 + 127,758
= 876,304. The candidate is seven above that reference (allowance 11)
and nine below published. Execution and setup growth matched published.
The Rust scorer reproduced the original verdicts on the retained
captures before rescoring them. All 32 per-record comparisons and all
32 cumulative comparisons passed.

Both generated project routes ran the same dual-entry engine image
(`d6695546933afd40013964746d10ec8361c109c16d1e1375b19da997cf27ac3e`).
All 128 comparisons passed across the eight kernels, two native modes,
work and total instructions, and size-5 level and size-5 minus size-1
growth. Native-on measures execution and native-off measures the whole
invocation. All fixture and output checks matched their route's frozen
baseline. The checked file image also gave identical execution counts
for Permute and its same-length rename in both modes at both sizes.

Native-on Permute instructions, with the baseline and candidate in each cell:

| Route | Size-5 work | Work growth | Size-5 total | Total growth |
| --- | ---: | ---: | ---: | ---: |
| File | 106,337,591 → 97,817,797 | 42,413,764 → 39,609,240 | 107,080,199 → 98,559,680 | 42,419,434 → 39,614,934 |
| Empty project | 105,736,031 → 97,817,797 | 42,415,008 → 39,609,240 | 106,477,544 → 98,559,680 | 42,422,447 → 39,614,934 |
| Polars/Candle project | 100,774,080 → 97,817,797 | 41,843,836 → 39,609,240 | 101,515,971 → 98,698,908 | 41,851,573 → 39,615,281 |

The separate all-static diagnostic measured 94,000,545 size-5 execution
work instructions. The guarded candidate retains 3,817,252 more on that
comparison. Public entry can pass a different receiver, so the ordinary
mismatch route remains available. The private recursive projection folds
the nominal check after validating the receiver at region entry.

The complete final on d217e489 passed all 107 stages, with 57.6 minutes
of accepted run and 58.1 minutes end to end. Both fixed-route bands
held, there were no memory.high, memory.max or OOM events, and the host
was in the performance profile.
