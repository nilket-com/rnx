# rnx 0107 evidence

Implementation: working tree over `1eecbf1` (`src/program.rs`, `src/server.rs`, `src/execute.rs`).

## Gates

1–3. `cargo test --locked --no-default-features --features server-runtime --lib source_tests`: 7 passed.

- `in_memory_source_takes_several_arguments`: `main(a, b)` from text returns 42 for (4, 2).
- `single_argument_prepare_is_unchanged`: `prepare` with one argument returns 2 for 1.
- `in_memory_compile_failure_names_the_source`: category `preparation`, path `<buffer 7>`, line 2, excerpt `    unknown`.
- `in_memory_source_refuses_module_declarations`: message contains `an in-memory source has no directory`, line 1.
- `in_memory_source_counts_against_the_allowance`: a source of `SOURCE_ALLOWANCE` + 20 bytes is refused as the allowance.
- `an_exhausted_budget_is_named`: category `vm`, message `the budget of 10000 instructions was exhausted`.

4. The default-feature suite passes. The `server-runtime` library suite without default features has two failures, `session::tests::over_the_ceiling_refuses_evaluation_but_not_inspection` and `inspect::tests::both_commands_answer_when_the_session_is_over_its_ceiling`; the first was reproduced on the unchanged tree (`git stash`), both depend on `count-allocations`' ceiling, and neither touches this record's code.

## Embedder

Polariton consumes this through `Program::compile_source` and `prepare_with` for sources declaring a top-level `main(workspace, viz)`; its own tests (ket) cover cancellation on rerun and buffer close, cleanup precedence, document-generation guards and batch publication.
