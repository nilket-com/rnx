# rnx-polars

A small synchronous Polars extension, staged through record 0058. Gate 2 covers
CSV and Parquet; gate 3 adds bounded preview. Project/notebook integration
and performance gates remain open. It is an independent workspace and does not add Polars to stock rnx.

```sh
cargo build --release --locked --manifest-path adapters/polars/Cargo.toml
adapters/polars/target/release/rnx-polars run your-script.rn
```

An application assembles it with
`rnx::Extensions::none().with("polars", rnx_polars::build)`.
Use the pinned Rune re-export from rnx when assembling other modules.

```rune
pub fn main(_) {
    fs::write_new("tiny.csv", "category,value\na,1\na,2\n🦀,3\n🦀,4\nmissing,\n")?;
    let schema = [("category", "string"), ("value", "i64")];
    let frame = polars::read_csv("tiny.csv", schema)?;
    let grouped = frame.lazy()
        .filter(polars::col("value").gt(polars::lit(1)?))
        .group_by([polars::col("category")])?;
    let result = grouped.agg([polars::col("value").sum().alias("total")])?
        .sort(["category"])?.collect()?;
    println!("{}", result.preview()?);
    result.write_parquet_new("tiny.parquet")?;
    let again = polars::read_parquet("tiny.parquet")?;
    0
}
```

Run this in a fresh directory: both writes refuse existing paths. Expected rows
are `("a", 2)` and `("🦀", 7)`, with string and i64 columns. Native frames remain opaque unless you call `preview()` explicitly. It returns
a string starting with the complete dimensions, for example:

```text
DataFrame: 2 rows × 2 columns
"category": string | "total": i64
"a" | 2
"🦀" | 7
```

Preview inspects at most ten rows and eight columns. Each name/string cell is
limited to eighty Unicode scalars; longer values get `…[truncated]` outside
their quotes. It never formats the whole frame first. Row/column omissions are
named below the dimensions. The returned string is at most 8192 UTF-8 bytes,
including a reserved byte-limit marker; it can stop before the row limit.
Strings are quoted, so `"null"` differs from null. Controls, quotes and backslashes
are escaped; finite floats use round-tripping spelling. The layout is independent
of terminal width and Polars formatting settings. `preview()` borrows the frame,
does not collect, perform I/O or print, and can be called repeatedly.

The four values are DataFrame, LazyFrame, LazyGroupBy and Expr. Receiver methods
borrow: a frame, plan, expression, path, schema or expression array can be reused.
`group_by`, `agg` and `sort` return Results because their arrays are validated.
`col`, `lazy`, `filter`, `gt`, `add`, `sum` and `alias` construct values directly.
`lit` accepts bool, i64, finite f64 and string through a Result. Use `gt` for
comparisons; `>` returns a boolean in Rune and cannot build an expression. Both
`a.add(b)` and `a + b` take Expr operands; use `lit` for scalars. Collect executes
again on each call; nonexistent columns are catchable errors at collect.

CSV schema is an ordered, nonempty vector of `(name, dtype)` tuples, with unique
nonempty names and exactly `string`, `i64`, `f64` or `bool`. The raw header must
match it exactly in arity, spelling and order. Polars reads the first record as
strings, then the **same open file** is rewound for typed reading. This is not a
snapshot against concurrent file mutation. No second CSV parser is used.

Measured with Rust Polars 0.55.2 and Python Polars 1.44.2:

| CSV case | Result |
| --- | --- |
| CRLF, quoted comma, newline and doubled quote | Preserved |
| Short data row | Missing fields padded with null |
| Unquoted empty field | Null |
| Quoted empty string | Empty string |
| Header only | Zero rows with declared schema |
| Extra data field or invalid integer | Refused |
| Duplicate, renamed, reordered, short or extra header | Refused |
| Empty file or invalid UTF-8 | Refused |

Reads accept local regular files, following symlinks; directories and FIFOs
refuse. No scan/glob/cloud API is exposed. Parquet and collected results must
have only string, i64, f64 or bool columns (nullable); other dtypes refuse by
column, even for empty data. There is no general data-size cap.

Parquet writes use create-new and uncompressed encoding. Both engine and final
flush errors are returned. **A failed write may leave a partial new file**; it
is not deleted or retried, and the next write refuses that path. There is no
atomic-publication or crash-durability promise. Some low-level errors are
reported by Polars as a generic Parquet transport error.

Each I/O or collect call runs on a fresh named `rnx-polars-engine` scoped thread
with no entered Tokio runtime. Owned Send engine inputs cross the boundary;
Rune values stay on the caller. The thread is joined before return. Engine
panics resume on the caller; spawn failures are errors. Calls remain synchronous
and cannot be preempted by budgets or Ctrl-C. Polars' separate global pool may
persist through session reset. `test-support` alone enables counters and context
assertions; it adds the observation function `polars::engine_counts`.

The engine pins differ between Rust and the Python wheel; gate 1 records their
provenance. No matched-engine or speed claim follows from this correctness gate.
The independent notices currently identify three missing upstream licence texts
explicitly; the complete notices/platform/regression gate remains open.

Validation:

```sh
cargo fmt --manifest-path adapters/polars/Cargo.toml --check
cargo clippy --locked --all-targets --manifest-path adapters/polars/Cargo.toml -- -D warnings
cargo test --release --locked --manifest-path adapters/polars/Cargo.toml -- --test-threads=1
cargo test --release --locked --manifest-path adapters/polars/Cargo.toml --features test-support -- --test-threads=1
python3 adapters/polars/scripts/third-party-notices.py --check
```

Run configurations serially in the shared target directory. The bench's
`probes/polars-contract/check.py` drives scripts and verifies both Parquet
producers through its pinned private Python environment. Same-handle and faulted
writer unit tests run the actual engine helpers and require no product hooks.
