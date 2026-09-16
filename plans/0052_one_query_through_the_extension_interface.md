# rnx 0052: one query through the extension interface

Status: proposed 2026-09-16; implementation stopped at the ownership prototype
on 2026-09-16. The seven agreed cases pass, but a future retained by an earlier
input keeps its socket after interruption. See the companion evidence. No
adapter implementation is claimed. Drafted by Claude; revised the same day after
Codex's two reviews (rnx/reviews/0052_review_codex.md: cancellation
ownership, JSON as text, streamed result accounting, typed preparation,
result-shape edges, fixed arity, fixture corrections; then client versus
backend lifetime, INT4 conversion, the driver's count, a value-level late
refusal, the finite timestamp range, and the allowance's name). For review before implementation. The fifty-second record is step three of the extensibility
sequence agreed on 2026-09-15: an independently maintained PostgreSQL
adapter, compiled through Cargo into an application executable by record
0051's interface, proven on one parameterized query. Pooling, transactions,
a web server and package declarations are later steps. The adapter remains
unimplemented; only the ownership prototype is measured in the companion
evidence. The decisions below remain proposed pending the lifecycle follow-up.

## Context

At `aee1d83`, `rnx::main_with(Extensions)` assembles an executable from
trusted native extensions, each a builder that fills a `rune::Module`
created under its declared crate name. Record 0051 proved the assembly
with a fixture whose functions answer, await and fail but touch nothing
outside the process. Nothing outside rnx has yet
used the interface for a capability rnx does not have.

rnx drives Rune futures on a current-thread tokio runtime, one `block_on`
per input, with the interrupt flag raced on a 5 ms cadence and
cancellation by drop (record 0032). A task spawned into that runtime runs
only while something is inside `block_on`, and aborting a task schedules
its cancellation rather than completing it; between inputs nothing runs
(record 0034 section 3 states the consequences for HTTP's connection
pool). The HTTP battery holds its client in a `State` that the session's
`:reset` clears and drains through a private runtime hook. An extension
has neither: whatever it keeps across inputs, a session cannot release,
and whatever it spawns, nothing drains. The guarded JSON reader is
private to rnx as well; 0051's three public names do not reach it. rnx's tokio features are `rt`, `time`
and `net`; an application's Cargo graph unifies features, so an adapter
may add what its driver needs without changing rnx.

The time battery represents a moment as `i64` milliseconds since the
epoch (record 0038). Record 0033 fixes how JSON numbers become Rune
values; record 0034 fixes the shape of a bounded call: one function, one
deadline with a default and an upper bound, one object back, failures as
`Err` a script can catch. This record follows those shapes rather than
inventing a database style.

PostgreSQL detects a lost client only at its next socket interaction
unless `client_connection_check_interval` is set, so a closed socket does
not end a running `pg_sleep`; only a server-side timeout or a cancel
request does. PostgreSQL 18.6 is installed on the development machine with `initdb` and
`pg_ctl`, so a fixture can run a private cluster in a temporary directory
over a unix socket without touching the system server or a TCP port.
The system server is not used. No Rust PostgreSQL driver is cached
locally; the adapter's first build fetches one. `tokio-postgres` 0.7.18 is
the candidate at drafting; the implementation confirms the exact pin
before its source, API and licence inventory gates run against it.

## Decision

### 1. Where the adapter lives

The adapter is the crate `rnx-postgres` at `adapters/postgres/` in the rnx
repository, its own `[workspace]` with its own lockfile and third-party
notices, exactly as `jupyter/` is. It depends on rnx by path and on
`tokio-postgres` pinned to an exact version in its manifest. rnx's own
manifest, lockfile and notices do not change. The crate builds one
executable, `rnx-pg`, whose `main` is `rnx::main_with(Extensions::none()
.with("postgres", rnx_postgres::build))`, and exposes `build` so another
application can include the adapter beside other extensions.

Moving the adapter to its own repository is a decision for the user, not
this record; it is a directory move plus changing the path dependency on
rnx to whatever that repository uses.

### 2. One function of fixed arity, one call, one connection

```rune
postgres::query(url, sql, params, #{}).await?
postgres::query(url, sql, params, #{timeout_ms: 5000}).await?
```

Four arguments always, as `process::run` and `http::request` take their
options: `#{}` means defaults. `url` is a `postgresql://` connection
string as `tokio-postgres` parses it, over TCP or a unix socket
directory; `sql` is one statement with `$1` placeholders, at most 1 MiB;
`params` is a vector of at most 1000 values; `options` accepts
`timeout_ms` only, an integer 1 through 90000, default 30000; any other
key, or a wrong type, is refused before connecting naming the key.

The call connects, prepares the statement, executes it, streams the rows,
closes the connection and returns:

```rune
#{columns: ["id", "name"], rows: [#{id: 1, name: "x"}], affected: 1}
```

`rows` is a vector of objects keyed by column name. Duplicate column names
(`SELECT 1 AS x, 2 AS x`) are refused after prepare and before execute,
naming the column, because an object cannot hold them losslessly.
`affected` is the count the driver extracts from the command
completion, the final numeric token of the tag: the row count for
`SELECT`, `INSERT`, `UPDATE`, `DELETE` and `MERGE`, with or without
`RETURNING` (where `rows` is also populated), and 0 for a command whose
tag carries no count. The adapter does not classify SQL text to tell a
`SELECT` from a `RETURNING`; the driver's count is the contract. Unsupported column
types are refused from the prepared statement's metadata, before
execution, so an empty or all-`NULL` result is refused the same way as a
full one.

A connection lives for one call and is owned by the call: the connection
future is driven inside the query future, joined with the statement's
own future, never spawned as a detached task. Dropping the query future
therefore drops the socket with it, on that turn, with nothing left for a
runtime rnx cannot drain. There is no pool, no session-held connection
and no transaction spanning calls, so nothing exists for `:reset` to
release and nothing stalls between inputs. Each call pays a connection
handshake; gate 8 measures it so step four's pooling has a number to
beat.

### 3. Three lifetimes: the client's, the statement's, the write's

The deadline is `timeout_ms`, and it is enforced twice, by different
parties, because they end different things. On the client, the call's
future is raced against the deadline from the moment the call starts;
losing drops the connection, which ends the adapter's descriptor and
tasks on that runtime turn. On the server, the connection is opened with
`options=-c statement_timeout=<timeout_ms>`, so the backend ends each
command itself and no cancel request is needed. The server's interval
starts when each command arrives, and prepare and execute are separate
commands, so the backend's lifetime is bounded relative to its own
execution, not by the client's absolute deadline: after a client-side
drop (an interrupt, a budget halt, a dropped pending future) the backend
runs until its statement timeout for the command in flight or its next
socket interaction, whichever is first. When both parties time out on
the same statement, either may report first: the script sees the
client's deadline error or the server's SQLSTATE 57014, and both are
correct outcomes the gates accept.

An `Err` after the statement was dispatched does not say whether it ran.
A write can commit before a read, decode or size failure or before a
connection loss is noticed, and an `Err` proves neither commit nor
rollback. The adapter never retries; the README names this ambiguous
completion and gate 5 executes a write whose returned value is refused
after execution and shows the row present.

### 4. Values in, typed; values out, exact or refused

Parameters are sent through `prepare_typed` with the type each Rune value
maps to, so the server never infers a narrower type than the value:
`bool` as `BOOL`, `i64` as `INT8`, `f64` as `FLOAT8`, `String` as
`TEXT`, `Bytes` as `BYTEA`, and unit as `NULL` with its slot left to the
server's inference. Any other Rune value, including nested vectors and
objects, is refused before connecting naming the parameter index and
Rune type. A `String` containing NUL is refused naming the parameter. An
`i64` bound where the target is `INT4` is the server's business under
its assignment rules: a value in range is converted, an out-of-range
value is the server's error with SQLSTATE 22003, and neither is a
client-side conversion failure. `f64` NaN and
infinities are sent as `FLOAT8` NaN and infinities, which PostgreSQL
represents, and come back as the same `f64` values.

Results decode exactly: `NULL` to unit; `bool`; `int2`, `int4`, `int8`
to `i64`; `float4`, `float8` to `f64`; `text`, `varchar`, `char(n)`,
`name` to `String`; `bytea` to `Bytes`; `json` and `jsonb` to their text
as the server sends it, `jsonb` therefore normalized by the server, for
the script to hand to `json::parse`, so record 0033's reader stays the
only one and 0051's boundary stays three names. `timestamptz` decodes to
`i64` milliseconds since the epoch, the time battery's moment, flooring
microseconds toward negative infinity; that floor is this record's one
stated approximation. `infinity`, `-infinity`, and any finite value
whose original microsecond instant lies outside the time battery's
accepted range (-377705023201000 through 253402207200999 milliseconds),
checked before flooring, are refused naming the column, so every moment the adapter returns is one `time::`
accepts. Every other column type, including `numeric`, `timestamp`
without zone, `date`, `uuid` and arrays, is refused naming the column and
the PostgreSQL type name.

### 5. A logical payload allowance, and what it does not cover

Rows are streamed with `query_raw`, decoded one at a time, and the call
refuses at row 10001 naming the bound. The 8 MiB allowance is a logical
payload charge, not a measure of retained memory: allocation overhead,
container headers and the runtime's own structures are not counted. The
charge rules are fixed so the boundary can be gated exactly: 16 per row;
per value, 16 for unit, `bool`, `i64`, `f64` and a moment, and 16 plus
the UTF-8 length for a `String` or JSON text and 16 plus the length for
`Bytes`; per row, the UTF-8 length of every column name, because each
row object carries its own keys; before the statement is sent, the UTF-8
length of the SQL text and the same per-value charge for each parameter.
The URL, options, statement metadata and the script's pre-existing
values are not charged. The call refuses when the account passes 8 MiB,
naming the bound and the row. This bounds what the adapter builds and
returns. It does not bound the driver: the
driver's codec buffers each complete backend frame before the adapter
sees it, so one oversized field is held in memory once before it is
refused. The README states that limitation; gate 4 measures it with a
16 MiB single field and records peak RSS beside the refusal.

### 6. Failures are errors a script catches, and the next call works

Every failure is `Err` with a message that begins with what was attempted
and where: `cannot query <database> at <host or socket directory>: `,
never the URL and never a password. Server errors carry the server's
message and its SQLSTATE in brackets. Adapter and driver errors (a bad
URL, an unsupported value, a refused connection, the deadline, a bound)
carry their reason without a SQLSTATE, since none exists. A script that
catches the error and calls again succeeds, which gate 5 proves in every
entry point.

TLS is not offered: `sslmode=require` or `verify-*` is refused naming the
option; `prefer` and `disable` connect in the clear. The README says so.

### 7. The fixture cluster

The fixture, under `probes/postgres/` in rnx-bench, runs `initdb` into a
temporary directory it created with mode 0700, writes a `pg_hba.conf`
whose first rule requires `scram-sha-256` for a role `locked` on local
connections and whose second trusts the fixture's own role, starts the
server with `listen_addresses=''` and `unix_socket_directories` set to
that directory so no TCP port is opened, and tags every adapter
connection with `application_name=rnx-pg-fixture` so its own monitoring
connection is excluded from `pg_stat_activity` checks. A guard stops the
server with `pg_ctl stop -m fast` and removes the directory on success,
on failure and on SIGINT, including SIGINT during `initdb` and during
startup. Before each run the fixture asserts no postmaster of its own is
left from a previous run. The system server and any user database are
never touched; the fixture refuses to run if `initdb` or `pg_ctl` is
missing rather than falling back.

## Acceptance gates

Tolerances are numbers: timing assertions allow 100 ms over the stated
bound; `:memory` comparisons allow 64 KiB.

1. **Assembly.** `rnx-pg` passes the 0051 single-file comparison cases
   byte for byte against stock rnx; stock rnx refuses `postgres::query`
   as a missing item; the adapter's exact pin, lockfile, notices and
   licence inventory are recorded, and rnx's manifest, lockfile and
   notices are unchanged.
2. **Binding, not interpolation.** Insert the value
   `'); DROP TABLE t; -- "x" \ 😀` through `$1`, read it back with
   `WHERE v = $1`, and compare byte for byte. The same text spliced into
   the SQL is prepared as one statement and fails with the server's
   syntax error and SQLSTATE 42601, recorded once. `NULL` with and without
   a type context, `bool`, `i64` extremes, `f64` `-0.0`, NaN and both
   infinities round-trip; a `String` with NUL is refused naming the
   parameter; `Bytes` of every byte value round-trips; `SELECT $1` with a
   text parameter and with an explicit `$1::int8` cast both succeed; an
   `i64` of 42 bound into an `INT4` column is converted and stored, and
   2147483648 is the server's error with SQLSTATE 22003; `SELECT $1` with
   the `String` `"x"` returns `"x"` and `SELECT $1::int8` with the `i64`
   7 returns 7; a parameter-count mismatch is refused naming the counts.
3. **Decoding, named.** A table with one column of every supported type
   decodes to the stated Rune values; a `timestamptz` of a known instant
   decodes to the millisecond the time battery formats back to the same
   instant, a negative sub-millisecond instant floors as stated, the range check
   is applied to the original microsecond instant before flooring so
   both ends of the time battery's range decode and one microsecond
   beyond each is refused, and both infinities are refused. `affected` is
   gated on a `SELECT` of three rows, an `UPDATE` of two, an
   `INSERT ... RETURNING` of one, and a `CREATE TABLE`, as 3, 2, 1 and 0. `json` and `jsonb` return text, `jsonb`
   normalized, and `json::parse` reads them. `numeric`, `timestamp`,
   `date`, `uuid` and `int4[]` each refuse naming the column and type
   before any row is returned, on a populated table and on an empty one,
   and a following supported query succeeds. Duplicate column names are
   refused before execution.
4. **Ownership and bounds.** The ownership prototype runs first and is
   recorded, and it observes the client, not the server: on success, a
   caught deadline, a conversion failure, an interrupt, a budget halt and
   a dropped pending future, the executable's open unix-socket
   descriptors (read from `/proc/<pid>/fd` by the fixture) are back to
   their pre-call count before the fixture sends the next input, and no
   spawned task outlives the call. Backend lifetime is measured
   separately through `pg_stat_activity` and reported as its own number.
   `timeout_ms` of 0, 90001, a non-integer and an unknown option key
   refuse before connecting; `SELECT 1 AS n FROM pg_sleep(5)` with a 200 ms deadline returns
   either the client's deadline error or SQLSTATE 57014 within the
   tolerance, both accepted, and the backend ends by its own statement
   timeout, observed in `pg_stat_activity` and timed from the server's
   receipt; an interrupt during `SELECT 1 AS n FROM pg_sleep(120)` with a
   90000 ms deadline, sent once the fixture has observed the backend
   active, returns to the prompt with the client's socket gone (the
   descriptor count, measured on its own); the backend's disappearance
   is measured separately and may come earlier than the statement
   timeout when the server notices the disconnection, so the gate
   records the observed time as the residual without asserting a lower
   bound. `generate_series` past 10000
   rows refuses at row 10001; a result whose charge is exactly the
   allowance succeeds and one charged byte more refuses naming the row,
   computed from the stated rules and including a parameter's
   contribution; a 16 MiB single field refuses and its peak RSS is
   recorded.
5. **Failures, then recovery, and the write that happened.** A refused
   connection (wrong socket directory), a wrong password for the SCRAM
   role, a syntax error, a unique violation and a type mismatch each
   return `Err` with the stated shape, with a SQLSTATE where the server
   produced one and none otherwise; a password containing a distinctive
   marker never appears in any diagnostic; in `run`, `eval` and a session
   the same context then runs a successful query. An `INSERT ... RETURNING ts` into a
   `timestamptz` column with the value `'infinity'` executes; a
   test-only hook in the adapter (compiled only under its `test-support`
   feature, never in `rnx-pg`) pauses the call after execution and before
   decoding; while it is paused, a separate fixture connection observes
   the committed row; the hook is released, decoding refuses `infinity`,
   and the call returns `Err`. Waiting for the backend to exit after the
   drop would leave a commit-or-rollback race, so commitment is observed
   before the refusal, not inferred after it. The gate records that this
   shows one committed case, not a rule that a refusal always commits. The kernel route: the adapter installed as a notebook
   worker through `rnx-jupyter install --rnx rnx-pg` into a temporary
   Jupyter directory, a cell that fails with a SQLSTATE error shows it as
   an ordinary error output, the next cell queries successfully, and the
   saved notebook validates.
6. **Nothing persists.** One session input runs 100 sequential calls in
   a loop, so the session's own input storage does not grow between
   measurements; afterwards `pg_stat_activity` shows no tagged connection
   and `:memory` is within 64 KiB of its value after a one-call input; `:reset` changes nothing about the
   adapter because it holds nothing.
7. **Cleanup.** The fixture leaves no postmaster, no socket file and no
   directory after success, after a deliberately failing gate, after
   SIGINT during a query, and after SIGINT during `initdb` and during
   server start; each case is run and its process table recorded.
8. **Cost.** Matched hyperfine: `rnx-pg version` and `eval 42` against
   stock rnx (assembly cost, expected within drift), and `SELECT 1` over
   the unix socket, 100 runs, reported as the per-call connection cost
   that step four's pooling must improve. Binary sizes and the adapter's
   resolved dependency count are recorded. No speedup is claimed.
9. **Regression and checks.** Root suites, kernel suites, the 0051 fixture
   suite, formatting and clippy at the inherited baseline; the adapter's
   own clippy with warnings denied; a Windows type-check attempt of the
   adapter recorded as full or isolated exactly as it ran, with the
   inherited `lib.exe` blocker named if it stops the full check; execution
   unverified as before.

## Guardrails and stop conditions

- One statement per call, one connection per call, the connection driven
  inside the call's future. Stop if the driver cannot be driven that way
  and a detached task is the only option; write up what rnx would need.
- Parameters are typed values through `prepare_typed`. Stop if any path
  formats a parameter into SQL text.
- No column type is decoded approximately except the stated timestamp
  floor. A type is exact or refused.
- The server-side statement timeout is set per connection from
  `timeout_ms`; the adapter never sends a cancel request in this record.
- The fixture never opens a TCP port and never touches the system
  cluster. Stop if `initdb` or `pg_ctl` is unavailable.
- The adapter changes nothing in rnx. If a gate needs an rnx change (a
  reset hook, a drain hook, a shared JSON reader), stop and write the
  change up separately.

## Risks and forward

A connection per call is honest and slow; gate 8 puts a number on it and
step four adds pooling with the lifetime rules a pool needs, which are a
record of their own because they meet `:reset`, the runtime's drain and
the worker's lifetime, none of which an extension can reach today.
Transactions across calls need a handle that outlives a call and the same
rules. The driver's unbounded frame buffering is accepted and stated, not
solved. The type set is small on purpose; `numeric` and dates need
representation decisions the time and JSON records did not make. TLS
needs a trust decision. The web application, step four's proving ground,
will ask for all of these at once, which is why this record proves the
adapter first.
