# Implementation plan: ADR 0227

Reapply the ADR 0225 minimal source (archived bundle, ref
`refs/archive/adr0225/minimal`, commit `26271ba2`) on top of `94c30cee`:
- the primitives helper and its tests;
- the arch-gated extern block;
- one call at the end of `reserved`.

Check that the diff against `94c30cee` is limited to those lines.

Before launching anything, run the exact local clippy command. Then run the
core controls and the targeted `huge_page_tests` Miri. Rerun the 0225 fault
diagnostic and the small exact-vector comparison against `94c30cee` across
the full 0226 protected matrix. If that passes, pin any fixed deltas, run the
10M/100M matrix, then the 0224-harness rnx timing cells (5 process samples)
and the host, integration, generated and cross gates, plus the commit
checker. Then publish the bench report.
