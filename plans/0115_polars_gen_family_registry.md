# rnx 0115: a family registry for polars-gen

Status: plan. Record 0114 is closed on `origin/main` at `32d1b40`. That record moved the generator into modules without changing control flow, and Codex asked for the registry to be a separate record. This is that record. It changes how family rules plug into the generator. For every release file in the repository, the generator's output stays byte-identical: no bindings, no counts. The one intended behaviour change is that a release file with an unknown top-level key is now refused instead of silently ignored.

## Why

0114 put each family's gate and emitter in its own module, but a family's rule still runs from up to seven shared places. The array-snapshot rule (0102) touches:
- a table field on `Release`;
- a `RefCell` slot on `World`;
- a per-pair gate in `emit_instantiations`;
- a return arm in `World::ret`;
- a consistency check in `emit_method_with`;
- a mirrored slot and an arm in the oracle;
- an empty-table line in each of 31 `Release { … }` literals across 13 self-test files.

The six other snapshot families (0099–0106) and the listed-bound families (0087, 0088, 0091, 0093, 0094, 0096, 0097) follow the same pattern. Across them, the shared code carries:
- **16 per-callable mode slots on `World`** (`Cell`/`RefCell`): set in three places (`emit_callable`, `emit_method_with`, `emit_instantiations`) and cleared in three. Two of those reset blocks are the same six-field guard, copied.
- **10 more slots mirrored on `Oracle`**, set and cleared by hand in `emit_oracle`.

Deep mapping code reads these slots with no signature naming them. This is action at a distance: a family sets a flag in one file, and a return arm in another file changes behaviour because of it. It is also why the family modules 0114 created are thin. `returns.rs` has 31 lines of code and `generic_inputs.rs` has 21, because their rules live in `world/mapping.rs` (1,748 code lines) and `emit/callable.rs` (987).

## The design

**1. `trait Family`**, in `families/mod.rs`. Each hook is named after the existing site it replaces, and every hook defaults to "not mine":
- `claim(world, c)`: the pipeline chain, before `emit_callable`. This covers the 0113 index, `FromIterator`, unary, operator and refusal rules, the 0076 instantiation route and the 0077 iterator-return route.
- `listed(world, c, pair)`: a callable's release-listed gate, today split between the `emit_callable`/`emit_method_with` guards and the per-pair gates in `emit_instantiations`. It returns the family's typed state for this callable or pair, or a refusal reason.
- `arg(…)` and `ret(…)`: the `World::arg`/`World::ret` arms that exist only for a listed family.
- `check(…)`: the consistency and preflight checks in `emit_method_with`.
- `oracle(…)`: the family's oracle state and case arm.

Each family module implements the hooks it has, next to its gate, release schema and self-test.

**2. One active scope** replaces the 16 slots on `World` and the 10 on `Oracle`. `World.active` holds each family's typed state for the callable being emitted. It is set by one function from the `listed` results, and cleared by one drop guard, on every path. The mapping and check hooks receive their own family's state as an argument, so no hook reads another family's state implicitly.

**3. Release tables grouped** into one `FamilyTables` struct, `#[serde(flatten)]` into `Release`, with `Default`. The TOML format does not change: every release file parses to the same values, and `surface.json` keeps its release name, source and SHA-256. Self-tests build releases with `..Default::default()`, so a new family no longer edits 31 literals.

**4. Unknown top-level keys are refused.** `Release` has no `deny_unknown_fields`, so today a misspelled table name (`[[array_snapshot]]` for `array_snapshots`) is silently ignored, and flattening would keep it that way. `Release::load` now rejects any top-level key outside the schema before generation, naming the key and the file.
- **The accepted keys** come from one declaration. A macro declares `FamilyTables`' fields and, in the same place, the list of their names; `Release`'s base fields are declared the same way. Adding a table therefore adds its key, and no second list is maintained by hand.
- **Controls:** a release file with `[[array_snapshot]]` is refused with that key named; the same file with `[[array_snapshots]]` loads.
- **Unchanged:** the five shipped release files have no unknown keys (checked), so their golden outputs stay byte-identical.
- **Out of scope:** nested fields inside a table are not validated in this record.

**Out of scope.** Shared mapping capabilities stay in `world/mapping.rs`: 0082 slices, 0084 iterator inputs, 0109 moves and 0076 alias instantiation. They apply by type shape, not by a release listing. Protocols (`emit_foreign`), serde and callbacks already enter through one call into their own modules, and stay as they are.

## Order is per hook, and pinned

Today's inline order differs between sites. For example:
- the `ret` arms run 0106, 0105, 0104, 0103, 0102, 0101, 0099, …;
- the instantiation gates run 0092, 0096, 0099, 0101, 0106, 0105, …;
- the method checks run 0106 … 0099, 0097, 0096, 0088, 0082.

So the registry keeps one ordered list per hook, not one global order.

The semantics are also per hook, and match today's code:
- `claim`, `arg` and `ret` are first-match: the first family that answers ends the search, exactly where today's `return` does.
- `listed` collects every applicable family's typed state, in the existing order, until the first refusal. Today `emit_callable` sets six states together, and `emit_instantiations` sets up to nine per pair, so several families are active at once. A refusal stops collection and discards what was collected, with the same exception text and route as today.
- `check` runs every family in order, as today's sequence of independent `if` blocks does.

A self-test pins every hook's (family, position) list. The evidence derives the same lists from the base source (the order of each site's `record NNNN` blocks at `32d1b40`) and shows the two agree.

## Proof

Control flow changes here, so the proof has five parts:
1. **Byte identity.** `probes/0114/golden.sh` compares the 0114 generator (`32d1b40`) and the 0115 generator over the same 60 runs, plus the `--check`, drift and `--self-test` controls. `diff -r` must be empty.
2. **Hook coverage.** With `POLARS_GEN_TRACE=hooks` set, the generator writes every (hook, family, outcome) it takes to a trace file. Golden does not set it, so golden's stderr is unaffected. The evidence tables which (family, hook, outcome) triples the 16 generating runs reach: state set, refusal, arm taken, check refused. Any triple the golden runs never reach gets a synthetic self-test asserting that outcome, and the evidence names each one. This is the answer to 0114's point that finite golden pairs cannot prove control flow: the evidence shows exactly which paths golden proves, and a test covers each of the rest.
3. **Overlap, by construction.** The shipped release files barely overlap: few callables are listed by more than one family. So coverage of single triples cannot prove how families combine. `probes/0115/overlap/` holds synthetic release files, derived from `0.55.2-joins.toml` with the same provenance, against the real 0.55.2 inventory. Each is built to force one combination:
   - an `arg` family and a `ret` family active on the same callable at once;
   - two competing `ret` arms for the same return;
   - a later `listed` refusal after an earlier family has already set state;
   - a rejected instantiation pair followed by a valid pair of the same callable;
   - a listed callable followed by an unlisted one on the same owner.

   The 0114 generator (`32d1b40`) and the 0115 generator run on each file, and their full outputs must be byte-identical. That covers the bindings, `surface.json` with every diagnostic and exception, the oracle tests and fixtures, stdout, stderr and the exit code. The evidence shows, from the hook trace, that each file actually reaches its intended combination. A file that reaches something else does not count.
4. **No leak between callables.** A new self-test emits a listed callable, then an unlisted one on the same owner, and asserts the second sees no active state. If the drop guard changes any golden output, the old code leaked state between callables. That is a behaviour change, and the record stops and reports it rather than keeping it.
5. **Mutation controls.** Each must fail:
   - swapping two families in one hook's order fails the order self-test;
   - removing the guard's clear fails the leak self-test;
   - dropping one family's `ret` arm fails golden;
   - making `listed` first-match fails the overlap comparison;
   - removing the unknown-key check lets the `[[array_snapshot]]` control load.

The 0108–0114 replays pass unchanged. The adapter output is byte-identical, so the adapter suites, the v2 oracle and the launch measurement do not need to be re-run.

## Authoring rule after this record

A family is one module under `families/`. It implements `Family`'s hooks, holds its release table inside `FamilyTables`, and has its self-tests. It adds one line to each hook order list it uses. Shared modules change only for a shared capability the plan names.

The evidence reports how far the shared files shrank. It also confirms that no family state remains on `World` or `Oracle` outside `active`.

## Stop rule

Stop and report to Codex if:
- byte identity needs any reorder the pinned lists do not already show;
- the guard exposes a leak;
- a hook's first-match and run-all semantics cannot both be kept for some family.

This is one plan and one implementation. The next parity record, generic_methods (793 rows), is the first to be written against the registry.
