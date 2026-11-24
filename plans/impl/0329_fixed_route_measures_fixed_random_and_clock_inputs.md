# Implementation plan 0329: Fixed-route measures fixed random and clock inputs

Status: Proposed

Paired ADR: ../adr/0329_fixed_route_measures_fixed_random_and_clock_inputs.md.

1. Boundary: published 4f0195be and the v3 attribution receipts. Run
   the input census (Decision 3) and report it, including the seed
   and clock paths on supported historical images. Resolve any
   potentially count-affecting input or stop.
2. Add the seed hook and the clock supply to the observer, with
   per-thread accounting and capture invalidation.
3. Freeze the method before scoring. Bind it into capture keys and
   exercise rejection of an old-method receipt. Inventory the policy's
   execution anchor (48fe3b15), native setup (ff5bf81e), installed
   setup (the 0324 freeze on ed6b1a07), and the later outer reference
   (5530079d). For each applicable quantity
   record its original revision and artifact hashes, measure the new
   method on the same artifacts, and report its transition delta. If a
   pinned artifact is unavailable, report that before substituting a
   rebuild. Preserve historical receipts and band limits.
4. Add the repeatability, second-seed, sensitivity, negative and
   planted-change controls.
5. fmt, Clippy, preflight and the targeted checks under the final's
   real recipe. Sign. Source review, one complete final, Results.
