# 0052 implementation evidence

Measured by Codex on nano/Linux, 2026-09-16. This is the adapter implementation,
after the accepted 0053 lifecycle change. The original
`0052_one_query_through_the_extension_interface_evidence.md` and bench
`results/postgres-0052/` still describe the original stop; neither is overwritten.
New fixtures and raw observations are in rnx-bench `probes/postgres/` and
`results/postgres-0052-adapter/`.

## Assembly and scope

`adapters/postgres` is an independent workspace. `rnx-pg` calls the public
`main_with(Extensions::none().with_lifecycle("postgres", build))`; `build` lends
the scope to the one native `query` function. The operation owns its connection
future and drives it alongside the statement. No connection task is spawned and
no extra runtime turn is used for cleanup. The default executable has no activated
test hook. The separate `rnx-pg-test` binary explicitly enables the feature-only
commitment pause.

The root manifest, lockfile, notices, sources, tests and entire kernel package
are byte-identical to accepted `f07d31c`. The root README only links the adapter.
The library boundary has no new names. The adapter lock was seeded from the root
lock, preserving shared dependency versions rather than measuring an incidental
upgrade. `tokio-postgres` is pinned to **0.7.18**. The Linux/Windows normal+build
union contains **184 packages**, versus **146** for the same root scope: 38 added,
none removed. This includes procedural macros and is not a binary-retention claim.

The independent notices collect 129 distinct texts. Five macro packages omitted
texts from their crate archives; exact packaged source revisions and fetched
hashes are recorded under the adapter's `third-party/licenses/`. The existing
syntree missing-text item remains explicit. Root notices and their narrower
scope are unchanged. The generator validates local and root fallback provenance.
The original rustyline-derive licence has a trailing space and final blank line;
`git diff --check` flags those vendored bytes and the reproduced notice line.
They are kept byte-exact so the recorded upstream hashes remain meaningful.

The 0051 comparison fixture passes 22 complete stdout/stderr/status comparisons
between stock and application, including CLI errors, budgets and diagnostics.
Stock rnx refuses `postgres::query` as a missing item. The existing extension
fixture also passes unchanged, including builder refusals, ordering, reset and
worker startup.

## Three implementation qualifications

1. **Timestamp test domains.** PostgreSQL refuses the lower rnx time endpoint:
   its own timestamp range begins in 4713 BC. The decoder unit gate checks both
   rnx endpoints and each adjacent microsecond; SQL gates check PostgreSQL's
   lower endpoint, rnx's upper endpoint, upper-plus-one-microsecond, both
   infinities and negative fractional flooring. Range checking still precedes
   flooring. The SQL refusal and exact decoder assertions are recorded rather
   than claiming a server round trip for an instant it cannot store.
2. **Declared parameter count.** `prepare_typed` declares parameters through its
   type hints. PostgreSQL accepts `SELECT $1` with `[1, 2]` and `SELECT 1` with
   `[1]`. Both are gated. Missing values for prepared metadata are refused with
   both counts. The implementation follows PostgreSQL here; rejecting unused
   parameters would need another SQL-analysis decision. This qualification is
   called out for review, not hidden behind the original mismatch gate.
3. **Native DNS work.** Driver `connect.rs` uses Tokio `lookup_host` for TCP
   names; Tokio `net/addr.rs` submits an OS lookup to its blocking pool. Dropping
   an already-running native lookup does not stop it. This is the same native
   exception recorded for HTTP in 0034, and is documented separately from the
   connection future's ownership. Unix-socket gates do not prove DNS cancellation.

## Binding, decoding and bounds

`contract.py` passes its SQL observations against a fresh PostgreSQL 18.6 cluster:

- The quote/SQL/backslash/emoji payload round-trips through `$1` byte for byte.
  The spliced version fails preparation with 42601. Multiple statements refuse.
- NULL with and without a type context, bool, signed extremes, text and explicit
  casts pass. Float NaN and infinities survive; negative zero is checked by its
  reciprocal's sign. All 256 byte values round-trip. Unsigned values and a NUL
  string refuse by parameter position. INT8-to-INT4 assignment accepts 42 and
  rejects 2147483648 with 22003.
- A persisted table covers supported scalar columns. Separate bytea and JSON
  gates cover those families. JSON and normalized JSONB remain strings and the
  existing `json::parse` reads u64::MAX exactly. A known timestamp formats back
  through `time::rfc3339`. Numeric, timestamp, date, uuid and arrays refuse on
  populated and emptied tables. A sequence proves duplicate names are refused
  before execution. Completion counts are 3, 2, 1 and 0 for the specified cases.
- Bad timeout/option values refuse before an intentionally impossible connection.
  SQL above 1 MiB and more than 1000 parameters refuse. Row 10001 refuses. The
  exact 8 MiB gate includes SQL, a parameter, row/header/name and value charges;
  one more byte refuses at row 1. The deadline gate accepts either the client
  error or server 57014 inside 200 ms plus the stated 100 ms tolerance.
- The 16 MiB field refuses after the driver holds its frame. `entrypoints.json`
  records external peak RSS; this is deliberately not an 8 MiB memory claim.

## Ownership, server lifetime and committed writes

The accepted 0053 prototype rerun is the prerequisite. `lifecycle.py` repeats
the ownership cases with the actual adapter: success, caught deadline, real
infinity decode refusal, interruption, bound/unpolled future discarded by a
failure, real VM budget exhaustion, lazy unawaited future and interrupted
retained binding. All descriptor counts return to baseline **before ack or a
subsequent input**. The retained wrapper subsequently reports operation cancelled.
A further case resets a successfully retained, polled query, checks closure before
ack and then executes another query successfully.

Every pending case has a distinct application name. An older sleeping backend
therefore cannot satisfy another case's activity observation. Select releases its
borrow while the input sleeps, leaving a visible open descriptor before the
failure or budget loop. The adapter source has no spawn; the prerequisite trace
also records zero connection-driving tasks.

Interruption's client and backend are different measurements. The client closes
before settlement. The 120-second sleep under a 90-second statement timeout
continues server-side until the timeout in this run. `lifecycle.json` records
signal-to-settlement and backend disappearance separately. Its 200 ms case also
times disappearance from the fixture's observation of the active backend; that
observation happens after command receipt and can understate elapsed server
time; it is not server-clock instrumentation. No backend-lifetime lower bound is asserted.

The commitment gate waits for command completion, holds one returned row in the
test-only path and pauses before decoding. A second connection sees one committed
row while the original call is paused. Releasing the hook then refuses infinity.
That proves one committed case before refusal, not that all errors commit.

## Recovery, notebook and cleanup

Connection, SCRAM authentication, syntax, unique and type failures are followed
by a successful query in the same run/eval/session context. The distinctive wrong
password never appears in output. A temporary kernelspec launches the adapter
through the unchanged kernel. A cell explicitly turns its caught SQLSTATE error
into a runtime error; the next cell succeeds. The notebook saves and validates
with nbformat. No real user kernelspec is modified. The existing top-level `?`
session-wrapper limitation is not changed or claimed fixed.

One-input loops of one and 100 calls differ by 206 live requested bytes, below
the 64 KiB tolerance. No tagged backend remains at the end. Reset also passes with
a pending operation, as described above.

Each fixture owns a mode-0700 directory and socket-only cluster. PostgreSQL
environment overrides are cleared; the system cluster is never queried. A stale
postmaster from a dead fixture owner refuses startup. The cleanup battery records
tool and postmaster PIDs and verifies their disappearance and directory removal
after success, deliberate failure and SIGINT during initdb, startup and a query.

## Cost and checks

Final binary hashes, bytes and tool versions are in `conditions.json`; matched
100-run, 10-warmup timings use core 4. The final timings are run after the other
fixtures finish. The startup means differ by less than 0.03 ms;
the JSON difference is not claimed as an optimization. Query timing includes
process startup, one connection, prepare, SELECT, rendering and teardown. It is
not a network-only or warm-query latency measurement.

| Workload | Stock A | Stock B | rnx-pg |
| --- | ---: | ---: | ---: |
| version | 0.544 ms | 0.539 ms | 0.559 ms |
| eval | 4.021 ms | 4.009 ms | 4.037 ms |
| bare run | 3.671 ms | 3.678 ms | 3.706 ms |
| 10k JSON | 11.850 ms | 11.828 ms | 11.521 ms |
| Process + connection + SELECT 1 | — | — | 5.533 ms |

Stock is 15,187,704 bytes; rnx-pg is 16,277,496 bytes. The two stock columns are repeated measurements of the same binary.

Final ownership run: 4.671 ms after SIGINT to settlement, 90.006 s from cell start to observed backend disappearance. Final oversized-field peak RSS: 28,788 KiB.

Checks: root default **369**, root test-support **410**, kernel default **23**,
kernel transport-probe **23**, adapter **3** in each feature configuration: zero
test failures. Formatting, both notice checks, release selfcheck and the 0051
fixture pass. Adapter clippy denies warnings and passes. Root clippy retains the
same **35 code-bearing diagnostics**, including the two inherited denied octal
literal errors; it is not claimed clean.

The full Windows cross-check stops in ring's build before the adapter is checked,
because `lib.exe` is unavailable. There is no claim of a successful full or
isolated Windows adapter check, nor of Windows execution. TLS, pools, cross-call
transactions, numeric/date representations and web serving remain deferred.


## Review follow-up: reusable URL and SQL bindings

Review found that the native signature's owned `String` arguments consumed Rune
bindings. The boundary now takes `&str` for both inputs and creates owned snapshots
for the lazy tracked future. The query body, driver and lifecycle wrapper are
unchanged. No borrow survives the native call; a script can also mutate its SQL
while an unpolled query retains the original text.

Two integration tests in the adapter's regular Cargo suite exercise repeated
calls and mutation after creating a lazy future, without a database. Both feature
configurations now pass **5 tests** (3 unit and 2 integration), and clippy with
warnings denied and formatting pass. Root runtime code is unchanged.

The real-database follow-up is `rnx-bench/probes/postgres/reuse.py`. Run and eval
query twice using the same URL and SQL bindings, returning 42 and 43 and showing
both original strings intact. A worker session retains those bindings across
separate inputs and repeats the calls. Its lazy-snapshot case mutates SQL after
creating a query, then successfully executes the original SQL. The private cluster
is stopped and removed. The full SQL contract fixture is rerun into the separate
`results/postgres-0052-reuse/` directory, preserving the original measurements.

Corrected release SHA-256: `8132775a5496c99e6d77c6f49fb2938c8d5978404123c6530924ed7e3ba90d4d`.
The earlier hashes and cost table describe the pre-fix implementation; timings
were not rerun for this follow-up. The pre-existing HTTP ownership issue is a
separate root change and remains outside 0052.
