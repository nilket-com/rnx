# rnx-polars

A small synchronous Polars extension, staged through record 0058. Gate 2 covers
CSV and Parquet; gate 3 adds bounded preview. Project/notebook integration
is exercised by gate 4; the timing gate is recorded below; Linux regression results and platform limits follow. It is an independent workspace and does not add Polars to stock rnx.

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
are `("a", 2)` and `("🦀", 7)`, with string and i64 columns. `preview()` returns
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

`format!("{frame}")` and `println!("{frame}")` return this same text wherever
the adapter is installed: the display protocol comes with the module, not with
the presenter. In a session or notebook whose application also registered the
presenter — a declaration with `presentation = true`, which `rnx project add
polars` and `:dep polars` write — a bare top-level `DataFrame` result shows it
too. A frame inside a vector, tuple or `Result` stays opaque, as do LazyFrame,
LazyGroupBy and Expr; showing a plan never runs it. Without the presenter a
bare frame is opaque and `preview()` or `format!` is the way to see it.

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
The independent notices include the alloc-stdlib text recovered from its exact
published revision. Two package-wide texts remain unavailable at their published
revisions: polars-parquet-format 0.1.0 and syntree 0.18.0. Their declared licences
and the missing texts remain explicit in the inventory; this is not a complete
licence-text clearance. Linux regression passes. The Windows cross-check stops
in native dependencies because this Linux host lacks lib.exe; neither a full
Windows type-check nor Windows execution is claimed.

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


## Project and notebook assembly

The checked-in project example lives outside this native package root at
`rnx-bench/examples/polars/`. With sibling rnx and rnx-bench checkouts, run:

```sh
rnx-project lock --manifest ../rnx-bench/examples/polars/rnx.toml --offline
rnx-project build --manifest ../rnx-bench/examples/polars/rnx.toml --offline
rnx-project run --manifest ../rnx-bench/examples/polars/rnx.toml -- /absolute/fresh/output/directory
```

Create that output directory first. The script refuses existing CSV/Parquet
files. The manifest declares `rnx-polars` with the plain `build` hook; the project
tool generates the executable. Local locks/receipts identify local paths and
native working-tree bytes and are not a portable dependency snapshot.

For Jupyter, install the **generated executable** from `.rnx/artifacts/` with
`rnx-jupyter install --rnx /absolute/path/to/artifact` (or use the ordinary
`rnx-polars` executable). Do not point the kernel at rnx-project: the kernel
needs a worker executable, not the project's command dispatcher. The Jupyter
CLI must be on PATH. Restart drops notebook bindings while the new worker keeps
the compiled-in adapter. A cell whose value is a bare frame shows its preview
as `text/plain` when the application registered the presenter;
`println!("{}", frame.preview()?)` works either way. Mapped source packages are
not supplied to notebook cells by this record.

The gate 4 fixture uses a private kernelspec directory and does not install into
the user's Jupyter registry. Its test-support builder marker
`RNX_POLARS_BUILD_MARKER` is absent from ordinary builds; when explicitly set in
a test build it appends one line per builder call.


## Measured tiny-pipeline costs (0058 gate 5)

On the recorded Linux host, one pinned CPU and one Polars thread, two interleaved
repeats of thirty warm launches per workload produced these medians in ms:

| Launch | Init (two repeats) | Full pipeline (two repeats) |
| --- | --- | --- |
| Python Polars 1.44.2 | 96.71 / 96.80 | 99.87 / 99.99 |
| Ordinary rnx-polars | 6.76 / 6.73 | 11.44 / 11.47 |
| Verified rnx-project run | 150.76 / 150.11 | 155.10 / 154.81 |
| Generated artifact directly, same map | 6.61 / 6.69 | 10.88 / 10.85 |
| Direct wrapper with matching project feature | 6.77 / 6.80 | 11.49 / 11.45 |

Direct rnx-polars is faster for this tiny end-to-end workload; verified project
launch is slower than Python. The project hashes about 7 MB of source-tree
inputs plus the executable and other inventory on each launch. That verification
cost is part of the product, not hidden from its timing. The generated-direct
control isolates it using the same artifact and map. The feature-aligned direct
control has the generated assembly's dependency versions/features.

Python uses normal bytecode caching after warm-up. Both pipelines create CSV,
validate its header through the same handle, read with a schema, perform the
native query, write/flush/close uncompressed create-new Parquet, read it back,
compare previews and collect again. Output-directory setup/removal is outside
timing. There is no fsync durability measurement and no query UDF. Median
pipeline peak RSS from separate launches was 86.5 MiB for Python, 45.9 MiB for
the ordinary adapter and 48.7 MiB for project launch.

Setup has a different tradeoff: with cached downloads and two build jobs, the
empty-target Rust build took 505.48 seconds, the warm build 0.19 seconds. Private
Python venv creation took 0.61 seconds and installing exact cached wheels took
0.48 seconds. The ordinary executable is 107,400,144 bytes. These are single-host
observations, not installation or build-time promises.

The Rust crate and Python wheel have different engine revisions. Wheel hashes
are fixed and installed code was verified, but complete wheel compiler/allocator
settings are not exposed by build_info. These numbers do not prove a compute win
or isolate language-boundary cost. All samples, the excluded preliminary
no-bytecode-cache run, graphs and provenance are in rnx-bench's
results/polars-cost-0058; no corrected-run samples were discarded.

The ownership probe also confirms the limitations: a native read can return
above a session ceiling before that ceiling is sampled. Reset releases its
frame while Polars pool threads persist. SIGINT during collect cannot stop the
native call. A file that finishes in the same poll can retain successful
completion; an ensuing await observes the signal after collect returns. The
observed roughly 312 ms signal-to-exit interval is not a cancellation bound.
