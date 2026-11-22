# Implementation plan 0322: The engine image ships as a package

Status: Proposed

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
