# 0088 owned Cow chunked-array returns: evidence

Plan `c66b6de`. `ChunkedArray::rechunk` and the List and Struct `to_physical_repr` return an owned wrapper to the script, from either the borrowed or the owned `Cow` branch. No Rust borrow crosses the binding boundary.

All 18 proven pairs emitted: three callables across two operation names. All 18 new oracle cases match Rust.

## Gate 1: freeze and probe

`probes/0088/pairs.json` holds the 18 rows: `polars_core:3434` `rechunk` on 16 aliases, `polars_core:3611` `to_physical_repr` on List, and `polars_core:3641` on Struct. Each row has its substituted signature, disposition before and after, and oracle outcome. All 18 were candidates, since every inner type is a wrapped, `Clone` chunked array.

Source, pinned polars-core 0.55.2:

- **`rechunk`** (`chunked_array/ops/chunkops.rs:162-182`) returns `Cow::Borrowed(self)` for one chunk. Otherwise it concatenates, keeps the sorted and fast-explode flags, and returns `Cow::Owned`. The `object` panic branch is not compiled under the adapter's features.
- **List `to_physical_repr`** (`list/mod.rs:47-82`) is borrowed when the inner dtype is already physical. Otherwise it builds a physical List, rechunking when the inner conversion did.
- **Struct `to_physical_repr`** (`struct_/mod.rs:138-170`) is borrowed when the fields are physical. Otherwise it builds a Struct from physical fields.

The ceiling was 3 callables, 2 operation names and 18 bindings; all were reached.

## Gate 2: narrow generation

The release file gained `[[cow_returns]]` entries with inventory `key`, `path` and `cite`. The key is needed because `to_physical_repr`'s canonical path is shared by several implementations. `World.cow_ok` joins the scoped guard from 0085–0087 in both emitters.

In the return mapper, `Cow<X>` under that scope is admitted only when X, or `Self` after family substitution, is a wrapped, `Clone` type. The conversion is `into_owned()` followed by the wrapper. A routed binding moves `into_owned()` inside the engine closure, as the plan requires; `rechunk` is routed, so its closure returns an owned array and only that leaves the thread. The oracle frames a `Cow` result as its owned value.

Synthetic controls (`cow-return self-test`):

- Listed `Cow<Self>` and `Cow<DataFrame>` emit with `into_owned()` and no `Cow` in the signature.
- The listed path under a different key, an unlisted `Cow<Self>`, a `Cow<str>` and a `Cow` of an Arrow array are refused.
- The scope is clear after emission.

Both adapter configurations compile all 18 pairs, and no other callable changed disposition.

Counts, 0087 to 0088:

| Measure | 0087 | 0088 |
|---|---:|---:|
| Generated operations | 2,097 | 2,100 |
| Unsupported | 1,935 | 1,932 |
| Bindings | 4,018 | 4,036 |
| Refused proven pairs | 196 | 178 |
| Oracle cases | 2,162 | 2,180 |

## Gate 3: behaviour and oracle

`tests/cow_returns.rs` covers two areas:

- **`rechunk`.** A single nullable chunk takes the borrowed branch: 3 rows, 1 null, 1 chunk, and it outlives its source array. A two-chunk nullable array takes the owned branch: 6 rows in 1 chunk with 2 nulls, mask `101101`, and the value at row 3 kept. The receiver still has 2 chunks and 6 rows. An empty array rechunks to 0 rows.
- **`to_physical_repr`.** An `Int64` list keeps its physical inner type (borrowed). A list of `Date` values becomes a list of `Int32` (owned conversion; the inner type is no longer temporal) while the source list stays temporal. A struct with a `Date` field becomes one with a physical field in the same way, keeping all 3 rows.

Chunk counts come from 0087's `chunk_lengths`. The pinned source has no failing conversion branch for these types, so there is no error case to exercise.

Oracle: 2,180 cases, all verified. The 18 new cases all match; the fixtures take the borrowed branch, and the behaviour suite covers the owned branch. No old case changed status. Tally: 2,063 match, 98 both_error, 17 both_panic, 2 row_order_differs.

Scoreboard, 0087 to 0088:

| Measure | 0087 | 0088 |
|---|---:|---:|
| Available to a script | 2,108 | 2,111 |
| Value-tested | 1,335 | 1,338 |
| Unsupported | 1,935 | 1,932 |

**Unit-test race fixed.** The first full run failed `production_limit_is_the_default`. It read the shared test override of the materialize bound while 0085's bitmap unit test had set it to 3, because the two modules used different locks. All unit tests that set or read the override now hold one `LIMIT_LOCK`, and the unit suite then passed five consecutive runs. The change is test-only and leaves the release binary unchanged.

## Gate 4: cost

Generator self-test, drift and accounting, both adapter suites with fail-fast disabled, and the full oracle pass. The retained 0087 binary's SHA-256 is `7345d2cc28469ba04733e8a90d43407f8b2ce64da40f23a9e5c183759da67027` and the 0088 binary's is `88581a1c6dda4a0dc6df8aa76a7c7b7f1e2f97e9e604ad3af5d13d0a670f453c`, measured after Codex's review note removed the leftover braces from the routed `rechunk` conversions (16 `unused_braces` warnings). Cold launch was measured in three interleaved 60-run sets against a budget of +5 ms:

| Set | 0087 median | 0088 median | Difference | Raw samples |
|---|---:|---:|---:|---|
| 1 | 18.32 ms | 17.30 ms | −1.02 ms | `probes/0088/launch-results-1.json` |
| 2 | 16.54 ms | 15.68 ms | −0.86 ms | `probes/0088/launch-results-2.json` |
| 3 | 18.57 ms | 18.25 ms | −0.32 ms | `probes/0088/launch-results-3.json` |

## Remaining

178 refused proven pairs remain, mostly Arrow array returns, `with_validities` and unreachable types. The `usize as i64` generator audit and the 0083 table's decisions are unchanged.
