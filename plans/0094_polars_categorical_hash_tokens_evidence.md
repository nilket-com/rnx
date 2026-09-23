# 0094 categorical hash tokens: evidence

Plan `d42f4b0`. Five categorical hash bindings now carry the full `u64` as exactly 16 lowercase hexadecimal digits. The three producers return a token, and the two consumers take one and pass Polars the parsed bits unchanged. Every other `u64` stays on the 0093 checked integer rule. Operation and binding counts are unchanged, and 17 categorical operations become value-tested because their receivers now have fixtures.

## Gate 1: scope and source

`probes/0094/targets.json` holds the five targets. For each it records key, path, direction, the exact parameter and return types, the hash domain, the binding before and after, the oracle status and the citation.

| Key | Method | Before | After |
|---|---|---|---|
| `polars_dtype:309` | `Categories::hash` | `Result<i64>` | `String` |
| `polars_dtype:377` | `FrozenCategories::hash` | `Result<i64>` | `String` |
| `polars_dtype:33` | `CategoricalMapping::cat_to_hash` | `Result<Option<i64>>` | `Result<Option<String>>` (the `cat` narrowing stays fallible) |
| `polars_dtype:27` | `CategoricalMapping::get_cat_with_hash` | `hash: i64` | `hash: &str` |
| `polars_dtype:30` | `CategoricalMapping::insert_cat_with_hash` | `hash: i64` | `hash: &str` |

Pinned polars-dtype 0.55.2 has two hash domains:

- **Stored hashes.** `Categories::hash` is `PlFixedStateQuality::default().hash_one(&self.id)` (`categorical/mod.rs:188-190`). `FrozenCategories::hash` returns `combined_hash`, a fixed-state hasher fed every string (`mod.rs:249-257`, returned at `309-311`). `cat_to_hash` returns the hash stored at insert time, again `PlFixedStateQuality` (`mapping.rs:85`, read at `115-117`).
- **Lookup hashes.** `get_cat_with_hash` and `insert_cat_with_hash` (`mapping.rs:60-62`, `73-92`) index the table under the caller's hash. `get_cat` and `insert_cat` pass `self.hasher.hash_one(s)`, the mapping's `PlSeedableRandomStateQuality` (`mapping.rs:53-56`, `66-69`).

So a `cat_to_hash` token is not a valid lookup hash. The docs say so, and a test shows it. Polars files an insert under whatever hash it receives, so a wrong but well-formed hash gives a string a second id. That is Polars' behaviour, and the binding reproduces it without a guard.

**Fixtures.** Each receiver fixture is built by a safe public path in `support::categorical_fixtures`, under the `test-support` feature:

- `Categories::new("rnx-0094-5", "rnx", U32)` and `FrozenCategories::new(["rnx-0094-5", "b"])` come back as `Arc`s. Their registries keep only weak references, so `Arc::try_unwrap` succeeds. A static lock around build and unwrap stops two concurrent oracle threads from sharing one registry `Arc`.
- The mapping is `CategoricalMapping::with_hasher(16, PlSeedableRandomStateQuality::seed_from_u64(94))` holding `rnx-0094-2` (id 0) and `rnx-0094-1` (id 1). The fixed seed makes separate instances compute identical lookup hashes, so Rust can pair with the script.

The names were chosen by a compile-time probe so that every producer returns a hash with its high bit set:

| Value | Token |
|---|---|
| `Categories::hash` | `e6acb8345c472361` |
| `FrozenCategories::hash` | `908462c41b6455c3` |
| `cat_to_hash(0)` | `c6c4167f8d117844` |
| `cat_to_hash(1)` | `32195c9114110703` (lower half, keeps its leading zeros) |
| lookup hash of `rnx-0094-2` | upper half |

The retained 0093 binary's SHA-256 is `455cb5be71c2fb120ad0faf50ea4654947a98da08f0820dc9c52baa9c5413650`.

## Gate 2: exact gate and codec

The release file has five `[[hash_tokens]]` entries. Each gives inventory `key`, canonical `path`, `direction` (`return` or `parameter`), the parameter name, the `source` type (`u64` or `Option<u64>`) and a citation. `hash_token_scope` validates every entry matching the callable's key and path before any binding text is emitted, and any mismatch refuses the callable as release policy. The scope is set in both `emit_callable` and `emit_method_with`, and the shared drop guard clears it.

- **Returns.** `support::hash_token` is `format!("{v:016x}")`. It is infallible, so both `hash` bindings dropped their `Result`.
- **Parameters.** `support::hash_from_token` runs in the binding's pre-call step, outside any engine closure. It accepts exactly 16 ASCII bytes `0-9a-f` and folds them into the `u64`. Anything else is a `ConversionError`, for example `get_cat_with_hash: hash must be 16 lowercase hex digits, got "…"`. The echo is truncated at 24 characters.

`hash-token self-test` covers the following:

- **The four shipped shapes.** A `u64` return, an `Option<u64>` return, and a `hash` parameter on `get_cat_with_hash` and `insert_cat_with_hash`. For each consumer it checks that parsing comes before the Polars call and that the parsed value reaches Polars with no `narrow`.
- **Unlisted methods stay checked.** A `hash` whose path is listed under another key stays on `support::widen::<u64>`. So does an unlisted `hash` on another owner.
- **Malformed entries are refused.** Each of the following is refused with the reason named and no binding text:
  - a `u32` return listed as `u64`
  - a `u64` return listed as `Option<u64>`
  - a `u32` parameter
  - a missing parameter
  - a blank citation
  - an unknown direction
  - a return entry that names a parameter
- **Scope.** The scope is clear afterwards.

The shipped `functions.rs` differs from 0093 in exactly five bindings. The count of `widen::<u64>` sites falls from 29 to 26, which are the three producers.

## Gate 3: bits and behaviour

The support unit test `hash_tokens_are_exact_and_strict` round-trips a set of values through the codec. They include `0`, `1`, `i64::MAX`, `i64::MAX + 1`, `u64::MAX` and a mixed value, so tokens include `0000000000000000`, `8000000000000000` and `ffffffffffffffff`. It rejects twelve malformed spellings: empty, 1, 15 and 17 digits, uppercase, `0x`, `+`, `-`, a space, `g`, `é` and a full-width digit. It also checks the truncated echo.

`tests/hash_tokens.rs` compares the script with direct Polars on separate fixture instances:

| Test | Script result, equal to direct Rust |
|---|---|
| Producers | `e6acb8345c472361 908462c41b6455c3 c6c4167f8d117844 32195c9114110703 none` (`cat_to_hash(5)` is `None`) |
| Lookups under the lookup hash | existing `rnx-0094-2` and `rnx-0094-1` give `Some(0) Some(1)`; `missing` gives `None` |
| Lookup under the stored-domain hash | `None` |
| Lookup of `rnx-0094-2` under `0`, `i64::MAX`, `i64::MAX + 1`, `u64::MAX` | `None` ×4 |
| Insert `new` under its lookup hash, again, then `get_cat("new")`, `len` | `Some(2) Some(2) Some(2) 3` |
| Insert `rnx-0094-2` under its stored hash, then `get_cat`, `len` | `Some(2) Some(0) 3`: a second id, as in Polars |
| Nine malformed tokens, each to both consumers | `ConversionError` naming the operation; the mapping keeps length 2 and still finds id 0 |

## Gate 4: oracle

The oracle's Rust side formats a listed return's `u64` as `{:016x}`. The script side compares the string. A listed parameter uses the argument shape `hash`: the script passes `"0000000000000002"` and Rust passes `2u64`, the same bits. The new categorical fixtures, plus a `CatSize` (`u32`) arm in the oracle's argument and return formatting, give 17 new cases, all matching:

- **`Categories`:** `hash`, `name`, `namespace`, `physical`.
- **`FrozenCategories`:** `hash`, `physical`.
- **`CategoricalMapping`:**
  - `cat_to_hash`, `cat_to_str`, `get_cat` and `get_cat_with_hash`
  - `insert_cat` and `insert_cat_with_hash`
  - `is_empty`, `len`, `max_categories`, `num_cats_upper_bound` and `set_max_categories`

Three runner controls on `Categories::hash`, whose fixture hash has its high bit set, pass:

| Control | Expected | Result |
|---|---|---|
| exact Rust hex | match | match |
| Rust formatter casting through `i64` | mismatch | mismatch |
| token with the high bit cleared (a truncating binding) | mismatch | mismatch |

Oracle: 2,251 cases, 17 more than 0093. Tally of the committed debug-profile results file: 2,134 match, 98 both_error, 17 both_panic, 2 row_order_differs. That is 0093's committed tally plus the 17 new matches, with no status movement among old cases. The two row-order differences are the known `LazyFrame::unique` and `unique_generic` permutations.

Scoreboard (`probes/0077/scoreboard.py`), 0093 to 0094:

| Measure | 0093 | 0094 |
|---|---:|---:|
| Available to a script | 2,117 | 2,117 |
| Value-tested | 1,344 | 1,361 |
| Unsupported | 1,926 | 1,926 |

## Gate 5: cost

All of these passed:

- The generator self-test: 19 suites, including `hash-token self-test`.
- The drift check (`--check`).
- The debug suite with `generated,test-support`: 100 tests plus 1 ignored.
- Both release configurations from the README, with fail-fast disabled: 9 tests with default features and 99 plus 1 ignored with `test-support`.
- `git diff --check`.

The 0094 binary's SHA-256 is `5ce1c4b57d115ff399eaa2d3cefdcf113f1f1425db2f8f4076508d062d58fb54`. Cold launch was measured in three interleaved 60-run sets against the retained 0093 binary, with a budget of +5 ms:

| Set | 0093 median | 0094 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 16.87 ms | 16.87 ms | 0.00 ms | `probes/0094/launch-results-1.json` |
| 2 | 15.76 ms | 15.23 ms | −0.53 ms | `probes/0094/launch-results-2.json` |
| 3 | 15.87 ms | 16.41 ms | +0.54 ms | `probes/0094/launch-results-3.json` |

## Remaining

A script cannot compute a mapping's lookup hash, because `CategoricalMapping::hasher` returns an unwrapped type and stays unsupported. So the two consumers are exact pass-throughs for a hash obtained elsewhere; `get_cat` and `insert_cat` remain the practical script path. A general unsigned integer value type would be a separate design. The function-generic and Arrow pools stay open.
