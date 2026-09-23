# rnx-polars

A small synchronous Polars extension, staged through record 0058. Gate 2 covers
CSV and Parquet; gate 3 adds bounded preview. Project/notebook integration
is exercised by gate 4; the timing gate is recorded below; Linux regression results and platform limits follow. It is an independent workspace and does not add Polars to stock rnx.

The Polars feature set is `lazy`, `csv`, `parquet` and, since record 0081,
the four narrow integer dtypes `dtype-i8`, `dtype-i16`, `dtype-u8` and
`dtype-u16`; the generated bindings, their oracle fixtures and the rustdoc
inventory they are generated from (`probes/0072/out/0.55.2-adapter-narrow`)
all describe that one build.

Borrowed slices (record 0082): a Polars method that returns or iterates
`&[T]`, or hands a callback `&[u8]`, gives the script an owned vector
copied under the materialize bound; the vector outlives its source and
never aliases Polars storage. `polars::set_materialize_limit` (test
support) bounds those copies too.

Function generics (record 0084): where Polars asks for `IntoIterator<Item = S>`
with `S: AsRef<str>` or `Into<PlSmallStr>`, or `AsRef<[IE]>` with
`IE: Into<Expr>`, the script passes a vector of strings or expressions
(`df.select_(["x", "z"])`, `expr.over([col("y")])`); the vector and its
elements stay usable afterwards. Where Polars asks for an iterator, the
script passes a vector: owned items are handed over by value, and string or
byte items are borrowed from the script's values only for the duration of
the call (`builder.append_values_iter(["a", "bc"])`).

Validity bitmaps (record 0085): `rechunk_validity()` returns an optional
vector of bools, one per row, and `iter_validities()` a vector with one
optional vector per chunk. `None` means the chunk has no validity mask (all
rows valid); it is never an empty vector. The bits are copied under the
materialize bound, counted across every chunk of one call, and the vectors
are the script's own.

Validity masks as input (record 0086): `set_validity`, `with_validity`,
`from_vec_validity` and `BooleanChunked::from_bitmap` take a vector of bools.
A mask for `set_validity` and `with_validity` must have one entry per row of
the receiver, and one for `from_vec_validity` one per value; otherwise the call
returns a `ShapeMismatch` error and nothing changes. `None` removes the mask;
`Some([])` is an explicit empty mask. `from_bitmap` reads the bools as the
values themselves. Struct columns are excluded, as Polars asserts. The script's
vector is only read.

Chunk lengths (record 0087): `chunk_lengths()` returns a vector of integers,
one per chunk in order (an empty array can still have one empty chunk). The
lengths are copied under the materialize bound, each checked to fit a script
integer, and the vector is the script's own.

Owned results (record 0088): `rechunk()` and the List and Struct
`to_physical_repr()` return a new wrapper the script owns, whether Polars
handed back its receiver unchanged or built a new array. The receiver is not
modified, and a physical conversion of a logical dtype (a list of dates, a
struct with a date field) really changes the dtype, as in Polars.

Generic free functions (record 0089): `arg_min_numeric` and `arg_max_numeric`
are static functions on each integer wrapper
(`polars::Int64Chunked::arg_max_numeric(ca)`, likewise `Int8`, `Int16`,
`Int32`, `UInt8`, `UInt16`, `IdxCa`, `UInt64`); the element type comes from the
wrapper, so the script never names it. They return the index of the extremum,
or `None` for an empty or all-null array, and honour the sorted flag exactly as
Polars does. Floats are not offered: Polars takes an unchecked path for a
sorted float.

Peaks (record 0090): `peak_max_with_start_end` and `peak_min_with_start_end`
are static functions on the ten numeric wrappers (the eight integers, `IdxCa`
included, and `Float32`/`Float64`). They take the array and two optional
boundary values compared against the first and last elements, and return a
`BooleanChunked`. An integer boundary that does not fit the wrapper's element
type (256 for `UInt8`, a negative for an unsigned type) is a
`ConversionError`; those bindings return a result, while the `Int64` and float
ones return the mask directly. A `Float32` boundary is the script float cast
with `as f32`, so NaN and infinities pass through and a large finite value
becomes infinite.

Index conversion (record 0091): `convert_and_bound_idx_ca(ca, target_len,
null_on_oob)` is a static function on the eight integer wrappers. It turns the
values into gather indices for an array of `target_len` rows: signed values
from `-target_len` count from the end, anything out of range becomes null, or
an `OutOfBounds` error when `null_on_oob` is false, and input nulls stay null.
`target_len` must be below `IdxSize::MAX`; the adapter checks this before
calling Polars (which would otherwise panic) and returns an `OutOfBounds`
error, and a negative `target_len` is a `ConversionError`.

Left subtraction (record 0092): `ca.lhs_sub(x)` computes `x - ca` on the ten
numeric wrappers. `x` is taken as the array's own element type: an integer
that does not fit it is a `ConversionError` (the `Int64` binding takes any
script integer), `Float32` casts with `as f32`, `Float64` passes directly.
Integer results wrap as in Polars (`0 - 1` on `UInt8` is `255`).

Left division and remainder (record 0095): `ca.lhs_div(x)` computes `x / ca`
and `ca.lhs_rem(x)` computes `x % ca` on the same ten numeric wrappers, with
the same scalar rule as `lhs_sub`. On the integer wrappers, division
floors, as in Python (`-7 / 2` is `-4`), and the remainder takes the
divisor's sign (`-7 % 3` is `2`). A zero divisor gives null, never a panic,
and `MIN / -1` wraps to `MIN`. On `Float32` and `Float64` both are IEEE, so a
zero divisor gives `inf`, `-inf` or `NaN`, not null. The remainder is
`x - d * floor(x / d)`, so `7 % inf` is `NaN`. Nulls in the array stay
null.

Null-aware vectors (record 0096): `ca.to_vec_null_aware()` on the ten
numeric wrappers returns one owned vector of options, with `None` exactly
where the array is null, as `to_vec()` does. Polars internally picks a
plain vector when there are no nulls; the script always sees options, so
the two branches look the same apart from the `None`s. The whole result is
checked against the materialize bound before Polars is called, so an
over-long array is a `MaterializeLimit` error that names the method, and
Polars never allocates. A `UInt64` value above `i64::MAX` fails the whole
call with 0093's `ConversionError`, leaving no partial vector.

Head, limit and tail (record 0097): every typed array wrapper (the numeric
ones, `IdxCa`, Boolean, String, Binary, BinaryOffset, List and Struct) has
`ca.limit(n)`, `ca.head(n)` and `ca.tail(n)`. `head` and `tail` take
`Some(n)` or `None`, which means 10. Each returns a new array of the same
type. The receiver stays usable, and the result stays valid after the
receiver is dropped. As in Polars, a length longer than the array gives
the whole array, and 0 gives an empty one. A negative length is a
`ConversionError`, and the receiver is unchanged. Polars's slice offsets are
signed, so these methods refuse a receiver longer than `i64::MAX` with a
`ConversionError` before the call. That length can only come from 0093's
shared-buffer appends.

Float checks (record 0098): `Float32Chunked` and `Float64Chunked` have
`is_nan()`, `is_not_nan()`, `is_finite()` and `is_infinite()`, which return a
`BooleanChunked`. A null stays null in each result. `is_not_nan` is true on
infinities, but `is_finite` is false on them. `none_to_nan()` replaces nulls
with a valid NaN and leaves every other value's bits alone. `to_canonical()`
turns −0.0 into +0.0 and every NaN into the canonical quiet NaN, and keeps
nulls. Each returns a new array the script owns.

Chunk snapshots (record 0099): `ca.chunks()` on the ten numeric wrappers
returns a copy of the array's storage as a vector of chunks. Each chunk is
a vector of options, with values and nulls in order. The outer vector keeps
Polars's chunk boundaries, including the single empty chunk of an empty
array. The copy is the script's own and outlives the receiver. Before
copying, the chunks plus their cells are checked against the materialize
bound, one slot each. A `UInt64` value above `i64::MAX` fails the whole call
with a `ConversionError`.

Record 0100 extends `chunks()` to the Boolean, String, Binary and
BinaryOffset wrappers:
- Boolean cells are bools.
- String cells are owned UTF-8 strings.
- Binary cells are vectors of byte integers 0 to 255. They stay raw, so zero
  and non-UTF-8 bytes are kept.

Every payload byte also counts one slot, so the bound covers chunks plus
cells plus bytes. It counts bytes, not characters. List and Struct chunks are
still refused.

Indexed chunks (record 0101): `ca.downcast_get(i)` on the same fourteen
wrappers returns chunk `i`, not row `i`, as an owned vector of options.
Past the last chunk it returns `None`. An empty array's chunk 0 is present
and empty, `Some([])`. Only that chunk is copied, and only that chunk counts
against the materialize bound: one slot, plus its cells, plus its payload
bytes. So one chunk of a large array can be read. A negative index is a
`ConversionError`.

Single arrays (record 0102): `ca.downcast_as_array()` on the same fourteen
wrappers returns the array's one chunk as an owned vector of options. An
empty array gives `[]`. As in Polars, an array with more than one chunk, or
none, fails Polars's one-chunk assertion and panics. It does not return the
first chunk. Rechunk first, or use `downcast_get(0)` or `chunks()`. The copy
counts one slot, plus its cells, plus its payload bytes against the
materialize bound.

Chunk iteration (record 0103): `ca.downcast_iter()` on the same fourteen
wrappers returns every chunk in iterator order, as `chunks()` does. The
result is a vector of chunks, each a vector of options. It is copied whole
before the call returns, and no iterator or Arrow array reaches the script.
The whole result is bounded as with `chunks()`: chunks plus cells plus
payload bytes, checked before copying.

Chunk views (record 0104): `ca.downcast_chunks()` on the same fourteen
wrappers returns every chunk in index order, as `downcast_iter()` does. The
result is an owned vector of chunks, each a vector of options. It is
bounded the same way: chunks plus cells plus payload bytes, checked before
copying.

Consuming chunk iteration (record 0105): `ca.downcast_into_iter()` on the
same fourteen wrappers returns every chunk in order, as `downcast_iter()`
does. In Rust it consumes the array. Here it consumes a clone, so the
script's array stays usable. The size bound (chunks plus cells plus payload
bytes) is checked on the script's array before that clone is made. So an
array too large to copy is refused without cloning it.

Integer read-back (record 0093): a script integer is an `i64`, so every
`u64`, `usize`, `isize`, `i128` or `u128` that Polars returns is converted
with a range check. A value outside `i64` is a `ConversionError` naming the
method, never a wrapped negative. This applies to direct returns, options,
tuples, vectors, iterators, copied slices and callback arguments; a callback
argument that does not fit fails the call with a `CallbackError`. So
`len()`, `null_count()`, `height()`, `shape()`, `get()` on `UInt64Chunked`
and index-returning methods now return a result:

```rune
let n = series.len()?;
let (rows, cols) = df.shape()?;
```

Seven methods whose value is proven to fit keep a plain integer:
`DataFrame::width`, `get_column_index`, `try_get_column_index`,
`max_n_chunks` and `first_col_n_chunks`, `Column::n_chunks` and the series
`n_chunks`. Each is listed in the release file with the source it is proven
from. A same-named method on another type is not covered by that proof.

Categorical hash tokens (record 0094): five categorical hash methods carry
the full `u64` as a string of exactly 16 lowercase hexadecimal digits, such
as `"0000000000000000"` or `"ffffffffffffffff"`. `Categories::hash`,
`FrozenCategories::hash` and `CategoricalMapping::cat_to_hash` return one.
`get_cat_with_hash` and `insert_cat_with_hash` take one as `hash`. Any other
spelling is a `ConversionError` before Polars is called: uppercase, a
prefix, a sign, the wrong length or a non-hex character. These are two
different hashes. The first three return a stable, fixed-state hash. The
two `*_with_hash` methods expect the mapping's own lookup hash, which is
what `get_cat` and `insert_cat` compute. A token from `cat_to_hash` is not a
valid lookup hash. Polars files an insert under whatever hash it is given,
so a wrong hash can give a string a second id. Other `u64` results stay
checked integers.

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

The hand-written surface is DataFrame, LazyFrame, LazyGroupBy and Expr with
the functions above. Receiver methods borrow: a frame, plan, expression,
path, schema or expression array can be reused. `group_by`, `agg` and
`sort` return Results because their arrays are validated. `col`, `lazy`,
`filter`, `gt`, `add`, `sum` and `alias` construct values directly. `lit`
accepts bool, i64, finite f64 and string through a Result. Use `gt` for
comparisons; `>` returns a boolean in Rune and cannot build an expression.
Both `a.add(b)` and `a + b` take Expr operands; use `lit` for scalars.
Collect executes again on each call; nonexistent columns are catchable
errors at collect.

## Generated surface (record 0073)

With the default `generated` feature the module also carries bindings
generated by `tools/polars-gen` from the Rust Polars API: every
inventoried callable of the API crates has a status in `surface.json`
(generated with its Rune path, adapted, unsupported with a reason, or out
of scope), and `plans/0073_polars_generator_evidence.md` and
`plans/0075_polars_coverage_two_evidence.md` give the counts and the
executed coverage. The release-specific inputs (API-crate list,
operations whose row order is unspecified, oracle exclusions) live in
`tools/polars-gen/releases/<name>.toml`; the shipped module is generated
with `0.55.2-joins.toml`, and `surface.json` records the file's digest. Building with `--no-default-features` gives the
hand-written adapter alone, at its former launch cost.

Policies for generated bindings:

- Ownership: a method taking `self` clones the wrapped value, so the Rune
  value stays usable; `&mut self` mutates the Rune value in place, and a
  `&mut Self` chain returns unit; borrowed returns are cloned out. All
  wrapped Polars values are cheap to clone.
- Conversions: Rune int, float, bool and string map to the Rust scalars; a
  narrower integer parameter makes the binding fallible with an error that
  names the parameter; `Option` is a Rune option, vectors and slices are
  Rune vectors, tuples are tuples, and `impl Into<T>` takes what `T` takes.
- Types: every concrete reachable Polars type a binding mentions has a
  wrapper under `polars::` with the type's own name; structs with public
  fields get `default_()` where `Default` exists, `with_<field>` setters
  and `<field>` getters; enums get one constructor per variant. A name
  that is a Rune keyword gets a trailing underscore: `select_`, `default_`.
  Where a hand-written binding has the same name, the hand-written one
  wins and the entry is listed as adapted. Since record 0075 a concrete
  alias of a generic type (`BooleanChunked`, `IdxCa`, `Schema`,
  `SchemaRef`) is wrapped under the alias's name, aliases of one
  instantiation sharing one wrapper (`IdxCa` also serves
  `UInt32Chunked`; `surface.json` lists `shared_with`), and a concrete
  type of an internal crate that a bound signature mentions is wrapped
  under `polars::<crate>::<Name>`: `polars::utils::PlRefPath`,
  `polars::compute::QuantileMethod`, `polars::arrow::ArrowDataType`.
  Methods on the generic types themselves stay unbound.
- Fixtures for the oracle: a type's test value comes from a core fixture,
  `Default` (when its `default_` binding exists), a unit variant, or,
  since record 0075, a derived recipe: a bound constructor (`new`,
  `from_*`, then alphabetical) or a data-carrying variant whose arguments
  all have fixtures. Fixtures are built in a separate setup stage on both
  sides and passed into the measured call, which constructs nothing; a
  setup failure on both sides is counted apart and verifies nothing, on
  one side it fails the run.
- Arity: Rune binds free functions of at most five parameters; the four
  API functions above that are listed as unsupported with the reason
  `arity`.
- Receivers (record 0076): one callable may be bound on several
  receivers, each a binding with its own oracle case; `surface.json`
  lists them under the entry's `bindings` with the route that produced
  them and the candidate routes that produced nothing under
  `exceptions`. `SeriesTrait` methods are bound on `polars::NullChunked`
  (the trait's public implementor) and, through `Series: Deref<Target =
  dyn SeriesTrait>`, on `polars::Series`, the way autoderef reaches them
  in Rust; where `Series` has an inherent method of the same name the
  inherent one is retained. Methods of the generic `ChunkedArray<T>` and
  `Logical<K, T>` are bound on their alias wrappers (`polars::Int64Chunked::…`)
  for every instantiation the inventory's recorded impl heads, bounds,
  `where` predicates and associated types prove applicable; unproven
  pairs are counted exceptions with their reason, and the release file
  records the families shipped under the launch budget and the cited
  exclusions the compiler forces.
- Arrays in the oracle are compared as the series they convert to, name,
  dtype and every element with nulls; typed source fixtures per family
  (`fx::series_bool()`, `fx::series_f64()`, …) feed the producer
  bindings that give each array alias its fixture.
- Iterator returns (record 0077): a binding whose Rust return is an
  iterator (`impl Iterator`, `DoubleEndedIterator`, `ExactSizeIterator`,
  `PolarsIterator`, `TrustedLen`, bare or inside `Option`/`PolarsResult`)
  returns a Rune vector, materialized inside the call, inside the engine
  closure when the binding is routed, so the script never holds a lazy
  iterator and a `&self` receiver stays usable. Materialization is
  bounded by an item count, 1 048 576: exactly that many items succeed,
  one more refuses with a `polars::Error` of kind `MaterializeLimit` and
  no partial vector. A length is trusted only from `ExactSizeIterator`
  (checked before the first item is taken); otherwise items are counted
  as they are taken and excess is detected by one extra `next` whose
  item is discarded. `&mut self` iterators are not bound. Items that do
  not map (arrow-internal arrays, bare slices) leave the callable
  unsupported with the item named.
- Constructors and operators (record 0078): a Rust `From<X>` impl on a
  wrapped type is the constructor `Type::from_<source>(x)`
  (`polars::Scalar::from_i8(2)`, `polars::Column::from_series(s)`); a
  by-reference impl is its own `from_<source>_ref`
  (`polars::SortOptions::from_sort_multiple_options_ref(o)`). Integer sources
  are checked (`from_i8(128)` is a `ConversionError`); `f32` sources
  are cast from the script's float (`from_f32(0.1)` holds the `f32`
  rounding, `1e40` is infinity), as the Rust call would. The flag and
  selector types support `-=`, `&=`, `|=` and `^=` in place, the right
  operand unchanged; `x |= x` is refused by Rune's borrow check with no
  mutation. Rune has no unary `!` for external types, so the complement
  is `x.not_()`, the receiver unchanged.
- Errors: `PolarsResult<T>` is a Rune `Result` whose error is a
  `polars::Error` with `kind()` and `message()`; `kind()` is the variant
  name Rust matches on.
- Execution: a binding whose owner, parameter or return is a frame,
  series, column, group-by or lazy frame runs on the engine thread, like
  the hand-written `collect`, so no Polars work runs on the session's
  runtime thread; the same for names beginning with collect, fetch, sink,
  scan, read, write, execute, concat, sort or rechunk. Plan construction
  (expressions, options, dtypes) runs directly. An engine thread that
  cannot start is a panic with a clear message; it does not change a
  binding's Rust fallibility. The one non-`Send` type, `AmortSeries`, is
  listed and not routed. The available audited name, length and null-count
  metadata methods on Series and Column, along with `with_name` and
  `rename`, run directly, including inside callbacks.
- Callback bindings accept Rune functions and closures with constant
  captures. A capture that cannot cross threads returns `CallbackCapture`.
  Every callback binding and every classified execution or schema sink runs
  through the engine boundary. A routed method called inside a callback
  fails that callback with the refused method named; an outer binding
  returns `CallbackError` and a hand-written sink returns the same text as
  its string error. `apply_mut` commits to its receiver only after every
  callback succeeds. `polars::set_callback_budget(n)` sets the instruction
  allowance per invocation; `0` removes the allowance. It cannot preempt a
  blocking native call. Vector arguments borrow the script's vector and
  clone its elements, leaving both usable after the binding returns.

`polars::version()` returns the Rust `polars` crate version the executable
was built with (`"0.55.2"`). Rust and Python Polars use separate version
tracks; their numbers alone don't establish release age or feature parity.

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

## Shared compilation

Record 0069: a Git declaration of this adapter written by `rnx project add`
or `:dep` carries `shared_build = true`, the maintainers' statement that
nothing in its dependency graph reads retained build output (`OUT_DIR`) at
runtime, so its assemblies compile in the cache's shared build directory.
The record's gate 3 exercised the Polars executable published from a shared
build through later builds in the same directory, a build script re-run there,
and the directory's removal. A fork of this adapter that adds such a reader
must drop the declaration; the tool refuses publication when the executable
holds the directory's path, and cannot see a reader that learns the path
another way.
