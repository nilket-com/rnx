# 0083 function-level generic census: evidence

Plan `f060b15`. This record changes probes and plans only; generated files, oracle results and launch measurements stay at their 0082 values (2,074 script-available, 1,312 value-tested, 1,969 unsupported, 287 refused proven pairs). Its output is an auditable implementation slice for the next record, with every candidate and refusal listed by inventory key in `probes/0083/candidates.md` and `probes/0083/census.json`.

## Gate 1: reproducible census

`probes/0083/census.py` joins the pinned `0.55.2-adapter-narrow` inventory to `adapters/polars/surface.json` by inventory key and refuses to run unless the inventory's provenance (release `0.55.2`, cfg `adapter-narrow`, the 13 resolved features) matches the shipped release file. Partition: 606 callables with a nonempty `generics_canonical`, 171 in the extractor's unsupported bucket, 435 eligible (432 canonical paths). Surface dispositions of the eligible: 113 generated, 5 adapted, 265 unsupported, 52 internal-crate out of scope. The 265 unsupported by generic role (a callable may carry several): argument 94, argument + associated 1, argument + associated + closure 1, argument + closure 10, argument + closure + return-only 8, argument + closure + return-only + unbound 18, argument + closure + self-bound 1, argument + closure + unbound 1, argument + return-only 2, argument + return-only + unbound 1, argument + self-bound 2, argument + unbound 32, closure 19, closure + return-only 8, closure + return-only + unbound 5, closure + unbound 5, return-only 13, return-only + self-bound 1, return-only + self-bound + unbound 6, self-bound 17, unbound 25. Separately, 436 receiver pairs across 34 inventory keys are unresolved for function-level generics in the family census; they are pairs of 34 methods, not operations.

A dry run of the generator with the `generic` bucket enabled (scratch copy, not committed) generated none of the 265 under today's inference rules (`Into`, `AsRef`, `IntoVec`, `Borrow`, `IntoIterator<Item = …>`); it only marked `lit` adapted. So the pool needs new inference rules, not a bucket switch; the 30 callables that dry run did generate are not function-generic and were left aside.

## Gate 2: concrete-type audit

Every unavailable callable received one decision from its generic roles, bounds and current refusal (52 candidates, 6 policy decisions, 76 callback-audit, 26 blocked by an unrelated refusal, 105 refused):

| decision | callables |
|---|---:|
| candidate: chained inference | 22 |
| candidate: iterator input, owned items | 3 |
| candidate: iterator input, owned items (per family) | 1 |
| candidate: iterator input, borrowed items | 5 |
| candidate: iterator input, borrowed items (per family) | 1 |
| candidate: family instantiation | 20 |
| policy: numeric scalar | 6 |
| callback audit | 76 |
| blocked elsewhere | 26 |
| refused: return-only | 20 |
| refused: foreign element | 26 |
| refused: Self bound | 15 |
| refused: bound | 15 |
| refused: iterator item | 10 |
| refused: unbounded generic | 9 |
| refused: free element generic | 3 |
| refused: unchecked precondition | 1 |
| refused: iterator of borrowed wrappers | 2 |
| refused: iterator without item | 1 |
| refused: other | 3 |

The family census's 436 pairs, by the same decision per method:

| decision | pairs |
|---|---:|
| callback audit | 176 |
| refused: foreign element | 112 |
| refused: Self bound | 48 |
| policy: numeric scalar | 48 |
| refused: iterator of borrowed wrappers | 16 |
| refused: unchecked precondition | 16 |
| candidate: family instantiation | 16 |
| candidate: iterator input, borrowed items (per family) | 4 |

Concrete types for the candidates. **Chained inference** (22, 21 on wrapped owners): `I: IntoIterator<Item = S>` with `S: AsRef<str>` or `Into<PlSmallStr>` becomes `I = Vec<String>, S = String`; `E: AsRef<[IE]>` with `IE: Into<Expr> + Clone` becomes `E = Vec<Expr>, IE = Expr`; `LazyFrame::rename` takes two such vectors; `Schema::try_project_with_alias` (`Vec<(String, String)>`) sits on a generic owner and stays blocked by it; `Schema::from_iter_check_duplicates` is not a chained candidate: its item converts into `(PlSmallStr, Field)`, a tuple holding a wrapped value that no proven input mapping covers (refused `bound`, found in Codex's review of the first census). The 21 direct operations: `drop_many`, `explode`, `group_by`, `group_by_stable`, `partition_by`, `partition_by_stable`, `select`, `group_by`, `group_by_stable`, `rename`, `over`, `over_with_options`, `sort_by`, `as_list`, `concat_expr`, `concat_list`, `by_name`, `cols`, `drop`, `field_by_names`, `rename_fields`. Today's `unused_generic` check refuses them because `S`/`IE` never appear in a parameter; the rule to add is that a generic bounded through another generic's `Item`/slice element is inferred with it. **Iterator inputs, owned items** (3 direct, 1 per family): a bare `Iterator`, `ExactSizeIterator` or `TrustedLen` bound with a scalar item is fed by `Vec<Item>::into_iter()`, which satisfies all three (`ListBooleanChunkedBuilder::append_iter`, `FileScanIR::gather_after_filter`, `ScanSources::gather`; per family `ChunkSet::scatter_single`). **Iterator inputs, borrowed items** (5 direct, 1 per family): `&str`, `&[u8]` and their `Option` forms are borrowed from a script vector of owned strings or bytes for the call (`ListBinaryChunkedBuilder::append_trusted_len_iter`, `ListBinaryChunkedBuilder::append_values_iter`, `ListStringChunkedBuilder::append_trusted_len_iter`, `ListStringChunkedBuilder::append_values_iter`, `FrozenCategories::new`; per family `Logical::from_str_iter`); this is an input rule, distinct from 0082's return copy. **Family instantiation** (20): free functions and builder methods over `T: PolarsNumericType | PolarsDataType | PolarsIntegerType | PolarsCategoricalType` or `T::Native` items need the applicability engine applied to a function generic; not part of the first slice. **Policy** (6): `N: Num + NumCast` from a script number needs the release file to name the concrete type. Return-only generics (20, `Series::sum::<T>` and kin) stay refused: the script cannot express the annotation (probe D shows the Rust form). Closures (76) wait for a 0079 audit entry each.

## Gate 3: ownership and execution probe

`probes/0083/probe` compiles against the adapter's exact Polars build (its `Cargo.lock` is the adapter's) and runs four checks (`cargo run` under `target/0083`): **A** chained inference on `DataFrame::select`, `drop_many`, `partition_by`, `group_by`, `LazyFrame::group_by`, `rename`, `Expr::sort_by`, `over`, `concat_list` and `cols` with `Vec<String>`/`Vec<Expr>`, including an empty selection (a zero-column frame) and the caller's vector intact after a clone-in; **B** `ChunkedArray::match_chunks` on `Int64Chunked` fed by `Vec<usize>::into_iter()`; **C** `ListStringChunkedBuilder::append_values_iter` and `ListBinaryChunkedBuilder::append_values_iter` fed by `Vec<String>`/`Vec<Vec<u8>>` borrowed for the call (strings and bytes readable afterwards, an empty vector accepted), and `ListBooleanChunkedBuilder::append_iter` meeting `TrustedLen` through `Vec::into_iter`; **D** the return-only form. All pass.

Probe B is a finding: Polars documents "it is the caller's responsibility" and enforces the single-chunk receiver and the chunk-length sum with `debug_assert!` only, then slices with `sliced_unchecked` (polars-core 0.55.2 `chunked_array/mod.rs:851-866`). The probe shows the three violations panicking in a debug build; in a release binding they would be undefined behaviour. `match_chunks` is therefore refused (`unchecked precondition`, 1 callable, 16 family pairs) unless a binding validates both preconditions itself, and the general rule follows: a function generic is admitted only when its Polars contract is checked at runtime or provable from the script values.

## Decision: the slice for record 0084

Implement chained inference first: the 21 direct operations above, all user-facing frame, lazy, expression and selector methods, with `Vec<String>` and `Vec<Expr>` inputs the existing vector rule already borrows; expected yield is up to 21 newly available operations (each has no binding today), zero receiver pairs, and one oracle case each. Second, the iterator-input rule: bare `Iterator`/`ExactSizeIterator`/`TrustedLen` bounds with scalar items via `Vec::into_iter`, and borrowed `&str`/`&[u8]` items via a script vector borrowed for the call, covering the 3 + 5 direct callables and, through the family engine, `Logical::from_str_iter` (4 unresolved pairs); `ChunkSet::scatter_single` sits on a generic owner outside the family census and needs its own pair proof; `match_chunks` stays refused. Family instantiation of function generics, the numeric-scalar policy, and the callback audits remain separate decisions. Reconciled with 0082: the 287 refused proven pairs are untouched by this record; the 436 unresolved pairs now carry a decision each.
