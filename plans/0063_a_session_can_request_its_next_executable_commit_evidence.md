# 0063 gate 5: commitment and adversarial lifecycle

Status: implemented, ready for review on Linux. Gates 1–4 are accepted. This
checkpoint changes only test-support instrumentation; ordinary transition policy
and dependencies are unchanged. The exact source patch against `b8ba8e6` is
archived in `rnx-bench/results/session-commit-0063/source.patch`. Conditions also
record frozen binaries, generated manifests/main/locks and artifact hashes. The
later evidence/status edits are not part of those fingerprinted build inputs.

## Observation boundary

`probes/session-commit` uses a tiny runtime facade re-exporting the real library,
the actual PostgreSQL adapter through a facade, and the actual HTTP battery.
The added Polars-named adapter is a small replacement-builder fixture. It does
not claim to exercise Polars; the real adapters are replayed separately in gate 6.

Each session retains a non-Send tracked future with a synchronous drop observer,
a Rune-owned value and context owner, a started HTTP request to a held local
server, and a started PostgreSQL query on a private Unix-socket cluster. A final
awaited sleep exposes the two request sockets before the editor returns. This
excludes Rustyline's per-prompt socketpair. The independent database observer
confirms the SQL is active, rather than inferring activity from an unpolled future.

Test-support-only stops are immediately before the precommit interrupt/stamp
checks, after the entry and cleanup have returned, and after the final stamp
check before exec. The last stop permits an actual OS exec failure. The ordinary
build contains none of these stops or trace-file writes. A 30-second fixture
bound prevents a missing release from hanging indefinitely.

## Eight cases, all passed

| Case | Observation |
| --- | --- |
| SIGINT immediately before commitment | Preparation refuses. No old owner drop or additional poll; both request socket identities remain before any later input. Binding and all three futures remain usable. |
| Artifact removed after successful probe, before commitment | Stamp check refuses; the same precommit preservation assertions pass. The private artifact is restored. |
| Successful transition with all owners started | Old operation/value/context drop before the replacement builder. Both old request sockets disappear; same process starts the added extension and old bindings are absent. |
| Started future's destructor panics at close | Named lifecycle failure, nonzero terminal exit, no replacement builder. No cleanup rollback is claimed. |
| Artifact removed after cleanup | Named restart-after-cleanup failure, nonzero exit, no exec; old owners are already gone. |
| Artifact removed after final check, before exec | Real exec error, nonzero exit after cleanup; not a simulated return value. |
| Replacement-only builder refusal | Startup probe succeeds, old session closes, replacement builder refuses and process exits nonzero. |
| Replacement-only builder panic | Startup probe succeeds, old session closes, replacement builder panic is converted to a named refusal and process exits nonzero. |

Precommit observations occur before another old-runtime turn. Only after those
assertions does the fixture release and await the preserved futures, checking
HTTP status, the typed SQL result and the fixture value. The probe has exited
before each stop. In terminal cases the process has exited before final resource
checks. PostgreSQL backend disappearance is measured independently and may lag
client closure until its statement timeout; that residual is recorded separately.

All session/probe PIDs are observed gone, held server threads are joined, and the
private postmaster is reaped. Full PTY transcripts, drop/poll events, socket
identities, backend observations and monotonic phase records accompany the matrix.

## Fixture corrections

The initial facade reused the shipped adapter's package version, which Cargo
correctly refused as an ambiguous lock entry. Early socket sampling included
Rustyline's transient pair; source inspection and an in-input observation window
corrected it. Two assertions initially expected unquoted object keys and an
announcement still to be in unread PTY bytes. The final driver checks the Rune
value itself and the full transcript. None changed the product contract.
