# rnx 0077: Polars iterator returns, evidence

Plan: `plans/0077_polars_iterator_returns.md` (c30e153, revised after
Codex's plan review). Production stays on 0.55.2; the baseline is record
0076 at a7434f8: 1922 generated callables, 2539 bindings, 1859 cases,
1679 verified, 180 setup failures.

## Gate 1: census

`surface.json` records under `iterators` every eligible API callable
whose return is an iterator and every instantiation pair whose return
is one, each with its item type, whether the length is known exactly,
the wrapper (`Plain`, `Option`, `Result`) and its disposition after
emission, and `materialize_limit` (1 048 576). The recognizer
(`iterator_return`) accepts the bound spellings the inventory shows
(`impl '_ + Send + Sync + ExactSizeIterator<Item = i64>`,
`impl PolarsIterator<Item = Option<&[u8]>>`, `impl DoubleEndedIterator<
Item = &[f64]>`, `Option<impl DoubleEndedIterator<Item = &[&[u8]]>>`,
`PolarsResult<impl Iterator<Item = Series>>`, `impl TrustedLen<Item =
usize>`), tells known from unknown length, sees the wrapper, and rejects
`impl Display` and an iterator without an `Item` (self-test).

| iterator candidates | count | disposition |
|---|---:|---|
| callables | 45 | materialized 8; item not mappable 23 (bare slice 30 pairs and callables together, unreachable polars types 27, generic types 24, foreign 5); not eligible 13 (`unsafe`, `_iter_struct_av`); out of scope 1 (`indexes_to_usizes`, internal crate) |
| instantiation pairs | 143 | emitted 36; refused 89 (item not mappable); not eligible 16; excluded by the release file 2 (`StructChunked::iter`, `no_null_iter`, `no_call_const`) |

The plan's 39 became 45 once trait methods and internal-crate callables
whose return is an iterator are counted the same way; the plan's 125
pairs are the 89 refused plus the 36 now emitted.

## Gate 2: rule and controls

Three helpers implement the inclusive contract, one per way a length
is known: `support::materialize_exact` (`ExactSizeIterator`, and
`PolarsIterator`, which has it as a supertrait: `len()` compared with
the bound before the first item), `support::materialize_trusted`
(`TrustedLen` only, which Polars 0.55.2 defines as `unsafe trait
TrustedLen: Iterator {}` with no `ExactSizeIterator` supertrait: the
contract's `size_hint` upper bound compared with the bound before the
first item, an unrepresentable bound refused the same way, the count
guard retained), and `support::materialize_unknown` (one item at a
time, at most the bound converted, one further `next` whose item is
discarded decides). The recognizer keeps the strongest knowledge among
a return's bounds; both take the bound as a parameter so the
controls run the production code path, and nothing is reserved from a
`size_hint`. A refusal is `polars::Error` of kind `MaterializeLimit`
with no partial vector. The generated binding creates, drives and
converts the iterator inside one block, which is the routed closure
when the binding is routed (`crate::engine::run(move || { let __it =
…; support::materialize_… })`), so only the owned vector leaves the
engine thread; a `&mut self` iterator is refused with the reason
`mutable iterator receiver` (`DataFrame::split_chunks`).

Controls, all passing:

- Unit (`support.rs`, with a counting iterator): a `TrustedLen`-only
  opaque return with a mappable item compiles against the trusted
  helper (the exact helper would not, as Codex's reproduction showed),
  materializes by its upper bound, refuses an over-limit or
  unrepresentable bound with zero `next` calls, and is still caught at
  L + 1 when the bound understates; empty, L − 1, L
  (succeed, L + 1 `next` calls, L conversions), L + 1 (refuses,
  exactly L + 1 `next` calls, L conversions, the lookahead item not
  converted), an unbounded iterator (refuses at L + 1 calls), an
  overstated `size_hint` (driven and counted, not trusted), a known
  length over the limit (refuses with zero `next` calls), a conversion
  failure (stops, returns its error), the production bound as the
  default; a routed borrowed iterator whose `next` runs on the
  `rnx-polars-engine` thread under an entered Tokio runtime, its
  borrowed elements detached before the owner drops.
- Integrated (generated runner, through `DataFrame::materialized_column_iter`
  on the three-column fixture and the test-support limit): a bound of 2
  refuses on both sides with `MaterializeLimit`; a bound of 3 succeeds
  and matches; the receiver is usable after a refusal; a value against a
  claimed refusal is a mismatch.

## Gate 3: emission

| | count |
|---|---:|
| newly compiled bindings | 41 (36 instantiation, 4 implementor, 1 inherent), 12 distinct callables, 8 of them concrete |
| newly verified cases | 29 match; 12 setup failures on the narrow-integer aliases |
| callable coverage | 1922 to 1930 |

Compilation was the second check: `StructChunked::iter` and
`no_null_iter` reach `StaticArray::iter` and `values_iter`, marked
`no_call_const` (polars-arrow-0.55.2 `array/static_array.rs:80,83`),
and are excluded by the release file with that citation. Two cases,
`row_decode_ordered` and `row_decode_unordered` on `BinaryOffsetChunked`,
decode arbitrary bytes as row-encoded data and differ run to run; they
are excluded from the oracle by the release file with that reason.

## Oracle

Vectors and options are compared through a length-framed text on both
sides (`[3:a, b, 1:c]`, `Some(1:x)`), so equal-length vectors whose
element texts would join alike stay apart; the Rust-side tuple
formatter is framed the same way, but the script side declines tuples,
so a tuple result is unverified ("return type has no comparison"), not
compared; a `Repr::Seq`
structural representation exists in `oracle.rs` with the same
guarantee and its own unit controls. Runner controls: `["a, b", "c"]`
against `["a", "b, c"]` is a mismatch; a changed element, reordered
elements (under either policy), and `Some` against `None` fail; the
same vector matches. A materialized element of series or frame type is
rendered through the structural comparator.

## Gate 4: BinaryOffset

`series_binary_offset` is built by `from_any_values_and_dtype` with
`AnyValue::Binary` (the probe in `probes/0077/dtype-probe` shows `cast`
keeps `Binary`); the `BinaryOffsetChunked` cases went from 36 setup
failures to 31 matches and 1 both-error (`row_decode_*` excluded). The
four narrow-integer aliases keep their setup failures (39 each, 156 in
all), the probe's finding recorded: no route builds `Int8`, `Int16`,
`UInt8` or `UInt16` under the adapter's features; enabling their dtypes
is a separate cost decision.

## Gate 5: totals

`probes/0076/reconcile.py` against a7434f8:

| level | 0076 | now | delta |
|---|---:|---:|---:|
| callables generated / unsupported | 1922 / 2114 | 1930 / 2106 | +8 / −8 |
| bindings: inherent / implementor / instantiation | 1038 / 104 / 650 | 1039 / 108 / 686 | +1 / +4 / +36 |
| bindings total | 2539 | 2580 | +41 |
| oracle cases / verified | 1859 / 1679 | 1898 / 1742 | +39 / +63 |
| match / both_error / both_panic / row-order | 1562 / 92 / 23 / 2 | 1624 / 93 / 23 / 2 | |
| fixture_failed (counted apart) | 180 | 156 | −24 |

Cold launch (`probes/0073/launch.py`, three 60-run interleaved
measurements against the 0076 binary, `probes/0077/launch-results-*`):

| measurement | 0076 median ms | 0077 median ms | delta |
|---|---:|---:|---:|
| 1 | 18.89 | 18.25 | −0.64 |
| 2 | 21.09 | 19.31 | −1.78 |
| 3 | 18.83 | 18.29 | −0.54 |

41 more bindings are inside the host's noise; the absolute numbers are
higher than earlier sessions', as before.

Unchanged and passing: drift, accounting with the pair-level
reconciliation, the deref controls, 32 lib controls, both harness tests,
clippy with and without test-support. Adjacent 0.54.4: generated 1876,
`cargo check` ok. rc2, experimental: build, oracle and controls pass;
generated 2101, match 1688, both_error 101, both_panic 25,
fixture_failed 153, row-order 7 under the unordered policy. The
unrecognized-configuration join control now uses an explicit
`LeftRight` order, deterministic on every release, after the `None`
order coincided with the reversed oracle at rc2.

## Codex review round 1 (300e867): one correction

`TrustedLen` had been classified as an exact length and routed to the
`ExactSizeIterator` helper, which an opaque `TrustedLen`-only return
cannot satisfy (compile failure, reproduced by Codex; today's emitted
bindings were unaffected because every mapped return also declares
`ExactSizeIterator` and the bare `TrustedLen` returns have unmappable
items). The generator now keeps three kinds of length knowledge and
selects `materialize_trusted` for a `TrustedLen`-only return; the
helper and its controls are described above. The scoreboard reports the
remainder as unsupported operations plus marker entries, and the nine
hand-written equivalents as separately evidenced.
