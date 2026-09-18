# 0063 gate 2: preparation without retiring the session

Status: implemented, ready for review on Linux. Source baseline is `cf6b6bf`.
The exact implementation source used by the fixtures is archived as the bench's
`results/session-preparation-0063/source.patch`, including its new files.
Gates 3–6 remain open. **This checkpoint cannot report ready or restart.** A
successful preparation ends with the explicit startup-check-not-enabled refusal;
the resulting project/cache data can remain. Gate 3 must install the real startup
proof before handover is enabled. No version-handshake fallback exists.

## Production changes

The project tool's existing launch validation is factored into `checked_artifact`.
Ordinary launches retain input verification, lock-pair checking, receipt and
shared-ready binding, artifact checking and stamp refresh. Session launches add a
private association; run and eval explicitly remove inherited association data.
The association records canonical manifest/tool and artifact paths, the artifact
digest, native/runtime declaration identity and installed namespace roster.
It does not use a raw receipt digest as executable identity.

Describe reads the real lock/receipt/ready/artifact paths without opening a
project writer or refreshing a receipt. Source contents may be stale, since
preparation will resolve them; native/runtime declarations, roster and launched
artifact must still agree. A metadata stamp refresh therefore preserves the
association. A changed or malformed lock/receipt cannot be reinterpreted as a
newly trusted executable. Association is routing metadata, not authentication.

The bounded gate-1 TLV codec is present at both private endpoints; the fixture
asserts byte equality. The tool validates catalogue names and supplies ownership,
additions, offline mode and entry-specific cost facts before terminal consent.
Describe creates no scratch/project/cache files. Prepare re-describes, compares
the original manifest and candidate, then takes the existing project lock and
calls ordinary author/lock/build internals without recursive lock acquisition.
Published files are not rolled back across those phases; failures name the phase
and ordinary retry route. Existing workflow publication/attachment checks apply.

The REPL's bridge does not receive a Session or runtime reference. It uses bounded
nonblocking control/output reads outside the VM. The initial protocol response
has a five-second bound; consented preparation has a thirty-minute bound, also
checked inside the tool's subprocess/wait loops. Live output is terminal-safe and
capped at 8 MiB; no unbounded diagnostic tail is retained. Cancellation signals
the tool so its existing Cargo-group cleanup runs, waits up to five seconds,
and contains an unresponsive direct helper. Escaped groups from trusted native
code remain outside that containment claim. No old-runtime drain is attempted.

Nonterminal `:dep` refuses before asking for consent. Installed namespaces are
captured only on the REPL path. Help/completion add the command; no public library
surface, CLI flag, dependency, persistent document format or lockfile changes.
Windows transition execution remains unsupported and unmeasured here.

Scratch creation uses the explicit runtime selection and state-root policy in the
plan. New directories/files are private, allocation is exclusive, and a taken
reservation refuses. The user's canonical root may be a symlink; retargeting it
after describe requires a new consent. A directly launched custom executable
cannot reconstruct its builders and is refused without a project association.

## Observations

The final driver reports **44 passing groups**. The checked-in setup creates a
small real project with a runtime facade over the actual rnx library, a lifecycle
fixture and tiny catalogue-shaped adapters. Those adapters deliberately do not
load Polars or PostgreSQL; this gate measures preparation, not engine behavior.
The generated artifact is built by the product tool in its real private cache.

* The initial association comes from an actual project session. Editing only the
  entry source still permits describe. Touching the actual artifact and running
  ordinary project eval refreshes the receipt, after which the original carrier
  remains accepted. Bad lock/receipt/Cargo-lock documents, changed declarations,
  wrong roster and a different executable refuse.
* An actual stock rnx child, spawned from Rune in an associated parent, inherits
  its carrier and refuses the parent's roster. A separate executable-mismatch
  case holds the roster constant. There is no cwd-based recovery to a new owner.
* Every `preserve-*` PTY case binds 42 and starts a non-Send tracked future via
  Rune select. The exact poll/drop log is unchanged before another input after
  failure. Releasing the fixture and awaiting it then returns its original 73.
  This covers authoring and both publication sides, resolution, build entry,
  attachment, malformed/carrier/name/tool refusals, an unresponsive tool,
  preparation interruption, a cache waiter and an active Cargo group.
* The waiter uses the actual per-key lock held by the driver. The compiler-body
  trap replaces only Cargo's compilation body with a waiting child. After
  cancellation neither process remains running; the assertion distinguishes a
  transient orphan zombie from a running process and does not claim to reap a
  non-child. The existing 0061 builder/waiter publication algorithm is reused.
* Scratch describe/decline creates nothing, notices distinguish Polars from
  PostgreSQL, unsafe managed paths refuse, and the missing runtime error includes
  the exact export line. Allocation is private and retained after a later author
  failure. A reservation race and changed canonical state root refuse.
* Piped input after `:dep` is evaluated rather than consumed as consent. Reopening
  the successfully built combined assembly and asking for its installed Polars
  namespace is a true no-op: no consent, changed files or lost binding.

PTY transcripts, the assertion matrix, real lock/receipt documents, build/check
logs and binary hashes are in the bench results directory. TERM is fixed to
xterm-256color with a 120-column terminal. No user's history, settings, project or
cache is used. No fixture process remains running after the matrix.

## Checks and corrections

Root suites run serially, with one test thread: **376 default, 419 test-support,
438 combined test-support/server-runtime/project-sources**, zero failures. Tool
suites: **40 passing in each configuration**, with the existing two explicitly
ignored integration tests not included in that number. Both workspaces format
cleanly. Tool strict Clippy passes across all targets in both configurations.
Root strict Clippy still reports the same thirteen baseline diagnostics recorded
at gate 1, with no new diagnostics; it is not claimed clean. Both notices checks
are current. Root and tool manifests/lockfiles and dependency graphs are unchanged.

The matrix found one product defect during development: an absent state root
created with the host umask could be group-writable and rejected by the tool's own
privacy checks. New state directories now use mode 0700. The final matrix proves
private allocation under the ordinary host mask. The canonical-root comparison
also refuses a changed symlink target between describe and prepare.

Fixture corrections were a missing local Rune alias for a derive, use of the
wrong env accessor/unwrap depth, awaiting the synchronous process facade, a
per-key lock created with group-writable permissions, and reading a key field
instead of hashing the identity's canonical bytes. None relaxed a gate. An early
root default run overlapped a feature rebuild and failed the worker's feature
marker check; all three final suites were rerun serially. Final fixture stock
binaries are copied before those rebuilds so their identity is stable.

No timing, actual adapter startup, cleanup-after-commit or replacement-success
claim is made. Gate 3 must prove settings-aware startup and supply the freshly
checked association to the replacement. Gates 4 and 5 then exercise the real
adapters and irreversible commitment boundary; gate 6 retains the launch-cost
and full regression obligations.
