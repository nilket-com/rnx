# 0063 gate 1: ownership and the irreversible handover

Status: mechanism prototype passes, ready for review before product integration.
Plan `bb3477e` includes the accepted two-phase consent correction, history recall,
private carrier, absolute/relative project gates and runtime installation as the
next record. Gates 2–6 remain open. No product implementation has changed.

## Sources and boundary

Bench `probes/session-transition/` builds isolated copies of root rnx and the
project tool from `bb3477e`. `results/session-transition-0063/` contains imported
source hashes, the root integration patch, compiler output, protocol results,
PTY bytes, destructor/event sequences and binary hashes. The prototype sources
are archived, not identified by hashes alone. Both dependency lockfiles are
byte-identical; no dependency or public library interface was added.

The root copy changes three existing files: a private extension-name observation,
a prototype REPL command branch, and the outer main_with return path. The normal
Session::close, VM, lifecycle, HTTP and runtime code is imported unchanged. New
private modules supply framing and the prototype bridge. The external-style
fixture uses the existing public Extensions/Scope/Rune interface for its native
value and a non-Send pending future.

The tool copy imports the accepted parser, bounded reader, catalogue selection
and candidate authoring unchanged. Its additional binary describes requests and
revalidates consent; it does not resolve, compile or claim a build-ready artifact.

## Protocol candidate

The candidate control transport is an inherited Unix socket pair. The receiving
tool seals its endpoint close-on-exec before processing a message. A child
observer cannot access it; a separate positive inherited-descriptor control can.
Protocol text printed on ordinary stdout does not form the control channel.

A frame is a big-endian u32 payload length, byte version/kind and tagged UTF-8
fields, each with a byte tag and u32 length. The length is refused before payload
allocation above 64 KiB. Duplicate/unknown fields, unknown versions, malformed
lengths, truncation and wrong message kinds refuse. Exact field sets are checked
for each phase. A final readiness frame must be followed by EOF.

The tool receives names, association, actual executable path, installed extension
names and offline mode. **Describe** returns its token, owning/proposed manifest,
validated additions/already-declared names, mode and applicable cost notice.
Polars gets the Polars precedent; postgres does not. The terminal renders those
facts rather than maintaining a catalogue. **Prepare** re-describes and compares
before touching scratch state. A changed manifest, even just a comment, requires
a new description and consent. Decline allocates no project directory.

The candidate association is the private environment carrier
`RNX_INTERNAL_SESSION_V1`, containing a hex-encoded versioned association frame
with a 16 KiB decoded limit. There is no public association CLI flag. It carries
canonical manifest/tool paths, runtime/native declaration identity, executable
content identity, and lock/receipt snapshot tokens. The terminal contributes
its actual executable and extension roster; mismatches refuse. Associated
sessions use the remembered tool; stock selection uses the explicit override or
PATH. This is trusted local routing metadata, not authentication or a sandbox.

**Important integration limit:** lock and receipt files in this gate are explicit
opaque snapshot sentinels. Their byte hashes demonstrate stale-association
classification; they are not production lock/receipt parsing or evidence that a
cache entry was built. Product integration must derive the association through
existing validated lock/receipt/artifact paths. In particular, it must decide
identity from the semantic assembly rather than accidentally making a harmless
receipt stamp refresh into a different executable. This probe does not settle
that by calling a sentinel a real receipt.

## Eleven fixture groups

| Group | Observation |
| --- | --- |
| Association | Application-source edits leave the description/association usable; changed runtime/native declaration, lock/receipt snapshots or installed roster refuse. |
| Consent revalidation | Editing the manifest between describe and prepare refuses without publication. |
| Read-only scratch description | Neither Polars nor postgres description/decline creates the state root; cost facts and offline mode come from the tool. |
| Scratch reservation and descriptor ownership | Prepare exclusively creates a 0700 session directory and 0600 marker file; control endpoint does not survive a child exec, with a positive leak-observation control. |
| Managed-path refusals | Taken directory, symlink and FIFO reservations refuse without following/reusing them. |
| Root spelling/containment | User's symlinked state root resolves to its canonical destination; state/native containment in either direction refuses. |
| Framing | Oversized declared length, wrong version/kind, unknown/duplicate fields, truncated payload and trailing association bytes refuse. |
| Precommit preservation | Decline and failed prepare leave the old integer binding and started pending future intact; repoll remains pending rather than cancelled. Unassociated custom executable refuses. |
| Cleanup and exec failures | Real lifecycle destructor failure prevents exec; deliberately missing replacement fails after cleanup. Neither returns a retired prompt. |
| Successful handover | Old resources drop before exec and the replacement has the same PID. |
| Tool discovery | Stock PATH lookup works; missing/incompatible tools refuse while the old session remains usable. |

The Rust framing unit test also passes. Seven ordinary entry comparisons against
the stock executable have identical status/stdout/stderr: version, successful,
empty and missing-source eval, missing-path and successful file run, and a piped
session retaining a binding.

## Real entry-stack observation

The script binds a native value and a tracked future, then uses Rune select with
a short timer to **poll** the future before requesting the transition. The drop
check is not an unpolled-future pass. Successful handover records:

```text
operation polled
operation dropped
context dropped
value dropped
entry stack returned
before exec
replacement pid=<same PID as the old session>
```

The pending replacement is held privately until main_inner returns. The existing
REPL closes its Session; its values, context and editor drop; the enclosing
lifecycle closes; only then does the outer private path exec. main_with retains
its public signature. In the cleanup-failure row, its existing named lifecycle
error propagates and the queued replacement is discarded. In the exec-failure
row, the error names failure after cleanup and the process exits nonzero.

For decline and failed prepare, the event file contains only `operation polled`
while the old prompt is usable. The test reads `held` as 42 and repolls the same
future before ordinary quit. There is no extra drain turn inserted to fake
preservation or disposal.

## What this gate substitutes

Consent is supplied by a labelled fixture variable; actual terminal confirmation,
Ctrl-C during preparation and nonterminal refusal belong to gate 2. Scratch uses
an exclusive fixed fixture name and marker manifest to force reservation races;
unique naming, HOME fallback and a usable retained scratch application are still
integration work. Prepared output is a supplied absolute executable; the success
case is a PID-recording replacement, not a newly built Rune session. The missing
executable intentionally injects an exec failure after commitment.

No Cargo build/attachment, real ready-artifact validation, five-second native
startup probe, history reload journey or full preparation supervisor is claimed.
The socket timeout bounds this protocol fixture only. Whole process-group
cancellation and output accounting remain in gates 2 and 3. The prototype kills
and reaps its direct helper on protocol error; it does not claim broader native
containment. All fixture processes are gone after the driver.

## Checks and next step

Both isolated builds and formatting pass. Tool strict Clippy passes. Root strict
Clippy currently reports **13 pre-existing diagnostics** in unchanged files.
The driver compares exact codes, messages and primary source locations with the
unmodified root and proves zero new diagnostics; it neither suppresses the old
warnings nor claims root strict Clippy is clean. The new codec's chunk-iteration
lint was corrected. An initial scaffold put modules before crate documentation;
that construction error was corrected and its output retained.

Product root/tool source, manifests, lockfiles, notices, adapters, kernel and
server are unchanged. Linux prototype only; no Windows execution or performance
claim. The mechanism needs no new public extension API or dependency and shows
the accepted commitment boundary through the real entry stack.

Next is integration of describe/consent/prepare with actual project validation,
publication, bounded subprocess supervision and the startup probe. The raw
snapshot tokens and fixture-ready executable above must not become shortcuts
around those existing checks.
