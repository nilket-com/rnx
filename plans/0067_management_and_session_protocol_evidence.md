# 0067 gate 2 — management in stock rnx and the session protocol

Status: implemented, ready for review. Gate 1 is accepted; gates 3–6 remain open.
This is the command/session port, not the completed Git-source workflow.

## Product boundary

The default stock binary links the private `tools/project` library and dispatches
`project`, `runtime`, `cache` and the private descriptor protocol before
`main_with`. The internal crate does not depend on the runner. Its thread-local
stock-entry flag distinguishes the real dispatcher from an embedding consumer;
it initializes no signals, storage, Git, Cargo or runner context. No environment
variable or runner public API was added for that flag. Ordinary environment
contents remain unchanged. A replacement process starts with fresh state.

Defaults-off generated runtimes explicitly enable allocation accounting and
project-sources. Both shipped adapters disable root defaults on their rnx edge;
the server already did. The actual generated fixture artifact has no
`rnx_project::` symbols. The runner-only graph excludes management. Neither stock
nor runner graphs include Polars or the PostgreSQL driver. The root workspace
excludes the independent tool workspace. The root lock preserves existing pinned
versions and adds the internal dependency graph; root notices now describe 144
third-party packages, 107 texts, 13 fetched texts and one unresolved text. Tool
notices remain current at 104 packages, 69 texts and six unavailable texts.

Stock sessions delegate to their own executable. Associated assembled sessions
use their recorded manager. An unassociated assembled session uses an absolute
RNX_PROJECT_TOOL, otherwise stock rnx on PATH, with a bounded private capability
exchange. Invalid explicit choices never fall back. Capability proves support
only; the real startup probe still executes and retires builders before handover.
The compatibility executable uses the same implementation with its old grammar.
Its private descriptor, process group, signal and exit-status handling are not a
shell forwarding approximation. The wire/capsule formats remain unchanged.

Clean acquired/unverified coordinates reach the tool-authored Git notice without
fetching. Dirty/unknown coordinates refuse with a quoted path-override recovery,
using the build-time checkout only when its supported layout still exists. Stock
Git discovery does not consult a selected runtime, including an uninterpretable
selection. Explicit runtime commands print how to select their source by override.
For this checkpoint only, consent to the Git default returns the named gate-3
refusal before acquisition or scratch writes. Path projects and overrides execute
the full preparation/probe/restart. No Git-source build is claimed here.

## Reproduction and evidence

Bench source: `probes/stock-management`; results: `results/stock-management-0067`.
Bench review commit `f904c47` preserves:

- `product.patch` against accepted rnx `4e887b6` and `clean.bundle`, containing
  the actual clean fixture revision with that prerequisite;
- both frontends' product binaries identified by SHA-256 and size;
- original/effective historical-driver hashes and exact effective scripts;
- complete matrix JSON, command logs and terminal traces;
- ordinary runner comparison against `4e887b6`, generated graph/symbol checks,
  root/tool test logs, strict tool clippy and notice checks.

The measured source snapshot precedes this evidence/status update and a README
correction from `session` to `repl`. Production implementation bytes are the same.
The literal `rnx session` was already not a runner command and remains so; bare
invocation and `repl` open the stock session. Retained path-fixture locks naturally
become stale when the root's documentation changes. Reruns start with whole fresh
target/results directories and never reuse a partial preparation matrix.

## Results

| Check | Integrated rnx | Compatibility entrypoint |
| --- | ---: | ---: |
| Preparation/cancellation matrix | 44 groups | 44 groups |
| Startup/probe/ownership matrix | 17 groups | 17 groups |
| Commitment matrix | 8 cases | 8 cases |

The preparation matrices retain a started tracked operation and binding through
injected failures, interrupts, malformed/inherited carriers, invalid tools and
writer contention. Real Cargo groups are killed and reaped. The startup matrices
exercise error, panic, abort, blocking, flooding, spoofed and malformed readiness,
cleanup failure and healthy same-PID handover. The commitment matrices hold real
HTTP requests, SQL queries on private clusters, tracked futures and native values
across precommit failures, then prove postcommit cleanup/exec failures end without
exec. Both private clusters and all fixture processes were reaped.

The 29 boundary groups add ordinary CLI/flag/diagnostic byte comparisons, files
named project/runtime/cache passed to `run`, and management bypass of settings
with a runner positive control. Selfcheck succeeds in both binaries but its
printed elapsed time is not compared byte-for-byte. Real clean and dirty builds
hold a started HTTP operation through coordinate refusal, decline and the gate-3
stop. Bindings survive, the request completes with its original body, fetch traps
remain untouched, and no scratch/cache is created. The selected-store trap is
unchanged.

Ten discovery groups cover missing tools, relative/invalid overrides, an
unsupported/old explicit peer, timeout/reaping, PATH discovery, the compatibility
capability response, and actual old-manager association interoperability. The
accepted old manager builds its own project, emits the capsule, and serves a
consented injected pre-author failure while the current runner's tracked future
stays usable. Ordinary old-manager launch of a new-generator lock refuses its
changed wrapper. Gate 3 still owes retained-envelope workflow compatibility.

The separate stock scratch journey uses a manager filename and state path with
spaces/apostrophes and neither frontend on PATH. The path-backed request builds,
probes and restarts in the same PID, drops the old binding, resets numbering,
keeps cwd, and handles a repeated request in the replacement. Its exact printed
reopen command executes under `/bin/sh`. The two printed project recovery lines
also execute exactly under `/bin/sh`, using an absolute quoted manager and the
`project` prefix, with no manager on PATH.

## Checks and qualifications

Root suites ran serially: default **376**, test-support **419**, defaults-off with
count-allocations and project-sources **389**, all passing. Tool suites each pass
**51**, with two explicitly ignored repository-audit/runner-integration tests;
the external fixtures exercise real runner integration. Formatting and strict
tool clippy across all targets pass in both configurations. Both notice checks
pass. The baseline worktree was removed through Git after consumers exited.

The private unpublished path dependency means `cargo package` refuses the missing
registry version. This is an explicit distribution boundary, not a claimed
passing `.crate` build: the release tests check that precise refusal, both
publish=false guards, source-manifest claims and Cargo's package-file/notice
selection. Git/source installation is the selected distribution in this record;
gate 1 proved Cargo Git discovery, and gate 4 owns the final installation journey.
The metadata failure fixtures now copy the internal crate before injecting their
original failures. Root's public embedding signature remains unchanged.

An early environment-carrier implementation failed the existing env::vars test;
it was replaced with internal process-local state, and all final suites pass.
Early protocol fixtures and the earlier source patch remain under `early/`.
An edit made during an early build correctly triggered input-change refusal.
A partial matrix rerun hit an already-published candidate and could not reach its
compilation trap; the final compatibility matrix ran from an absent target.
Harness corrections included selfcheck's dynamic elapsed output, `repl` spelling,
fixture event variables, HTTP's public body field, and exact diagnostic wording.
No ownership, cancellation or commitment assertion was weakened.

There is no launch-performance claim at gate 2. Gate 3 owns Git acquisition,
raw-blob verification and persisted source/envelope integration; gate 4 owns the
fresh installation/real-adapter journeys; gates 5/6 own timing and final regression.
