# Implementation plan 0324: Files run on the installed engine image

Status: Proposed

Paired ADR: ../adr/0324_files_run_on_the_installed_engine_image.md.

1. Boundary: Codex's 6/6 proposal and the split rulings, reconciled
   against published 05283812. Record any deviation before editing.
2. Embed the stock bundle on the CLI side. Add content-addressed atomic
   materialization of the image and the file helper, the pre-exec
   ownership and identity checks, and startup linking through the pinned
   descriptor. Measure full verification first.
3. Carry structured refusals across the image boundary. Keep earlier
   ABI callers.
4. Make the installed image the default for `rnx run FILE`, with the
   counted compatibility route. Remove the opt-in split and the
   in-process new-engine file path.
5. Add the install, cache-safety, route and refusal controls. Run the
   cutover packet against frozen 05283812 and the cost measurements.
6. fmt, Clippy, preflight and the targeted checks, following the
   pre-sign coverage rule. Sign. Source review, one complete final,
   Results.
