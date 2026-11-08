# ADR 0232: Bound validation results and required cross verification

Status: Accepted

Validation must not report a successful record when a required gate was omitted
or a child controller failed despite exiting zero. Add a shared gate policy and
child-result contract, and repair aggregate column test discovery. Production
runtime code and historical plans/probes are unchanged.

Derive the complete record diff from the parent of its plan commit through the
candidate; do not accept a caller-selected narrower baseline. Inspect additions,
modifications, deletions and both sides of renames using NUL-delimited Git data.
Changes under root src/ or rune/crates/ require cross. Conservatively include
root and engine manifests/locks and cross-controller/fixture inputs. A tools-only
record may still select cross explicitly. The required-set result binds baseline,
candidate, source snapshot and policy version.

Introduce a canonical proportional selection/finalization command shared with
full acceptance and recovery. A final acceptance manifest cannot omit required
cross, substitute a different command named cross, or accept stale cross evidence.
Development scopes remain usable before integration but are explicitly incomplete
for record acceptance. Document this finalizer as the publication gate; do not
claim that arbitrary ignored scripts can be made impossible to bypass.

Each child-controller stage passes only when its process exits zero and its own
terminal receipt reports PASS with the expected invocation and source/recipe
identity. Allocate an unpredictable invocation identifier and a fresh attempt
directory before launch. Bind stage, argv, working directory, declared environment,
source snapshot and selected artifacts. Reject preexisting, missing, malformed,
wrong-schema, FAIL, mismatched or incomplete receipts. Do not choose the newest
file from a directory. Preserve failure evidence and nonzero parent exit.

Use the existing durable receipt primitive: sync evidence, atomically publish the
receipt and sync its directory. Producers record the supplied context through a
small shared protocol. A wrapper must not manufacture PASS from exit zero for a
receipt-producing controller. Explicit raw-command adapters are allowed for Cargo,
unittest and other commands that do not implement this protocol; they retain
existing positive-test, artifact and semantic checks and publish bound evidence.
Unknown stage kinds fail closed. Historical measurement sidecars use an explicit
checked adapter with their existing configuration/binary/profile bindings; no
relaxation of measurement semantics or immutable source pins.

Integrate the contract at the committed parent edges listed in the inventory:
acceptance, development, declared chains, recovery and remote acceptance. Update
receipt producers as required. Preserve explicit resume: reuse only validated
completed evidence with matching identities, never retry a completed failure.

Rename the colliding column test module and verify the normal aggregate discovery
command. Keep all tests; do not solve discovery by skipping or serializing files.

Acceptance uses isolated negative controls for omitted cross and changed source,
rename/delete paths, narrower baselines, spoofed stage labels, all exit/receipt
disagreements, stale invocation/source/recipe bindings, corrupt artifacts, crash
before terminal publication and explicit resume. Run real small leaf/controller
smokes, aggregate column discovery and affected tool suites. Run cross once for
the changed verifier integration. No performance, full Miri or large matrix runs
are needed for this tooling-only record. Hygiene and commit discipline stay intact.
