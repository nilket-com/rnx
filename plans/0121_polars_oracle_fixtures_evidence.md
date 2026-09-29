# rnx 0121 evidence: oracle fixtures for the untested generated bindings

**All 363 v2 `fixture_failed` cases now run:**
- 322 match;
- 41 fail on both sides alike, for argument reasons that 0120's Int64 and Float64 siblings already show;
- none is a mismatch.

Every surface entry and binding is unchanged at both pins. Only oracle annotations and results moved.

Replay:
- `probes/0121/baseline.py` (the pool, from the committed 0120 results);
- `probes/0121/surface_gate.py` (the surface gate);
- `probes/0121/moved.py` (every moved case);
- `probes/0121/verify.sh` (all three against the committed evidence).

## Scoreboard

| | 0120 | 0121 |
|---|---:|---:|
| v2 available, of applicable | 4,115 of 6,421 (64.1%) | 4,115 of 6,421 (64.1%) |
| v2 value-tested | 2,750 (42.8%) | **2,793 (43.5%)** |
| v2 `fixture_failed` cases | 363 | **0** |
| 0.55.2 available / value-tested | 2,853 / 1,913 | 2,853 / **1,921** |

**The value-tested gain is smaller than proposed.** I estimated about 3,100; the count rose by 43. The scoreboard counts operations, not cases. Most of the 322 repaired cases are further receivers (Int128, UInt128, Float16) of operations already value-tested on another type. The cases still gain what the record is for: each of those bindings now has its own value comparison.

## The pool, reproducible

`baseline.py` over 0120's committed `oracle-results-v2.json.gz` gives exactly 363 cases (`baseline-v2.json`, byte-compared by Codex):
- 102 `i128`, 100 `u128`, 98 `f16` and 20 `decimal`;
- 18 `try_from_storage` (map), 14 `ext` (extension), 8 `deserialize_json_from_str` (a plan) and 3 `new` (fractions).

After this record, the same script over the new v2 results gives 0.

## What was repaired

**Typed source fixtures** (`TYPED_FIXTURES`): `series_i128`, `series_u128`, `series_f16` and `series_decimal`. Each casts before it is unpacked; an i64 series unpacked as Int128 was the failure. Each holds `[1, null, 3]` (or `[1.25, null, 3.5]`), with the null in the middle as `series_nulls`.
- They feed `Series::i128`, `u128`, `f16` and `decimal`, which exist only at v2.
- At 0.55.2 they compile and are never called. Decimal is spelled through `from_arrow_dtype`, because the `Decimal` variant does not exist at 0.55.2.

**Boundary values stay in the controls, not in the fixtures.** An i64 source cannot supply a value beyond i64 or an f16 subnormal (Codex). A fixture holding one would also turn most reads into a matched `ConversionError` instead of a value comparison. So the boundaries are exercised by `tests/oracle_fixtures.rs`, from string and float sources on both sides:
- `i128::MAX` and `u128::MAX` are exact in Rust and a checked `ConversionError` at the script boundary;
- 2⁻²⁴ in f16 reads back as exactly 2⁻²⁴ on both sides.

**Pin fixtures** (`PIN_FIXTURES`, new) are emitted only where the type is wrapped, with `{T}` standing for the wrapper's spelling. Each is built from public constructors with valid inputs, where the derived recipe had fed placeholder arguments the constructor rejects:

| fixture | built by | why the recipe failed |
|---|---|---|
| `map_chunked` | `MapChunked::try_from_storage(Map(String, Int64), List(Struct{key, value}))`, with field names from `map_entries_dtype()` | an i64 series is not map storage (`map.rs:64-76`) |
| `extension_chunked` | `ExtensionChunked::from_storage` of the generic type `get_extension_type_or_generic("rnx.oracle", Int64, None)` over Int64 | an extension unpacked from an i64 series |
| `extension_type` | the same generic type, on its own | `Series::into_extension`'s argument, and the type's protocols |
| `dsl_plan` | the df fixture's lazy plan | the generic string literal is not plan JSON |
| `fractions` | `Fractions::new(vec![0.25, 0.75])` | the generic float literal 1.5 is outside [0, 1] (`binning.rs:50-56`) |

**A departure from the plan: fixtures instead of a literal table.** The plan proposed a closed table of argument literals for the JSON and fraction cases. Every failing case needed a valid `DslPlan` or `Fractions` value, not the constructor call. A receiver fixture supplies that directly and needs no table, so none was added. Probing showed the smallest useful plan's JSON is about 1 KB of IPC bytes, too large for a literal anyway.

**Pin fixtures never feed recipe derivation.** `DslPlan` is wrapped at 0.55.2 too. The first build let its fixture join recipe derivation, and `Schema`'s recipe switched from `DataFrame::schema` to `DslPlan::compute_schema`. That would silently change old cases' inputs, and the surface gate caught it.

The fix: a pin fixture serves only a case's own receiver or argument. An `Oracle::deriving` flag hides pin fixtures while recipes are derived. The gate then passes, and 0.55.2's `DslPlan` cases gain a fixture (below).

## The surface gate (both pins)

`surface_gate.py` removes only an explicit allowlist of oracle fields, then compares both surfaces recursively; any other difference refuses. The allowlist:
- **top level:** `source` (the inventory file name), `oracle_cases`, `fixtures`, `oracle_skipped`;
- **entry:** `execution`;
- **binding:** `case_id`, `disposition`.

The first form of the gate selected fields to compare instead, so a moved binding `reentry` passed it (review, round 1). The self-test (`--self-test`, run by `verify.sh`) now checks:
- that allowed annotations pass;
- that a moved binding `reentry`, `route_reason` or `route`, a moved entry `bucket`, `kind`, `exceptions`, `note` or `status`, and a moved top-level field are each refused.

| | entries | moved | annotations changed |
|---|---:|---:|---:|
| 0.55.2 | 5,071 | 0 | 13 (`DslPlan` cases that had no fixture) |
| v2 | 6,992 | 0 | 160 |

The freeze is unchanged: 4,838 and 6,405 checked, none moved.

## Oracle movements (`moved.py`)

**v2 against 0120's committed results.**
- `fixture_failed` → `match`: 322.
- `fixture_failed` → `both_error`: 41.
- `match` → `row_order_differs`: 1 (`LazyFrame::full_join`, a listed unordered operation, run to run).
- No regressions and none removed.

**The 41 `both_error`s, by cause.** None is a mismatch or a product finding. Each errors alike on both sides, and each group already errors on 0120's Int64 or Float64 receiver of the same operation, except the null case.

| operations | cases | cause |
|---|---:|---|
| `add_to`, `subtract`, `multiply`, `divide`, `remainder` | 15 | the generic argument is the i64 series; a wider receiver refuses the dtype (the Int64 receiver matches, Float64 errors alike) |
| `quantile`, `quantile_reduce`, `quantiles_reduce` | 9 | the generic float literal 1.5 is outside [0, 1] (errors on Float64 and Int64 alike) |
| `broadcast_owned_to` | 3 | the generic length does not fit (errors on every width) |
| `arg_sort_multiple` | 3 | the generic `by` columns do not match (errors on every width) |
| `unpack_series_matching_type` | 3 | the generic series argument is i64 |
| `cont_slice` | 3 | **the fixture's null**: a slice cannot be taken over nulls. The Int64 and Float64 siblings (no null) match; this is the cost of the null the plan asked for |
| `map_get`, `map_contains_key` | 2 | the generic key series is i64 against String keys |
| `MapChunked`/`ExtensionChunked::cast_with_options` | 2 | the generic target dtype is not castable |
| `prepare_cloud_plan` | 1 | the plan fixture has no cloud sink |

Fixing these needs per-operation argument fixtures, the same pattern that affects the existing receivers; that is a possible later record. None is hidden, and none is counted as value-tested.

**0.55.2 against 0120's committed results (badc7a8).**
- 8 new cases, all `match`. They are the `DslPlan` cases that had no fixture before: `compute_schema`, `describe`, `describe_tree_format`, `Default`, `LazyFrame::from_logical_plan`, `From<DslPlan>`, `collect_all_with_engine` and `explain_all`.
- 2 unordered row-order flips (`LazyFrame::unique` and `unique_generic`, `row_order_differs` → `match`).
- No regressions and none removed. 0.55.2 had no `fixture_failed` cases.

## Controls (`tests/oracle_fixtures.rs`, both pins)

- **Typed sources:** each (i128, u128, f16) has its dtype, length 3 and one null in Rust. Unpacked through its producer in the script, it reads the same first and last values, with the null at index 1. Decimal is `Decimal(10, 2)` with its null. f16 is read through Float64, because `Float16Chunked::get` returns the opaque `pf16`.
- **Boundaries** from string and float sources, as above.
- **Pin fixtures, through the script:**
  - the map is `map[str, i64]` of length 1;
  - the extension chunk is `ext[rnx.oracle]` of length 3;
  - the extension type displays `rnx.oracle`;
  - the fractions equal themselves;
  - the plan collects to 3 rows, in Rust and through `LazyFrame::from_logical_plan`.
- **Pin behaviour:** at 0.55.2 the v2-only tests return early, the plan test runs, and the file compiles at both pins (4 of 4).

## Tests, suites and launch

- **Generator:** 49 of 49.
- **`tests/oracle_fixtures.rs`:** 4 of 4 at both pins.
- **Production suites:** release default (17), release test-support (224) and debug generated plus test-support (225) all pass.
- **v2 build:** every stage ok. `oracle: cases_failed` is the standing state from the 41 alike errors and the earlier ones.
- **Launch:** not measured, a departure from the plan's wording, because the release build cannot change. `verify.sh` checks this rather than asserting it:
  - every file under `adapters/polars/src` equals 0120's (badc7a8), except `src/generated/fixtures.rs`;
  - so do `Cargo.toml` and `Cargo.lock`;
  - `fixtures.rs` is compiled only with `test-support` (its `#[cfg]` in `generated/mod.rs` is checked);
  - the release binary is built from identical sources.

## Review round 1 (Codex)

**The surface gate was a field selection** (blocking proof gap). The reviewer changed an existing binding's `reentry` and the gate passed. It is now the allowlist-and-recursive comparison above, with mutation controls for binding, entry and top-level fields and an allowed annotation change. Both pins pass it unchanged (0 moved). The entry-level `oracle` field is not on the allowlist; it did not move.

**Launch was omitted.** It is now a checked claim in `verify.sh`: the release build's sources equal 0120's.
