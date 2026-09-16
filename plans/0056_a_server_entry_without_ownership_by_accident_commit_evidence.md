# rnx 0056: gate 3, a rejected COMMIT is not a lost reply

Status: accepted and pushed 2026-09-16 at rnx `1aa600e` and rnx-bench
`181a444`. Gates 1 and 2 are accepted and pushed. Gate 4 extraction evidence
is recorded separately; gates 5 and 6 remain open. No root source, manifest,
lockfile, notices, kernel, adapter or accepted pool prototype changed.

## Reproduce and identify

The new source-only workspace is rnx-bench `probes/commit-classification/`,
with tokio-postgres exactly 0.7.18. Run its `run.py` for two repeats, each on a
fresh private PostgreSQL cluster. The shared helper starts PostgreSQL 18.6 on a
0700 Unix-socket directory with no TCP listener, observes zero tagged backends,
and stops and reaps the postmaster. The system server is never used.

Artifacts are in `results/commit-classification-0056/`: per-case wire events,
observer results, cluster lifecycle events, and conditions with source, binary
and lock hashes, toolchain and resolved package/licence inventory. The lock is
seeded from the accepted transaction prototype: every third-party package and
version is a member of that existing graph. The fixture is the only new package.
This is a debug correctness run, not a latency or allocation measurement.

## Classification observations

| Case, both repeats | COMMIT SQLSTATE | Classification | Observer before retirement |
| --- | --- | --- | --- |
| Deferred foreign key | 23503 | rejected | Victim audit and child insert absent |
| Serializable dependency cycle | 40001 | rejected | Victim audit absent and updated row still 0 |
| Deadlock in deferred trigger | 40P01 | rejected | Victim audit and trigger-bearing insert absent |
| Explicit unclassified-code control | 40003 | ambiguous | Victim writes absent, but classifier does not infer that outcome |

The observer first sees the victim backend idle with no transaction, then reads
its audit and application rows before the client is dropped. This avoids a
vacuous absence-of-uncommitted-data check. Each case observes the victim's own
pre-transaction state, not a claim that concurrent committed work disappeared.
In the serialization case the antagonist legitimately commits its different row.

All three definitive failures are returned by the victim's actual COMMIT call,
not by a prior statement. For deadlock, a deferred constraint trigger attempts
an advisory lock during COMMIT. The antagonist already owns that lock and is
independently observed waiting for the victim's lock. The victim's deadlock
check is 50 ms and the antagonist's is 10 seconds, making the victim detect the
cycle and reject COMMIT. Statement timeout is eight seconds in this isolated
fixture, not a change to the accepted pool's clocks.

The transparent, unchanged fault proxy records exactly one literal COMMIT on
each victim backend's connection in all eight runs. No retries or reconciliation
queries are performed by the classification path. The separate observer is
fixture evidence, not proposed production recovery logic.

## What the driver permits us to conclude

The classifier requires a driver DbError from the single outstanding COMMIT,
nonlocalized parsed severity ERROR, and class 23 or exactly 40001 or 40P01.
Other SQLSTATEs, fatal/connection errors and errors without a server reply stay
ambiguous. The 40003 negative control proves class 40 is not accepted wholesale,
even when this particular fixture independently observes a rollback.

Source: pinned tokio-postgres 0.7.18 `src/client.rs` converts an ErrorResponse
into an error from response polling; `src/simple_query.rs::batch_execute` uses
that polling and normally waits through ReadyForQuery on success. An error does
not certify that the client has consumed the following ReadyForQuery. Therefore
all rejected victim connections are retired: client dropped, connection driver
joined, and a new backend PID serves the next query. All driver joins finish;
no driver is detached or aborted. Independent psql observes zero tagged backends
after every case, and both private postmasters are reaped.

This is the classification input for extraction. It does not modify the old
pool, claim rejected connections can already be reused, or add a public error
type. Integration into the server owner belongs to gate 4.

## Unchanged regression

The original eight-case `server-transactions/transactions.py` ran unchanged on
another fresh private cluster, writing into this gate's `transaction-regression`
subdirectory. Its executable hash matches the accepted prototype's build.json;
its archived root is `beffe80`. It is a replay of the accepted implementation,
not a claim that the new classifier has already been integrated there.

All eight cases pass. The lost-COMMIT-reply and COMMIT-cancellation rows report
`ambiguous commit; no retry`, each with one COMMIT and a committed row observed
while acknowledgement is withheld. Lost rollback acknowledgement and connection
loss retire the lease; the next borrower gets a different backend. Successful
rollback precedes reuse, and cancellation, budget and pool-contention cases
retain their original outcomes. Every case reports zero remaining backends and
the private postmaster is reaped. Raw events, proxy traces and observations are
retained rather than summarised as an inferred cleanup result.

## Validation and limits

The new locked probe builds offline, formatting passes, and clippy passes with
warnings denied. Two full classification repeats and the eight-case unchanged
regression pass. Root changes are plan/evidence text only, so root suites were
not rerun for this step. Linux/PostgreSQL 18.6 only; no other platform or server
version execution is claimed. No HTTP extraction, deployment or performance
claim is made. Gates 4–6 await the next reviewed step.
