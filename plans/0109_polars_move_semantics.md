# rnx 0109: move semantics for non-Clone values (batch rule 1)

Status: plan. The plan was drafted by Claude while Codex is paused for the user's quota, and awaits Codex review before push. It builds on 0108 (`feb609f` + `1ae609e`, local). This is the first of the ten 0108 batch rules, and it is one rule, not one per method.

## Target

At the v2 pin, 0108 records 245 applicable callables that are missing with the `non_clone_move` cause:

- **238 consuming methods on a non-`Clone` owner.** These are the Expr namespaces `str` 56, `dt` 42, `list` 31, `arr` 24, `bin` 13, `name` 8, `cat` 7, `meta` 7, `struct_` 7, `map_` 6 and `ext` 2, plus `DslBuilder` 16, `JoinBuilder` 15 and five single builders or writers.
- **Four by-value arguments of a non-`Clone` type.**
- **Three derived listings** (a conversion source, a duplicate, and an every-implementor listing).

All eleven namespace constructors (`Expr::str` and the rest) are already bound and return the wrapped namespace. Only the methods on the namespaces are refused. A script therefore cannot write `e.str().contains(p)`, one of the most-used operations in Polars.

## Rule

A `self` receiver or top-level by-value argument whose type is a wrapped non-`Clone` owner is **moved out of the Rune value**, as in Rust:

- The binding takes `this: W` (or `arg: W`) by value and passes `this.0`.
- Rune's owned conversion (`FromValue for T: Any` → `AnyObj::downcast`, rune 0.14.2 `runtime/any_obj.rs:210-235`) takes the value and marks the slot as taken. Any later use of that script variable is an access error, never a stale or aliased value.
- Clone owners keep today's behaviour: the value is cloned and stays usable.
- Arguments nested in a container (a `Vec`, an `Option`) stay refused. They are converted through borrowed elements, and moving out of a shared container needs its own contract.
- The deref route, a `self` receiver through `Deref`, stays refused as today.

This mirrors Rust exactly, so namespace chaining (`e.str().contains(p)`) and builder chaining (`b.a().b().finish()`) read naturally. Unlike a replaying wrapper, no hidden copy of the source is kept.

## Oracle

A moved receiver cannot take the oracle's usual second call on the same value. Its generated case therefore calls once (`has_receiver: false`), and the Rust side is unchanged (`callee(__recv, args)`). A focused test file (`tests/move_semantics.rs`) checks four things:

1. A namespace method and a builder chain run through the generated bindings and match direct Rust.
2. Reusing a moved namespace or builder is an access error, not a value. The moved-argument path is covered at generation only, by the self-test below. At both pins no callable that otherwise binds takes a top-level by-value non-`Clone` argument, so no script-constructible case exists. (Corrected on review: the draft also promised a runtime argument case.)
3. A Clone receiver (`Expr`) is still reusable after a consuming call.
4. A nested non-`Clone` argument stays refused.

## Checks

- Generator self-test `move-semantics`:
  - A non-`Clone` `self` receiver emits `this: W` with `this.0`.
  - A `Clone` one still clones.
  - A top-level non-`Clone` argument moves; a nested one is refused.
  - A deref route stays refused.
- Production 0.55.2 is regenerated. The 0.55.2 pool of the same cause (for example `ListNameSpace::agg`) now emits too, and the 0.55.2 drift, suites, scoreboard and oracle results are recorded.
- The v2 probe (`probes/0108/build.sh`) is rerun, and the census and batch table are refreshed. The evidence quotes the new v2 applicable and available counts and the moved-cause remainder, with a reason for every exception.
- Launch cost is measured against the 0108 production binary with `probes/0073/launch.py` (+5 ms budget), because production gains bindings.
