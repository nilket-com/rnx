# rnx 0113 evidence: non-serde generic trait impls as one batch

All 345 rows that 0112 deferred are decided in one record. The v2 figures come from `probes/0108/build.sh` (da47b74, the Python wheel's 145 features) and are committed in `probes/0113/evidence/`. `probes/0113/verify.sh` replays them against the 0112 inventory and the 0108 baselines, and reproduces the census, batch table and the **345-row audit** (`targets.json.gz`) byte for byte. The 0108–0112 replays still pass.

## The audit

| family | generated | refused |
|---|---:|---:|
| operators (`Add`, `Sub`, `Mul`, `Div`, `Rem`, `BitAnd`, `BitOr`, `BitXor`) | 61 | 57 |
| unary (`Neg`, `Not`) | 4 | 0 |
| `FromIterator` | 43 | 6 |
| `Index` | 2 | 6 |
| every other family | 0 | 166 |
| **total** | **110** (344 bindings) | **235** |

Every refusal carries a named reason. The "other" rows name their family's missing contract:
- borrowed views (`Deref`, `AsRef`, …);
- iteration (items borrow the receiver or have no proven bound);
- conversions (outward targets, foreign or Arrow sources, unproved generic heads);
- in-place mutation (`Extend`'s partial-mutation contract is undefined);
- value protocols on generic or lifetime-bearing heads;
- formatting (no bounded builder);
- service traits.

## The rules

All gates run before any binding text or registration, on the exact recorded shape. The trait arguments come from the inventory's `name`, because `canonical_path` names only the trait.

**Operators** (`op_arms`, `emit_op_groups`). A row must have:
- the trait path;
- a `self` receiver, one parameter, and `Self::Output` with `Output` from the impl record, substituted;
- a head that is the owner or `&owner`. Rows listed on another type's page are refused as outward listings. A generic `ChunkedArray<T>` head is instantiated per alias through `applicability`.

The right-hand side is one of:
- `Self`, with the same borrow as the head;
- a wrapped type named by the trait argument, whose parameter must agree in type and borrow;
- a scalar parameter bounded exactly by `Num + NumCast` or `Num + ToPrimitive`.

`i64` and `f64` are the proven natives: num-traits 0.2.19, pinned in both locks, `lib.rs:175,392`, `cast.rs:196,375,692,696`. The output must be a wrapped type or `Result<wrapped, PolarsError>`.

One Rune slot holds one function per type, so each (type, operator) gets one dispatching function over its proven arms, which chooses by the runtime right-hand side (a wrapped value, `i64` or `f64`):
- a borrowed-head arm serves its operand kind;
- an owned-head arm is used only where no borrowed arm exists and the owner is Clone. Other owned twins are refused by name ("served by the borrowed-head impl");
- a slot owned by the hand-written adapter (`Expr`'s `Add`) is refused.

Operators return `Result`, so a script writes `(a + b)?`. That is faithful where the Rust impl is fallible (`&Series + &Series`), makes a foreign right-hand side a catchable error, and matches how every other fallible adapter binding behaves. The cost is ergonomic: `a + b + c` needs parentheses and `?`.

**Unary** (`unary_generic_shape`). `Neg` and `Not` are admitted on exactly `self`, no parameter, `Output` = the owner, no bound, and go to the existing unary route. **Rune 0.14.2 has no unary-minus protocol** (rune-core `src/protocol.rs` lists none), so `Neg` is the `neg()` method, like `not_()`. The previous `Neg` arm used a nonexistent `NEG` and had never been reached.

**`FromIterator`** (`emit_from_iter`). This is an explicit item grammar over the recorded trait argument:
- the primitives, their `Option` and `&` forms;
- `String`, `&str`, `Option<&str>`, `Option<String>`;
- the owner itself;
- `Option<<T as PolarsNumericType>::Native>` on each proven `ChunkedArray` alias.

The script vector goes through the existing argument rules (narrow integers checked, `Option` for nulls), and then `<Owner as FromIterator<_>>::from_iter` runs over the exact item iterator (`.iter()` for `&` items). The constructors are named `Type::from_iter_<item>`. Outside the grammar, or on an unproved head (`Schema<F>`, `IcebergSchema<T>`, `NoNull<…>`, arrow arrays), the row is refused.

**`Index`** (`index_arm`, `emit_index_groups`). A row must have:
- a `&self` receiver, one key parameter equal to the trait argument, `&Self::Output`, and `Output` a wrapped Clone type;
- the admitted keys `usize` (checked narrowing) and `&str`.

The rows form one `INDEX_GET` per owner, and the value is cloned out, so no borrow escapes. Polars' own `Index` panics on a missing name or position, and the binding panics identically: the engine boundary propagates panics, and the oracle compares them. The six range keys return an unsized `[Column]` view and are refused. `df["x"]` and `df[0]` work.

## Controls

**Generator self-test `generic-impls`:**
- For operators, a borrowed `Self` arm and a scalar arm group into one `ADD` (one registration) dispatching on a wrapped value, `i64` and `f64`.
- Six operator rows with one field changed are refused before any text: receiver, associated name, parameter, scalar bound, outward head, output.
- For `FromIterator`, the well-formed row binds `from_iter_u8`, and three malformed rows write nothing: item, receiver, return.
- For `Index`, `usize` binds one `INDEX_GET`, and a range key, a mismatched key and a non-reference return are refused.

**Mutation checks:**
- With the outward-head check disabled, the self-test fails (`op_outward` admitted).
- With the scalar-bound check disabled, the self-test still passes. The row stays refused by a second gate, the concrete-head bound check, so that check is defence in depth rather than the sole guard.

**Compile evidence.** No separate UFCS probe files were written. Both pins' full generated modules compile (the production 0.55.2 build and the v2 build at da47b74) with every admitted arm, which exercises the actual impl resolution. The negative probes are the self-test's malformed rows.

**Focused test `tests/generic_impls.rs`**, 6 of 6 pass at 0.55.2:
- nullable `Series` operators (`+ - *` with a `Series`, `* 2`, `/ 1.5`, `% 2`) match direct Rust;
- integer division by zero follows Polars' own semantics;
- a foreign right-hand side is a catchable error and the receiver stays usable;
- `from_iter_option_u8` with nulls, `u8` narrowing (300 refused), the empty vector and `Option<&str>` all behave as Rust does;
- `df["x"]` and `df[0]` clone the column and match Rust, and a missing name panics in both.

`DataFrame - Series` is v2-only: 0.55.2's narrow build lacks `dataframe_arithmetic`. It is covered by the v2 oracle.

## v2 (the target)

| | 0112 | 0113 |
|---|---:|---:|
| applicable | 6,441 | 6,441 |
| available | 3,651 (56.7%) | **3,761 (58.4%)** |
| value-tested | 2,415 | **2,513 (39.0%)** |
| to 90% | 2,146 | 2,036 |

Oracle: 4,157 cases, 3,625 matches, 0 mismatches, 0 broken.
- The **339 new cases**:
  - 280 match.
  - 51 fail identically on both sides at the known v2-only `f16`/`i128`/`u128` fixture recipes (0108's fixture gap).
  - 8 are `both_error` `InvalidOperation`: `DataFrame` arithmetic on the fixture shapes, which Polars rejects the same way in Rust.
- The shared-case moves are `inner_join`/`unique`/`unique_generic` passing between `row_order_differs` and `match` under their approved policies.
- The v2 no-default stage passes.

## 0.55.2 production

- Available (0077 scoreboard): 2,486 → **2,585**.
- Value-tested: 1,631 → **1,727**.
- Unsupported: 1,444 → 1,377.

The oracle grew from 2,934 to 3,210 cases, and all 276 new cases match with no status change.

Suites, with debug last: `--release` exit 0; `--release --features test-support` exit 0; debug `--features generated,test-support` exit 0.

## Launch

Measured with `probes/0073/launch.py`, 60 interleaved launches per set, old = 0112 (aa2b344), new = 0113. Budget +5 ms.

| set | old median | new median | delta |
|---|---:|---:|---:|
| 1 | 20.03 ms | 21.04 ms | +1.01 |
| 2 | 18.65 ms | 18.91 ms | +0.26 |
| 3 | 20.35 ms | 21.86 ms | +1.51 |

(Remeasured after review round 1.)

`cargo fmt` is clean on the adapter, generator and extractor.

## Review round 1 (Codex)

**1. Operator arm order.** `emit_op_groups` sorted by the right-hand side's `Debug` text, which includes the operand's own borrow, before the head's borrow. For `ChunkedArray<T>: Add<Self>` the owned twin therefore sorted first. The emitted `Float32Chunked + Float32Chunked` arm was `this.0.clone() + __b.0.clone()`, and the actually borrowed impl row was refused as "served by the borrowed-head impl".

The fix: the order is now operand kind (which ignores the operand's borrow), then borrowed head first. The arm is now `&this.0 + &__b.0`.

The self-test adds an owned `Self` twin fed **before** the borrowed row. It asserts that the body is `&this.0 + &__b.0` with no cloned form, that the borrowed row is generated, and that the owned twin is refused. Mutation check: with the old sort restored, the self-test fails.

**2. `FromIterator` input bound.** The script vector was cloned by `support::borrow_vec` before any length check, so the whole-call materialize bound was not enforced.

The fix: every `from_iter_*` binding first calls `support::vec_len_bounded`. It reads the script vector's length without copying, and refuses above `materialize_limit()` (inclusive) with `MaterializeLimit`, before any allocation. The focused test `from_iter_input_is_bounded_before_any_copy` sets the bound to 3: exactly 3 items pass, 4 are refused, and both source vectors remain usable.

**Effect.** Counts are unchanged: v2 3,761 available, 2,513 value-tested; 0.55.2 2,585 and 1,727. The `Self` arms now bind the borrowed rows, so 65 production case ids (80 at v2) moved from the owned rows' keys to the borrowed rows' keys, with identical statuses (all match; the v2 fixture-gap cases unchanged). All three production suites pass, debug last. The bundle was regenerated, and all six replays pass.
