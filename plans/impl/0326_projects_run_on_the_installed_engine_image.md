# Implementation plan 0326: Projects run on the installed engine image

Status: Implemented

Paired ADR: ../adr/0326_projects_run_on_the_installed_engine_image.md.

1. Boundary: Codex's projects proposal, reconciled against published
   fef4fcb1. Record any deviation before editing.
2. The pair continuation by exec: the private handoff, signal blocking
   across exec, the empty-buffer assertion and the legacy-only entry,
   with its witness before any routing is enabled.
3. Embed the engine and the startup-linked helper (stock or pair) in the
   generated app, behind one dispatch point shared by `rnx project run`
   and direct runs. Bind identity at publication.
4. Shared-cache coexistence with a measured, bounded retention policy.
   Make prune best-effort.
5. Ineligible routing with reasons and counts. Remove the in-process
   new-engine branch and the opt-in package step.
6. Add the install, cache, route and packet controls, and the startup,
   build-cost and size measurements. Bring the project setup delta for
   acceptance. Apply the runner's `--message-format=json*` match.
7. fmt, Clippy, preflight and the targeted checks under the final's real
   recipe. Sign. Source review, one complete final, Results.

## Results

Historical locks in both manifest formats retain their exact wrapper recipes.
Without a relock they keep their previous runner. Each verified file launch
through the project manager records one `historical_lock` event. A stale
lock stops before a route event or script effect. Existing executables
invoked directly keep their original reporting behavior.

To migrate an eligible project, update the pinned runtime if it predates
the private file handoff, then run `rnx project lock --manifest PATH` and
`rnx project build --manifest PATH`. A fresh lock that still pins an older
runtime records `runtime_without_handoff` at manager launch.

The rebased project execution screen stopped before signing. Against the
published fef4fcb1 generated apps, 120 of 128 comparisons passed. All eight
failures were on AWFY Permute. Native-on execution W at size 5 rose 0.58%
for the empty project and 2.45% for the Polars/Candle project. Native-off W
growth for the pair rose 2.82%. Both routes produced the expected output
at both sizes. The 0325 image counts reproduced against the new reference.
In the native-off pair, the size-5 profile attributed 5,665,181 additional
instructions to dispatch and 1,515,500 to calls from dispatch to Store::tag.

The 12 matched startup comparisons are recorded in PROJECT-SETUP.md.
With a verified warm cache, direct launch total instructions fell by
657,504 for the empty project and rose by 912,771 for the pair. Fresh
project builds took 85.84 and 178.90 seconds for the empty project
(baseline and image), and 457.70 and 916.47 seconds for the pair.

A same-length rename of the benchmark's recursive method left size-5
execution W identical on both image routes and the baseline empty app.
On the baseline adapter pair alone it raised native-on execution W by
1,759,637 and native-off execution W by 7,447,600. With the renamed
method the image still spent 582,981 more native-on instructions on
the pair (0.60%). The official screen used the unmodified benchmark.

The targeted pre-sign checks passed with fd 9 occupied. All seven empty
and nine adapter-pair parity cases matched through both public entry
points, including the CSV larger than 2 GiB. Relocation, continuation,
cache coexistence, historical locks, overrides and manifest reasons
passed. The manifest fixture needed a valid project-source-version
reply for source aliases. Its corrected run covered both manifest
formats and both entry points.

The supplementary stock-file startup measurement found 32 native
mappings on both sides in both modes. Adjusted outer W rose by
1,563 instructions and adjusted outer T fell by 1,362 in both modes.
This measurement covered only the outer window. The complete stock
gate has no verdict on this unsigned tree.

The approved pair reference correction uses the matched renamed
fef4fcb1 control at both sizes in both modes. Registering the unrelated
Tensor::permute selected the unconditional direct form in the original
generated pair, with restricted entry. The corrected screen passed 124
of 128 comparisons. Native-on work and total levels failed on both
projects. The original-fixture measurements above are retained.

The empty size-1 app and image each entered the dispatcher closure once,
dispatched 404,414 bytecodes, and entered the native method region once.
Closure self cost rose from 26,488,529 to 27,073,095 instructions. The
image spent 330,864 additional instructions computing the opcode jump
table address and 220,480 computing a region-header scan length on paths
that had an empty table. These account for 94.3% of the closure delta.

The cutover accepts the four native-on Permute size-5 level comparisons
once. The execution deltas were +562,999 W and +562,742 T for the empty
project, and +582,981 W and +722,021 T for the corrected pair. W growth
fell by 948 and 1,092 respectively. The other 124 comparisons passed,
including every growth check. The dual-run comparison retires with
0326. The standing file gate keeps its existing limits.

Both dispatcher closures reserve a 0x668-byte frame and have no FS
segment reference. The executable and image disassemblies contain 82
and 81 call instructions respectively. At size 5 they computed the
opcode jump-table address 73,626 and 404,642 times. This is consistent
with register allocation changing across the two link contexts.

A scratch image built with the generated app's release settings widened
the empty project's native-on size-5 W gap from 0.579% to 0.968% and
failed the image's own Permute setup limit. Actual compiler invocations
showed cross-crate ThinLTO and a maximum of 8,192 codegen units for Rune
in the shipped image, versus local ThinLTO and the default maximum of
16 in the app. A separate out-of-line header lookup improved both
routes. Its counterfactual image-to-app gap was +0.917%. The header
change is deferred to its own engine record for image-to-image scoring.

The empty project's 178.90-second cold build included 92 seconds to
compile the common project engine and 84 seconds to build the app.
The pair then reused that verified engine from the same cache. Its
916.47-second build included 453 seconds for the provider static archive
and 454 seconds for the generated app with the full legacy continuation.
Cargo rounds those stage times to seconds. The remaining wall time
includes helper linking, verification and packaging. The corresponding
baseline app builds took 84 and 455 seconds inside Cargo.

Both projects embed the same 28,112,760-byte engine (SHA-256 prefix
dd734b1e). The stock CLI embeds a distinct 28,006,856-byte image
(49860bd6). The project image enables the private refusal handoff.
The empty project shares its 17,160-byte helper with the CLI, and the
pair's provider-linked helper occupies 42,741,640 bytes. Total executable
sizes rose from 26,402,616 to 54,688,872 bytes for the empty project and
from 211,472,344 to 282,447,384 bytes for the pair. A fresh pair assembly
still builds the provider archive in a private target directory.

FINAL-v2 found 1,563 extra stock-launch W instructions. Embedded generation names are now checked at compile time and cache directory names are checked when read.

The complete final on 3223ae2a passed all 107 stages, with 51.1 minutes
of accepted run and 51.6 minutes end to end. Both fixed-route bands
held, there were no memory.high, memory.max or OOM events, and the host
was in the performance profile.
