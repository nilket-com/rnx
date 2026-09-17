# rnx 0063: a session can request its next executable

Status: accepted for implementation after the 2026-09-17 review, with the
two-phase consent protocol below. Baseline: rnx `78c514d` / rnx-bench `ad0fb88`.
The first gate is a transition/ownership prototype, before product integration.

## Context

0062 gives `polars` and `postgres` a concrete meaning in an existing manifest.
0061 shares their compiled assembly; 0060 opens that executable as a session.
The remaining user work is to leave a prompt, find or create the project, author
its declarations, lock/build and open the resulting executable.

A session should be able to request that workflow. This is replacement of the
executable, not registration of another module in the current context. Rune's
being interpreted does not prohibit native loading; rnx has no supported dynamic
loading boundary, and 0051's builders run once per serving process. This record
uses that existing contract rather than inventing a plugin ABI.

Three implementation facts constrain the transition. Project session currently
execs its checked artifact without passing a project association. Ordinary eval
constructs extensions but does not load session settings. Finally, Session::close
retires lifecycle state, cancels HTTP and drains its runtime: it is irreversible.
A prepared replacement can fail without touching the live session, but cleanup
followed by exec cannot promise rollback on every possible failure.

## Decisions

### 1. A terminal command with a visible restart

Add `:dep NAME [NAME...]` and `:dep --offline NAME [NAME...]` to interactive
sessions, help and completion. Reuse the catalogue's exact names and 1–32 distinct
name admission; reject unknown options, duplicate names/options and missing
names. No shell parsing, URLs, versions, arbitrary Rust expressions or implicit
registry search. No command execution inside Rune input, eval, run, the notebook
worker or the settings evaluator. In this first record, dependency transitions
require terminal input/output; a piped session refuses with the explicit
add/lock/build/session route rather than consuming subsequent input as consent.

The terminal first asks the tool to **describe** the request. The tool validates
names against its catalogue and returns additions, already-declared names, the
owning manifest or proposed scratch path, offline mode and applicable cost facts.
Only then does the terminal display the returned plan and ask for consent. No
project mutation, Cargo resolution or compilation occurs during describe. The
following Polars example uses tool-supplied cost facts, not literals in the REPL:

> This prepares a new executable and restarts the session on success.
> Existing bindings and declarations will be lost. Saved history remains
> available with up-arrow in the new session; nothing is replayed.
> A first Polars build took about 100 seconds with registry sources cached;
> its retained shared entry is about 1.5 GB. A reusable entry avoids compilation.
> Continue? [y/N]

This is an observation, not a promised duration or a claim that an entry exists.
Print whether Cargo may fetch sources or --offline forbids fetching. The command
itself must not start an expensive operation before this message and affirmative
answer. Decline, EOF or Ctrl-C here changes no project or session state. During
preparation print the current phase (author, resolve, build/attach, startup
check); Cargo diagnostics are bounded and terminal-safe. Ctrl-C cancels
preparation, reaps its owned subprocesses and returns to the old prompt.

If all requested namespaces are already installed in this session, report that
and do nothing: no build, reset or replacement. This is not an update command.
The no-op check uses the session's installed extension names and the tool's
validated manifest together; disagreement refuses as a stale association. Mixed
requests preserve the existing declarations and add only missing ones. A postgres
request does not receive the Polars cost notice unless Polars is also being added.
Listing/discovering names continues through `rnx-project adapters`; the REPL
must not maintain an independent adapter catalogue.

### 2. Explicit project ownership, persistent scratch otherwise

A session launched by `rnx-project session --manifest FILE` owns that declared
project for dependency authoring. Carry a private, versioned association from the
tool into the REPL: canonical manifest and tool paths, and the identity of the
artifact launched. Keep this metadata out of Rune state and the source map.
Choose the carrier in gate 1; it must not be a public command-line flag.
Preserve it on reset and successful replacement. Do not infer a project by
walking upward from the working directory or from the executable's cache path.
No association is supplied to run, eval or worker.

Before mutating an associated project, check that its declaration/receipt still
corresponds to the session's launched executable. If another command has rebuilt
or replaced that association, refuse and tell the user to reopen the project
session. Do not silently switch an existing process to a newly selected runtime
or discard extensions. Source changes alone may require an ordinary explicit
refresh during preparation; the prototype must distinguish that from a changed
runtime/native declaration. The association is routing metadata in the trusted
local-project model, not authentication or a sandbox.

A stock session without extensions or an association creates its own scratch
project after confirmation. Use an absolute `XDG_STATE_HOME/rnx/sessions` or,
when unset, `HOME/.local/state/rnx/sessions`, with an exclusively allocated
session directory below it. Canonicalize the user's root (a symlink to another
disk is allowed); below it use private directories and refuse managed symlinks,
special files and non-Unicode paths. No scratch file goes into the caller's
working directory. Check the same native-root containment exclusions as 0061.

The scratch contains an ordinary manifest, a fixed inert entry file and the
normal project lock/receipt files. Its entry is never evaluated by :dep. It
remains owned by this session across successive additions and reset, and is
retained after exit so the printed `rnx-project session --manifest ...` command
can reopen it. It is not an automatically deleted temporary directory. Assemblies
still live in 0061's shared cache; scratch projects do not duplicate compilation.
Eviction and automatic session-directory cleanup are deferred.

Stock rnx cannot infer which source checkout built it. For this first workflow,
`RNX_DEP_RUNTIME` explicitly selects an absolute runtime source root for scratch
creation. If absent, refuse before writing anything and show the exact setup
form `export RNX_DEP_RUNTIME=/absolute/path/to/rnx`, or the command to open an
existing project session. Do not embed a developer checkout path into a
release executable. An associated project uses its own runtime declaration and
ignores this scratch selection. A directly launched custom executable with
extensions but no project association refuses the transition: its extension
builders cannot be reconstructed from namespace names alone.

Use `RNX_PROJECT_TOOL`, when explicitly set, as an absolute executable path;
otherwise locate `rnx-project` on PATH once when :dep is requested. Project
sessions remember the actual launching tool. Negotiate the transition protocol
before mutation; missing or incompatible tools refuse with the old prompt intact.
No runtime checkout or tool discovery is added to ordinary evaluations.

### 3. Delegate preparation to the project tool

Root rnx remains the terminal/session owner. The independent tool owns manifest
parsing, catalogue expansion, lock/build, cache attachment and artifact checking.
Introduce a private versioned preparation protocol between them, not copies of
those algorithms in the root crate and not a new public library API.

The protocol has two explicit phases: **describe**, then (only on consent)
**prepare**. Describe validates all names and ownership and returns the bounded
plan and per-entry cost facts. The terminal renders and confirms that plan;
prepare revalidates that the description still applies before any mutation.
Changed ownership, declarations or requested additions require a new description
and consent, never silent expansion of what the user approved. Scratch allocation
happens only in prepare; a proposed path is not an ownership claim or reservation.
If it is taken meanwhile, refuse and redescribe rather than overwriting it.

Prototype a bounded inherited control channel with length-prefixed fields and
explicit phases/results. Limit a request or final launch description to 64 KiB,
reject unsupported versions, duplicate/unknown fields, malformed lengths and
truncated/trailing data. Human output is separate from control data. A user/native
builder printing apparent protocol text must never manufacture readiness. Seal
control descriptors before starting Cargo or an artifact; no unrelated child may
keep the protocol pipe or project lock alive. Root integration adds no dependency
merely to parse this private handoff; choose the exact encoding in gate 1.

Acquire the existing project command lock for preparation, then reuse the 0062
authoring and existing lock/build internals without recursively acquiring it.
Revalidate inputs and the checked artifact before reporting ready. A shared hit
must satisfy 0061's attachment checks, not just path existence. No new manifest,
lock, receipt or assembly-key version is justified by this command. Legacy
projects undergo explicit relock as part of the consented workflow; never promote
an old local artifact. Overrides refuse dependency additions as in 0062.

This workflow is **not a transaction over project files**. Once add or lock has
published, a later build/probe failure can leave the manifest or lock updated,
and a successful cache entry can remain even if the user cancels. Report the
last completed phase and ordinary retry commands. The old live session and its
bindings remain usable during these failures; published files are not falsely
reported as rolled back. Never modify a ready shared entry to undo preparation.

Use the tool's Unix subprocess ownership for Cargo and probe children. Give
preparation a 30-minute overall wall deadline including lock wait, resolution,
compilation and probe, with cancellation available throughout. Cap retained
stdout/stderr tails at 64 KiB per stream and total live output at 8 MiB; exceeding
the latter refuses and cancels rather than growing memory or blocking forever.
No unbounded stdin forwarding to builders. State process-group containment's
existing limitations for trusted native code; do not claim escaped descendants
are contained merely because the direct child was reaped.

### 4. Prove startup, not just protocol support

Before reporting ready, run the **new checked artifact** in a separate process
that constructs a serving context, invokes every extension builder, evaluates a
fixed harmless Rune expression such as `42`, and successfully closes the probe
session. A version/source-map handshake alone never satisfies this gate.

The probe must follow the prospective session's settings-selection and context
construction paths, using its working directory, relevant environment and
explicit presentation flags. Ordinary `eval 42` skips settings today. Add a
private startup-probe mode which loads settings as a session does and then
performs that fixed eval and cleanup, rather than pretending ordinary eval covers
this distinction. It does not read terminal/history input, run the application
entry, load mapped Rune sources or execute arbitrary probe text. Retain current
settings warning/fallback behavior; a warning is not silently converted into a
new fatal settings policy.

Require a versioned success result on the sealed control channel, the expected
evaluation result, and zero exit after cleanup. Bound the whole probe, including
builders and cleanup, to five seconds, with 64 KiB per captured stream within the
overall output bound. Timeout, panic, abort, nonzero exit, malformed response,
output flood or cleanup failure refuses preparation; terminate/reap the owned
probe group. Name the failing phase; tool-generated diagnostics must not echo
credential-bearing environment values or connection URLs. Native output remains
untrusted, bounded and terminal-safe, not universally redactable. Probe output
does not appear as a user cell or advance the old session's input count.

The probe and replacement are different processes. Builders run once in each;
trusted adapters with external startup side effects can therefore run those
side effects twice. Say this plainly. The probe proves the observed startup,
not that an environment change or a nondeterministic builder cannot fail later.
Settings/history/terminal conditions can also change before replacement.

### 5. A single irreversible handover point

During parsing, consent, authoring, build and probe, do not reset, close, evaluate
in or otherwise retire the old Session. In particular, editing-time Ctrl-C must
not acquire the old "clear all HTTP" behavior removed by 0055. Test a retained,
started tracked operation, not just an integer binding, through preparation
failure and cancellation. No extra old-runtime turn is supplied as a workaround.

After ready, recheck the launch description/artifact identity and cancellation,
then print that the confirmed restart is beginning. This is the commitment point.
Save history as normally, close the old session through its existing lifecycle/
HTTP/runtime shutdown path, and drop its values, editor and context before exec.
Return a pending replacement through the private entry stack as necessary so
exec does not bypass those destructors. Keep `main_with`'s public signature.
Do not use process exit as a substitute for successful old-owner cleanup.

Cleanup failure refuses to launch the new artifact and ends the retired session
with a named error, as existing failed retirement does. A failed exec after
cleanup likewise reports failure and exits nonzero; it cannot restore the old
bindings. Do not return a prompt backed by a retired context. This is the explicit
qualification to "the old session survives failure": **all preparation failures
preserve it; failures after commitment cannot promise that**. Preserving it even
through cleanup/exec failure would require a different hosting/activation model
and is outside this proposal.

On success, exec the checked artifact directly into session mode, retaining the
caller's working directory, presentation choice and project association. Do not
proxy terminal bytes through the tool or keep a parent session process parked.
The new prompt begins at input 1 with fresh bindings and the complete declared
extension set. No history replay, attempted serialization of Rune/native values,
or source-map import. The exec window and a second startup failure are residuals,
not waived by a successful probe. Check only the artifact identity policy already
specified by 0059; do not introduce a different quiet trust policy here.

## Gates and stop points

1. **Ownership and handover prototype before product integration.** From an
   external fixture, demonstrate private project association, scratch placement
   outside native roots, tool discovery/version refusal, bounded control framing
   and descriptor closure. Exercise describe/decline without writes and a
   changed description refusing prepare. Prototype returning a prepared replacement through
   the actual entry stack. Show old-owner disposal occurs before exec, and that
   close failure and exec failure never produce a prompt using retired state.
   Demonstrate source-only staleness versus changed assembly association. Stop
   for review if this requires a public extension API, a dependency addition,
   ambiguous ownership, or weakening the stated failure boundary.
2. **Preparation and cancellation matrix.** Unknown/duplicate names, missing
   runtime/tool, custom unassociated executable, unsupported protocol, project
   contention, malformed/stale associations and unsafe managed scratch paths.
   Inject failures at author/lock/build/attachment and both publication sides;
   report persisted project state honestly. Interrupt builder and waiter cases
   reuse 0061's ownership observations. A previous bound value and a started
   tracked future remain usable after every precommit failure, without a hidden
   runtime drain. Nonterminal invocation refuses without consuming later input.
3. **Real startup gate.** Healthy, erroring, panicking, aborting, blocking and
   output-flooding fixture builders; malformed/truncated/spoofed readiness and
   cleanup failure. Five-second failure is observed externally with processes
   reaped and the old session intact. Assert settings selection/fallback, zero
   history/entry evaluation, builder invocation counts per process, and renewed
   initialization after actual replacement. Version-only positive control must
   fail to establish readiness for a broken builder.
4. **Polars dogfood journey.** Stock session with explicitly selected runtime:
   notice/decline without writes, then :dep polars creating a scratch project,
   cold target versus a second stock session attaching to the shared assembly,
   retained scratch reopened through the printed command, a bound frame across
   several inputs and a catchable error with preview. In a project session,
   :dep postgres preserves polars, authors its lifecycle declaration and executes
   a typed query on a private cluster, with both absolute and relative runtime
   declarations. Assert explicit binding loss, fresh input
   numbering, unchanged working directory, terminal ownership, history not
   replayed but available for up-arrow recall, no child leakage, and all-installed
   requests as a true no-op.
5. **Commit boundary and adversarial lifecycle.** Started HTTP and tracked
   PostgreSQL operations, cleanup/destructor failure, artifact replacement after
   probing, exec failure injection, cancellation immediately before commitment,
   and actual replacement startup failure. Separate precommit preservation from
   terminal postcommit outcomes. Never claim cleanup rollback. Ordinary run,
   eval, session interrupts and notebook worker behavior retain their contracts.
6. **Regression and costs.** Root/tool suites, strict checks/notices, public API
   and default dependency graph; existing project/cache/adapter fixtures. Measure
   preparation by phase: author/resolve, cold compile, ready attachment, startup
   probe, cleanup and first prompt. Report the approximate 100-second Polars
   precedent before work, and the observed cold cost afterwards. Cache hits are
   proven with compilation traps, not latency alone. Preserve the existing
   25 ms project-launch overhead gate and matched stock startup; no transition
   overhead on ordinary cells. Linux execution first; Windows type-check only
   and explicit unsupported transition, preserving ordinary session behavior.

## Guardrails and later work

This is a reviewable proposed UX, including its one-time scratch runtime setup
and its explicit commitment boundary. It is not a promise of transparent hot
loading or rollback after destructive cleanup. Native adapters remain trusted;
compilation and startup can have external side effects even when they fail.

The next record after 0063 is runtime installation: a source or prebuilt runtime
tree the tool can locate without RNX_DEP_RUNTIME. This first :dep workflow is
for users who have the rnx checkout; it does not yet deliver a pip-install-like
experience from a stock binary alone.

Fetching named adapters from a registry, preserving
bindings across executable changes, mapped sources at the prompt, notebook
transitions, automatic frame display, cache eviction/live-entry tracking and
Windows execution remain separate decisions. Keep meaningful measured stops in
history; fold incidental corrections into their implementation checkpoint.
