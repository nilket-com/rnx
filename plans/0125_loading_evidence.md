# rnx 0125 evidence: loading, from CSV inference, JSON files and plain-string paths

**The measure is met.** At v2, W1 ("load each format and combine") runs from input to a displayed result, checked against its Rust twin. The usable workflows go from W2, W3, W4, W7, W8 to **W1, W2, W3, W4, W7, W8**, and every computed frame displays both ways (38 of 38).

**What a user can now write:**
- `polars::read_csv(path)`: Polars' own schema inference, dates kept as strings;
- `polars::read_json(path)`: a local JSON array of objects;
- `polars::LazyFrame::scan_parquet("data/sales.parquet", …)`: any listed Polars path parameter takes a plain local path string. A URI is refused before Polars, by name.

**At 0.55.2,** the shipped build, the three loaders work and display too. W1 stops exactly at the user's constraint: its CSV-bytes line turns on `try_parse_dates`, and Polars 0.55.2 panics in CSV schema inference.

**Replay:** `probes/0125/replay.sh` runs the Rust twins at v2 and builds the before state from 0124's tree (`40f0334`: its generator, releases and adapter source) and the after state at both pins. Both states run the same W1 recipe, and every table is byte-compared with the committed ones.

## 1. CSV schema inference (family B)

- `read_csv` is one Rune `raw_function` that dispatches on the argument count:
  - **one argument:** `files::csv_inferred`, which is `CsvReadOptions::default().with_has_header(true)` with `with_try_parse_dates(false)` set **explicitly**, so the 0.55.2 constraint is visible in the code;
  - **two arguments:** today's strict loader, unchanged;
  - **any other count:** a VM error, "read_csv takes (path) or (path, schema), found N arguments".
- The same host rules as before: a local regular file only, the read on the engine thread, and the same `validate`.
- **Tests** (`tests/loading.rs`, 7 of 7 at both pins with review round 1's control):
  - inference gives `i64, str, str, i64, f64, bool`, with the date a string;
  - the two-argument form reads the same file;
  - every strict refusal is unchanged: header arity, names and order, a non-regular file;
  - zero and three arguments are refused, naming both forms.
- **The twin:** the probe's `w1.csv_file` twin is now Rust's own inference over the file. It was the explicit-schema `sales()`, which only happened to agree.

## 2. JSON (families C1 and C2)

**The split, at the user's model boundary.** The generated `JsonReader` is `JsonReader<'a, R>`, whose lifetime exists only for `schema_overwrite: Option<&'a Schema>`. Binding it needs a lifetime-instantiation model, so it stays refused, and `w1.json_bytes` stays blocked and counted as C2.

**C1, `read_json(path)`,** is a hand-written loader beside `read_csv` and `read_parquet`: Polars' `JsonReader::new(file).finish()` with default options, the same host rules and the same `validate`. The catalogue documents it.

**W1's JSON line** is `polars::read_json(path)` in both the before and the after replays. Its twin is still Rust's `JsonReader` over the same file (review of the plan: an honest split).

### The `json` feature in the shipped build, and the inventory that follows it (review of the plan, R1)

- **The dependency.** `polars` gains `json`, which also resolves `dtype-struct`. The resolved set goes from 0081's 13 features to 15, and one crate, `polars-json`, is added. The adapter lock change is additive only: 13 new crates, and no existing crate's version moves.
- **Cost, measured on the same source with and without `json`,** default release builds, three rounds of 60 interleaved launches:
  - launch median deltas of +0.37, +0.22 and −0.10 ms, so no change;
  - binary 172.2 → 173.7 MB (+1.49 MB, +0.9%);
  - release build 5 min 54 s → 7 min 6 s.
- **The inventory.**
  - `probes/0072/inventory/doc.sh 0.55.2 adapter-json` is a new configuration with its own lock (`locks/0.55.2-adapter-json.lock`, sha256 `bda34cef…`). It documented all 25 activated Polars crates, with none failed.
  - The extracted inventory (sha256 `7fe4ce66…`) has 6,443 callables: 78 added, none removed, compared by canonical path and kind.
  - The added callables come from `polars_arrow::array` (26, `StructArray`), `polars_io::ndjson` (18), `polars_lazy::scan` (14), `polars_io::json` (11), `polars_plan::dsl` (7) and 2 more.
- **The release** names `cfg = "adapter-json"` and pins the 15-feature resolved set. `tests/generated.rs` moves one configuration on:
  - the shipped policy expects `adapter-json`;
  - the `adapter` and `adapter-narrow` inventories are both refused by configuration;
  - the new inventory with `lazy` removed, or `dtype-i128` added, is refused by feature set.

  18 of 18 pass, drift included.

### Reconciliation: every new row, and every movement

**Rustdoc renumbering (found in implementation).** A callable's `key` is a rustdoc item id, and rustdoc renumbers items when a crate gains modules. 1,127 unchanged callables got new keys, so the freeze reported 693 frozen bindings as "moved or disappeared" although none had moved. This is the first inventory change since the freeze existed (0119).

The fix translates keys, and leaves the freeze's checks alone:
- **`probes/0125/rekey.py`** maps each old key to the new key of the callable with the same identity. The identity is every inventory field that describes the item (path, kind, owner, impl head and target, trait arguments, generics and bounds, parameters, return, receiver). It must be unique in both inventories, or the run refuses. A keyed release row's `path` must be the mapped callable's path.
  - Result: all 5,074 frozen bindings mapped one-to-one, and exactly the 693 that "moved" were translated.
  - The 40 keyed release rows needed no translation, because their crates weren't renumbered.
- **Ids that embed a key.** 11 bindings carry the old key numbers in their id: operator rows (`…_on__selector_selector_polars_plan_630_6881`) and `from_iter_self`. `pin_frozen_ids` (0120) kept frozen ids only on the instantiation path. `emit::pin_frozen_everywhere` now runs once before the freeze, on every path: a binding whose (key, Rune path) is frozen takes its frozen id, and a pinned id that another binding holds is refused. The generator prints "frozen ids: 11 restored after rustdoc renumbering". The ids are internal, with no Rune path or contract change. Self-test `pin_everywhere_self_test` covers the restore and the collision refusal.

**`StructArray` at 0.55.2.** With `dtype-struct` resolved, `StructArray` has inventory rows at 0.55.2 for the first time; 0120 had noted "no inventory row at this pin". Generation refused it fail-closed ("a required receiver guard is missing"). It is now listed as 0120 lists it at v2, with 0.55.2's own citations:
- the concrete-array row;
- the `sliced` guard `range_len_all` (`polars-arrow 0.55.2 array/mod.rs:464-471`, `impl_sliced!` used at `struct_/mod.rs:202`);
- the validity allowlist entry.

`tests/concrete_arrays.rs`'s `struct_fields_are_owned_arrays` asserted the array absent at 0.55.2. It now runs v2's real assertions at both pins: length 3, 2 field arrays, a string field, and `sliced(1, 3)` refused with `OutOfBounds`. 11 of 11 pass.

**The 0.55.2 surface.** The frozen lists (`probes/0125/frozen-*.json`) are rebuilt from 0124's surface with `probes/0119/frozen.py`, and re-keyed at 0.55.2. 0123's `[[frozen_widenings]]` (6 at 0.55.2, 12 at v2) are removed, because the rebuilt lists already hold the widened contracts, and the freeze refuses an unused widening.

| 0.55.2 | 0124 | 0125 |
|---|---|---|
| generated | 2,843 | 2,884 (+41) |
| adapted | 430 | 436 (+6) |
| unsupported | 1,419 | 1,448 (+29) |
| out of scope | 379 | 379 |

- **No existing entry changed status.** All 76 new entries are new rows, each with its status and reason in `surface.json`.
- **The 41 generated:**
  - 6 `StructArray` methods and its 3 protocols;
  - `LazyJsonLineReader` (`new` and 9 builder methods, plus `CLONE`);
  - the NDJSON option types' protocols (7 on `NDJsonWriterOptions`, 4 on `NDJsonReadOptions`);
  - `StructNameSpace::json_encode`, `LazyFrame::unnest`;
  - 8 `polars_io` JSON and NDJSON functions (`remove_bom`, `parse_ndjson`, `count_rows` and others).
- **Freeze:** "frozen bindings: 5074 checked, none moved, no contract changed except 2 listed widenings" (the path widenings, section 3).

The v2 surface is unchanged in counts: generated 4,105, adapted 582, unsupported 1,906, out of scope 399. Its inventory is unchanged, and no ids needed restoring.

**Oracle at 0.55.2:** 3,967 → 3,997 cases.
- **The 30 new cases:** 29 match, and 1 `both_error` (`parse_ndjson`: both sides fail with `ComputeError` on the shared fixture).
- **No existing case changed status.**
- **One existing detail moved.** `StructFunction`'s `Display` fixture now shows `struct.to_json`, not `struct.field_by_name(x)`: `json` adds a `JsonEncode` variant, which the fixture now picks. It still matches.
- The standing unordered `unique` flips were restored.

## 3. Plain-string paths (family D)

**The rule, `[[path_arguments]]`** (`families/path_arguments.rs`, a callable-scope family on the 0115 registry, beside 0086's `bitmap_input`):
- **Validated fail-closed:** `PlRefPath` wrapped; `From<&str>` recorded for it; each row cited and unique; its callable present exactly once, with the named parameter taking `PlRefPath` **by value**. A listed row whose callable is not generated is refused after generation.
- **The rows:** at 0.55.2, `scan_parquet`, `LazyCsvReader::new` and `LazyJsonLineReader::new`; at v2, the same three plus `scan_ipc`. Each is cited to its pin's source line.
- **The argument:** one Rune value, dispatched before any Polars call. A wrapped `PlRefPath` is cloned, and a string goes through Polars' own `From<&str>`; both then pass the local check. Anything else is a VM error naming the qualified operation, for example "LazyFrame::scan_parquet: `path` must be String or PlRefPath, found ::std::i64". No binding's fallibility changes.

**Local only, checked before and after conversion** (review of the plan, R2; `support::local_path`):
1. **The raw string:** refused if it contains `://`, or starts with a `scheme:` prefix of two or more scheme characters. A Windows drive (`C:`) stays a path.
2. **The converted `PlRefPath`:** refused if Polars' own `has_scheme()` is true. Its `CloudScheme` includes `File` and `FileNoHostname`.

**Controls** (`tests/loading.rs`, both pins):
- **direct URIs**, refused by check 1: `s3://`, `gs://`, `az://`, `http://`, `https://`, `file:///x`, `file:/x`, `file:x`;
- **normalisation to cloud:** the test first asserts that `\\?\s3:\\bucket\x.parquet` becomes `s3://bucket/x.parquet` through `PlRefPath::from` at the pin, then that the script is refused by check 2 ("Polars reads it with a URI scheme"). The same holds for `\\?\file:\x.parquet`;
- **ordinary local paths reach Polars** (review of the plan: include Windows drive paths): `/no/such/x.parquet`, `no/such/x.parquet`, `dir/a:b.parquet`, `dir/*.parquet`, `C:/data/x.parquet`, `C:\data\x.parquet`, `\\?\C:\data\x.parquet`;
- **a real read:** `scan_parquet` over a temporary Parquet file collects 2 rows. `LazyCsvReader::new(path)` takes a string, and `LazyJsonLineReader::new("s3://…")` is refused by name.

**Freeze.** The contracts widen from `PlRefPath` to `String or PlRefPath (a local path)`:
- 2 at 0.55.2 (`scan_parquet`, `LazyCsvReader::new`; `LazyJsonLineReader::new` is new there);
- 4 at v2.

Each is listed exactly in `[[frozen_widenings]]`, generated from the freeze's own messages.

**Generator:** 52 of 52 tests. The self-test `path_arguments_self_test` covers:
- validation;
- the refusals: no `From<&str>`, a borrowed parameter, a missing parameter or callable, an uncited row, a duplicate, an unused row;
- the dispatch code.

0115's `hook_orders_are_pinned` names the new family in `CALLABLE` and `ARG_TOP`.

## 4. The probe (`probes/0125`, from 0124's)

The changes:
- `w1.csv_file`'s twin is Rust's inference;
- a new step `w1.json_file` (`read_json`), with its twin `JsonReader` over the file;
- W1's JSON line is `read_json`;
- family C is split into C1 (`json_file`) and C2 (`json_reader_lifetime`).

The committed twins differ from 0124's by exactly the new `w1.json_file` line. A fresh run agrees through the probe's own normalisation, which covers row order and the `std` digits, as in 0122.

| pin, state | steps (works / blocked, + presented) | workflows compute | frames displayed both ways | usable workflows |
|---|---|---|---|---|
| v2, before (0124) | 33 / 6, +1 | 5 of 8 | 34 of 34 | W2, W3, W4, W7, W8 |
| v2, after | **36 / 3, +1** | **6 of 8** | **38 of 38** | **W1, W2, W3, W4, W7, W8** |
| 0.55.2, before | 24 / 15, +1 | 2 of 8 | 23 of 23 | W2, W3 |
| 0.55.2, after | 27 / 12, +1 | 2 of 8 | 26 of 26 | W2, W3 |

At 0.55.2 there are no Rust twins, so "works" means the step ran.

**v2, what flipped:** `w1.csv_file` (B), `w1.json_file` (C1), `w1.scan_parquet` (D) and the composed W1, each computing, matching its twin and displaying both ways. `w1.json_bytes` stays blocked by C2.

**0.55.2:** `w1.csv_file`, `w1.json_file` and `w1.scan_parquet` work and display. `w1.csv_bytes` and W1 stop at "thread 'rnx-polars-engine' panicked at … polars-io-0.55.2/src/csv/read/schema_inference…": the date-parsing crash, the user's constraint, recorded and not worked around.

**The remaining v2 ranking:** G (eager group-by: `w5.eager_group_by` and W5), then C2 (`w1.json_bytes`), then H and I (pivot).

## Review round 1 (Codex)

**R1 (blocking): script path strings were consumed.** `read_csv_either` and the generated path dispatch extracted the path with `rune::from_value::<String>(value.clone())`. `Value::clone` shares the payload, so the extraction **took** the string out of the script's binding. After `polars::read_csv(p)` or `scan_parquet(p, …)`, the script's `p.len()` failed with "Cannot read, value is M-000000". This also regressed the old two-argument `read_csv`, which borrowed `&str`. It is the defect class that 0055 fixed for `http` (a native function taking an owned `String`).
- **The fix:** both sites borrow, with `Value::borrow_string_ref()`, and copy from the borrow. `values::schema` already borrowed. Regenerating changed exactly the 3 path bindings' dispatch at 0.55.2.
- **The generator's self-test** now asserts that the dispatch borrows and never uses `from_value::<String>`.
- **Control `a_path_binding_survives_every_call`** (both pins): after each call the script reads its path again, and the lengths add up. It covers:
  - both `read_csv` forms on success and on a missing file;
  - `scan_parquet` on success and on a missing file, whose Polars error comes at `collect`;
  - `LazyCsvReader::new`.

  A URI refusal is a VM error that ends the script, so there is no later use to observe. Its dispatch borrows on the same path, which the self-test pins.
- **Mutation:** with the consuming extraction restored in `read_csv`, the control fails with Codex's exact message, "Cannot read, value is M-000000".

**R2 (proof): the rekey identity omitted semantic fields.** `rekey.py` listed its identity fields and left out `impl_assoc` and `trait_lifetimes` (the latter present on only 84 callables). A changed associated type or lifetime could have mapped as "unchanged". Codex's comparison found no such movement in the real inventories, and neither did the rerun: the mapping is identical.
- **The fix:** the identity is now every recorded field except a closed, commented `NOT_IDENTITY` list:
  - numbering: `key`;
  - docs: `docs_first`;
  - reachability, which a feature may extend: `found_paths`, `crate_paths`, `owner_aliases`, `trait_reachable`, `implementors`;
  - the classifier's opinions: `bucket`, `rules`, `decided_by`.

  So any later field counts by default. Attributes such as `deprecated`, `hidden` and `unstable` are identity, the stricter choice.
- **`probes/0125/rekey_check.sh`** (run by `replay.sh`) replays the rekey from the retained inputs: the frozen list rebuilt from 0124's surface and catalogue at `40f0334`, and the two inventories. The result equals the committed `frozen-0.55.2.json` byte for byte.
- **`probes/0125/rekey_controls.py`**, on synthetic inventories:
  - a renumbered callable maps, even with a new re-export;
  - drift in `impl_assoc`, in `trait_lifetimes`, or in a hypothetical new field is refused ("0 new callables with its identity");
  - an ambiguous identity is refused on the old side and on the new side;
  - a keyed release row naming another path is refused.
- **Mutation:** against the committed fixed-list `rekey.py`, the `impl_assoc` control fails, because the changed callable was silently mapped ("1 keys translated").

## Suites and launch

- **Production suites (0.55.2):** release default (33), release test-support (244) and debug generated plus test-support (245) all pass, after review round 1. The first run failed only `struct_fields_are_owned_arrays`, the expectation 0120 wrote for an absent `StructArray` (section 2).
- **v2 build (`probes/0108/build.sh`):** every stage ok; `oracle: cases_failed` is the standing state; `no_default` ok. `tests/loading.rs` passes 7 of 7 at v2.
- **Launch against 0124 (`40f0334`),** default release builds, three rounds of 60 interleaved launches (`launch-results-*.json`): median deltas of +0.40, +0.16 and +0.42 ms.
  - All three are positive, about +0.3 ms (roughly 1.5%), inside the run-to-run p90 spread (28.6 to 33.7 ms) but consistent in sign, so this is reported as a small increase, not as no change.
  - The `json` feature alone measured +0.37, +0.22 and −0.10 ms (section 2). The rest is consistent with the 41 new generated bindings, which register at startup.
  - Binary: 172.2 → 175.3 MB (+3.2 MB, +1.8%), of which `json` is 1.49 MB.

## Next

- **G, the eager group-by,** is the top computation family (one step and W5).
- **C2,** the `JsonReader` lifetime model, and path collections (`scan_parquet_files`, `new_paths`), each need a model record of their own.
- **Reported, unchanged:** `validate`'s four-dtype rule for the hand-written loaders (the preview shows every dtype since 0124); `rnx eval` presentation; the `?` friction.
