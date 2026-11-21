# Implementation plan 0319: Engine handles own a provider value

Status: Implemented

Paired ADR: ../adr/0319_engine_handles_own_a_provider_value.md.

1. Boundary: Codex's 0319 proposal at 230ab56e. Record any deviation
   before editing. State the program's consumption point, including when
   preparation fails.
2. Add the versioned handle surface: the context, program and invocation
   lifecycle; typed, generation-checked tokens; re-entry rejection across
   both ABI versions.
3. Add the engine-owned opaque slots, the synthetic provider (create,
   shared read, drop) and the credit transfer under a drop guard.
4. Add the checked-file witness, and the lifecycle, credit and
   planted-fault controls. Check v1 compatibility. Capture the frozen
   split-pair matrix, and report stock-to-sibling differences as
   diagnostics only.
5. fmt, Clippy and preflight. Sign. Then run `final.sh --required` on the
   signed commit. Source review, one complete final, Results.

## Results

The engine image now has a versioned handle surface alongside ABI v1.
The C witness runs a real checked file through it. The file creates an
opaque provider value, aliases it, reads it twice and returns a scalar.
The provider then drops the value exactly once, and the run took
`FileProgram::Checked`. The plain and tagged providers returned 42 and 44
from the same engine image bytes.
- The lifecycle controls passed:
  - stale, reused, wrong-kind, foreign-context and wrong-thread tokens;
  - two live threads;
  - repeated drive and release;
  - re-entry;
  - truncated diagnostics after an effect;
  - abandonment before drive;
  - two runs on one context;
  - both sides of the consumption point.
- The credit controls passed: the exact boundary, one byte short,
  overflow, provider failure and rejection after creation. The drop and
  credit counters balanced.
- Planted faults in generation checking, drop, peak credit and retained
  charge each failed their control.
- An existing v1 caller still works.

The provider and the handle dispatcher live in `engine/image`. Root
`src` gained only `checked_sources`, which opens the entry and returns
its sources and loader. Rune gained `initialize_image_thread`, which
stock never calls.

The first frozen sibling comparison failed. Native `shared_rows` setup T
was 161,243 for the plain launcher and 161,270 for the tagged one, +27
against an allowance of +11, at both sizes.
- Rune's `Cache::enabled` read `PERF_BUILDID_DIR` through one `getenv`,
  inside the first measured invocation.
- The environment bytes were identical. The lookup key's stack address
  differed by 32 bytes, so `__strncmp_avx2` took its page-boundary path
  three times for one launcher and never for the other, at 9
  instructions each.
- The image now decides native eligibility at v1 context creation and v2
  seal. In the rerun of the frozen comparison all 32 rows passed, and
  `shared_rows` setup T was 160,231 for both launchers at both sizes.
- No measured native setup or execution window made a Rune eligibility
  lookup, and `shared_rows` made no `getenv` calls at all.
- Cranelift 0.135.5 still reads the same variable once for each function
  it defines. Its per-invocation counts agree across launchers and sizes:
  1 each for `fib`, `loops` and `numeric_calls`, 2 each for
  `awfy_sieve`, `awfy_permute` and `vec_iter_bool`, and 3 for
  `vec_iter_i64`.

Compared with stock, every execution check passed. Six native setup rows
exceeded the stock per-change limits. That is diagnostic only, and the
cutover bound governs it. `shared_rows`, which was over the limit in
0318, is now within it. At size 5:

| Kernel | Count | Increase |
| --- | --- | ---: |
| loops | W | 818 |
| numeric_calls | W | 779 |
| awfy_sieve | W | 2,034 |
| awfy_permute | T | 166,780 |
| vec_iter_i64 | W | 3,204 |
| vec_iter_bool | W | 2,152 |

The first required run, on af8b4a44, failed the stock host inventory
and cross. The synthetic provider's registrations were under root `src`.
They now live in `engine/image`. The implementation was rebased onto
the final signed plan as a docs-only change: the tree differs from the
tested commit only in this record's ADR.

A required final on 05aa0b0a passed all 14 selected stages in 30.3
minutes. The complete final on 05236ec6 passed all 104 stages, with 41.2
minutes of accepted run and 42.0 minutes end to end. Both fixed-route
bands held, there were no memory.high, memory.max or OOM events, and the
host was in the performance profile.
