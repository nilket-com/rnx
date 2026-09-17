# rnx 0061 gate 3: publication and interrupted owners

Status: publication core ready for Linux review. Gates 1–2 are accepted and pushed
at rnx b96a94a / 3cc0b51 and rnx-bench 0147c24 / 3f23e43. Product lock/receipt
migration and command selection remain gate 4; Polars costs and regressions remain
gates 5–6. This checkpoint does not turn shared builds on for ordinary commands.

## Measured implementation

`tools/project/src/cache_entry.rs` owns the per-key close-on-exec advisory lock,
stable assembly/target location, checked artifact installation and ready-last
publication. `cache_storage.rs` promotes gate one's path and Cargo-context checks.
`cache_identity.rs` can repeat the real native/external inventory using recorded
package associations, without asking Cargo to resolve again. No dependency, CLI
option, root API or root source changed.

The caller holds its project lock first. Acquiring the key lock grants exclusive
access, not readiness: inputs are rechecked, then the entry is inspected afresh.
An unpublished entry is cleaned under the lock and built at its final path. A
published entry is validated and never silently rebuilt or repaired. The returned
entry owns the lock through the caller's receipt publication.

Cargo runs through the existing owned process-group implementation. The exact
locked graph and wrapper are written before `build --locked --release`; native
and context observations are compared before/after compilation and before ready.
The artifact is installed at its digest-derived path and fully checked against
that digest. Bounded readiness commits the canonical identity, key, digest and
fixed relative artifact location. Its temporary file is synced, renamed last,
and the directory synced. No directory containing compiled outputs is renamed.
This is application-level readiness, not recursive durability of Cargo outputs.

A hit fully hashes against the **already committed** digest. Malformed readiness,
a wrong identity or corrupted artifact refuses at the entry rather than promoting
newly observed contents. Checked managed reads use no-follow/non-blocking opens.
The user-selected root can have a symlink spelling; managed descendants cannot.
Unpublished failures retain a bounded `failure.txt`; a ready entry is not modified
even to record an attachment failure.

## Reproduction and scope

In rnx-bench:

```sh
python3 probes/cache-publication/build.py
python3 probes/cache-publication/check.py
```

The isolated binary copies and hash-checks the actual product Rust modules. Only
the driver is fixture code. `results/cache-publication-0061` contains conditions,
imported source hashes, a reconstructible source patch from rnx 3cc0b51, the binary
hash, native fixture sources, canonical identity, Cargo context/lock and raw
process outcomes. Compiler invocation traces are retained as separate JSONL files.

The fixture holds a real project lock and calls the production entry operation.
Its bounded snapshot callback checks project manifest, script and a lock marker;
it publishes explicitly named **fixture receipts**. Actual rnx.lock format 2,
receipt v3 and CLI routing are not substituted by this callback: they are gate 4.
Likewise missing-cache refusal exercises the production ready/checked-artifact
boundary that shared launch will call, not an already integrated shared `run`.

The two tiny native crates use the real generated wrapper and Cargo compilation.
The executable reads its consumer's source file as text and a retained OUT_DIR
file; this gate claims ownership and distinct consumer inputs, not Rune evaluation.
The actual two-project Polars run/eval/session journey is gate 5.

## Results

All **37 publication/concurrency cases pass** on the final measured sources.

| Case | Observation |
| --- | --- |
| Two projects, one key | One Cargo build; both attach the same artifact, execute, and read their own source text. |
| Two keys | Both real build scripts reach their hold points before either is released; no global mutex. |
| Ready hit | Compilation/metadata trap enabled; only permitted version queries, attachment succeeds. |
| Miss trap positive control | Cargo build is attempted and trapped; no ready document. |
| Builder SIGTERM and SIGINT | Signal delivered while Cargo's build script is active; tool exits 143/130, Cargo group is killed/reaped, observed script PID disappears, no builder receipt or ready. Waiter then builds, reports miss, and publishes. Two build invocations. |
| Waiter SIGTERM and SIGKILL | Blocked waiter dies, leaves no receipt; builder remains active and completes. Fresh process attaches with compilation trapped. One build invocation. |
| Before build / after build / before ready / after ready-temp write | Failure leaves no ready or receipt; retry actually builds rather than attaching to partial state. |
| After ready / before attach / fixture receipt failure | Ready remains valid, failed project has no receipt; another project attaches with compilation trapped. |
| Native / Cargo-home config / managed config edit | Revalidation refuses; no ready document. |
| Project source or lock edit | Assembly may become ready; changed project receives no receipt. |
| Project edited while waiting | Recheck after key acquisition refuses attachment; holder's valid build remains. |
| Malformed / unknown-field / wrong-version / wrong-key / arbitrary-path readiness | Refused without modifying ready bytes or compiling. |
| Corrupt artifact | Full attachment check refuses against the recorded digest. |
| Managed symlinks / ready and lock FIFOs / writable entry | Refused without hanging. |
| Root symlink / relative root | Canonical root alias succeeds; relative selection refuses. |
| Missing cache after attachment | Readiness boundary refuses without invoking Cargo or a compiler. |

The build-script descriptor table also confirms the per-key lock does not survive
exec into build children. Child PIDs observed in the interrupted-builder cases no
longer exist at fixture completion. No fixture compiler, build script or probe
remains running.

Interruption uses the existing SIGINT/SIGTERM ownership contract. SIGKILL is tested
for the **waiter**. SIGKILL of an active builder cannot run that process's child
cleanup and is not claimed safe for concurrent retry by this gate. It does not
turn lock release into proof of child termination.

## Checks and remaining work

- Tool default and test-support suites: **40 passed, 2 ignored each**, no failures.
- Tool formatting and strict all-target Clippy: pass in both configurations.
- Tool notices: current; manifest/lockfile/notices unchanged.
- Windows MSVC all-target type check: passes; the pre-existing test-only
  `artifact.rs` UNIX_EPOCH warning remains. No Windows execution claimed.
- Root source, root manifest/lockfile/notices, kernel, adapters and server unchanged.

The existing gate-two driver gains only the module declaration needed to compile
its copied current product identity module; its accepted evidence is unchanged.
There is no cache-hit latency claim here. Gate 4 must integrate the callback with
full project verification and real receipts, preserve old local/override commands,
and exercise the missing-cache refusal through real launches. Gate 5 measures the
attachment/full-hash cost and retained Polars directory size separately from the
ordinary launch path.
