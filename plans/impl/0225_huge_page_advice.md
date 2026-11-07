# Implementation plan: ADR 0225

Add a safe `advise_huge_capacity(&mut Vec<T>)` in the primitives module.
Linux/non-Miri uses one extern block (madvise, getpagesize), with cached,
validated runtime page size and best-effort ignored advice result. Other
platforms and Miri use an inline no-op. Pure checked inward-rounding arithmetic
is tested separately, including overflow, empty and unaligned ranges.

Keep all existing reserved/filled call sites and symbols. Advise at the end of
reserved, after allocation and capacity-limit checks; filled delegates to it
before initialization. This issues advice once, not again in NumericOutput.
The 4 MiB capacity threshold excludes small compiler metadata allocations;
borrowed inputs never pass this owned-allocation boundary. Core remains
forbid(unsafe_code), with no new dependency. If protected operands::execute
still drifts after this one minimal-diff attempt, stop and report disassembly;
no further code-generation tweaks are authorized.

Measure as follows:
- Rerun the 0225 per-call fault diagnostic on the candidate, with faults and
  AnonHugePages.
- Run the protected Ir small and large matrix against `45823dda`, pinning any
  fixed per-call delta from the advice branch.
- Run the 0224 harness rnx cells at 100M, first and warm, with 5 process
  samples (quiet serial, descriptive).
- Run the host, integration, generated and cross gates and the commit
  checker.

Remove nothing from the BACKLOG. Add "0226: bounded last-owner output
recycling (repeated calls)".

Reviewed bounded follow-up: make advise_huge_capacity inline(never), changing
no other runtime code. Screen booleq, boolxor and multiply3 at 4096/65536 rows,
first and warm calls, before the full small matrix and fault capture. Any
remaining per-row execute slope stops this attempt; archive the implementation
and withdraw this record before packed Boolean xor/equality work.

## Disposition

Withdrawn after the bounded outlined-helper screen failed. All three signed
implementation states, the plan, capture receipts and disassembly attribution
are preserved in a verified local archive. This implementation commit ships
only the withdrawal documentation; the runtime and backlog match the published
0224 baseline. No host gates or further performance experiments were run.
