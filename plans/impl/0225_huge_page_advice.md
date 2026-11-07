# Implementation plan: ADR 0225

Add a safe `advise_huge_capacity(&mut Vec<T>)` in the primitives module.
Linux/non-Miri uses one extern block (madvise, getpagesize), with cached,
validated runtime page size and best-effort ignored advice result. Other
platforms and Miri use an inline no-op. Pure checked inward-rounding arithmetic
is tested separately, including overflow, empty and unaligned ranges.

Add runtime-only reserved_output/filled_output wrappers. Keep the shared
reserved/filled helpers and compiler metadata unchanged. Update run/flat
output, validity and scratch sites only. After allocation and capacity-limit
checks, advise once before any write; NumericOutput must not advise again.
Skip zero-sized types and capacity below 4 MiB. Never inspect uninitialized
contents. Core remains forbid(unsafe_code), with no new dependency.

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
