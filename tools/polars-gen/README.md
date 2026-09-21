# polars-gen

Generates the Polars adapter's Rune bindings from the record 0072
inventory (`probes/0072/out/<release>-adapter/result/inventory.json`).

    cargo run --manifest-path tools/polars-gen/Cargo.toml -- \
        probes/0072/out/0.55.2-adapter/result/inventory.json adapters/polars \
        --release tools/polars-gen/releases/0.55.2-joins.toml \
        --buckets mechanical,conversion,option_struct

`--release` is required and names a file in `releases/`: the release's
API-crate list, its `[provenance]` (the crates.io `release` or Git `rev`
the inventory must record, or the generator refuses to run), the
operations whose row order is unspecified (by canonical path, with
`args` giving the exact fixture expression per parameter or `*` for one
that cannot affect order, and a citation), and the oracle exclusions. `0.55.2.toml` holds the record 0073 constants,
`0.55.2-joins.toml` adds the join-order policy and is the shipped one,
`0.54.4.toml` serves the adjacent check and `rc2.toml` the experimental
0074 run. `surface.json` records the file's name, source and SHA-256.
`--self-test` runs the generator's own controls: the order policy, and
the wrapper identity rules (equivalent aliases share one wrapper,
same-name distinct types get distinct Rune paths).

Record 0076 added the deref route (trait methods on a type whose `Deref`
target is that trait), the null-series core fixture, the instantiation
of `ChunkedArray` and `Logical` methods on their alias wrappers from an
applicability evaluation over the inventory's recorded impl heads,
bounds, `where` predicates and associated types (three results: proven,
rejected, unresolved; only proven pairs are emitted, and Rust
compilation is the second check), typed source fixtures and a producer
fixture rule, and the structural array comparator (`into_series`,
never `Debug`). The release file's `[instantiation]` table names the
families shipped and the cited exclusions; `surface.json` records the
whole census under `instantiation`. `--self-test` covers the deref
targets and the applicability rules from synthetic inventories.

Writes `adapters/polars/src/generated/` (types, functions, catalogue,
fixtures), `adapters/polars/tests/generated_oracle.rs` and
`adapters/polars/surface.json`, the accounting of every eligible callable
as generated, adapted, unsupported with a reason, or out of scope.
`--check` writes nothing and fails if the committed files differ; the
adapter's `generated_files_do_not_drift` test runs it.

Policies (ownership, conversions, errors, execution routing, keyword
renames) are described in `plans/0073_polars_generator_evidence.md`
and encoded in `src/main.rs`; record 0075 added the fixture recipes
(constructors and data-carrying variants), the alias and internal-crate
wrappers with impl-aware `Clone`/`Debug`/`Default` facts, and the
free-function arity rule, described in
`plans/0075_polars_coverage_two_evidence.md`. The generator reads the
0072 inventory's `alias_target`, `impl_for`, `impl_bounds` and
`implementors` fields when present.

The generated oracle test compares values structurally through
`adapters/polars/src/oracle.rs`: frames, series and columns cell by cell
with dtypes and nulls, errors by kind, panics by message, and a second
call on the same receiver. Each case runs in two stages on both sides:
`setup` builds the fixtures and returns them, and `main(__fx)` (the Rust
oracle's staged body) makes the measured call on those prepared values,
constructing nothing itself; a setup failure on both sides is counted
apart and verifies nothing, on one side it fails the run, and a second
Rust run whose setup fails is a nondeterminism failure. Every outcome that is not an approved match
fails; the negative controls in `oracle.rs` prove that.
