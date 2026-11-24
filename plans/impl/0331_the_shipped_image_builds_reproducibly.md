# Implementation plan 0331: The shipped image builds reproducibly

Status: Proposed

Paired ADR: ../adr/0331_the_shipped_image_builds_reproducibly.md.

1. Boundary: published 870413ea and the build-identity evidence from
   the 0329 investigation. List every input that reaches the image,
   helper and CLI bytes. Record any deviation before editing.
2. Resolve and bind each exported commit and its rune subtree at the
   archive producer. Trace the declaration and Cargo rerun inputs
   through the outer and nested builds. Preserve the exact-root Git
   path for ordinary builds and the independent acquisition checks.
3. Map compiler-visible paths in the stock release recipe and handle
   the generated source-hint literal in the shipped export. Include
   the effective build recipe in fixed-route identity.
4. Compare independent cold exports and locate any remaining byte
   differences. Retain the artifact hashes, input manifest and
   differing sections. Apply the ADR's review boundary.
5. Exercise the public CLI extraction, provenance and warm rebuild,
   behavior-changing plant and loaded-segment path controls from the ADR.
6. fmt, Clippy, preflight and the targeted checks under the final's
   real recipe. Sign. Source review, one complete final, Results.
