# rnx 0125: loading, from CSV inference, JSON files and plain-string paths

Status: plan. Record 0124 is closed on `origin/main` at `40f0334`. The user chose loading as one workflow record:
- CSV schema inference;
- JSON reading;
- plain-string file paths.

**The measure (the user's):** W1 runs from input to a displayed result, checked against Rust. It is measured at v2, where the Rust twins are.

**Constraints (the user's):**
- The known 0.55.2 crash in automatic date parsing stays an explicit constraint. No loader in this record turns date parsing on.
- If a loading fix needs a new ownership or trait model, the record splits at that boundary.

## Where W1 stands (0124's probe, v2)

W1 is "load each format and combine". 4 of its 7 steps work: the explicit-schema CSV, CSV bytes, the Parquet file and Parquet bytes. The composed W1 is blocked by all three loading families:

| family | step | the refusal today |
|---|---|---|
| **B**, CSV inference | `polars::read_csv(path)` | "Wrong number of arguments 1, expected 2": the hand-written loader takes only an explicit `(name, dtype)` schema |
| **C**, JSON | `polars::JsonReader::new(bytes)?.finish()?` | "Missing item ::polars::JsonReader::new" |
| **D**, paths | `LazyFrame::scan_parquet(path, args)` | "Expected type `::polars::utils::PlRefPath` but found `::std::string::String`" |

## 1. CSV schema inference (family B)

`polars::read_csv(path)` with one argument reads the file with **Polars' own schema inference**, `CsvReadOptions::default().with_has_header(true)`: the default inference length, and date parsing **off**. So a date column stays a string, as it does in Polars by default.

- **The two-argument form is unchanged.** `read_csv(path, schema)` stays the strict loader: the header must match the schema's names and order.
- **Mechanism.** Rune has no optional arguments, but 0.14.2's `Module::raw_function` receives the argument count. `read_csv` becomes one raw function: one argument infers, two are the strict schema, and any other count is refused, naming both forms. The catalogue entry becomes `read_csv(path[, schema])`. This is host code; no generator change.
- **Tests** (review of the plan):
  - the one-argument form infers;
  - the two-argument form is byte-for-byte today's strict loader;
  - zero and three arguments are refused, naming both forms;
  - every existing strict-schema refusal is unchanged: header arity, names and order, a non-regular file, an unsupported dtype.
- **The same host rules as today:**
  - a local regular file only (`files::regular`);
  - the read runs on the engine thread;
  - the result passes the same `validate`.

  `validate` admits the four dtypes 0058 shipped, and CSV inference without date parsing yields only those four. Whether `validate` should now admit every dtype, since the preview shows them all after 0124, is a separate question and is reported, not changed here.

## 2. JSON (family C): a hand-written file loader, and the generated reader split off

**The split.** The generated `JsonReader` is `JsonReader<'a, R>`. Its lifetime exists only for `schema_overwrite: Option<&'a Schema>` (polars-io `json/mod.rs`, at both pins). 0118 refused lifetime-bearing owners by name, and binding it needs a lifetime-instantiation model. That is a new ownership model, so per the user's rule it is split out, and `w1.json_bytes` stays blocked, attributed to it (family **C2**, `JsonReader` lifetime owner).

**The loader (family C1).** `polars::read_json(path)` is a hand-written file loader beside `read_csv` and `read_parquet`, with the same host rules:
- a local regular file only;
- on the engine thread;
- the same `validate`;
- Polars' own `JsonReader::new(file).finish()` with default options: a JSON array of objects, schema inferred by Polars.

It needs no new model.

**0.55.2.** The production adapter's `polars` dependency has no `json` feature; the v2 scratch has it, through the wheel's feature set. This record adds `json` to the production dependency, so `read_json` works for users of the shipped build.

**The inventory follows the build (review of the plan, R1: 0081's exact-feature invariant).** The shipped inventory and its provenance must describe the build that ships. So this record repeats 0081's procedure:
- **A new documentation configuration, `adapter-json`** (`probes/0072/inventory/doc.sh 0.55.2 adapter-json`), with its own lock and output directory. It documents every activated Polars crate under the pinned nightly, and `pins.json` records the complete resolved feature set: 0081's 13, plus `json` and whatever `json` itself activates, listed exactly.
- **The release names the new configuration.** `0.55.2-joins.toml` names `cfg = "adapter-json"` and pins that complete resolved set under `[provenance]`. The generator compares it exactly, as it does today.
- **The old inventory is rejected, and proven.** 0081's retained-inventory tests move one configuration on:
  - the `adapter-narrow` inventory is refused by configuration;
  - the new inventory with a feature removed, or one added, is refused by feature set.

  The archived `adapter-narrow` baseline is kept.
- **Reconciliation.** Every row the `json` feature makes newly reachable is accounted for by name, generated or unsupported with its reason, and so is any movement of an existing row. Existing bindings stay frozen: none moves or changes contract. The surface and denominator are reported for the new build.
- **Cost.** Build time, binary size and launch are measured against 0124. The stop rule is below.

**W1's JSON line changes.** The composed W1 currently reads `polars::JsonReader::new(fs::read_bytes(path)?)?.finish()?`. It becomes `polars::read_json(path)?`, which is how a Rune user loads a JSON file, as with `read_csv` and `read_parquet`. Its Rust twin stays `JsonReader` over the same file, so the result is still checked against Rust. The Rust-shaped `w1.json_bytes` step stays in the probe, blocked by C2. A new step `w1.json_file` covers the loader.

## 3. Plain-string paths (family D)

**The gap.** Polars takes a file path as `polars_utils::pl_path::PlRefPath`. The generator wraps it as `polars::utils::PlRefPath`, and the path-taking operations are already generated, but a script has no way to build one: `PlRefPath`'s constructors aren't bound. (Implementation: this corrects the plan's first draft, which said the type isn't wrapped.) `PlRefPath` has `impl From<&str>` (`pl_path.rs:398` at both pins).

**The rule: a closed, cited table (`[[path_arguments]]`).** A by-value `PlRefPath` parameter at depth 0 of a listed callable accepts a script string. The string is converted with Polars' own `<PlRefPath as From<&str>>::from`.
- **The same scalar-mapping mechanism** as `String` and `PlSmallStr` (`SCALAR_MAPPED`), restricted to the listed callables. There is no new ownership or trait model: the value is built at the boundary and moved in.
- **Candidate rows** (the exact set is established by generation, and fail closed):
  - `LazyFrame::scan_parquet` and `LazyCsvReader::new` at both pins;
  - `LazyFrame::scan_ipc` and `LazyJsonLineReader::new` at v2, where they are in the inventory.
- **Excluded by name:**
  - a collection of paths (`scan_parquet_files`, `new_paths`, `with_paths`);
  - a borrowed or nested `PlRefPath`;
  - every writer and filesystem utility that takes one (`Writable::try_new`, `mkdir_recursive`, the cloud and object-store builders).
- **Local paths only, checked after conversion** (review of the plan, R2). `PlRefPath::from` normalises its input: it rewrites Windows path spellings, including an extended-path prefix, even on Unix. So a raw check can pass a string that the conversion then turns into `s3://…`. And Polars' `CloudScheme` also recognises `file:` without `//`. So there are two checks, both before any Polars call:
  1. the raw string is refused if it has a URI scheme (`scheme:` or `://`), which is the declared policy;
  2. the **converted** `PlRefPath` is refused if Polars itself sees a scheme: `PlPath::has_scheme()` (`pl_path.rs:131`, the same at both pins), whose `CloudScheme` includes `File`. So `file:` is refused too, and a local path is whatever Polars reads as scheme-free.

  The refusal is a VM argument error, as in 0123, naming the operation, the argument and the reason. No binding's fallibility changes. Cloud and HTTP reads are out of scope, because they would give a script network access. A local glob is Polars' own behaviour and is kept.
- **The freeze.** A listed binding moves from `unsupported` to `generated`. Nothing already bound changes.

**Proof.** `scan_parquet("<path>", ScanArgsParquet::default())?.collect()?` equals Rust's `scan_parquet(PlRefPath::from(path), …)`. There are also controls, at both pins:
- **direct URIs:** `s3://`, `gs://`, `az://`, `http://`, `https://`;
- **`file:` variants:** `file:///x`, `file:/x`, `file:x`;
- **normalisation to cloud:** an extended-path-prefixed, backslash-spelled cloud path that `PlRefPath::from` turns into `s3://`;
- **ordinary local paths:** absolute, relative, a path with a colon inside a later segment, and a glob. These are accepted.

Every refused case is refused before Polars, with the reason. In addition:
- an unlisted `PlRefPath` callable stays refused;
- a non-string argument is a VM type error.

## The probe (`probes/0125`, from 0124's)

- **`w1.csv_file`'s twin becomes real Rust inference** (`CsvReadOptions::default().with_has_header(true)` over the file). It was the explicit-schema `sales()`, which only happens to agree.
- **New step `w1.json_file`:** `polars::read_json(path)`, whose twin is `JsonReader` over the file.
- **W1** uses `read_json`, as above, in the before and the after replays alike, so both run the same recipe against the same Rust twin (review of the plan). The fixed families are B, C1 and D. C2 stays open, and the original `w1.json_bytes` step stays blocked and counted.
- **Expected at v2:** `w1.csv_file`, `w1.json_file` and `w1.scan_parquet` work and display, and the composed W1 computes, matches its twin and displays. W1 joins the usable workflows: W1, W2, W3, W4, W7, W8.
- **At 0.55.2:** W1 still stops at the date-parsing crash, because its CSV-bytes line turns on `try_parse_dates`. This is the user's explicit constraint. It is recorded as W1's 0.55.2 outcome with the crash's own text, and not worked around.
- **Replay:** before and after at both pins, as in 0124.

## Gates

- **Freeze:** every 0124 binding is unchanged. The only generated movement is the listed path rows (unsupported → generated). The frozen lists are rebuilt from 0124's surface.
- **Probe:** as above, with the before and after tables replayable.
- **Oracle:** no mismatches. The path rows get fixtures over a temporary Parquet file where the oracle allows; otherwise they are covered by the adapter test above.
- **Suites and launch:** the usual suites, debug last. Launch is measured against 0124, with the `json` feature's cost reported separately (build time, binary size, launch).

## Stop rules

- **A loading fix turns out to need a new ownership, lifetime or trait model** (for example, a `PlRefPath` parameter whose conversion isn't a by-value `From<&str>`): split that row out and name it.
- **The `json` feature costs launch beyond run-to-run noise:** stop and report before shipping `read_json` at 0.55.2. The v2 measure doesn't depend on it.
- **Anything would turn date parsing on by default:** stop (the 0.55.2 crash constraint).

## Out of scope, reported

- The generated `JsonReader` (C2, a lifetime-owner model), and path collections.
- Cloud and HTTP reads.
- `validate`'s four-dtype rule for the hand-written loaders.
- The 0.55.2 date-parsing crash itself.
- `rnx eval` presentation.
- The `?` friction.
