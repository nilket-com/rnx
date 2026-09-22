# rnx 0078: Polars `From` constructors and assignment operators

Status: revised after Codex's plan review (reviews/0078_review_codex.md),
ready for impl. The parity map (d6d5435) named this as the mechanical
record; 137 was its candidate upper bound, and the accounting below
replaces it with what the census supports, as a conditional bound.

## Problem

358 eligible callables are foreign trait impls the generator has no
Rune protocol for. Two classes among them are operations a script would
call and map with today's rules: 115 `From<X>` impls on 56 wrapped
owners (`Scalar` 16, `Expr` 10, `Series` 5, `FunctionExpr` 4, …), and 28
assignment and unary operators on six flag and selector types
(`StatisticsFlags`, `ScanFlags`, `OptFlags`, `TimeUnitSet`, `Selector`,
`DataTypeSelector`: `-=`, `&=`, `|=`, `^=` with a `Self` right-hand side,
and `!`). The rest of the 358 are `Hash` (114), `TrivialClone` (75),
`Drop` (5) and flag markers, which stay where the map put them.

## Accounting before the rule

Measured from the inventory and today's surface, not estimated:

| `From` impls | count | what happens |
|---|---:|---|
| source is a wrapped type | 85 | bound |
| source is a scalar or string | 21 | bound; 8 of them (`i8`, `i16`, `u8`, `u16`, `i32`, `u32`) narrow a Rune `int` through the checked `TryFrom<i64>` helper and are fallible, the conversion error naming the parameter; the 2 `f32` sources take a Rune `float` through the existing `as f32` cast, which is infallible and may round or overflow to infinity (see the policy below) |
| source is not mappable (`polars_arrow::Field`, `IdxItem` containers, `i128`, `u128`, `pf16`, `Vec<u8>`, `CloudWriter`) | 9 | unsupported, reason `conversion source` with the type |
| of the 106 mappable, by-reference twin of a by-value impl of the same source (`From<&SortOptions>` beside `From<SortOptions>`) | 19 | both bound, under distinct names (`from_sort_options`, `from_sort_options_ref`), each with its own oracle case against its own impl; nothing is adapted as provided by the other |
| distinct sources sharing a last segment on one owner | 1 | `Field` from `polars_core::Field` and from `polars_arrow::Field`: both names are qualified by the source's crate (`from_core_field`, `from_arrow_field`); the arrow one is then refused as unmappable |
| names already taken on the wrapper by an inherent `from_*` method | 0 today | the impl is unsupported, reason `name taken by inherent from_x`; it is not counted available |

So at most 106 `From` bindings and 106 operations, plus 28 operators
(23 assignment protocols, 5 `Not`): a conditional upper bound of 134
bindings and 134 operations, one binding per impl, before compile and
oracle. No entry is counted available because a name exists.

## Decision

### 1. `From<X>` as `from_<source>` constructors

`impl From<X> for T` on a wrapped `T` binds as a free function on the
wrapper, `polars::T::from_<source>(x)`, where `<source>` is the source
type's last path segment in snake case (`from_sort_options`,
`from_series`, `from_i64`); a `Vec<X>` source is `from_vec_<x>`, an
`Option<X>` source `from_option_<x>`. The argument goes through the
existing argument rules (a wrapped value by reference is cloned, a
scalar narrowed with a fallible binding, a string as `&str`); the
result is the wrapped `T`. The Rust call is `<T as From<X>>::from(x)`,
never a method named `from` that an inherent impl might shadow.

Collisions are resolved by naming or refusal, never by counting one
impl as covering another:

- A by-reference impl and its by-value twin on one owner are two
  operations. Both are bound, the by-value one as `from_<source>` and
  the by-reference one as `from_<source>_ref`; the Rune argument is the
  same kind of value on both (cloned out of the Rune value), but each
  binding calls its own impl by UFCS (`<T as From<X>>::from(x)` against
  `<T as From<&X>>::from(&x)`), each has its own oracle case against
  that impl with the source checked unchanged afterwards, and each is
  value-tested only when its own case passes. Rust permits the two
  impls to differ, and the record makes no equivalence claim.
- Two distinct sources whose last segment coincides on one owner get
  names qualified by the source's crate short name (`from_core_field`,
  `from_arrow_field`), deterministically for both, so neither is the
  "first".
- A name an inherent method already holds is a different operation:
  the `From` impl is unsupported with the reason `name taken by inherent
  from_x`, and stays out of "available".

The scoreboard's "available" therefore stays what it is: generated
entries plus the nine hand-written equivalents named in the script.
Nothing is promoted by a reason string.

**`f32` policy.** The record keeps the existing conversion for `f32`
parameters: a Rune `float` is cast with `as f32`, which never fails,
rounds to the nearest `f32`, and turns a magnitude above `f32::MAX`
into infinity; the catalogue says "float (as f32)". Changing that to a
checked conversion would change every existing `f32` argument and is a
policy record of its own. The Rust oracle applies the same cast before
the `From` call, so both sides see the same rounded or infinite value.

### 2. Assignment operators as Rune assign protocols

`SubAssign`, `BitAndAssign`, `BitOrAssign`, `BitXorAssign` with a `Self`
right-hand side bind as Rune's `SUB_ASSIGN`, `BIT_AND_ASSIGN`,
`BIT_OR_ASSIGN`, `BIT_XOR_ASSIGN` protocols, which rune-core 0.14.2
defines beside `ADD_ASSIGN`. Mutation semantics are the existing
`&mut self` policy: `x -= y` mutates the Rune value `x` in place, `y`
is cloned out of its Rune value and stays usable, nothing is returned.
The oracle treats them as mutating cases: the receiver's state after
the operation is compared, and the second-call check does not apply.

`Not` has no Rune protocol in 0.14.2 (no `NOT` beside `NEG`), so `!x`
cannot dispatch to an external type. It binds as the instance method
`not_()` (the trailing underscore because `not` is a keyword),
returning a new value and leaving the receiver unchanged; the catalogue
says so, and the entry is generated with the note "Rune has no unary
NOT protocol; bound as a method".

### 3. Reporting

Newly available operations (generated plus adapted-as-provided) and
newly value-tested operations are reported apart, by class, against
the 0077 scoreboard, with the refusals by reason (`conversion source`,
the taken names, the unmappable second `Field`). Each `From` case needs
a fixture for the source and a comparator for the owner; a case without
either is unverified with that reason, and the evidence says how many
of the 87 that leaves.

## Gates

1. **Census and naming.** The `From` impls and operators listed in
   `surface.json` under `conversions` with their source, target, the
   name the rule gives, and the disposition before emission; the
   collision rule applied and its outcomes counted. Controls (self-test,
   synthetic inventory): a by-reference twin gets its own `_ref` binding
   and no adapted entry, including a synthetic pair whose two impls
   return different values, which must yield two bindings and two cases;
   two distinct sources with one last segment get crate-qualified
   names; an unrelated inherent `from_x` makes the impl unsupported with
   the collision named; an integer source is fallible and an `f32`
   source is not; an unmappable source is refused with the type named.
2. **Bindings and oracle.** Emission, compiled as the second check; the
   `From` cases through the existing constructor case shape (receiver
   none, one argument), each impl its own case with the source checked
   unchanged afterwards; the assign operators through the mutating case
   shape, `not_` through the ordinary instance shape. Controls through
   the generated runner, with nontrivial operands (flags with several
   bits set, selectors that differ): `x -= y`, `&=`, `|=`, `^=` change
   `x` the way the Rust impl does, the full left value compared, and
   leave `y` unchanged; an aliased operand (`x |= x`, and a second Rune
   binding to the same value) is refused by Rune's dynamic borrow check
   with an error and no partial mutation, the value compared unchanged
   afterwards, which is the intended result; `not_()` on an input whose
   complement differs returns the complement and leaves the receiver
   unchanged; `from_i8` at the endpoints `-128` and `127` succeeds and
   at `128` refuses with the conversion kind on both sides; `from_f32`
   with `0.1` gives the `f32`-rounded value and with `1e40` gives
   infinity on both sides; a `from_<wrapped>` and its `_ref` twin each
   equal their own Rust impl on the fixture.
3. **Evidence.** Available and value-tested operations by class against
   the 0077 scoreboard; refusals by reason; the drift, accounting, lib
   and harness suites, clippy in both configurations; cold launch three
   times against the 0077 binary; the adjacent 0.54.4 check and the rc2
   experimental run; the scoreboard script updated for the adapted
   "provided by" entries; README updates.

| number | how it is measured |
|---|---|
| newly available operations | generated `From` and operator entries plus entries adapted as provided by a twin binding, by class |
| newly value-tested operations | those with at least one oracle value match, by class |
| refused | by reason: unmappable source, taken name, unmappable second source |
| cost | launch medians before and after |

## Out of scope

`Hash`, `TrivialClone`, `Drop` and flag markers (no script-level
operation); `From` impls on unwrapped owners; a `NOT` protocol
(upstream Rune); the other foreign trait impls (`Extend`, `FromIterator`,
`IntoIterator`, `Deref` on other types) that the map lists under trait
impls in the generic bucket.
