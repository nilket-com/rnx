# rnx 0041 evidence: naming at the retained call site

Plan commit: 7d06f93. Bench reference: rnx-bench cee9fcf,
probes/method-naming and results/method_naming_0041.

## Gates

1. Run, eval and the session name `missing` on i64. The regular integration
   test pins the excerpt and caret. Six release comparisons also substitute
   only the intended sentence in the before output and require exact equality;
   run's diagnostic is entirely unchanged.
2. Retained closures at input 4 report the defining source after one or two
   renumbers. Redefining the same binding with a different missing method in
   numbering 2 and calling it in numbering 3 names the second method and its
   numbering. Stored source indices and maps are untouched.
3. The string chain names frobnicate. The earlier to_uppercase call remains in
   the source excerpt, naturally; it is not named as the fault.
4. Object Values stays hashed. A panic quoting the exact hashed message stays
   a panic, even with a matching candidate in its input. The defensive unit
   test removes the retained closure's map and calls it from an input containing
   a matching candidate: the origin is None and Rune's hashed message survives.
   The test separately proves that the caller's candidate would have matched,
   so a current-input fallback would fail it.
5. A unit test uses Session::set_budget and an awaiting expression
   `async { 1 }.await.missing()`. It searches budgets 1..100 for the coincident
   named fault and exhausted guard, then requires the exhaustion clause after
   the complete named sentence. A short synchronous expression cannot place
   this boundary: its driver polls under fixed 10,000-instruction slices.
   Earlier located budget halts inside the async block do not satisfy the gate.
6. Both full suites pass sequentially under TERM=xterm-256color: 309 default
   and 347 test-support. Existing tests are unedited. Runner and naming helper
   are unedited, no dependency or notices change. Windows execution is unverified.

The only production addition borrows the origin's retained input by stable
index, calls method::named on Rune's unmodified message, and keeps that message
if there is no source or no proof. The existing execution path appends budget
context afterwards. There is no copied source or current-input fallback.

## Cost

Measured by Codex on nano, Linux x86_64, 2026-09-14. Rust 1.98.1,
Rune 0.14.2, hyperfine 1.20.0, CPU 4, 10 warmups, 100 samples per command.
Before is the existing accepted 0040 release, saved before changing code;
after implements plan 7d06f93. Exact hashes identify both binaries.

| Command | Before mean ± σ (ms) | After mean ± σ (ms) |
| --- | --- | --- |
| version | 0.558 ± 0.020 | 0.540 ± 0.016 |
| help | 0.558 ± 0.020 | 0.544 ± 0.018 |
| eval | 4.081 ± 0.045 | 4.003 ± 0.022 |
| run | 3.719 ± 0.013 | 3.665 ± 0.029 |
| json | 11.645 ± 0.141 | 11.597 ± 0.161 |

No startup regression detected in this run. Outliers were reported; small
improvements are observations, not attributed speedups. These are whole-process
times. Binary size: 14,878,600 → 14,879,080 bytes (+480). Session startup
allocation reference: 1,823,087 → 1,823,086 bytes. Later allocation sample:
1,831,716 → 1,831,714 bytes. These samples include presentation and harness
conditions; they are allocator requests, not RSS or an attributed saving.
