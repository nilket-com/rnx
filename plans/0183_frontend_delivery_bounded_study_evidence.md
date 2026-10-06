# rnx 0183: frontend-delivery counting result

**Status:** closure evidence for review. One official run completed with registered disposition **A-mixed**. The record closes at Stage A. No engine, build-profile or product change is adopted; 0179's STOP stands. No localisation or causal intervention ran.

## Protocol and receipts

The plan is `0183_frontend_delivery_bounded_study.md` (17d6d5a); the source-only grouping audit is `0183_grouping_audit.md` (2d3a11b). The audit excluded F2 and F3 before discovery: TakenAlone safety alongside possibly active programmable counters was not established on this host. This is a conditional safety conclusion, not evidence that an unsafe operation occurred. No spelling of either excluded event was opened.

Benchmark history is on `w-0183-driver`, through 14d9cfa0. Source 384c0fae, clean execution tree 46c1e72d. Discovery, pin controls and rehearsal each passed a separate review before the next phase. Official attempt 1 ran once, exited 0, and retained all 280 samples in `results/frontend-0183/official1/raw.jsonl`; its SHA-256 is `02c2868fe77583ea7909b9e4e1c654deb0e518e3b00450227dbbaada342ad891`. No repair, replay or additional sample followed the result.

The retained 0179 base and candidate primaries were hash-checked before and after each sample, using the same staged path, fixed environment and CPU 4. R and F1 each completed 140 samples in the registered five-repetition ABBA order. Both groups reproduced the instruction and cycle anchors. Admission and the identity rechecks before each group passed. The final sentinel receipt completed with zero occurrences.

Discovery identity SHA-256: `c281c11d5b0a0d3552415836e66635a9aed03c6fa3162c06f7f921848882f2aa`. Availability SHA-256: `c60910efcf21eb4ccacb92a54547cc005c0ffc296ec9cbdd473b845528b8383d`. These are frozen in the driver. The 0180/0181 drivers and 0179 helper remain unmodified and are hash-verified before import.

## Independent reconstruction

Codex's read-only `probes/frontend-0183/audit.py` checks the exact raw hash, all 280 row identities and their order, argv, environment, affinity, status, output, lifecycle and stage hashes. It uses the reviewed counter classifier, independently reconstructs all 73 quantities (medians, five paired contrasts, base p10–p90 widths and resolution), checks every reproduction anchor, and independently derives the signal-window decisions, disposition and contrast flag. All quantities equal the retained report; both seven-row evidence tables match their values. It confirms the admission pins, identity-version receipts, exclusions, closure consequence and zero-occurrence sentinel receipt. The audit opens no counters and executes no subject; `results/frontend-0183/audit.json` retains its result.

## Result

D is candidate pooled median minus base pooled median, in the F1 group. “Resolved” is the pre-registered descriptive rule, not a confidence interval or causal test.

| Window | D(fetch-penalty cycles) | D(total cycles) | Ratio of differences | Registered status |
| --- | ---: | ---: | ---: | --- |
| numeric | +28,371,443.0 | +36,034,770.0 | 0.7873 | strong |
| signed range | +12,536,458.0 | +53,746,886.0 | 0.2332 | intermediate |
| negative range | +12,972,139.5 | +54,096,339.0 | 0.2398 | intermediate |

Both quantities resolve upward in these three windows. One strong and two intermediate windows yield **A-mixed**, not A-strengthened. The contrast flag is false: while's penalty difference is unresolved and slightly negative; empty's resolves downward. Calls, reported only, has a penalty difference larger than its total-cycle difference (+14.85 million versus +10.94 million). Fib's penalty and cycle differences are unresolved under the registered rule.

These ratios compare magnitudes of differences of medians. They do not partition total cycles into explained and unexplained components. Together with 0181's increased legacy-decode uops and lower decoded-cache share, the counting result adds evidence compatible with a frontend-delivery explanation, with different magnitudes across the three signal windows. It does not locate the switches, establish causality, or explain the compiler's changed interpreter code. Counts cover whole processes on these two binaries and one host, not just the VM or other builds.

No locating event was collected, so no Stage B proposal is permitted and Stages B/C do not run. The architecture work in 0184 remains independent of this diagnostic.

## Disclosures

Before discovery, review found malformed `resolved`/`direction` fields could reach the decision boundary. The driver was repaired and the reproducers retained as controls; no measured summary had used those malformed fields. Controls passed 15/15, 15/15 and finally 16/16, including inherited control replays. Codex independently replayed the final set, 16/16. General Stage B eligibility is intentionally unimplemented because its prerequisite events were excluded. Sentinel scope remains inherited: official scans on success and failure, rehearsal on success, discovery without a sentinel and with a fixed minimal environment. The driver-side prose was corrected at closure to avoid treating the ratio as cycle accounting; measurements and decisions were unchanged.
