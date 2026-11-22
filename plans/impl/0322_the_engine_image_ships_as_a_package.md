# Implementation plan 0322: The engine image ships as a package

Status: Implemented

Paired ADR: ../adr/0322_the_engine_image_ships_as_a_package.md.

1. Boundary: Codex's 5/6 proposal and checkpoint rulings, reconciled
   against the published base. Record any deviation before editing.
2. Add the versioned stock and provider file entries and the CLI witness.
3. Move the launcher and provider into their product homes, with
   metadata from the product installers. Demos and fixtures consume
   them.
4. Add the stock packager and the project builder's opt-in package mode,
   the supported-set and declaration diagnostics, entry selection, and
   the artifact and cache contract.
5. Add the package, isolation, refusal, cost and cleanup controls. Run
   the speed screens on packaged artifacts.
6. fmt, Clippy, preflight and the targeted checks. Sign. Source review,
   one complete final, Results.

## Results

The stock package and the generated Polars/Candle package ran after
relocation with the build checkout hidden. Both ran an ordinary file
with a sibling module and preserved script arguments, including an empty
argument. The paired package also ran the checked CSV-to-tensor model
and returned 64. A terminal error with a diagnostic over 9 KiB performed
its preceding append once. SIGINT returned 130. Explicit process exits 0
and 23 preserved both unterminated output streams byte-for-byte against
stock and ended the launcher process. With the provider installed, the
names and metadata scripts see matched stock, including Dense's
named-field struct shape. All earlier ABI versions passed their contract
checks.

Changing the provider alone changed the model dimensions from (3, 8, 1)
to (3, 8, 2) and changed the launcher digest. All three measured
launchers used the same 26,157,512-byte engine. The provider-only build
invoked Cargo for the provider alone. An unchanged rebuild reused the
package in place. Failed and interrupted links preserved the prior
generation.

On the measurement host, a cold release engine took 70.44 s, a stock
launcher 0.54 s, and a generated launcher 372.22 s. An unchanged
generated build took 0.61 s and the provider-only rebuild took 341.11 s.
The stock and generated launchers occupied 17,088 and 43,841,432 bytes
of file content. Allocated disk after cleanup was 26,161,152 bytes for
the frozen engine bundle, 26,189,824 for stock, and 70,012,928 for the
generated package. Private build maxima sampled at stage boundaries were
843,857,920, 26,193,920, 2,813,263,872 and 2,813,194,240 bytes for
engine, stock, generated and provider-only, respectively. Compiler and
linker transient peaks were unmeasured.

The ordinary stock build passed all 32 per-record and cumulative
fixed-route comparisons. Packaged stock passed all 32 native execution
checks. The two generated siblings passed all 32 comparisons. The 48
stock-to-image setup observations ranged from −583 to +301,849
instructions, with the largest difference on AWFY Permute, total
instructions at the larger size. These observations carry forward to
record 6.

The file census counted 92 programs admitted by both engines, 152 that
failed on stock, and zero admitted only by stock. For cutover, check
typed pre-effect refusals on real files, legacy selection and fallback,
unsupported adapter sets and declarations, project source-alias
mappings, and non-UTF-8 file paths and arguments. The stock CLI receives
OS strings while the v4 request accepts UTF-8. The census alone does not
establish parity across those cases.

The complete final on e53827e0 passed all 107 stages, with 53.8 minutes
of accepted run and 54.7 minutes end to end. Both fixed-route bands
held, there were no memory.high, memory.max or OOM events, and the host
was in the performance profile.
