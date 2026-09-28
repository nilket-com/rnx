# rnx 0111 evidence: Rust traits at the Rune protocol boundary

The v2 figures come from `probes/0108/build.sh` (da47b74, the Python wheel's 145 features) and are committed in `probes/0111/evidence/`. `probes/0111/verify.sh` replays them against the 0110 inventory and the 0108 baselines, and reproduces the census, batch table and target table byte for byte. The 0108–0110 replays still pass.

## The rule (in `emit_foreign`)

**Markers, cited; the row leaves the applicable denominator:**
- `TrivialClone` (106 rows at v2). rustc 1.98.1 `core/src/clone.rs:255-283`: an unstable `#[marker]` trait with no method, "used for some optimizations", whose `Clone::clone` "must be equivalent to copying", and "this isn't part of any API guarantee". `Clone` already carries the operation.
- `Drop` (5 rows). The destructor runs when the owning Rune value is dropped. `tests/protocols.rs::an_owned_value_drops_exactly_once_moved_or_not` proves "exactly once" through a `#[derive(Any)]` probe with a `Drop` counter, for a plain drop, a move to another binding, a by-value move into a function, and two values in a vector. The generated wrappers are plain `#[derive(Any)] struct W(T)`, so Rust's drop glue runs `T::drop` once when the wrapper is dropped.

**`Eq`.** The row stays a marker, but a proven Rust `Eq` now also installs Rune **`EQ`** from the same Rust `==`. Rune 0.14.2 compares `HashMap` keys with `EQ`, not `PARTIAL_EQ` (`hashbrown/table.rs:287-335`). `Eq` is never inferred from `PartialEq` or `StructuralPartialEq`, and hand-written wrappers are left alone.

**Bindings:**
- **`Hash`** binds **`HASH`**: `(this: &W, hasher: &mut rune::runtime::Hasher) { core::hash::Hash::hash(&this.0, hasher) }`. This is admitted **only with a proven `Eq` on the owner** (its foreign impl record, unbounded), which installs EQ as above. `Hash` without `Eq` is a named refusal (39 at v2; for example `JoinType` and `FillNullStrategy` derive `PartialEq + Hash` but not `Eq`). `Hash` on a hand-written wrapper is refused (1).
- `PartialEq` accepts a parameter spelled `&Self` as well as `&Owner`, which adds the 12 rows that previously missed the route.
- `PartialOrd` binds `PARTIAL_CMP`, returning the Rust `Option<Ordering>`; no total order is synthesized. `Ord` binds `CMP`. Both require a `&Self` or `&Owner` parameter.
- `FromStr` becomes `Type::parse(s)`, a fallible constructor. A failed parse is a catchable conversion error carrying the Rust error's `Debug` (so `Err = ()` works).

**Named refusals:**
- `Flags`/`PublicFlags` (8): the type's inherent bitflags methods carry the operations; the trait adds `Bits`-typed internals.
- `Write` (1): no byte-buffer, count or I/O-error boundary.
- `TryFrom<StructArray>` (1): no script value for the source.
- The 9 trait methods with no public trait path (`RoundSeries` 5, `WritableTrait` 3, `ByteSource` 1) stay refused. Nothing private is made public.

**Self-test `protocols`**, through `emit_foreign`:
- `Eq` installs EQ from `==`;
- `Hash` with `Eq` emits HASH through `core::hash::Hash::hash`;
- `Hash` without `Eq` is refused;
- `TrivialClone` is a marker;
- `PartialOrd` on `&Self` emits PARTIAL_CMP;
- `Flags` is refused by name.

**Mutation check.** With the `Eq` requirement removed, the self-test fails: the `Eq`-less `Hash` is generated. The rule was then restored.

## The 312 rows (`probes/0111/evidence/targets.json.gz`)

| disposition | rows | by trait |
|---|---:|---|
| generated | **142** | Hash 125, PartialEq 12, PartialOrd 2, FromStr 2, Ord 1 |
| marker | **111** | TrivialClone 106, Drop 5 |
| refused | **59** | Hash 40 (39 without Eq, 1 hand-written), Flags 4, PublicFlags 4, RoundSeries 5, WritableTrait 3, ByteSource 1, TryFrom 1, Write 1 |

## v2 (the target)

| | 0110 | 0111 |
|---|---:|---:|
| full | 8,389 | 8,389 |
| markers apart | 417 | 528 (+106 TrivialClone, +5 Drop) |
| applicable | 6,552 | **6,441** |
| available | 3,312 (50.5%) | **3,454 (53.6%)** |
| value-tested | 2,132 | **2,244 (34.8%)** |
| to 90% | 2,585 | 2,343 |

The denominator moves only by the 111 cited markers. It would have been 6,441 whatever the numerator, as the plan predicted.

The census classifier is unchanged, so it files the new named refusals under `other` (111). Changing the classifier would break the byte-identical replays of the 0108–0110 evidence. The target table carries the per-trait reasons instead.

Oracle at v2: 3,645 cases, 3,172 matches, 0 mismatches, 0 broken.
- All **112 new cases match**: HASH through a real `HashMap` insert and lookup plus `std::ops::eq`, compared with a Rust `HashSet`; `PartialEq`; `std::ops::partial_cmp`/`cmp`; and the `parse` round trip of the fixture's Display.
- In the 3,533 shared cases the only moves are `full_join`/`left_join` passing between `match` and `row_order_differs` under their approved policy.
- The v2 no-default stage passes.

Hash values are compared as behavior under one hasher state: equal fixtures find each other in a map, and `std::ops::eq` agrees with Rust `==`. Independent random-state numbers are never compared. This is separate from record 0094's categorical hash tokens.

## 0.55.2 production

- Available (0077 scoreboard): 2,318 → **2,420**.
- Value-tested: 1,499 → **1,575**.
- Unsupported: 1,724 → 1,542.
- Markers reported apart: 380.

The oracle grew from 2,802 to 2,878 cases, and all 76 new cases match. The one status move is `LazyFrame::unique_generic`, from `row_order_differs` to `match` (approved).

Suites, with debug last: `--release` exit 0; `--release --features test-support` exit 0; debug `--features generated,test-support` exit 0.

The focused test `tests/protocols.rs` passes 5 of 5:
- `TimeUnit` is a working `HashMap` key: equal keys meet, distinct keys stay apart, a second insert replaces. It matches a Rust `HashMap`.
- `JoinType` (PartialEq and Hash, no Eq) cannot key a map. The failure is pinned to "Unsupported unary operation `HASH`", not a later EQ failure, while `==` still works.
- `partial_cmp` over three `TimeUnit` pairs matches Rust.
- `CategoricalPhysical::parse` on u8, u16, u32, u64 and "nope" matches `FromStr` (u64 and "nope" fail catchably).
- The drop-exactly-once probe passes.

## Launch

Measured with `probes/0073/launch.py`, 60 interleaved launches per set, old = 0110 (3c8c2f5), new = 0111. Budget +5 ms.

| set | old median | new median | delta |
|---|---:|---:|---:|
| 1 | 17.66 ms | 19.78 ms | +2.12 |
| 2 | 18.37 ms | 19.30 ms | +0.93 |
| 3 | 18.55 ms | 20.17 ms | +1.62 |

`cargo fmt` is clean on the adapter, generator and extractor.

## Review round 1 (Codex): exact shape gates

**Finding.** The new arms dispatched by the short trait name and emitted fixed bodies without checking the canonical trait path or signature. The positive `Hash` control even used zero parameters, where the inventory records `&mut __H`.

**Fix.** `protocol_shape` runs before any binding text for every mapped trait. It checks the canonical trait path (from ` as <path>`), the receiver, the parameters and the return, against the shapes recorded in the pinned inventories:

| trait | path | receiver | parameters | return |
|---|---|---|---|---|
| TrivialClone | `core::clone::TrivialClone` | none | () | none |
| Drop | `core::ops::drop::Drop` | `&mut self` | () | none |
| Eq | `core::cmp::Eq` | `&self` or none | () | none |
| Hash | `core::hash::Hash` | `&self` | one `&mut H` (a generic hasher parameter: `H`, `__H`) | none |
| PartialEq | `core::cmp::PartialEq` | `&self` | `&Self` or `&Owner` | `bool` |
| PartialOrd | `core::cmp::PartialOrd` | `&self` | `&Self` or `&Owner` | `Option<Ordering>` |
| Ord | `core::cmp::Ord` | `&self` | `&Self` or `&Owner` | `Ordering` |
| FromStr | `core::str::traits::FromStr` | none | `&str` | `Result<Self, Self::Err>` or `PolarsResult<Self>` |

A drifted shape is refused with "protocol shape: …".

**Self-test.** `protocols` now uses the real shapes; the positive `Hash` control is `&mut __H`, the loose one `&mut H`. It adds eight malformed-inventory controls through `emit_foreign`:
- Hash with `&mut u64`;
- Hash returning `bool`;
- Hash under another trait path;
- PartialOrd returning `bool`;
- Ord with two parameters;
- FromStr with a `&self` receiver;
- FromStr returning `Self`;
- Eq with a parameter.

Each must be `unsupported` with the shape reason, and must write no function text and no registration.

**Mutation check.** With the gate disabled, the self-test fails. The gate was then restored.

**Effect on real data.** Nothing changed. Regenerating both pins gives 0 status changes, and the generated adapter files are byte-identical to 8bc8fc9: every real row already had its recorded shape, and the gate now enforces it. The evidence bundle and the counts stand, and `probes/0111/verify.sh` passes.

Codex accepted the unchanged census classifier (the target table names each refusal) and the behavioral hash evidence. A recording-hasher control would strengthen it, but is not required.
