# rnx 0203: pin Rune N5 scalar self recursion

Authority: user N5/pin order and Claude rune-native seq24947.
Base rnx0202 is 39634c5f7a059d3eb1ce06c4b98edae3fb1a6cd8. Published Rune
N5 is 2565a2a4563a915d49de061931f2951fd5dede31. Its final-source prerequisites,
qualified instruction replay, startup/counter/seal checks, production recursion
proof and fresh-clone all-suite plus targeted/canonical Miri passed. Performance
captures retain their original b04c17f8 source identity; a reviewed tree proof
confirms final changes affect tooling only. N5 has an explicit measured host-entry
allowance; interpreter-only instruction counts remain exactly unchanged.

Update only two direct manifests, eight live Rune lock graphs, both RUNE_VERSION
constants and the release metadata expectation. Preserve all non-Rune graph
records, dependencies, versions, features, routing, APIs and benchmark policies.
Prove every Rune query/fragment matches the published revision and other graph
records remain unchanged. No engine or benchmark implementation changes.

Run all existing 23 fresh-clone acceptance checks under the verification account.
Formatting, pin/protocol controls and the existing unchanged-source Clippy
comparison precede expensive tests, with an empty project target at the start.
Keep all existing checks and their policies unchanged.

Build the signed locked production CLI without Cargo patches or overrides.
Prove actual native recursion entry for fib and a running-total recursive program,
plus the small-recursion wrapper control. Report first-call replay separately
from warm completion, retain native-off zero-entry controls and exact output
parity across default/on/off, legacy and Lua. Include a fib witness with neither
--native nor --budget, proving the shipped default invocation enters native code.
Resolve the current binary's symbols and verify executable non-writable mappings;
never use a remembered mangled symbol. This metadata-only pin repeats no timing
or performance captures and claims no gain for permute or shared_rows.

One signed plan and one signed implementation on local main, without branches or
worktrees. Block on durable exits and read terminal receipts once. Publish only
with Claude acceptance and both fresh-clone and locked-CLI checks PASS. Then
start the queued rnx-bench README refresh with its read-only inventory.
