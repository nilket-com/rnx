# rnx 0176: outlined range dispatch evidence

**Result: STOP.** The single outlined candidate preserves the range allocation improvement and improves its range workloads, but unrelated workloads still violate the frozen no-regression gates. Fibonacci is +7.795% native instructions / +7.73% wall time; calls is +3.228% / +4.02%. The kill criterion therefore parks this implementation approach pending a separately reviewed dispatch redesign. No gate is waived, no second candidate is selected, and no engine change is adopted in fork main or shipped rnx.

## Source and semantics

The accepted plan is `084d2b4`, with helper/borrow checkpoint `257cd30` and the reviewer observation about plain Call/op_call recorded in `3691dbd`. The engine candidate is `6f54bd32ee1038e3927ef384d1a914d8d5b36c4f`, on the previous stopped candidate `863370da279031b369ebb0ac1c0d80cd9ca159f6`. The deciding baseline remains `eaa59fc208c136ead86f8c4fa565431ea18de88b`, production-equivalent to `3e7d4da9ce908eeb7e0e1ac119dec24f68d5449a`. Fork main stays `bb8e69372353c50e271c9f115bc771c77aa6b83e`; shipped Rune stays 0.14.2.

Only `crates/rune/src/runtime/vm.rs` changes from the previous candidate (four hunks, 58 added / 22 removed lines). One private `#[inline(never)]` helper receives a private carrier for associated, inline-Type, or native-Function calls. Hot arms retain rejecting predicates; handler resolution and tag proof move into the helper. Native-Function borrowing ends before mutation. Context-first associated resolution, unit-function shadow precedence, ordinary fallback and halt propagation stay intact. The original admitted range body is identical modulo whitespace, including argument consumption, borrow/error order, typed next, the second budget charge and ordinary Option materialization when fusion cannot finish. No new unsafe code, compiler/bytecode/public API, registration, profile, or test-golden change.

Codex's final clean-hash tests pass: 610 all-feature unit tests plus two integration tests, 15 explicitly non-tracing specialization/fallback tests, and no-std alloc check. Claude independently ran the base and candidate suites (611 and 612 total) and the 15 non-tracing cases. Tracing and no-std never specialize. Existing raw-stack, explicit allocation-limit (including usize::MAX), custom handler, shadowing, diagnostics, budget halt/resume and failure-order contracts remain frozen. Physical allocator-failure opportunities can differ when allocations disappear; explicit allocation-limit scopes fall back.

Source repair disclosure: an aborted edit followed by rustfmt initially produced a formatting-only commit; the intended edit then included unrelated import/Isolated::new formatting drift. That drift was restored before freezing `6f54bd32`. The superseded test chain is retained but not credited, since source changed while it ran. All final tests were rerun at the clean final hash. Neither superseded source received diagnostic or deciding measurements. Receipts: `rnx-bench/results/outlined-range-0176/source-tests/`.

## Builds, driver and lifecycle

Claude owns the measurement tooling and diagnostic; Codex owns the engine edit and independent audits. Driver history and author commits are preserved by merge. The deciding measurement algorithm is copied from reviewed 0175 tooling `640a792d`; changes are record identities/paths, pinned candidate and receipts, plus the separately reviewed diagnostic. No profile substitution or workload warm-up block was introduced.

Fresh independent cleaned targets use the same fork path and default release profile. Measured Rune features are `alloc,anyhow,fmt,serde,std` (no tracing); build receipts retain effective features. Baseline primary SHA-256 `7a66b042…` and rebuilt previous candidate `ec6962fb…` reproduce their old measured identities. Outlined primary is `3461f9539edac646bed542bea72be63da42c06267f52959825bf2a319eba2d2c`. The previous candidate is diagnostic-only, never the deciding baseline. Counting builds are separate.

The reviewed driver passed 16 controls; the diagnostic passed five mocked failure/retention controls. The diagnostic revision retains tool results before checking them, checks nm status before declaring a symbol absent, binds Ir summaries to collected totals, and retains complete symbol tables/disassembly. The abbreviated direct-call-name extraction can truncate nested generic names; the full objdump text is authoritative. No method gate was loosened.

Exactly one diagnostic and one official run executed from committed clean sources under `/tmp/rnx-runtime-bench.lock`; both exited 0. Scientific STOP is distinct from process failure. No official repair/replay or source change followed the diagnostic. All process ownership, deadlines, reap and survivor gates passed. Safe subject environments are retained, not inherited environments. The fake-secret sentinel scan is zero and Claude reports a zero known-credential scan. Rotation of the credential exposed in an earlier record remains separately unconfirmed.

## Frozen deciding samples

The 18 historical workloads and three range controls remain separate. PMU retains 420 samples (five true ABBA repetitions, ten per side per workload), instructions:u and cycles, core 4, at least 99% running. Wall retains 252 five-sample block receipts, yielding 30 fresh pipe-captured processes per side/workload through the resident observer. Hyperfine pipe calibration differences are +0.004 ms for true and −0.005 ms for floor, inside the 0.15 ms gate. Applicable historical reproduction gates pass; tiny FIFO floor/empty windows remain descriptive.

All 40 correctness cases pass with only the two historical normalizations. Counting outputs and budget statuses are exact. Numeric allocation calls fall 96.46%; context registration adds exactly one call (floor/empty zero), and manual-next remains ordinary. Zero/default/unlimited/tight-halt controls preserve the expected status and counters. No opposing instruction/wall disagreement occurs. Gains do not override STOP precedence.

| Workload | Instructions change | Wall median ms, base → candidate | Wall change | Base p10–p90 width ms |
| --- | ---: | ---: | ---: | ---: |
| floor | +0.004% | 0.490 → 0.489 | -0.20% | 0.145 |
| empty-context | +0.153% | 0.498 → 0.499 | +0.28% | 0.154 |
| context | +2.087% | 3.702 → 3.753 | +1.36% | 0.388 |
| runtime | +1.807% | 3.980 → 4.010 | +0.75% | 0.424 |
| compile-answer | +1.539% | 4.343 → 4.355 | +0.29% | 0.513 |
| run-answer | +1.541% | 4.343 → 4.385 | +0.97% | 0.545 |
| run-empty | +1.800% | 4.027 → 4.067 | +1.00% | 0.452 |
| run-numeric | -32.036% | 112.790 → 67.138 | -40.48% | 1.897 |
| run-fib | +7.795% | 51.975 → 55.995 | +7.73% | 1.183 |
| run-strings | -9.603% | 18.487 → 16.465 | -10.94% | 1.136 |
| run-while | -0.922% | 69.397 → 68.589 | -1.16% | 0.879 |
| run-compare | -0.982% | 106.504 → 105.660 | -0.79% | 1.129 |
| run-calls | +3.228% | 95.028 → 98.852 | +4.02% | 1.756 |
| run-vector | -0.695% | 84.083 → 82.782 | -1.55% | 1.622 |
| run-overwrite_inline | -3.812% | 68.278 → 67.392 | -1.30% | 0.914 |
| run-overwrite_mixed | -1.543% | 48.027 → 48.263 | +0.49% | 1.037 |
| run-overwrite_deep | -0.918% | 21.454 → 20.785 | -3.12% | 1.771 |
| run-overwrite_alias | +1.520% | 4.426 → 4.475 | +1.10% | 0.530 |
| run-range_signed | -39.076% | 84.675 → 40.427 | -52.26% | 1.175 |
| run-range_negative | -39.076% | 84.797 → 40.966 | -51.69% | 1.123 |
| run-range_while | -0.911% | 49.450 → 48.219 | -2.49% | 1.058 |

Instructions regress above 0.5% for context, runtime, compile-answer, run-answer, run-empty, run-fib, run-calls and run-overwrite_alias. Fib and calls also exceed their own wall-width gates. Numeric and the two admitted range workloads exceed the 10% instruction-gain criterion, and allocation exceeds the 90% reduction criterion, but unrelated regressions force STOP. Lua numbers are descriptive complete process-plus-workload windows, not startup attribution or a general ranking.

Codex independently reconstructed raw PMU/wall medians, bands, historical reproduction, calibration, allocation, lifecycle and decision using the 0175 audit: 902 retained raw rows, 21 workloads, the same STOP and no disagreements. The report binds raw.jsonl SHA-256 `6382d9f76639811443bbdce2cbd03bb0f9c7d22f1ff371589cf0fea7cae3767d`.

## Diagnostic observations and limits

Callgrind 3.26.0 reports instrumented Ir events, not native instructions:u. Twelve runs cover baseline / previous candidate / outlined candidate on exactly context, numeric, fib and calls. Codex's read-only diagnostic audit checks all 51 successful/reaped command receipts, twelve raw Ir summaries against collected totals and both annotation tables, and symbol addresses/sizes against full retained nm output.

| Workload | Baseline Ir | Previous Ir | Outlined Ir |
| --- | ---: | ---: | ---: |
| context | 27,355,090 | 27,913,369 | 27,913,423 |
| numeric | 1,759,601,801 | 1,222,060,388 | 1,196,060,401 |
| fib | 834,108,864 | 905,439,300 | 899,083,137 |
| calls | 1,780,681,434 | 1,851,140,086 | 1,838,140,120 |

Vm::run shrinks 102,615 → 100,063 → 99,475 bytes; op_call is 2,908 → 2,952 → 2,952. Previous try_range_next is 1,362 bytes, outlined try_range_dispatch 2,629. op_call_fn/associated are not separate symbols in these binaries. Thus shrinking the dispatch symbol is insufficient to pass the operational hypothesis H1.

Exclusive attribution redistributes work: outlined fib attributes 45,764,784 Ir to Repr drop glue and adds 20,339,912 to Value drop glue, while run's exclusive attribution falls 5,402,840. Calls shows a similar redistribution; startup has changed B-tree comparison attribution. These are observations across compiled function boundaries, not isolated overhead estimates. **Repr drop symbols exist in both baseline and candidate (22 each)**; absence from a 99.9% thresholded table proves neither absence of work nor a newly introduced function. No controlled causal intervention establishes that the outlined callsite caused a particular inlining/layout decision. Named script calls in fib/calls are not evidence of executing the new CallFn predicates. The cause remains unestablished, and no extra variant is tried.

Raw diagnostic files are losslessly xz-compressed after the official run; annotation/disassembly and command ledgers remain retained. Audits and their argv/status receipts are in `results/range-iteration-0176/independent-audit/`. The candidate will be retained only on a named non-main branch after reviewer acceptance; this record closes the single experiment as STOP.
