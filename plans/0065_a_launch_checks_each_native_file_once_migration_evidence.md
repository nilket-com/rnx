# 0065 gate 3: authenticated runtime migration

Status: gate 3 passes on Linux, ready for review. Gates 4–6 remain open.

## Implementation

Only the project tool changes. A private installation-1 decoder retains the old
strict metadata checks and SHA-256 ID derivation. Explicit installation from the
exact owned retained-source path, under the installation writer lock, checks the
entry/administration, runs Git integrity validation, reads the source through the
frozen bounded SHA-256 reader, and compares its recorded tree digest, file count
and byte count. It validates layout before any permission repair. Git routing,
configuration, filters and hooks use the installer's existing isolated command
builder; no legacy launch fallback is introduced.

After authentication, the ordinary BLAKE3 inventory/copy/index/publication path
creates the new ID. The old document's source path, commit and dirty-snapshot
label become the new source provenance; new installation time and tool identity
are recorded, and `migrated_from` names the old format/ID outside the new identity.
The old source and installation document are not rewritten. The real migrated
index has no alternates/worktree links; matching loose objects have distinct
inodes and identical bytes in the two entries. Only validated Git
administration permission drift may be tightened, following 0064. Same-new-ID
reinstallation validates/reselects without rewriting its first provenance.

Authentication is repeated before publication and selection; observed old-source
or provenance changes refuse. Publication retains the existing distinction:
incomplete staging is undiscoverable; a published complete entry may be installed
but unselected; failure after selection rename reports that selection may have
changed. This is not an atomic snapshot against arbitrary concurrent edits.

`runtime show`, `runtime select <old-id>` and stock :dep discovery recognize a
strict installation-1/current-1 envelope and print the exact shell-quoted
`rnx-project runtime install --from '<store>/entries/<id>/source'` command.
Discovery authenticates no source, runs no Git, and performs no migration/repair.
Malformed or mixed metadata remains an error. Source authentication occurs only
on explicit import, not on a new BLAKE3 hash of whatever bytes happen to be there.
No :dep protocol version or root code changed; opaque identities still cross the
existing carrier.

## Failure and recovery evidence

Bench sources: `probes/runtime-migration`; results: `results/runtime-migration-0065`.
The README gives the exact sequence and prerequisites. `conditions.json` pins
the measured tool sources and binaries; `implementation.patch` preserves the
tool change from a7c3f7d alongside the committed root implementation. The user’s installation-1
store was not used.

Thirty migration cases use genuine old product-created installation/current
format 1. The fixture checkout is renamed away before migration. They cover:

- Read-only show/select refusals with exact shell-parsed recovery commands,
  including spaces and an apostrophe in the store path.
- Source, loose-object, metadata, ID, mixed-field and unknown-format corruption.
  A drifted index stays drifted when authentication fails: no permission repair
  happens before content and integrity validation.
- Every existing copy/index/document/entry/selection failure point; prepublication
  failures leave no entry, published failures retain a complete entry, and late
  failures name possible selection change.
- SIGINT/SIGTERM during copy and Git steps, with staging removed and old selection
  unchanged; late source/provenance edits before rename refuse publication.
- Exact retained-source eligibility, authenticated Git-mode repair, unchanged
  old document/content/inodes/times, new ID/provenance, and repeat migration without
  rewriting the new entry.

A real stock session starts and retains a pending HTTP request. The old-selection
:dep refusal prints recovery before consent, leaves the binding usable, creates
no scratch/cache, and the original request subsequently returns 73. No hidden
runtime drain makes that result possible.

The accepted 90-case installer publication driver also passes with the new
product. Its only semantic fixture edit is the bad-tool-digest field's new name;
paths are isolated and the effective driver is archived. Tool formatting, strict
all-target clippy and both suites pass (46 tests, zero failures, two unchanged
ignored tests each); notices are current. Root source/Cargo, kernel, adapters,
server and all tool dependency files are unchanged from gate 2.

## Real old consumers and the migrated default

The full fixture archives exactly rnx 7cd3205, installs through its actual old
tool, and physically renames the checkout before its first assembly build from
an empty private cache. A small independent adapter exposes a read of a file
created in retained OUT_DIR beside real Polars. Generated Cargo files and native
inventories have no surviving reference to the unavailable checkout; original
path provenance is intentionally retained in installation metadata.

Before migration, the fixture starts that old-key artifact as a session, keeps a
binding, and writes a kernelspec through the actual kernel installer naming the
same artifact. After migration it changes the retained output and reads the new
value through the still-live old session. It then **starts** the prewritten
kernelspec and executes a cell which reads `after migration` and returns true
from a Polars expression. The kernelspec bytes and artifact are unchanged.
Kernel/session cleanup is checked after shutdown; no user's kernelspec is touched.

The accepted four real :dep journeys then run from the migrated default with an
empty second cache: stock Polars with a frame journey, second stock consumer with
compilation trapped, absolute-path mixed Polars/PostgreSQL, and relative-path
second mixed consumer with compilation trapped. Both mixed projects execute a
typed parameterized query against a private PostgreSQL cluster. Startup probes,
same-PID transitions, history, binding loss, reset/error recovery and printed
scratch reopen commands retain the established assertions. The old session still
reads its retained output after these builds; the old tool can still run its
untouched old project.

An additional genuine old-tool :dep scratch is created under the old selection.
Repeating migration to select the new default leaves its lock pair and receipt
byte-identical. The old tool reopens it with Polars live. An explicit new-tool
relock writes format 3 but keeps that scratch's declared old installed source;
changing the default does not silently retarget an existing project.

The tool necessarily differs from the old snapshot to implement migration. The
launcher/runtime root source is unchanged; no new launcher-versus-runtime version
compatibility promise is inferred from that fact.

## Cost and retained state

The full-source migration took 2.33 seconds in this single observation, including
repeated old authentication. This is not a latency distribution or gate. The old
runtime remains at 8,475,023 logical bytes and the new entry adds 8,475,143 logical
bytes, including Git objects. Allocated disk use is not substituted for those
logical counts. Old and new assembly directories remain whole as well.

The first migrated-default Polars transition took about 112.6 seconds with a
fresh target and cached registry sources; its second stock consumer took 2.01
seconds with compilation forbidden. The mixed transition took about 116.6 seconds
and its second consumer 1.81 seconds, also compiler-trapped. These are individual
journey observations, not new launch-performance claims. Native inventory reuse
and the final slope measurement remain gates 4 and 5.

Fixture corrections: corruption injection initially tried writing a read-only
loose object; the fixture now changes its own copy's mode temporarily and restores
it. The late-edit pause initially used remove-marker semantics from the project
hook; the installation hook requires a `.release` sidecar. Both were harness
errors, corrected before the final matrix. No product rule was relaxed for them.

Old entries are deliberately not pruned. A generator/format version does not
establish that a project, old tool, session, kernel or retained-output reader has
stopped using an entry. The removal record remains immediately after 0065.
