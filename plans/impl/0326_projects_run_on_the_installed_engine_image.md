# Implementation plan 0326: Projects run on the installed engine image

Status: Proposed

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
