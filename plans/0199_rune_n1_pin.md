# rnx 0199: ship Rune N1 through the dependency pin

Authority: chatd rune-routing seq24403. Base: rnx0198 c9e86ed.
Pin Rune from145e754086816eb479b0a07eddac02d4ce39819f to landed N1
482109111e7c8ba932b58839b95d319d45e9cd26, so ordinary numeric assignment
loops receive N1's guarded native eligibility through the existing runtime.

Update the two direct manifests, every Rune source identity in the eight
product lockfiles, both reported RUNE_VERSION constants, and the existing
release-metadata test's expected revision. Keep the dependency graph, versions,
features, routing code and runtime policy unchanged. Check all Rune packages,
both rev/query and fragment identities, and every non-Rune package entry.

Run the existing 23-gate fresh rnx acceptance under the verification account,
including actual CLI corpus/native off/on comparisons, generated projects,
adapters, demos/server checks and the scoped unchanged-file Clippy baseline.
Build the exact pinned production release CLI and demonstrate the original
loops kernel actually enters and returns from native code, using production
breakpoint counts with a disabled-policy control and identical output. A
requested-on diagnostic is not proof of entry.

This is correctness-only under the hot-path policy: N1 already measured the
engine's performance. Do not repeat performance captures or add engine/API,
recursion, collection, routing or tooling work. Preserve runtime failure
no-retry and the existing whole-program prepare/refusal boundary.

One signed plan and one signed implementation on local main; no branch or
worktree. Use disposable exact-hash clones for acceptance. Long jobs run in
durable units and report terminal receipts, with no polling or progress
watchers. Publish after the required checks and review.
