# Implementation plan 0331: The shipped image builds reproducibly

Status: Implemented

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

## Results

The pre-sign comparison used Linux x86-64, Rust and Cargo 1.98.1, the
locked dependencies, and the stock release features. The outer Cargo
link embeds the nested image and the C helper. Their inputs include
the Cranelift-generated Rust sources, the standalone Rust startup
object, the C compiler, linker, ar, objcopy, and the build scripts.
The two cold targets used different physical roots, temporary
directories and enclosing Git commits on the same host.

The exporter binds the commit and its rune subtree to the archive.
The emitter checks the declaration against Git's embedded commit and
root-tree IDs. Neither enclosing HEAD is used. The warm Cargo fixture
changed both IDs and observed the new identity. It also refused a
missing declaration, an old commit paired with a new export, and an
incorrect subtree. The shipped CLI uses the existing placeholder for
its checkout suggestion.

The first prototype could not start the pinned compiler in a mount
namespace. The release recipe instead remaps source, target, temporary
and nested output prefixes in the compiler invocation. The first
cold pair with remapping matched on all three files. Its planted
diagnostic was after a file runner that exits the process. Moving the
plant to image entry made its output observable through the shipped CLI.

On f2fad95d the two cold builds produced identical image, startup
helper and CLI bytes. Their loaded segments contained none of the
physical build roots tested. The behavior plant changed the image and
CLI hashes and printed its marker during a script run. The helper kept
its hash. The standalone control retains each artifact, SHA-256, input
manifest, and public invocation output.

The first fixed-route screen against published 870413ea passed all 32
per-record and 32 cumulative comparisons. Two controller tests still
expected the old paired-builder digest. The revised recipe witnesses
bind the exporter and the paired builder. The build-only mutation
includes the exporter and verifies that the measurement key stays the
same when the artifact bytes match. Both witness tests
passed on the successor. One recheck omitted the Valgrind installation
from the pre-sign environment and failed in the image-observer control.

The successor passed final-controls, clippy-final and fixed-route with
the configured Valgrind installation. All 32 per-record comparisons
and all 32 cumulative comparisons passed. The earlier root Clippy,
formatting and controller-controls stages also passed.

Preflight caught two fixture-size declarations that still described
the old witnesses. The updated ceilings equal the respective file
sizes of 846768 and 14581 bytes.

The complete final on 97b9e542 passed all 107 stages, with 50.9 minutes
of accepted run and 51.5 minutes end to end. Both fixed-route bands
held, there were no memory.high, memory.max or OOM events, and the host
was in the performance profile. No owned entries were added to /tmp or
/var/tmp.
