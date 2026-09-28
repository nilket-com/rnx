# rnx 0109 evidence: move semantics for non-Clone values

The v2 figures come from the 0108 probe (`probes/0108/build.sh`, v2.toml at da47b74 with the wheel's 145 features) and are committed in `probes/0109/evidence/`. `probes/0109/verify.sh` replays them: the 0108 inventory and baselines plus this record's surface and oracle results reproduce the census and batch table byte for byte.

## The rule

In the generator (`tools/polars-gen/src/main.rs`):
- A `self` receiver on a wrapped non-`Clone` owner binds `this: W` and passes `this.0`. Rune's owned conversion (`AnyObj::downcast` → `access.try_take()`, rune 0.14.2 `runtime/any_obj.rs:210-235`) takes the value, as Rust does.
- A top-level by-value non-`Clone` argument moves the same way (`W-move:` shape).
- Inside a container (a `Vec`, an `Option`, a re-dispatched `Self` or alias), such an argument stays refused.
- `Clone` owners still clone.
- The deref route stays refused for `self`.

Oracle:
- A moved receiver's case calls once (`has_receiver: false`). The Rust side is unchanged.
- The producer rule for fixtures (record 0076, rule 5) now also accepts a consuming zero-argument producer on a `Clone` owner. For example, `Expr::str(self)` projects a `StringNameSpace` fixture as `fx::expr().str()` in Rune and `Expr::str(expr())` in Rust. That gives the namespaces receivers.

Self-test `move-semantics` covers the four cases:
- a non-`Clone` receiver moves;
- a `Clone` receiver still clones;
- a top-level argument moves;
- a `Vec` of the type is refused.

All generator self-tests pass.

## v2 (the target)

| | 0108 | 0109 |
|---|---:|---:|
| applicable | 6,553 | 6,552 (one listing moved to duplicates) |
| available | 2,818 (43.0%) | **3,059 (46.7%)** |
| value-tested | 1,738 (26.5%) | **1,937 (29.6%)** |
| missing `non_clone_move` | 245 | 1 |
| to 90% | 3,080 | 2,838 |

Row by row, of the 245 `non_clone_move` rows in 0108 (corrected on review):
- **242** became available.
- 1 became a duplicate listing.
- 1 became `arity`: `DateLikeNameSpace::replace`, 9 arguments with the receiver now bindable. The two `Expr::qcut` arity refusals already existed in 0108.
- 1 remains: `DslResolverTrait::cse_eq`, whose `other` argument arrives through the every-implementor route, a re-dispatch that stays refused.

Separately, `collect_concurrently`, which was available, became a release-policy refusal (below). The net available gain is therefore **241** (2,818 → 3,059).

The moved-argument emission is covered by the generator self-test only. No callable that otherwise binds takes a top-level by-value non-`Clone` argument at either pin (0 `W-move` bindings emitted), so no runtime argument case exists.

Oracle at v2: 3,006 cases, 2,657 matches.
- 2,797 cases are shared with 0108 and none regressed. The one status change is `LazyFrame::join` going from `row_order_differs` to `match`, an approved unordered case whose row order varies per run.
- Of the 209 new cases, 199 match; the other 10 fail identically on both sides (7 both_panic, 3 both_error).
- The 172 fixture failures and 2 oracle panics are unchanged from 0108.

The v2 `--no-default-features` stage passes.

## 0.55.2 production

- Available (0077 scoreboard): 2,136 → **2,218**.
- Value-tested: 1,380 → **1,428**.
- Unsupported: 1,907 → 1,824.

The namespaces bound at 0.55.2 are `list` 21, `binary` 8, `name` 7, `struct_` 6 and `cat` 3. The builders are `DslBuilder` 16, `JoinBuilder` 15, and `DataFrameBuilder`, `SeriesBuilder` and the CSV `Builder` with 1 each.

The oracle grew from 2,439 to 2,489 cases:
- 48 new matches and 2 new both_panic.
- No regression; `LazyFrame::unique` went from `row_order_differs` to `match`, an approved unordered case.

Suites, with debug last: `--release` exit 0; `--release --features test-support` exit 0; debug `--features generated,test-support` exit 0.

The focused test `adapters/polars/tests/move_semantics.rs` passes 5 of 5:
- `name` namespace methods inside a `select_` match direct Polars.
- A `JoinBuilder` chain (`with`, `left_on`/`right_on` fallible, `how`, `maintain_order`, `finish`) matches direct Polars.
- Reusing a moved namespace or builder is refused by the VM ("Cannot take"), never read.
- A builder method that returns a new builder stays usable.
- A reused `Expr` (a Clone receiver) is still usable after consuming calls.

## Hazard found and closed: `collect_concurrently`

The new `InProcessQuery` fixture (projected through `LazyFrame::collect_concurrently(self)`) aborted the oracle process: "Rayon: detected unexpected panic; aborting".

Cause, identical in polars-lazy 0.55.2 and at da47b74 (`src/frame/exitable.rs`): the query runs on a detached `RAYON.spawn_fifo` or `spawn_blocking` job that ends in `tx.send(result).unwrap()`. When the `InProcessQuery` is dropped before the result arrives, its receiver is gone, the send fails, and rayon aborts the process on the panic.

`collect_concurrently` was already bound in production before this record, so any script could abort rnx that way.

It is now a `[[refused]]` release policy in both `0.55.2-joins.toml` and `probes/0108/v2.toml`, with the citation. `InProcessQuery`'s methods stay bound but have no producer and no oracle case.

## Launch

Measured with `probes/0073/launch.py`, 60 interleaved launches per set, old = the 0108 production build (f4a4724), new = 0109. Budget +5 ms.

| set | old median | new median | delta |
|---|---:|---:|---:|
| 1 | 17.25 ms | 18.01 ms | +0.76 |
| 2 | 15.71 ms | 17.14 ms | +1.43 |
| 3 | 18.64 ms | 19.38 ms | +0.74 |

Results are in `probes/0109/launch-results-{1,2,3}.json`.
