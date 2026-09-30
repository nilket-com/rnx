# rnx 0123 evidence: schema display, `cast` with a `DataType`, and a display check

**The two changes:**
- `df.schema()` now shows Polars' own text in a template, in `println!` and with `{:?}`.
- `col("qty").cast(polars::DataType::Float64())` works, through Polars' own `From<DataType> for DataTypeExpr`. A wrong argument is refused before Polars with a VM error naming both accepted types, and no binding becomes fallible.

**The probe now measures display apart from computation**, through the real presenter: the one a `:dep polars` session registers.
- Before this record, 3 of 8 composed workflows computed, but only **W8** was usable end to end.
- After it, 5 compute and **W2, W3 and W8** are usable.

**Replay:**
- `probes/0123/replay.sh` rebuilds the before state (0122's generator) and the after state, probes both at v2 with display, probes 0.55.2, compares every table byte for byte, and runs the mutation controls.
- `probes/0123/mutations.sh` alone runs the controls.

## 1. Schema display

**What Polars offers.** `Schema` derives `Debug` and has no `Display` impl (`polars-schema schema.rs:10-13` at da47b74, and the same `#[derive(Debug, Clone)]` at 0.55.2). 0113 refused its `Debug` impl because it sits on the generic head `Schema<Field, Metadata>`.

**The rule, a closed and cited row (`[[protocol_instantiations]]`).** It binds the impl `polars_schema::schema::Schema as core::fmt::Debug` onto the `SchemaRef` wrapper (`Arc<Schema<DataType, ()>>`). It is emitted through the protocol family's own `emit_foreign`, unchanged, with the same shape check and the same `DEBUG_FMT` code. The compiler proves the bound for this instantiation, since the binding does not compile otherwise. The same row asks for `DISPLAY_FMT` with the same Polars `Debug` text, because Rust has no `Display` for the type.

**Validation, fail closed:**
- **First pass, row by row:** each row cited, unique, and the `Debug` trait only.
- **Second pass, against the world:** a wrapped carrier, the impl in the inventory, and a carrier that instantiates the impl's owner (review, round 1). The carrier's wrapper identity, through at most one `Arc` (`SchemaRef` is `Arc<Schema<DataType, ()>>`), must have the owner's path, exactly the impl head's number of arguments (`Schema<Field, Metadata>` has two), and no argument left open as an impl parameter.

Self-test `protocol_instantiations` covers:
- the row-level refusals;
- a mispaired carrier (`Column` for Schema's `Debug`);
- the structural check alone: `SchemaRef`'s identity passes, while a different head, a wrong arity or an open parameter is refused.

Mutation controls on the real releases: `Clone` exits 2 ("not an allowed protocol"), and `Column` as the carrier exits 2 ("does not instantiate").

**The surface:** the Schema `Debug` entry moves from `unsupported` (0113 family) to `generated`, with the binding `polars::SchemaRef DEBUG_FMT`, at both pins.

**Found in implementation:** a catalogue entry for the display row (`polars::SchemaRef DISPLAY_FMT`) is not a valid help path. The real binary validates help paths when it installs the extension, so the whole extension failed to load; the adapter tests passed only because they skip that validation. Protocols carry no catalogue entry, as the protocol family's bindings don't, and the display row now follows suit. The probe's real-binary runs are what caught it.

**Proof** (`tests/schema_and_cast.rs`, at both pins): `${df.schema()}` and `format!("{:?}", df.schema())` both equal Rust's `format!("{:?}", df.schema())`. In the probe, `w2.schema` and the composed W2 compute, and W2 displays.

## 2. `cast` with a `DataType`

**The rule (`[[into_arguments]]`, a closed and cited table).** A direct parameter of type `impl Into<T>`, where `T` is a listed target, accepts:
- a `T`;
- or a value of a listed source `S` whose `From<S> for T` is recorded in the inventory, converted with Polars' own `<T as From<S>>::from`.

**The binding.** The binding takes one `rune::Value` and dispatches on its type before any Polars call. Each branch borrows the wrapped value and clones it, exactly as the existing `&W` argument did, so the value stays usable afterwards (tested). Anything else is refused by `VmError::panic` naming the operation, the argument, both accepted types and the value found. The binding's return type and fallibility are unchanged: it wraps its unchanged body in `VmResult`, so `cast` still returns an `Expr` and needs no `?`.

**The refusal is a VM argument-type error, by design** (the plan is amended; review, round 1). A typed, script-catchable `ConversionError` would make all 12 bindings fallible, so every `cast` would need `?`, which is the fallibility change the user ruled out. The refusal works like Rune's own "Expected type …" errors. The test asserts the exact boundary:
- the exact message, `cast: `dtype` must be DataTypeExpr or DataType, found ::std::string::String` (and the `strict_cast`/`::std::i64` form);
- that a script `match` on the call cannot catch it;
- that a valid call returns the `Expr` itself and chains without `?`.

**Validation, fail closed:** cited, unique, non-empty, the target and every source wrapped, and each `From<S> for T` recorded. Self-test `into_arguments` covers the five refusals and the argument's shape.

**This record lists one target,** `DataTypeExpr`, with source `DataType`. It applies to **direct parameters only**. The first run also reached `replace_strict`'s `return_dtype: Option<impl Into<DataTypeExpr>>`, where the argument's shape differs. The plan's stop rule applied, and the rule is now limited to depth 0.

**The parameters covered:**
- **v2, 12:** `Expr::cast`, `strict_cast`, `cast_with_options`; `LazyFrame::cast_all`; `BinaryNameSpace::reinterpret`; `StringNameSpace::json_decode`, `strptime`; `CategoricalNameSpace::to`; `ExtensionNameSpace::to`; the free `cast`, `int_range`, `int_ranges`.
- **0.55.2, 6:** `cast`, `strict_cast`, `cast_with_options`, `cast_all`, `CategoricalNameSpace::to`, the free `cast`. The narrow build has no others.

**The freeze.** The frozen lists are rebuilt from 0122's surfaces:
- **v2:** 6,770 bindings. The 0122 surface was regenerated by the 0122 generator in a worktree, and it equals the committed evidence.
- **0.55.2:** 5,073 bindings, from the e16b445 commit.

Both are strict supersets of 0120's lists, and they now include 0120's own new bindings.

**Widenings.** A contract change passes only through a listed widening in `[[frozen_widenings]]`: the exact old summary, the exact new one, and a citation. There are 12 rows at v2 and 6 at 0.55.2, generated from the freeze's own messages. A listed widening that goes unused is itself refused. Both pins print "none moved, no contract changed except N listed widenings".

**Mutation controls at both pins (`mutations.sh`):**
- the unmutated release generates;
- an unlisted source (`Column`, with no recorded `From`) exits 2;
- an unlisted target (`Expr`, with no recorded `From<DataType>`) exits 2;
- the rule removed exits 2, because the 12 or 6 widenings go unused;
- one widening's new contract altered exits 2 ("changed its contract");
- a carrier that does not instantiate the impl's owner (`Column`) exits 2 ("does not instantiate");
- a `Clone` protocol row exits 2.

**Proof** (`tests/schema_and_cast.rs`):
- **Valid dtypes:** `cast(DataType::Float64())` and `cast(DataTypeExpr::from_data_type(...))` each equal Rust's `cast(DataType::Float64)`. `int_range(…, DataType::Int32())` is Int32 at v2 (0.55.2 has no `int_range`).
- **A refused argument:** `cast("f64")` and `strict_cast(3)` are VM argument-type errors with the exact messages above. They cannot be caught as a `Result`.
- **Borrowed, not moved:** a dtype passed to two casts stays usable.
- **A conversion Polars refuses:** `strict_cast` of `["7", "x", "9"]` to Int64 surfaces as Polars' own error. The hand-written `collect` returns it as its text, `polars collect: …`, with Polars' message unchanged.

**The oracle:** unchanged at v2. The 12 bindings still pass their `DataTypeExpr` fixture, and the schema protocol has no script-level oracle trigger; the tests above cover both. At 0.55.2 the only movements are the two unordered `unique` flips, and the committed results are kept.

## 3. The display check

`probe.py --display` shows every computed frame result two ways, through `rnx-polars-fixture`, the build that registers the adapter's presenter as a `:dep polars` session does:
- **printed:** `println!("{}", result)`, the frame's `DISPLAY_FMT`;
- **presented:** the same computation as one bare expression at the session prompt, record 0068's presenter.

**Outcomes:**
- `displayed`: the preview header, with no column cut;
- `partial`: shown, but with columns cut by the display limit;
- `refused`: attributed to its cause.

A workflow is **usable** only if it computes (it matches its Rust twin) and displays fully both ways.

**Two display families:**

| family | what | v2 steps kept from displaying | v2 workflows kept from being usable |
|---|---|---:|---|
| **P. preview dtypes** | preview (record 0058, `preview.rs dtype()`) renders only String/Int64/Float64/Boolean; a Date, a count (UInt32) or a weekday shows "preview unavailable: unsupported column dtype" | 6 (`w1.csv_bytes`, `w2.null_count`, `w4.parse_date`, `w4.temporal`, `w5.lazy_group_by`, `w7.rank`) | W4 |
| **Q. preview columns** | preview shows the first columns up to its limit and omits the rest. Derived columns are appended last, so they are the ones cut, and a dtype it cannot show is never reached (Polars' own display elides the middle instead) | 2 | W7 |

**W7 shows why Q matters.** W7 computes, and its preview "displays", but the three window columns it computed (`region_qty`, `running`, `rank`) are exactly the ones cut: "[0 rows and 3 columns omitted by display limits]". The `rank` column is UInt32, which P would refuse, but the preview never reached it. The check therefore counts a column-cut display as `partial`, not usable.

**`rnx eval` does not present.** Even with the presenter registered, `rnx eval` shows a bare frame as `<::polars::DataFrame>`, and so does the plain `rnx-polars` binary, which registers no presenter at all (only `rnx-polars-fixture` and a `:dep polars` session do). This is reported, not ranked, because the probe's routes are print and the session.

## Review round 1 (Codex)

- **R1 (blocking): the carrier was not proven to instantiate the impl's owner.** The positive self-test even paired Schema's `Debug` with `Column`. Validation now requires the exact structural instantiation above. There is a wrong-carrier control on both real releases, and the self-test's former positive pairing is now a refusal. Generated output is unchanged at both pins.
- **R2 (contract): the plan promised a `ConversionError`.** The plan now specifies the VM argument-type refusal and why, and the test asserts that exact boundary.

## Review round 2 (Codex)

**R2 accepted. One R1 gap remained:** the open-parameter check compared each carrier argument as a whole, so `Schema<Vec<Field>, ()>` passed with `Field` left open inside `Vec`. `carrier_instantiates` now walks every nested type (paths, references, slices, tuples and `impl` bounds) for an impl parameter. The self-test adds nested controls (`Vec<Field>`, `Option<Metadata>`, `&Field`, a tuple holding `Metadata`, `[Field]`), each refused. The `SchemaRef` row and generated output are unchanged at both pins.

## The numbers

| | computed steps | computed composed workflows | frames displayed both ways / partial | usable workflows |
|---|---|---|---|---|
| v2, before (0122) | 31 of 39 (7 blocked) | 3 of 8 | 21 / 3 of 31 | **W8** |
| v2, after | 33 of 39 (5 blocked) | 5 of 8 | 24 / 3 of 34 | **W2, W3, W8** |
| 0.55.2, after | 24 of 39 (14 blocked) | 2 of 8 | 19 / 2 of 23 | W2, W3 |

The v2 ranking of what remains:
- **display:** P (6 steps, W4), then Q (2 steps, W7);
- **computation:** G (eager group-by, W5), then B, C and D (W1 needs all three), then H and I (pivot needs both).

## Tests, suites and launch

- **Generator:** 51 of 51, with the new `into_arguments` and `protocol_instantiations` self-tests, and the freeze widening control.
- **`tests/schema_and_cast.rs`:** 4 of 4 at both pins. At 0.55.2 only the `int_range` sub-check is skipped.
- **Production suites:** release default (19), release test-support (230) and debug generated plus test-support (231) all pass.
- **v2 build:** every stage ok; `oracle: cases_failed` is the standing state.
- **Launch** against 0122 (e16b445), default release builds, three rounds of 60 interleaved launches: median deltas +0.06, +0.19 and −0.05 ms, so no change (`launch-results-*.json`).

## Next

- **0124 candidates, from the display ranking:** P (preview dtypes) and Q (preview column selection). Both are small presentation changes, and together they would make W4 and W7 usable.
- **The `?` friction** remains out of scope, for its own design pass.
