# rnx 0107: in-memory source programs and multi-argument handlers for embedders

Status: plan. Record 0106 closed at plan `f0e7aea` and implementation `1eecbf1` on `origin/main`.

## Target and source contract

Polariton (ket) embeds rnx through the `server-runtime` API of record 0056 to run a Rune source pane locally with rnx's batteries beside its own `workspace` and `viz` modules. Two gaps block that without copying rnx internals: `Program::compile` reads only an entry **file**, while an editor holds its source as text; and `Program::prepare` passes exactly **one** argument, while Polariton's established source contract is `main(workspace, viz)`. A third gap is diagnostic: the server path reports budget exhaustion as Rune's internal `Halted for unexpected reason \`limited\``, where the CLI (records 0021/0022) names the budget.

Close all three additively. Existing `compile` and `prepare` behaviour is unchanged.

- `Program::compile_source(name, text, extensions)` compiles text held by the caller. `name` identifies it in diagnostics and is never opened. The text counts against the same 8 MiB source allowance as a file entry. The source carries `name` as its path, so a compile failure's `Failure::path`, `position` and `excerpt` locate it exactly as for a file. An in-memory entry has no directory, so every `mod` declaration is refused with a located preparation failure naming the module; no working-directory search is added. `compile` and `compile_source` share one build routine.
- `Program::prepare_with(extensions, handler, Vec<Value>, budget)` constructs an invocation for a handler of any arity; `prepare` delegates with one argument. Budget and extension rules are those of `prepare`.
- In `Invocation::run`, the server path reuses the CLI's `execute::Settled` reading (made `pub(crate)`): a halt with no location while the budget is spent is reported as `the budget of N instructions was exhausted`; a located failure on the last permitted instruction keeps its diagnostic with the budget named after it.

The context an embedder receives is the whole `server-runtime` context (json, io, process with refused exit, fs, path, time, text, env, http), not an HTTP-only surface; embedders document that. Embedders depend with `default-features = false` so `count-allocations` installs no global allocator.

## Gates and verification

1. **In-memory entry.** Test a two-argument handler compiled from text; a compile failure whose path is the given name, whose line and excerpt are the failing line; a `mod` declaration refused by name at line 1; and text one byte over the allowance refused as the allowance.
2. **Additive arguments.** Test `prepare` with one argument unchanged, and `prepare_with` with two.
3. **Budget naming.** Test an unbounded loop under a 10,000-instruction budget reports exactly `the budget of 10000 instructions was exhausted` in category `vm`.
4. **No regressions.** Run the default-feature suite and the `server-runtime` library suite. Record any failure that also occurs on the unchanged tree.

Put test names, suite outcomes and the pre-existing failures in `plans/0107_caller_owned_source_programs_evidence.md`.
