# rnx 0115 evidence: a family registry for polars-gen

17 release-listed families and the pipeline's 8 dispatch rules now plug into the generator through one `Family` trait and per-hook registry lists. The 16 per-callable mode slots on `World` and the 8 family slots on `Oracle` are gone. Each family reads only its own state, which lives in `World::active` / `Oracle::active` for exactly the callable, pair or case being emitted. `probes/0115/verify.sh` replays every proof below from a clean checkout, building the 0114 generator (`32d1b40`) in a worktree. It takes about 3 m 10 s.

For every release file in the repository the output is byte-identical: no bindings, no counts, no oracle case changes. The one intended behaviour change is that a release file with an unknown top-level key is refused.

## The registry

| hook | list (order at `32d1b40`) | semantics |
|---|---|---|
| `listed` | `CALLABLE`: cow_return, bounded_readback, bitmap_return, bitmap_input, iterator_return, hash_token | collect every applicable state until the first refusal |
| `listed_pair_pre` | `PAIR_PRE`: scalar_generic, null_aware | collect until the first refusal |
| `listed_pair_post` | `PAIR_POST`: chunk_snapshot, indexed_chunk, layout, owned_iter, view_snapshot, iter_snapshot, array_snapshot, sized_self | collect until the first refusal |
| `ret` at `Top` | `RET_TOP`: layout, owned_iter, view_snapshot, iter_snapshot, array_snapshot, indexed_chunk, chunk_snapshot, null_aware, iterator_return | first answer |
| `ret` at `Cow` / `Scalar` / `Unwrapped` | cow_return / hash_token, bounded_readback / bitmap_return | first answer |
| `arg` at `Top` / `Scalar` | bitmap_input / arg_guard, hash_token | first answer |
| `check` | `CHECK`: layout, owned_iter, view_snapshot, iter_snapshot, array_snapshot, indexed_chunk, chunk_snapshot, sized_self, null_aware | run all; each preflight is prepended; the first refusal ends emission |
| `oracle_state` | `ORACLE_SCRIPT_STATE`: layout; `ORACLE_STATE`: hash_token, layout, owned_iter, view_snapshot, iter_snapshot, array_snapshot, indexed_chunk, chunk_snapshot | set for `script_fmt` / `oracle_fmt` respectively, cleared after |
| `oracle_fmt` at `Top` / `Ref` / `Scalar`; `script_fmt` | six snapshots / chunk_snapshot / hash_token; layout | first answer |
| `claim` | `CLAIM`: instantiation_route (0076), iterator_return_route (0077), out_of_scope, index, from_iter, unary, operator, generic_impl_refusal (0113) | first claim; an unclaimed callable goes to `emit_callable` |

**The scopes.** `Active::enter(scope, states)` replaces the states of that scope's families, and the guard clears every family of the scope on drop. That is exactly the old set-all-then-clear-all blocks: the two copied `BitmapScope` guards in `emit_callable` and `emit_method_with`, the nine sets and clears around `emit_method` in `emit_instantiations`, the `arg_guard` guard in free instantiations, and the oracle's sets and clears. There are four scopes: `CALLABLE`, `PAIR`, `FREE` and the two oracle lists.

**The sites.** `World::ret`'s top arms are replaced one to one by the `Top` dispatch, because they were a contiguous sequence of family blocks. The other sites are named where they were:
- `Cow` sits immediately before `match t`. The only arms before the old guarded `Cow` arm are `Tuple` and `Ref`, which cannot match a path.
- `Scalar` sits at the head of the path arm. The arms before the old guarded `u64`/`usize` arms name `bool`, `i64`, `f64` and `f32`.
- `Unwrapped` sits in place.
- For `arg`, `Top` sits in place and `Scalar` sits at the head of the path arm, before the `bool`/`i64`/`f64`/`f32` literals.
- For the oracle, the `Top` arms are one to one, and `Ref` sits after them and before `match t`. The only arms before the old chunk arm are the tuple arms and the `&str` reference arm, which cannot match a chunk list. `Scalar` sits at the head of the path arm, where the bitmap/`bool`/`f32` arms before it name other types. The script tuple sits in place.

**Moved bodies.** Each moved arm body keeps its text. The substitutions are:
- the state read: `self.array_snapshot.borrow().clone()` → `state.two()`;
- `self.` → `world.` or `o.`;
- `return Ok(x)` → `return Ok(Some(x))`, so that "no answer" still falls through.

Guarded match arms whose body is an expression run it inside `(|| -> Result<…> { body })()`, keeping `return Err` and `?` exact. The small scalar arms that used `ret`'s local `r(...)` closure are written out as that closure's expansion. The checks and pair gates are written out with the same categories and reason strings.

**Release tables.** `release_schema!` declares `Release` and the flattened `FamilyTables` together with their `KEYS`. The 31 self-test `Release { … }` literals now name only the tables they use and take the rest from `FamilyTables::default()`, which removed 774 lines.

## Proof 1: byte identity

`probes/0114/golden.sh` runs `32d1b40` and 0115 over the full matrix:
- 60 runs (16 of which generate), covering every generated file, stdout, stderr and exit code;
- plus the `--check` pass, the `--check` drift case and the `--self-test` output.

`diff -r` is empty, and the new outputs equal 0114's committed 298 digests.

## Proof 2: hook coverage (`probes/0115/coverage.sh`, `coverage.tsv`)

`POLARS_GEN_TRACE=<file>` writes every hook a family takes, with its target: the callable key, or `key|alias` for a pair. Over the 16 generating runs, 91 (hook, family, outcome) lines are reached (`coverage.tsv`, committed with counts and aggregated over targets). All eight claims are among them. **No shipped run ever enters a scope with two families**: every `scope` line names one family. That confirms the review's point, that the shipped releases barely overlap, and it is why proof 3 exists.

Tracing `--self-test` adds 13 triples: the `ret` refusals of the nine arms that can refuse, plus the `check` refusals of `null_aware`, `owned_iter` and `sized_self`, and the `hash_token` listing refusal.

The overlap cases (proof 3) add the `null_aware` and `scalar_generic` pair refusals. Of the triples the code can produce, eight are reached by no inventory, because an earlier gate always refuses first:
- the `check` refusals of the five borrowed snapshots and of layout: each family's pair gate checks the method's own shape, so a mismatched method is refused before `check` runs;
- the `sized_self` pair refusal: the census marks a malformed listing as unresolved first;
- the `arg_guard` refusal: the free instantiation's guard shape refuses an unknown guard first.

`families::tests::unreachable_refusals_are_named` calls each of these hooks directly and asserts the exact category and reason that `emit_method_with` or the pair loop would report. The other enumerated combinations cannot occur by construction: a snapshot check never preflights, a preflight check never passes, only `hash_token` can refuse at the callable scope, and `ok_arg` never fails.

## Proof 3: overlap, byte for byte (`probes/0115/overlap.py`, `overlap.sh`)

Fourteen synthetic cases run against the pinned 0.55.2 inventory, with release files derived from `0.55.2-joins.toml` (same provenance). The generators compared are `32d1b40` and 0115. Every case's full output is identical: the seven files including `surface.json` with every diagnostic and exception, the oracle tests and fixtures, stdout, stderr and the exit code. The 0115 trace reaches every line each case expects; a case that stops short fails.

| case | forces | result |
|---|---|---|
| `o1_arg_and_ret` | two synthetic clones of `with_validity`: a bitmap input with a `Cow<Self>` return (bitmap_input + cow_return), and with a `usize` return (bitmap_input + bounded_readback) | both families are active in one scope (`scope cow_return+bitmap_input`, `scope bounded_readback+bitmap_input`); `arg` and `ret` are both taken; each clone generates 16 bindings |
| `o2_*` (5) | one method listed under two snapshot families | the second family's pair gate refuses the method; where the first family precedes it in `PAIR_POST` (iter→array, view→iter, owned→view), the first family's state was collected before the refusal |
| `o3_pair_refusal_after_state` | `chunks` as a chunk snapshot (all pairs) and an array snapshot (Int64 only) | chunk_snapshot is active, then array_snapshot is refused |
| `o3_callable_refusal_after_state` | `rechunk` as a Cow return and a malformed hash token | cow_return is active, then the hash_token listing is refused ("release policy") |
| `o4_rejected_then_valid_pair` | `chunks` listed for Int64 only | 15 pairs refused ("not a listed pair"), and the Int64 pair generated |
| `o5_listed_then_unlisted` | `to_physical_repr` 3611 listed and 3641 (same path, emitted next) unlisted | 3611 is generated; 3641 is refused with "foreign type: … Cow<StructChunked>". Listed in production, 3641 generates one binding, so no `Cow` state reached it |
| `g_null_aware_pair_refused`, `g_scalar_generic_pair_refused` | a type dropped from the listing | the pair gate refuses it |
| `g_sized_self_upstream`, `g_arg_guard_upstream` | a malformed listing | refused upstream, by the census and by the guard shape; byte-identical either way |

## Proof 4: order and scope tests

- **Orders.** `probes/0115/orders.py` derives nine registry lists from the `32d1b40` source: `RET_TOP`, `CALLABLE`, `PAIR_PRE`, `PAIR_POST`, `CHECK`, `ORACLE_TOP`, `ORACLE_STATE`, `RET_SCALAR` and `ARG_SCALAR`. It uses each site's `record NNNN` block order, or its slot assignment order. It also derives `CLAIM`, from the order of the inline chain in `pipeline.rs`. All ten equal the registry. The remaining lists hold one family each, or are the clear-only `PAIR`/`FREE`.
- **`hook_orders_are_pinned`.** It pins every list, and asserts that every family a pair gate can set is cleared by `PAIR`.
- **`scopes_replace_and_clear`.** An inner callable scope replaces and then clears the callable families; a pair scope's state survives it; nothing survives the outer drops.
- **`collect_gathers_until_the_first_refusal`.** It tests exactly what its name says.
- **No leak.** The cow-return self-test now also asserts that nothing is active between the listed callables and the unlisted ones it emits next on the same owner.
- **Unknown keys.** `unknown_release_keys` checks the key gate:
  - `[[array_snapshot]]` is refused by name;
  - `[[array_snapshots]]` loads, and the flattened table is read;
  - every shipped release file passes;
  - the flattened field is not itself a key, and no key is declared twice.

  End to end, the real binary refuses the typo file with exit 2 ("unknown top-level key `array_snapshot`") and writes nothing.

`cargo test --release --locked`: 41 of 41 pass (34 self-test wrappers and 7 registry tests). `--self-test` output is unchanged, and golden compares it.

## Proof 5: mutations (`probes/0115/mutations.sh`)

| mutation | caught by |
|---|---|
| `RET_SCALAR`'s two families swapped | `hook_orders_are_pinned` |
| the scope guard's clear removed | the scope test and the cow-return no-leak assertion |
| `ARRAY` dropped from `RET_TOP` | golden (outputs differ from `32d1b40`) |
| `collect` made first-match | the overlap comparison |
| the unknown-key check removed | `unknown_release_keys` |
| two claims swapped in `CLAIM` | `hook_orders_are_pinned` |
| preflights placed in list order instead of before the earlier ones | `checks_run_all_until_a_refusal` |
| array and indexed swapped in `RET_TOP` | `ret_first_answer_wins` |

## Size

| file | code lines, `32d1b40` → 0115 |
|---|---:|
| `world/mapping.rs` | 1,748 → 1,198 |
| `world/mod.rs` | 755 → 705 |
| `emit/callable.rs` | 987 → 731 |
| `emit/instantiations.rs` | 490 → 201 |
| `oracle/mod.rs` | 1,037 → 915 |
| `oracle/emit.rs` | 1,283 → 1,189 |
| `families/mod.rs` | 11 → 445 (trait, lists, scopes, trace) |
| `families/snapshots.rs` | 621 → 1,921 |
| `families/bounds.rs` | 434 → 813 |
| `families/returns.rs` | 31 → 220 |
| `families/generic_inputs.rs` | 21 → 160 |
| `families/free_instantiations.rs` | 181 → 236 |

**Net change.**
- The shared files lost about 1,380 code lines.
- The whole crate grew from 23,882 to 24,838 lines (+4%).
- About 600 of the added lines are hook signatures: rustfmt writes each of roughly 60 hook functions' seven-parameter signatures on about ten lines.
- About 330 are the new tests.

If the signatures are worth shrinking, a context struct per hook would cut them to about three lines each. That would change every moved body's parameter names, so it is not part of this record.

No family state remains on `World` or `Oracle` outside `active`. The remaining cells are the temp counter, the oracle's shown-type set and the mask-length fixture hint, none of which is family state. Build warnings are the base's 23, with none new; `cargo fmt --check` and `git diff --check` pass.

## Authoring rule

`tools/polars-gen/README.md` states it. A release-listed family is one module:
- it implements `Family`, and puts its table in `FamilyTables`, whose field names are the accepted release keys;
- it adds one line to each list it uses, at a position its plan argues;
- it reads only its own state.

`POLARS_GEN_TRACE` shows what it reaches.

## Review round 1 (Codex)

**1. The `claim` hook was missing.** `pipeline.rs` still hard-coded the 0076/0077/0113 chain that the plan puts under `claim`. It is now implemented as promised:
- **The rules.** `Family::claim(cx, c)` receives a `Claims` context: the world, the output, the release, the buckets, the census pairs, and the pending operator and `INDEX_GET` groups. The eight rules are claims, in `CLAIM`, in the inline chain's order. 0076, 0077 and out-of-scope live in the new `families/routes.rs`; the five 0113 claims live in `families/generic_impls.rs`.
- **The moves.** Each claim's body is the old branch, with the pipeline's locals read from the context. The deferred groups moved verbatim into `generic_impls::finish`, and the census disposition pass moved verbatim into `census::census_dispositions`.
- **The pipeline.** `pipeline.rs` is now 87 lines (it was 271): sort, census, claim or `emit_callable`, finish, dispositions.
- **Proof.** Golden is identical. `orders.py` derives `CLAIM` from the base chain, the coverage run reaches all eight claims, and the claim-order mutation is caught.

**2. The competing `ret` arms were not proven, and the trace had no target.**
- **Per-target traces.** Every trace line now carries its target: the callable key, or `key|alias` for a pair. The target is set in `pipeline` (per callable), `emit_method_with`, the instantiation pair loop, the free-instantiation loop and the oracle's case loop. Nested targets restore the outer one.
- **Grouped expectations.** Each overlap expectation is a group whose lines must all occur on one concrete target matching its pattern, and a `!` group must be absent. So each case now proves its states met on the same callable or pair:
  - O1: `scope cow_return+bitmap_input`, `arg bitmap_input taken` and `ret cow_return taken` all on `synthetic:0115-1|StructChunked`, and on its 15 other aliases; likewise for the `usize` clone.
  - O2: where the first family comes earlier, its state is collected and the second family refuses on the same `…|Int64Chunked` pair. Where it comes later, it is proven never collected.
  - O3 pair: chunk active and array refused on `3681|Int64Chunked`.
  - O3 callable: cow active and the hash listing refused on `3434|…`.
  - O4: refused on `3681|Int32Chunked`, generated on `3681|Int64Chunked`.
  - O5: taken on `3611|ListChunked`, with no `cow_return` line at all on any 3641 target.
  - The two upstream cases: no gate or hook line on their targets.
- **Collision controls.** Real return shapes cannot collide on a first answer: each pair gate checks its own method's shape first, as O2 shows. So two direct two-family controls were added:
  - `ret_first_answer_wins`: array and indexed are active together on `Option<&PrimitiveArray<i64>>`. The earlier array snapshot answers (its refusal), whatever the order of activation; alone, the indexed snapshot converts it.
  - `checks_run_all_until_a_refusal`: layout and sized-self both preflight, and the later family's preflight is placed first and makes the binding fallible; a layout preflight followed by a view refusal ends with the view's category.

  To test the checks directly, the inline loop moved into `families::run_checks`. Its preflights are prepended as before, and golden is identical.

**Size after round 1.** The crate is 25,245 lines (it was 23,882 at `32d1b40`). `pipeline.rs` lost 184 lines and `emit/callable.rs` 260. The extra lines are the `claim` implementations, `routes.rs`, and the new tests.
