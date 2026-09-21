# polars-gen

Generates the Polars adapter's Rune bindings from the record 0072
inventory (`probes/0072/out/<release>-adapter/result/inventory.json`).

    cargo run --manifest-path tools/polars-gen/Cargo.toml -- \
        probes/0072/out/0.55.2-adapter/result/inventory.json adapters/polars \
        --buckets mechanical,conversion,option_struct

Writes `adapters/polars/src/generated/` (types, functions, catalogue,
fixtures), `adapters/polars/tests/generated_oracle.rs` and
`adapters/polars/surface.json`, the accounting of every eligible callable
as generated, adapted, unsupported with a reason, or out of scope.
`--check` writes nothing and fails if the committed files differ; the
adapter's `generated_files_do_not_drift` test runs it.

Policies (ownership, conversions, errors, execution routing, keyword
renames) are described in `plans/0073_polars_generator_evidence.md`
and encoded in `src/main.rs`.

The generated oracle test compares values structurally through
`adapters/polars/src/oracle.rs`: frames, series and columns cell by cell
with dtypes and nulls, errors by kind, panics by message, and a second
call on the same receiver. Every outcome that is not an approved match
fails; the negative controls in `oracle.rs` prove that.
