# rnx 0112 evidence: serde impls through a bounded JSON boundary

The scope was split before the implementation commit, with Codex agreeing (see the plan's last section). 0112 closes on the **serde lane**. The frozen target table inventories all 732 `generic_impls` rows and marks the 345 non-serde rows `deferred to 0113, unproved`.

The v2 figures come from `probes/0108/build.sh` (da47b74, the Python wheel's 145 features) and are committed in `probes/0112/evidence/`. `probes/0112/verify.sh` replays them and reproduces the census, batch table and target table byte for byte. The 0108–0111 replays still pass.

## The lane

**Admission (`serde_trait`, `serde_shape`), exact and fail-closed.** Everything below is checked before any binding text:
- the trait path is `serde_core::ser::Serialize` or `serde_core::de::Deserialize` (the pinned serde splits its traits into `serde_core`);
- `Serialize`: receiver `&self`, one generic parameter `S`, return `Result<S::Ok, S::Error>`, or the fully qualified `Result<<S as Serializer>::Ok, <S as Serializer>::Error>` of the hand-written `Series` impl. It is the same projection, and nothing looser is accepted;
- `Deserialize`: no receiver, one generic parameter `D`, return `Result<Self, D::Error>` (or the qualified form);
- a concrete head: `impl_for == owner`, no bounds, no where clauses, owner not generic;
- the trait's lifetime arguments (new, below): none for `Serialize`, exactly one non-`'static` lifetime for `Deserialize`.

**Emission.**
- `value.to_json() -> Result<String, Error>` runs `support::to_json(&this.0, op)` inside `engine::run`, which is scoped and borrows with no clone.
- `Type::from_json(s) -> Result<Type, Error>` runs `support::json_len_ok` before anything parses, then `support::from_json::<T>` inside `engine::run`.
- Serde is instantiated only through serde_json; no generic serializer is exposed to Rune.

**Bound.** `support::JSON_LIMIT` is 64 MiB of UTF-8 bytes, inclusive, separate from the materialize item bound.
- Serialization writes through `BoundedJson`, an `io::Write` that refuses the write that would cross the bound, so no unbounded text is ever built.
- A script text is measured before parsing.
- Errors are catchable, with kind `JsonLimit` or `Json` and a message naming the operation and cause.
- `polars::set_json_limit` exists under `test-support` only.

**Dependencies.**
- `serde` and `serde_json` are optional normal dependencies enabled by the `generated` feature; the no-default build stays lean.
- The production lock gained exactly one edge, rnx-polars → serde, with no new package or version. The v2 probe lock (`probes/0108/locks/adapter.lock`) gained the same single edge.

**Interchange caveat.** This is a pinned-version interchange format, not a promise that Polars JSON is stable across Polars releases or feature sets.

## A gap the inventory could not show: `Deserialize<'static>`

The first v2 build failed on `AnonymousScanOptions`. It implements only `Deserialize<'static>` (it borrows static data), so it is not `DeserializeOwned` and cannot parse a script's text.

The inventory recorded it as plain `Deserialize`, because the renderer drops lifetime arguments. The extractor now records a foreign impl's trait lifetime arguments in a new `trait_lifetimes` field, omitted when empty. The rest of the inventory is untouched: callable identities (key, path, bucket) and the supporting records are byte-identical at both pins.

The counts:
- v2: `'de` 215, `'a` 7, `'static` 2.
- 0.55.2: `'de` 80, `'a` 2, `'static` 1.

The gate refuses `'static`.

## Controls

**Generator self-test `serde`**, through `emit_callable`:
- The derive shapes bind `to_json` (through `support::to_json(&this.0`) and `from_json` (through `support::json_len_ok`).
- The qualified `Series` spelling passes the shape check.
- A drifted return, an extra parameter, a generic head, a wrong receiver and a `'static` lifetime are refused as "serde shape", writing no text and no registration.
- Another trait path is not the serde lane at all.
- Mutation check: with the shape gate disabled, the self-test fails.

**Focused test `tests/json_boundary.rs`**, 5 of 5 pass:
- `RowIndex` and `SortOptions` round-trip exactly as `serde_json` does;
- a multi-byte UTF-8 `RowIndex` name (Polars serializes a frame as IPC bytes, whose JSON is ASCII; a `Field` holds a `DataType`, which can carry a `Series`) round-trips. At a bound of exactly its byte length both parse and write succeed; one byte less is refused as `JsonLimit` for parsing, before any parse, and for writing;
- `""`, `{`, `not json` and `[1, 2]` are catchable `Json` errors, as direct `serde_json` also errs;
- a refused `to_json` leaves its receiver usable;
- `DataFrame` and `Series` have no JSON binding.

## The 732 rows (`probes/0112/evidence/targets.json.gz`)

| lane | disposition | rows |
|---|---|---:|
| serde | generated | **197** |
| serde | refused | 190: 166 IPC-bearing or of unknown content (review rounds 1–3, below), 16 by shape (generic or bounded heads such as `ChunkedArray<T>`, `Schema<Field, Metadata>`, `SpecialEq<T>`, and the `Deserialize<'static>`), 8 owners not wrapped |
| non-serde | `deferred to 0113, unproved` | 345 (not unsupported by a failed mapping; not counted available) |

## v2 (the target)

| | 0111 | 0112 |
|---|---:|---:|
| applicable | 6,441 | 6,441 |
| available | 3,454 (53.6%) | **3,651 (56.7%)** |
| value-tested | 2,244 | **2,415 (37.5%)** |
| to 90% | 2,343 | 2,146 |

Oracle (final run, after review round 3): 3,818 cases, 3,342 matches, 0 mismatches, 0 broken.
- The **173 new cases**:
  - 171 match. `Serialize`: `to_json` compared with `serde_json::to_string`. `Deserialize`: `from_json(to_json(x)).to_json()` compared with the same round trip in Rust. A `Deserialize`-only type is skipped by name.
  - 2 fail identically on both sides at the existing `Fractions::new(1.5)` fixture recipe, which is out of range.
- The only shared-case move is `left_join` to `row_order_differs`, which its approved policy permits.
- The v2 no-default stage passes.

## 0.55.2 production

- Available (0077 scoreboard): 2,420 → **2,486**.
- Value-tested: 1,575 → **1,631**.

The oracle grew from 2,878 to 2,934 cases, and all 56 new cases match. The one status move is `LazyFrame::unique_generic`, an approved unordered case.

Suites, with debug last: `--release` exit 0; `--release --features test-support` exit 0; debug `--features generated,test-support` exit 0.

## Launch

Measured with `probes/0073/launch.py`, 60 interleaved launches per set, old = 0111 (ea5ade1), new = 0112. Budget +5 ms.

| set | old median | new median | delta |
|---|---:|---:|---:|
| 1 | 18.43 ms | 18.15 ms | −0.28 |
| 2 | 18.95 ms | 18.48 ms | −0.47 |
| 3 | 18.59 ms | 18.73 ms | +0.14 |

`cargo fmt` is clean on the adapter, generator and extractor.

## Review round 1 (Codex): the bound must hold upstream of the writer

**Finding.** `Serialize for DataFrame` (polars-core `serde/df.rs:158-170`) encodes IPC into a fresh `Vec` before handing bytes to the serializer, and `Serialize for Series` (`serde/series.rs:39-51`) does the same through `serialize_to_bytes`, at both pins. `fx::df().to_json()` could therefore allocate the whole payload before `BoundedJson` saw a byte.

**Why a preflight is not enough.** A counting-sink preflight (`serialize_into_writer` to a writer that only counts, then refuse when `2n + 1` exceeds the bound) would still leave three unbounded intermediates:
- the Arrow IPC encoder buffers each record batch in memory, even when writing into a counting writer;
- a frame or series nested in a container (`Expr` with a literal `Series`, `DslPlan` with a frame scan, `LiteralValue`, `Column`, `Scalar`) buffers through its own `Serialize`, which an outer preflight cannot intercept;
- `from_json` of an IPC-bearing type may receive a compressed IPC payload, whose decompressed size the text bound does not limit.

**Fix: `serde_ipc_bearing`, a named refusal in both directions**, before any binding text, for:
- `DataFrame`, `Series` and `Column`;
- every owner whose recorded public fields or enum payloads reach one of them, walked recursively over the inventory's supporting records;
- every struct with no public field, whose content the inventory cannot show to be frame-free.

At v2 that is 46 owners, or 92 bindings (`Expr`, `DslPlan`, `LiteralValue`, `AggExpr`, `FunctionExpr`, `Scalar`, `ScalarColumn`, `TimeZone`, `CompatLevel` and others; the list is in the target table). With this, the 64 MiB byte bound holds for every admitted binding. A bounded frame/series JSON path (IPC streamed into the bounded JSON writer, with compressed input refused) is left for a later record.

**Controls.**
- The serde self-test adds an owner whose public field is `Arc<DataFrame>` and a struct with no public field. Both are refused as "serde through an unbounded IPC buffer", with no text and no registration. Mutation check: with the gate disabled, the self-test fails.
- The focused test moves its round trips to frame-free `Field`/`SortOptions`, and adds a check that `DataFrame` and `Series` have no `to_json`.

**Minor.** `BoundedJson::write` states the cumulative bound as `b.len() > limit - buf.len()`, with no overflowing addition.

**Effect of round 1.** v2 available 3,817 → 3,725; 0.55.2 2,518 → 2,498. It was superseded by round 2, below.

## Review round 2 (Codex): opaque structs at any depth, unresolved paths unknown

**Finding.** Opaque (no-public-field) structs were refused only at the root, so `AsOfOptions` stayed admitted through its public `Option<Scalar>` field, although `Scalar` holds Series-bearing values. An unresolved child path was also treated as safe.

**Fix.** In `serde_ipc_bearing`:
- an opaque struct is refused at **any depth**;
- a Polars path with **no supporting record** is refused as unknown, and only non-Polars leaves (std, core, alloc, foreign crates) count as frame-free, with their generic arguments walked as paths;
- type aliases are followed to their target.

`AsOfOptions` is now refused in both directions (`AsOfOptions -> Scalar reaches polars_core::scalar::Scalar: no public field …`).

**Cited exceptions (`FRAME_FREE_OPAQUE`).** These are opaque leaves whose private fields were read in the pinned sources at both pins:

| Type | Fields |
|---|---|
| `PlSmallStr` | `CompactString` |
| `PlRefStr` | `Arc<str>` |
| `PlRefPath` | `PlRefStr` |
| `TimeZone` | `PlSmallStr` |
| `GzipLevel` | `u8` |
| `ZstdLevel` | `i32` |
| `BrotliLevel` | `u32` |
| `UnsafeBool` | `bool` |
| `Fractions` | `Vec<f64>` |
| `CompatLevel` | `u16` |
| `ListType` | `{}` |
| `StatisticsFlags` | `bitflags` over `u32` |
| `Schema<Field, Metadata>` | a `PlIndexMap<PlSmallStr, Field>` plus `Metadata`, whose type arguments the walk visits |

Every other opaque struct stays refused. Reading the sources showed two such refusals are necessary:
- `Breaks` is `Breaks(Series)`.
- `DataType` serializes an `Enum`'s categories **as a `Series`** (polars-core `src/datatypes/_serde.rs:222-226`), so `DataType`, `Categories` and everything holding them (for example `Field`) are IPC-bearing.

**Controls.**
- The serde self-test adds a nested opaque owner (the `AsOfOptions -> Option<Opaque>` shape), which must be refused in both directions with no text or registration.
- An owner whose field is a cited leaf (`PlSmallStr`) must bind.
- Mutation check: with the root-only rule restored, the nested control fails.
- The focused test's multi-byte round trip moved from `Field` (now refused) to `RowIndex`.

**Effect of round 2.** v2 available 3,661; 0.55.2 2,490. It was superseded by round 3, below.

## Review round 3 (Codex): tuple-struct fields

**Finding.** The extractor records only named fields. `polars_plan::dsl::file_scan::TableStatistics` has `public_fields = 1` and `fields_canonical = []`, yet it is `pub struct TableStatistics(pub Arc<DataFrame>)` (polars-plan src/dsl/file_scan/mod.rs:347), so both of its JSON directions were admitted.

**Fix (fail closed).** A struct whose public-field count exceeds its recorded fields is refused as unknown content ("tuple fields are not extracted"). At v2, 18 structs have that gap. Recording tuple fields in the extractor would change the supporting records used across the generator, so it is left to a record that measures that change.

**Controls.**
- The serde self-test adds a tuple struct (one public field, none recorded), which must be refused with no text. Mutation check: with the rule disabled, it fails.
- **A real-data control:** `probes/0112/verify.sh` asserts on the replayed v2 surface that `TableStatistics` is `unsupported` in both the `Serialize` and `Deserialize` directions, with the tuple-field reason.

**Final.** The serde lane: generated 197, refused 190 (IPC or unknown 166, shape 16, not wrapped 8).
- v2: available 3,651 (56.7%), value-tested 2,415.
- 0.55.2: 2,486.

The bundle was regenerated from the final run, and all five replays (0108–0112) pass.
