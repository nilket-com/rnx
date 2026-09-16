# rnx-postgres

An independent native extension, assembled into `rnx-pg` through rnx's supported
extension interface. Stock `rnx` does not include PostgreSQL. The package has its
own workspace, lockfile and notices; building the root rnx package does not build it.

```sh
cargo build --locked --release --manifest-path adapters/postgres/Cargo.toml --bin rnx-pg
adapters/postgres/target/release/rnx-pg
```

At the prompt, using a database you provide:

```rune
let url = "postgresql://me@localhost/my_database?sslmode=disable";
let result = postgres::query(url, "SELECT $1::int8 AS n", [42], #{}).await?;
result.rows[0].n
```

`postgres::query(url, sql, params, options).await` returns a catchable `Result`.
The success value has `columns` (names in server order), `rows` (objects keyed by
those names), and `affected` (the driver's command-completion count, including
SELECT rows, or zero for commands with no count). There are always four arguments.
Duplicate column names are refused before execution; use SQL aliases to distinguish them.

Options accept only `timeout_ms`: an integer from 1 through 90000, default 30000.
Unknown options are errors. One call opens one connection, prepares one statement,
streams its result and closes. There is no pool, retry, cross-call transaction or
TLS. `sslmode=require` and `verify-*` are refused; `prefer` and `disable` use
cleartext. Unix socket directories and TCP hosts use the driver's connection syntax.

Parameters use `$1`, `$2`, etc. They are bound separately from SQL, never
interpolated. Unit is NULL with its type inferred by the server; bool, i64, f64,
String and Bytes are declared as BOOL, INT8, FLOAT8, TEXT and BYTEA. A String
containing NUL or any other Rune type is refused by parameter position (starting
at 1). PostgreSQL performs SQL casts and column assignment conversions: 42 fits
an int4 column; 2147483648 is its SQLSTATE 22003 error. NaN and infinities are
supported float8 values.

Parameter counts follow PostgreSQL's typed preparation. Too few values for the
prepared statement are refused with both counts. Supplying type hints also declares
parameters, including unused ones: `SELECT $1` with `[1, 2]` is accepted, as is
`SELECT 1` with `[1]`. This adapter does not parse SQL to reject unused arguments.

| PostgreSQL result | Rune value |
| --- | --- |
| NULL | unit `()` |
| bool | bool |
| int2, int4, int8 | exact i64 |
| float4, float8 | f64 (float4 widens exactly) |
| text, varchar, char(n), name | String |
| bytea | Bytes |
| json, jsonb | String; call `json::parse` to decode it |
| timestamptz | epoch milliseconds, floored toward negative infinity |

Unsupported column types are refused from prepared metadata even when no rows
would be returned. JSON is the server's text, including jsonb normalization; there
is no second JSON reader. Timestamp infinities are refused. Finite timestamps
must lie inside rnx's time range, -377705023201000 through 253402207200999 ms,
**before** flooring microseconds. PostgreSQL's own lower range is narrower.
There is no other approximate conversion; numeric, date, timestamp without zone,
uuid, arrays and domains are outside this first contract.

SQL is limited to 1 MiB and parameters to 1000. Results refuse row 10001. An 8 MiB
logical payload account charges SQL bytes, each parameter and result value (16
bytes plus string/byte contents), each row (16), and column-name UTF-8 bytes per
row. URL, options, metadata and allocation overhead are excluded. This bounds
the adapter's returned payload, **not peak memory**: the driver buffers a complete
backend frame before the adapter can refuse it. A single large field therefore
occupies driver memory first. Refusals identify the bound and row; row zero denotes
SQL and parameters.

The client deadline starts when the query future is first polled and includes
connect, prepare and reading. Connection options are replaced with a server
`statement_timeout` of the same duration; URL options cannot override it. The
server starts its timer per command on arrival, so prepare and execute have
separate server intervals. Either the client deadline or server SQLSTATE 57014
may report first. An interrupt or revoked retained future closes the client
socket without another runtime turn. The backend can keep executing until it
notices the disconnect or its statement timeout. No cancel request is sent.

The connection future is driven inside the tracked query, never spawned. TCP
hostname resolution is a separate native limitation: Tokio uses the OS resolver
on its blocking pool, and dropping a lookup does not stop an already running OS
lookup. As with rnx's HTTP path, runtime shutdown does not wait for that work.
Unix socket ownership gates do not claim to measure DNS cancellation.

**An error after dispatch does not prove rollback.** A write can commit before a
decode failure, size refusal or disconnect. Inspect or reconcile according to
your application's transaction rules; the adapter never retries automatically.
Errors name database and host, with server messages and SQLSTATE where available.
They do not print the connection URL or password. Lifecycle revocation uses rnx's
fixed `operation cancelled` error. The existing session-wrapper limitation on
top-level `?` is unchanged; matching the Result explicitly keeps its error text.

To assemble another executable:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    rnx::main_with(
        rnx::Extensions::none().with_lifecycle("postgres", rnx_postgres::build)
    )
}
```

The same executable can be installed as the Jupyter worker with
`rnx-jupyter install --rnx /absolute/path/to/rnx-pg`. Builders use rnx's re-exported
Rune pin. The root counting allocator remains in force.

Checks use a private PostgreSQL cluster in `rnx-bench/probes/postgres`; they never
use the system database. `rnx-pg-test` requires `test-support` and activates a
one-row pause after command completion for the committed-write gate. The normal
`rnx-pg` binary never activates that hook. Windows execution remains unverified;
the full cross-check is blocked in the inherited ring build by missing `lib.exe`.

`scripts/third-party-notices.py --check` verifies the independent dependency
inventory and texts, including checked provenance for missing crate-package
texts. The pre-existing missing syntree text remains explicitly listed.
