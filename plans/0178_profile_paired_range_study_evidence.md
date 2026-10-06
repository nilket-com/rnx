# rnx 0178: profile-paired range study evidence

**Result: all four frozen cells STOP.** P1 (one codegen unit) nearly eliminates the stopped candidates' startup instruction regression, but both retain unrelated interpreter regressions above 0.5%. No profile/candidate meets the frozen gates. This diagnostic adopts no candidate or profile, does not reverse 0175/0176, and leaves the range approach parked. Fork main and shipped Rune 0.14.2 are unchanged.

## Provenance, builds and repairs

Accepted plan `31ca480`; Claude tooling `10c99af3`, manifest `9d763a88`, original preparation receipts `b1564200`, reviewed guard repairs `65b18377` and additional receipts `83803797`. Diagnostic `0f898be8`, official matrix `d1ba7159`; Codex integrates their history through merge `7921f41a` and adds independent audit scripts and retained reports. Claude owns builds/measurement; Codex reviews tooling and reconstructs results.

Profiles are exactly P0 Cargo default release and P1 with only codegen-units=1, opt-level 3/panic-unwind/default debug settings otherwise unchanged. lto=false does not mean all crate-local LTO is disabled. Sources are baseline eaa59fc208c136ead86f8c4fa565431ea18de88b, S75 863370da279031b369ebb0ac1c0d80cd9ca159f6, S76 6f54bd32ee1038e3927ef384d1a914d8d5b36c4f. No source edit, compiler upgrade, new candidate or hidden profile selection.

All 18 primary/counter/allocation builds are fresh at the fixed fork/target paths and fixed order; -j8, locked/offline after one separately reported locked fetch, rustc 1.98.1, no RUSTFLAGS, effective Rune features alloc/anyhow/fmt/serde/std. Target rustc invocations have cgu 1 for P1 and no CGU override for P0; compile-time host build dependencies are identified separately. Codex independently verified every artifact hash, role/revision, effective target flag and Rune feature against actual logs. P0 B/S75/S76 reproduce prior primary hashes 7a66b042/ec6962fb/3461f953 (counting builds also match). P1 B d66a5ea5 reproduces 0174; P1 S75/S76 are e099c552/98a7cea7. Full SHA-256s and .text hashes are in the manifest/build receipts.

Build cost is the cargo-build compile/link command only, excluding subsequent copy/hash extraction: P0 36.04–37.26s, P1 77.89–78.23s across nine distinct source/kind observations each. These are not repeat-build distributions, product cargo-install costs or adoption recommendations. The manifest profile is restored/verified after each source/profile cell's three kinds, rather than each individual kind; all three share the same settings. Both details are disclosed departures from the plan's broader cost/restoration wording; durations were neither altered nor remeasured. Historical 0174 product costs are separate.

Suites: B 611 passed, S75/S76 each 612; candidates' named non-tracing cases 15 each; no-std checks pass for all three. Test-profile inventory golden agrees; release inventory independence remains an explicitly untested assumption. Four rehearsals pass 40 cases against both pair baseline and P0 B, with the two historical normalizations only. Allocation/budget qualification is identical in each profile: context +1, floor/empty 0, ordinary manual-next Options, numeric 1,036,660→36,661 (−96.46%). Zero/default/unlimited/tight-halt controls retain exact outputs/statuses; explicit memory-limit scopes, tracing and no-std fall back.

Before S3/S4, review found missing profile metadata validation, absent new matrix/reproduction controls, and build lifecycle results recorded without enforcement. Shared freeze/verify now binds exact profiles, eighteen keys, row profile/source/kind/revision/section/CGU/hash; bind_pair resets all state and hashes all six role artifacts. Ten new controls cover swapped/missing/mislabelled inputs, same-process role rebinding, scientific STOP continuation, infrastructure stop/partial retention, P0-only reproduction and status 0 lifecycle failure. All ten pass; inherited sixteen rerun pass; five diagnostic controls and seven parser fixtures pass. Original preparation receipts and manifest remain byte-identical. No rebuild required: all recorded original build ledger lifecycle fields were clean. Original fetch receipt recorded status/duration/timeout but not the additional lifecycle fields; new code retains/gates those on future execution, not retroactively manufactured fields.

## Official matrix and independent reconstruction

One S3 diagnostic and one S4 matrix, committed sources, stages under the shared lock, both exit 0. All 24 diagnostics and all 4 matrix cells complete. No official failure, repair/replay or post-result source/profile/gate change. Scientific STOP continues to later cells; infrastructure failure would stop with partial evidence. Fixed pair order P0:S75, P1:S75, P1:S76, P0:S76. Each pair has its own fresh same-profile baseline samples.

Every pre-decision correctness, allocation, calibration, applicable reproduction and retention gate passes. P0 alone uses default-profile historical 2% references; P1 explicitly records not applicable and emits no historical reproduction samples. Every calibration is inside 0.007 ms, below 0.15 ms. Representative inherited-child affinity controls pass; per-sample fields remain controller affinity, not observed child affinity. Safe environments and process ownership/reap/survivor/deadline receipts are retained. Each fake-secret scan is 0; Claude reports a zero known-credential scan. Earlier exposed credential rotation remains separately unconfirmed.

Codex independently parses 1680 native PMU samples and 1008 wall-block receipts (5040 timed child processes), checks exact ABBA/BAAB ordering, counters/running ratios, receipt medians and base p10–p90 widths, calibration/reproduction applicability, allocations, lifecycle, staged hashes and all four decisions. No subject is re-executed by this audit. All four are STOP with no instruction/wall disagreements. The first combined audit expected outer status text “0”; actual format is “exit 0”. That read-only assertion was corrected before edge replay, disclosed in matrix-audit.json, with no measured evidence changed.

Each cell's triggering gates, from raw reconstruction:

- p0-s75: context:instructions, runtime:instructions, compile-answer:instructions, run-answer:instructions, run-empty:instructions, run-fib:instructions, run-fib:wall, run-calls:instructions, run-calls:wall, run-overwrite_alias:instructions.
- p1-s75: run-fib:instructions, run-while:instructions, run-compare:instructions, run-calls:instructions, run-calls:wall, run-vector:instructions, run-overwrite_inline:instructions, run-overwrite_mixed:instructions, run-overwrite_deep:instructions, run-range_while:instructions.
- p1-s76: run-fib:instructions, run-while:instructions, run-compare:instructions, run-compare:wall, run-calls:instructions, run-vector:instructions, run-overwrite_inline:instructions, run-overwrite_inline:wall, run-overwrite_mixed:instructions, run-overwrite_deep:instructions, run-range_while:instructions.
- p0-s76: context:instructions, runtime:instructions, compile-answer:instructions, run-answer:instructions, run-empty:instructions, run-fib:instructions, run-fib:wall, run-calls:instructions, run-calls:wall, run-overwrite_alias:instructions.

All 21 workload results below are **instructions change / wall change** against that cell's fresh same-profile baseline. Full samples, medians, widths and hashes are retained per cell; no cross-profile gate or averaging.

| Workload | P0:S75 | P1:S75 | P1:S76 | P0:S76 |
| --- | ---: | ---: | ---: | ---: |
| floor | +0.038% / +0.07% | +0.046% / +0.07% | -0.084% / +5.94% | +0.086% / +3.16% |
| empty-context | +0.041% / +2.16% | +0.011% / +3.04% | +0.104% / +0.77% | +0.137% / +3.00% |
| context | +2.085% / +0.79% | +0.002% / +0.69% | +0.001% / +0.82% | +2.087% / +0.96% |
| runtime | +1.807% / +0.62% | +0.001% / +0.44% | +0.000% / +1.19% | +1.806% / +0.79% |
| compile-answer | +1.540% / +0.49% | +0.000% / +0.91% | +0.000% / +0.49% | +1.542% / +1.19% |
| run-answer | +1.540% / -0.29% | +0.000% / +0.49% | +0.002% / +0.76% | +1.540% / +0.20% |
| run-empty | +1.800% / +0.20% | +0.002% / -0.65% | +0.001% / +1.60% | +1.800% / +0.38% |
| run-numeric | -30.558% / -39.91% | -29.630% / -33.75% | -27.978% / -33.27% | -32.036% / -40.59% |
| run-fib | +8.557% / +7.52% | +2.267% / +0.86% | +2.267% / +0.51% | +7.795% / +7.56% |
| run-strings | -9.076% / -9.75% | -9.110% / -9.49% | -8.560% / -10.40% | -9.603% / -10.77% |
| run-while | -0.047% / -0.09% | +2.682% / +0.02% | +2.761% / +0.85% | -0.922% / -1.09% |
| run-compare | -0.058% / -0.59% | +2.797% / +0.28% | +2.897% / +2.96% | -0.983% / -0.86% |
| run-calls | +3.958% / +5.11% | +2.117% / +1.47% | +2.375% / -0.27% | +3.228% / +4.37% |
| run-vector | +0.081% / -1.96% | +2.406% / -0.60% | +2.497% / +0.96% | -0.695% / -1.59% |
| run-overwrite_inline | -2.936% / -1.50% | +2.787% / +0.51% | +3.125% / +1.68% | -3.812% / -1.17% |
| run-overwrite_mixed | -0.981% / -0.14% | +1.785% / +0.39% | +1.824% / +1.51% | -1.542% / +0.14% |
| run-overwrite_deep | -0.565% / -3.31% | +0.954% / +0.64% | +1.183% / +0.74% | -0.918% / -3.41% |
| run-overwrite_alias | +1.518% / +0.31% | +0.001% / +0.53% | +0.000% / +1.08% | +1.519% / +0.78% |
| run-range_signed | -37.475% / -50.00% | -38.834% / -45.73% | -36.783% / -45.30% | -39.076% / -52.17% |
| run-range_negative | -37.475% / -49.88% | -38.834% / -45.98% | -36.783% / -45.47% | -39.076% / -52.21% |
| run-range_while | -0.058% / +1.37% | +2.454% / +1.40% | +2.749% / -0.38% | -0.911% / -2.01% |

Numeric and both admitted range controls clear the 10% instruction improvement and 90% allocation reduction conditions in every cell. They cannot override a failed unrelated-workload gate. Under P0 startup and fib/calls repeat the stopped results. Under P1 startup is within about 0.002%, but fib/calls and several other interpreter fixtures exceed 0.5%. No cell earns even a diagnostic WIN.

## Diagnostic and attribution limits

Callgrind Ir is instrumented events, not native instructions:u. Independently audited 98 successful/reaped tool receipts, 24 raw event totals vs stderr and both annotation tables, and full retained nm sizes/addresses. Replaying the reviewed edge parser exactly reproduces the edge report (excluding compressed-SHA metadata added after lossless compression). Raw compressed artifacts remain retained.

| Workload | P0 B | P0 S75 | P0 S76 | P1 B | P1 S75 | P1 S76 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| context | 27,355,056 | 27,913,379 | 27,913,433 | 27,027,057 | 27,027,362 | 27,027,362 |
| run-fib | 834,108,466 | 905,438,920 | 899,082,757 | 925,924,949 | 946,900,736 | 946,900,745 |
| run-calls | 1,780,681,404 | 1,851,140,072 | 1,838,140,106 | 1,937,333,063 | 1,978,333,392 | 1,983,333,398 |
| run-numeric | 1,759,601,771 | 1,222,060,374 | 1,196,060,387 | 1,816,253,449 | 1,278,253,786 | 1,308,253,821 |

P0 reproduces the located pop_call_frame→Repr drop edge (fib 2,542,488 calls / 45,764,784 inclusive Ir in both candidates, none recorded in B) and context B-tree comparison edge. P1 has no recorded pop_call_frame drop-glue caller edge in any subject, and no recorded standalone context comparison edge. This says compiled attribution changes; it does not say destruction/comparison work disappears. P1 fib/calls differences concentrate in Vm::run exclusive attribution (fib about 20.98M, calls about 41.00M for S75); these are not isolated causal overhead estimates. Same-named symbols may be ambiguous, and edge-inclusive costs must not be summed with overlapping exclusive costs. CGU assignments, a particular inlining bonus or the cause of remaining costs are not proven.

Cross-profile baseline wall medians, **descriptive only**, from each pair's separate schedule:

| Workload | P0:S75 B ms | P1:S75 B ms | P1:S76 B ms | P0:S76 B ms |
| --- | ---: | ---: | ---: | ---: |
| context | 3.719 | 3.548 | 3.560 | 3.737 |
| run-fib | 52.116 | 58.615 | 58.636 | 51.947 |
| run-calls | 94.837 | 108.063 | 108.211 | 94.709 |
| run-numeric | 112.897 | 102.809 | 103.036 | 113.144 |

P1 B is slower on fib/calls here but faster on numeric/context; do not generalize that every loop slows or claim a separately randomized cross-profile effect. A better absolute time on one workload does not waive a same-profile regression elsewhere. Contemporary Lua references remain descriptive complete process-plus-workload windows.

## Closure boundary

The profile experiment removes the measured startup shift, but it does not make either range candidate pass. The regressions were located, not explained; changed code can affect compilation of paths it never executes. No variant, profile, threshold or baseline is selected after results. No public upstream filing, deployment, dependency switch or fork-main change follows. Publication is generic rnx evidence and bench results only. Another architecture/profile policy requires its own reviewed plan and user direction; this record closes its negative matrix as measured.
