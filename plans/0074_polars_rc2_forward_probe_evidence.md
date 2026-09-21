# rnx 0074: forward-upgrade probe against Polars 2.0 rc2, evidence

Plan: `plans/0074_polars_rc2_forward_probe.md` (d8365a1). Tool:
`probes/0074/`, one command `probes/0074/run.sh`. Third version, after
Codex's reviews of c56f415 and f8c5b4b: entries are identified by path,
kind, receiver and signature including generic bounds, so multiplicity
survives and a changed bound is a reshape; the runner starts from fresh
outputs, records a status per stage, invalidates itself on any
infrastructure failure and only reports oracle results it emitted; the
frozen inputs are digests in `frozen.json` that the runner verifies
before it starts; and the conclusions below say only what the retained
data shows.

Run 20260921T143258Z-2109690. Stages: doc reused, diag_joins ok, extract ok, generate ok, build ok, oracle cases_failed, oracle_controls ok, frozen ok, tools ok.
The rc2 documentation is the first run's, reused under its lock; every
other output is this run's. The adapter, the generator and every manifest
outside the probe are unchanged; the generator and extractor are the
accepted 98885e0 sources, verified by digest.

## Answer

The generator held under a real upstream move. Against the Rust workspace
sources at the Python 2.0 rc2 tag, the unchanged generator produced 1658
bindings that compiled with zero errors, no entry changed accounting
status, and 800 of 841 oracle cases matched Rust structurally in order.
What the probe surfaced was not a binding fault but two things about the
oracle and one about scope: the three lazy joins return rows in an order
that varies from run to run at rc2, which the classifier reports as a
failure because the fixtures on 0.55.2 had made a stable order look like
a contract; `Expr::unique_stable` is behind a feature at rc2 that the
adapter does not enable; and a new crate, `polars_defs`, now holds the
join and interval option types, which the probe's fixed API-crate subset
counts as out of scope. Coverage at rc2 stays about a third.

## Pins and provenance

| side | source | resolved features | crates documented | failed | toolchain | format |
|---|---|---:|---:|---|---|---:|
| 0.55.2 | crates.io =0.55.2 | 9 | 24 | none | nightly-2026-09-20 | 61 |
| rc2 | Rust workspace sources at git da47b7405f0e2d188e71cce7c2d588c695f4098d | 9 | 27 | none | nightly-2026-09-20 | 61 |

Resolved feature sets are identical.
Crate sets differ: only 0.55.2 []; only rc2 ['polars-defs', 'polars-descriptions', 'polars-observer'].
Locks: probes/0074/locks/doc-rc2.lock and probes/0074/locks/adapter.lock, digests verified at the start of the run; the 384 non-Polars packages in the adapter lock come from crates.io. Wheel-build equivalence with the Python release was not tested.

"Rust workspace sources at the Python release tag" is the precise claim:
the 27 Polars workspace packages resolve to that commit, other
dependencies come from crates.io under the locks, and nothing here shows
parity with the wheel's feature set, resolution, toolchain or build
flags. The workspace version string at the commit is 0.55.1 and is not
used for anything.

## Denominators, same extractor and rules

| side | gross | reachable | excluded | unknown | eligible | derived among callables |
|---|---:|---:|---:|---:|---:|---:|
| 0.55.2 | 16261 | 6349 | 1294 | 0 | **5055** | 1556 |
| rc2 | 16714 | 6698 | 1306 | 0 | **5392** | 1777 |

## API delta

Identity is canonical path, kind, receiver and semantic signature, where
the signature includes each generic parameter with its canonical bounds.
Within a path group entries are matched exactly first; a reshape is
reported only when one unmatched entry remains on each side with the
same kind and receiver; the rest are additions and removals. Totals
reconcile with the inputs by assertion, and `report.py --self-test`
reproduces the review's counterexamples: the two `ChunkedArray::par_iter`
entries on 0.55.2 are one identical entry and one removal, not a reshape;
`Schema::retain_mut`, whose callback bound changed mutability, and
`FileScanIR::gather_after_filter`, whose iterator bound gained `Clone`,
are reshapes.

| measure | count |
|---|---:|
| eligible entries in 0.55.2 (reconciles with surface.json in-scope entries: 4345) | 4345 |
| eligible entries at rc2 | 4419 |
| identical entries | 4118 |
| reshaped (one unmatched entry per side of a path group, same kind and receiver) | 31 |
| removed at rc2 | 196 |
| added at rc2 | 270 |

Added by kind: foreign_trait_impl 140, inherent 95, free_fn 19, trait_method 16. Removed by kind: foreign_trait_impl 105, inherent 86, trait_method 3, free_fn 2.

Most-changed owners (added): `polars_core::chunked_array::temporal::string::infer` 10, `polars_io::ipc::ipc_stream::IpcStreamReader` 7, `polars_core::frame::dataframe::DataFrame` 6, `polars_core::frame::column::Column` 6, `polars_plan::dsl::file_scan::MetadataPerSource` 5, `polars_core::series::Series` 5, `polars_io::cloud::http_rate_limit::Pacer` 5, `polars_core::chunked_array::temporal::string::StringMethods` 5

Most-changed owners (removed): `polars_io::predicates::ColumnStats` 12, `polars_core::frame::group_by::GroupBy` 12, `polars_core::chunked_array::ChunkedArray` 11, `polars_ops::frame::join::args::JoinType` 8, `polars_ops::frame::join::args::JoinArgs` 6, `polars_core::frame::dataframe::DataFrame` 6, `polars_io::utils::file::async_writable::AsyncWritable` 5, `polars_ops::frame::join::args::JoinType as core::convert` 3

| reshaped entry | 0.55.2 | rc2 |
|---|---|---|
| `polars_io::cloud::concurrency::ConcurrencyController::new` | `(polars_io::cloud::concurrency::ControllerConfig) -> Self` | `(polars_io::cloud::concurrency::ControllerConfig, core::option::Option<polars_io::cloud::http_rate_limit::PacingBudget>) -> Self` |
| `polars_io::cloud::concurrency::admission::InFlightBudget::new` | `(u64, u64, u32) -> Self` | `(u64, u64, u64, u64) -> Self` |
| `polars_io::cloud::concurrency::get_request_budget` | `() -> u32` | `() -> u64` |
| `polars_io::cloud::glob::glob` | `(polars_utils::pl_path::PlRefPath, core::option::Option<&polars_io::cloud::options::CloudOptions>) -> polars_error::PolarsResult<alloc::vec::Vec<alloc::string::String>>` | `(polars_utils::pl_path::PlRefPath, core::option::Option<&polars_io::cloud::options::CloudOptions>) -> polars_error::PolarsResult<alloc::vec::Vec<(alloc::string::String, u64)>>` |
| `polars_io::csv::read::streaming::read_until_start_and_infer_schema` | `(&polars_io::csv::read::options::CsvReadOptions, core::option::Option<polars_core::schema::SchemaRef>, core::option::Option<usize>, core::option::Option<polars_io::csv::read::streaming::InspectContentFn>, &mut polars_io::utils::compression::ByteSourceReader<polars_io::utils::stream_buf_reader::ReaderSource>) -> polars_error::PolarsResult<(polars_core::schema::Schema, polars_buffer::buffer::Buffer<u8>)>` | `(&polars_io::csv::read::options::CsvReadOptions, core::option::Option<polars_core::schema::SchemaRef>, bool, bool, core::option::Option<usize>, core::option::Option<polars_io::csv::read::streaming::InspectContentFn>, &mut polars_io::utils::compression::ByteSourceReader<polars_io::utils::stream_buf_reader::ReaderSource>) -> polars_error::PolarsResult<(polars_core::schema::Schema, polars_buffer::buffer::Buffer<u8>)>` |
| `polars_io::csv::read::streaming::read_until_start_and_infer_schema_from_compressed_reader` | `(&polars_io::csv::read::options::CsvReadOptions, core::option::Option<polars_core::schema::SchemaRef>, core::option::Option<polars_io::csv::read::streaming::InspectContentFn>, &mut polars_io::utils::compression::CompressedReader) -> polars_error::PolarsResult<(polars_core::schema::Schema, polars_buffer::buffer::Buffer<u8>)>` | `(&polars_io::csv::read::options::CsvReadOptions, core::option::Option<polars_core::schema::SchemaRef>, bool, bool, core::option::Option<polars_io::csv::read::streaming::InspectContentFn>, &mut polars_io::utils::compression::CompressedReader) -> polars_error::PolarsResult<(polars_core::schema::Schema, polars_buffer::buffer::Buffer<u8>)>` |
| `polars_io::path_utils::expand_paths_hive` | `(&[polars_utils::pl_path::PlRefPath], bool, &[polars_utils::pl_str::PlSmallStr], &mut core::option::Option<polars_io::cloud::options::CloudOptions>, bool) -> polars_error::PolarsResult<(polars_buffer::buffer::Buffer<polars_utils::pl_path::PlRefPath>, usize)>` | `(&[polars_utils::pl_path::PlRefPath], bool, &[polars_utils::pl_str::PlSmallStr], &mut core::option::Option<polars_io::cloud::options::CloudOptions>, bool) -> polars_error::PolarsResult<(polars_buffer::buffer::Buffer<polars_utils::pl_path::PlRefPath>, usize, polars_io::path_utils::BytesPerSource)>` |
| `polars_io::predicates::ColumnPredicateExpr::new` | `(polars_utils::pl_str::PlSmallStr, polars_core::datatypes::dtype::DataType, alloc::sync::Arc<dyn polars_io::predicates::PhysicalIoExpr>, core::option::Option<polars_io::predicates::SpecializedColumnPredicate>) -> Self` | `(polars_utils::pl_str::PlSmallStr, polars_core::datatypes::dtype::DataType, polars_arrow::datatypes::ArrowDataType, alloc::sync::Arc<dyn polars_io::predicates::PhysicalIoExpr>, core::option::Option<polars_io::predicates::SpecializedColumnPredicate>) -> Self` |
| `polars_io::utils::mkdir::mkdir_recursive` | `(&polars_utils::pl_path::PlRefPath) -> core::io::error::Result<()>` | `(&polars_utils::pl_path::PlRefPath) -> polars_error::PolarsResult<()>` |
| `polars_io::utils::mkdir::tokio_mkdir_recursive` | `(&polars_utils::pl_path::PlRefPath) -> core::io::error::Result<()>` | `(&polars_utils::pl_path::PlRefPath) -> polars_error::PolarsResult<()>` |
| `polars_lazy::frame::JoinBuilder::build_side` | `(core::option::Option<polars_ops::frame::join::args::JoinBuildSide>) -> Self` | `(core::option::Option<polars_defs::join::JoinBuildSide>) -> Self` |
| `polars_lazy::frame::JoinBuilder::coalesce` | `(polars_ops::frame::join::args::JoinCoalesce) -> Self` | `(polars_defs::join::JoinCoalesce) -> Self` |
| `polars_lazy::frame::JoinBuilder::finish` | `() -> polars_lazy::frame::LazyFrame` | `() -> polars_error::PolarsResult<polars_lazy::frame::LazyFrame>` |
| `polars_lazy::frame::JoinBuilder::how` | `(polars_ops::frame::join::args::JoinType) -> Self` | `(polars_defs::join::JoinType) -> Self` |
| `polars_lazy::frame::JoinBuilder::maintain_order` | `(polars_ops::frame::join::args::MaintainOrderJoin) -> Self` | `(polars_defs::join::MaintainOrderJoin) -> Self` |
| `polars_lazy::frame::JoinBuilder::validate` | `(polars_ops::frame::join::args::JoinValidation) -> Self` | `(polars_defs::join::JoinValidation) -> Self` |
| `polars_lazy::frame::LazyFrame::join` | `(polars_lazy::frame::LazyFrame, E, E, polars_ops::frame::join::args::JoinArgs) -> polars_lazy::frame::LazyFrame where E: core::convert::AsRef<[polars_plan::dsl::expr::Expr]>` | `(polars_lazy::frame::LazyFrame, E, E, polars_defs::join::JoinArgs) -> polars_error::PolarsResult<polars_lazy::frame::LazyFrame> where E: core::convert::AsRef<[polars_plan::dsl::expr::Expr]>` |
| `polars_ops::frame::join::DataFrameJoinOps::join` | `(&polars_core::frame::dataframe::DataFrame, impl core::iter::traits::collect::IntoIterator<Item = impl core::convert::AsRef<str>>, impl core::iter::traits::collect::IntoIterator<Item = impl core::convert::AsRef<str>>, polars_ops::frame::join::args::JoinArgs, core::option::Option<polars_ops::frame::join::args::JoinTypeOptions>) -> polars_error::PolarsResult<polars_core::frame::dataframe::DataFrame> where impl AsRef<str>: core::convert::AsRef<str>; impl AsRef<str>: core::convert::AsRef<str>; impl IntoIterator<Item = impl AsRef<str>>: core::iter::traits::collect::IntoIterator<Item = impl core::convert::AsRef<str>>; impl IntoIterator<Item = impl AsRef<str>>: core::iter::traits::collect::IntoIterator<Item = impl core::convert::AsRef<str>>` | `(&polars_core::frame::dataframe::DataFrame, impl core::iter::traits::collect::IntoIterator<Item = impl core::convert::AsRef<str>>, impl core::iter::traits::collect::IntoIterator<Item = impl core::convert::AsRef<str>>, polars_defs::join::JoinArgs, core::option::Option<polars_defs::join::JoinTypeOptions>) -> polars_error::PolarsResult<polars_core::frame::dataframe::DataFrame> where impl AsRef<str>: core::convert::AsRef<str>; impl AsRef<str>: core::convert::AsRef<str>; impl IntoIterator<Item = impl AsRef<str>>: core::iter::traits::collect::IntoIterator<Item = impl core::convert::AsRef<str>>; impl IntoIterator<Item = impl AsRef<str>>: core::iter::traits::collect::IntoIterator<Item = impl core::convert::AsRef<str>>` |
| `polars_ops::frame::join::cross_join::CrossJoin::cross_join` | `(&polars_core::frame::dataframe::DataFrame, core::option::Option<polars_utils::pl_str::PlSmallStr>, core::option::Option<(i64, usize)>, polars_ops::frame::join::args::MaintainOrderJoin) -> polars_error::PolarsResult<polars_core::frame::dataframe::DataFrame>` | `(&polars_core::frame::dataframe::DataFrame, core::option::Option<polars_utils::pl_str::PlSmallStr>, core::option::Option<(i64, usize)>, polars_defs::join::MaintainOrderJoin) -> polars_error::PolarsResult<polars_core::frame::dataframe::DataFrame>` |
| `polars_ops::frame::join::hash_join::single_keys_dispatch::SeriesJoin::hash_join_inner` | `(&polars_core::series::Series, polars_ops::frame::join::args::JoinValidation, bool) -> polars_error::PolarsResult<(polars_ops::frame::join::args::InnerJoinIds, bool)>` | `(&polars_core::series::Series, polars_defs::join::JoinValidation, bool) -> polars_error::PolarsResult<(polars_ops::frame::join::args::InnerJoinIds, bool)>` |
| `polars_ops::frame::join::hash_join::single_keys_dispatch::SeriesJoin::hash_join_left` | `(&polars_core::series::Series, polars_ops::frame::join::args::JoinValidation, bool) -> polars_error::PolarsResult<polars_ops::frame::join::args::LeftJoinIds>` | `(&polars_core::series::Series, polars_defs::join::JoinValidation, bool) -> polars_error::PolarsResult<polars_ops::frame::join::args::LeftJoinIds>` |
| `polars_ops::frame::join::hash_join::single_keys_dispatch::SeriesJoin::hash_join_outer` | `(&polars_core::series::Series, polars_ops::frame::join::args::JoinValidation, bool) -> polars_error::PolarsResult<(polars_arrow::array::primitive::PrimitiveArray<polars_utils::index::IdxSize>, polars_arrow::array::primitive::PrimitiveArray<polars_utils::index::IdxSize>)>` | `(&polars_core::series::Series, polars_defs::join::JoinValidation, bool) -> polars_error::PolarsResult<(polars_arrow::array::primitive::PrimitiveArray<polars_utils::index::IdxSize>, polars_arrow::array::primitive::PrimitiveArray<polars_utils::index::IdxSize>)>` |
| `polars_ops::frame::join::merge_join::gather_and_postprocess` | `(polars_core::frame::dataframe::DataFrame, polars_core::frame::dataframe::DataFrame, core::option::Option<&[polars_utils::index::IdxSize]>, core::option::Option<&[polars_utils::index::IdxSize]>, &mut core::option::Option<(polars_core::frame::builder::DataFrameBuilder, polars_core::frame::builder::DataFrameBuilder)>, &polars_ops::frame::join::args::JoinArgs, &[polars_utils::pl_str::PlSmallStr], &[polars_utils::pl_str::PlSmallStr], bool, &polars_core::schema::Schema) -> polars_error::PolarsResult<polars_core::frame::dataframe::DataFrame>` | `(polars_core::frame::dataframe::DataFrame, polars_core::frame::dataframe::DataFrame, core::option::Option<&[polars_utils::index::IdxSize]>, core::option::Option<&[polars_utils::index::IdxSize]>, &mut core::option::Option<(polars_core::frame::builder::DataFrameBuilder, polars_core::frame::builder::DataFrameBuilder)>, &polars_defs::join::JoinArgs, &[polars_utils::pl_str::PlSmallStr], &[polars_utils::pl_str::PlSmallStr], bool, &polars_core::schema::Schema) -> polars_error::PolarsResult<polars_core::frame::dataframe::DataFrame>` |
| `polars_ops::series::ops::linear_space::new_linear_space_f32` | `(f32, f32, u64, polars_ops::series::ops::linear_space::ClosedInterval, polars_utils::pl_str::PlSmallStr) -> polars_error::PolarsResult<polars_core::datatypes::Float32Chunked>` | `(f32, f32, u64, polars_defs::expr::ClosedInterval, polars_utils::pl_str::PlSmallStr) -> polars_error::PolarsResult<polars_core::datatypes::Float32Chunked>` |
| `polars_ops::series::ops::linear_space::new_linear_space_f64` | `(f64, f64, u64, polars_ops::series::ops::linear_space::ClosedInterval, polars_utils::pl_str::PlSmallStr) -> polars_error::PolarsResult<polars_core::datatypes::Float64Chunked>` | `(f64, f64, u64, polars_defs::expr::ClosedInterval, polars_utils::pl_str::PlSmallStr) -> polars_error::PolarsResult<polars_core::datatypes::Float64Chunked>` |
| `polars_plan::dsl::builder_dsl::DslBuilder::join` | `(polars_plan::dsl::plan::DslPlan, alloc::vec::Vec<polars_plan::dsl::expr::Expr>, alloc::vec::Vec<polars_plan::dsl::expr::Expr>, alloc::sync::Arc<polars_plan::dsl::options::JoinOptions>) -> Self` | `(polars_plan::dsl::plan::DslPlan, alloc::vec::Vec<polars_plan::dsl::expr::Expr>, alloc::vec::Vec<polars_plan::dsl::expr::Expr>, alloc::sync::Arc<polars_plan::dsl::options::JoinOptions>) -> polars_error::PolarsResult<Self>` |
| `polars_plan::dsl::file_scan::FileScanIR::gather_after_filter` | `(bool, I) -> () where I: core::iter::traits::iterator::Iterator<Item = usize>` | `(bool, I) -> () where I: core::iter::traits::iterator::Iterator<Item = usize> + core::clone::Clone` |
| `polars_plan::dsl::options::JoinOptions as core::convert::From` | `(polars_plan::dsl::options::JoinOptionsIR) -> Self` | `(polars_plan::plans::options::JoinOptionsIR) -> Self` |
| `polars_plan::dsl::scan_sources::ScanSources::expand_paths_with_hive_update` | `(&mut polars_plan::dsl::file_scan::UnifiedScanArgs) -> polars_error::PolarsResult<Self>` | `(&mut polars_plan::dsl::file_scan::UnifiedScanArgs) -> polars_error::PolarsResult<(Self, polars_io::path_utils::BytesPerSource)>` |
| `polars_schema::schema::Schema::get_at_index_mut` | `(usize) -> core::option::Option<(&mut polars_utils::pl_str::PlSmallStr, &mut Field)>` | `(usize) -> core::option::Option<(&polars_utils::pl_str::PlSmallStr, &mut Field)>` |
| `polars_schema::schema::Schema::retain_mut` | `(F) -> () where F: core::ops::function::FnMut(&mut polars_utils::pl_str::PlSmallStr, &mut Field) -> bool` | `(F) -> () where F: core::ops::function::FnMut(&polars_utils::pl_str::PlSmallStr, &mut Field) -> bool` |

Types: 60 added, 17 removed at rc2.

The 263 entries that left scope moved to `polars_defs`, which the fixed
API-crate list of 0073 does not include; the removals under `JoinType`
and `JoinArgs` are those moves. Joins now return `PolarsResult`:
`LazyFrame::join`, `JoinBuilder::finish` and `DslBuilder::join` became
fallible, which the generator absorbed into a Rune `Result` unchanged.

## Accounting under the unchanged generator

| status | 0.55.2 | rc2 |
|---|---:|---:|
| generated | 1622 | 1658 |
| adapted | 309 | 311 |
| unsupported | 2414 | 2450 |
| out_of_scope | 710 | 973 |

Same identity, status changed: 0.

Wrapper types: 237 on 0.55.2, 251 at rc2. Oracle cases emitted: 832 and 841.

## Compilation

`cargo build --locked --release --features test-support`: ok.

No errors.

## Oracle differences

Controls: ok. Cases: 841 emitted at rc2 (results verified as this run's, run 20260921T143258Z-2109690).

| tally | 0.55.2 | rc2 |
|---|---:|---:|
| both_error | 19 | 25 |
| both_panic | 10 | 10 |
| match | 802 | 800 |
| mismatch | 0 | 1 |
| oracle_nondeterministic | 0 | 1 |
| reuse_failed | 0 | 1 |
| row_order_differs | 1 | 3 |

Cases in both runs: 789; only 0.55.2: 43; only rc2: 52; outcome changed among common cases: 6.

New cases at rc2 by outcome: {'match': 46, 'both_error': 6}. Cases gone at rc2 by their 0.55.2 outcome: {'match': 42, 'both_panic': 1}.

| common case whose outcome changed | 0.55.2 | rc2 | detail |
|---|---|---|---|
| `polars_lazy::frame::LazyFrame::full_join` | match | mismatch | rune "collected shape=(3, 6); columns=[x:Int64,y:String,z:Float64,x_right:Int64,y_right:String,z_right:Float64,]; rows=[(Int64(2), String(\" |
| `polars_lazy::frame::LazyFrame::inner_join` | match | reuse_failed | second call on the same receiver gave Value(Frame { head: "collected shape=(3, 5); columns=[x:Int64,y:String,z:Float64,y_right:String,z_righ |
| `polars_lazy::frame::LazyFrame::left_join` | match | oracle_nondeterministic | polars gave Value(Frame { head: "collected shape=(3, 5); columns=[x:Int64,y:String,z:Float64,y_right:String,z_right:Float64,]", rows: ["(Int |
| `polars_lazy::frame::LazyFrame::unique_generic` | match | row_order_differs | collected shape=(3, 3); columns=[x:Int64,y:String,z:Float64,]; rows=[(Int64(1), String("a"), Float64(1.5)) / (Int64(3), String("c"), Float64 |
| `polars_plan::dsl::expr::Expr::unique` | match | row_order_differs | selected shape=(3, 1); columns=[x:Int64,]; rows=[(Int64(3)) / (Int64(1)) / (Int64(2))] |
| `polars_plan::dsl::expr::Expr::unique_stable` | match | both_panic | activate 'is_first_distinct, ' feature |

Read from the joined case records:

- **The six extra both-error cases are six new cases**, present at rc2 and
  absent on 0.55.2; among the 789 common cases none changed from a value
  to an error.
- **The three joins.** Each fails, and the failing outcome varies with
  scheduling between replays: this run gave one mismatch, one reuse
  failure and one nondeterministic oracle; the first run gave three
  nondeterministic oracles; Codex's gave one mismatch and two. The
  labeled diagnostic below, outside the classifier, compares the
  binding's collected rows with Rust's as multisets with the schema:

| join | schema equal | row multiset equal | rows (binding / Rust) | orders seen in 6 Rust runs |
|---|---|---|---|---:|
| inner_join | True | True | 3 / 3 | 4 |
| left_join | True | True | 3 / 3 | 4 |
| full_join | True | True | 3 / 3 | 5 |

  Schemas and row multisets are equal for all three; six Rust runs
  produce four or five distinct orders. On 0.55.2 the same fixtures
  returned one order every time. This is not established as a 2.0
  contract change: in the pinned 0.55.2 sources `JoinArgs::default()`
  already sets `maintain_order` to `MaintainOrderJoin::None`, and
  `inner_join`, `left_join` and `full_join` use `JoinArgs::new`, so the
  old contract permitted any order and the stable order was an accident
  of the small fixture. What changed is the execution, and what it
  exposed is an assumption in the oracle's fixtures. The direct repeated
  join check is archived as `probes/0074/diag-join-order.rn`; run against
  the rc2 binary it printed three orders in four runs.
- **`unique_generic` and `Expr::unique`**: same rows in another order under
  the unordered policy those operations already have; approved.
- **`Expr::unique_stable`**: gone at rc2 as a case (its 0.55.2 outcome was
  a match; at rc2 it panics on both sides with "activate
  'is_first_distinct' feature"); a working binding on 0.55.2 is feature
  gated at rc2.

The oracle test therefore exits nonzero at rc2 for the three joins, and
the run's status says `oracle: cases_failed`. That is the classifier
working as accepted in 0073.

## Excluded cases at rc2

| disposition | entries |
|---|---:|
| case | 841 |
| no fixture for the receiver type | 543 |
| protocol Clone has no script-level trigger | 102 |
| protocol Debug has no script-level trigger | 95 |
| return type has no comparison | 34 |
| no fixture for a parameter | 30 |
| receiver type has no comparison | 7 |
| operator result type not wrapped | 4 |
| excluded: nondeterministic oracle | 2 |

## Census: why fixture-free receivers have no fixture

The frozen generator already builds fixtures from `Default` and from an
enum's first unit variant, so "add Default fixtures" is not a
recommendation. Owners are resolved by their inventory kind; "no public
fields" means the wrapper cannot build the value, not that the struct is
empty. The 543 fixture-free bindings are:

| why the frozen rules give no fixture | entries |
|---|---:|
| struct with no public fields (private fields or unit), no Default, no core fixture | 260 |
| enum without a unit variant | 142 |
| struct with public fields, no Default | 78 |
| trait-owned method: no wrapped implementor has a fixture | 63 |

Largest receiver types: `SeriesTrait` 43 (trait-owned method: no wrapped implementor has a fixture), `StatisticsFlags` 29 (struct with no public fields (private fields or unit), no Default, no core fixture), `LazyCsvReader` 28 (struct with no public fields (private fields or unit), no Default, no core fixture), `TimeUnitSet` 22 (struct with no public fields (private fields or unit), no Default, no core fixture), `ScalarColumn` 21 (struct with no public fields (private fields or unit), no Default, no core fixture), `ScanFlags` 21 (struct with no public fields (private fields or unit), no Default, no core fixture), `GroupsType` 15 (enum without a unit variant), `LiteralValue` 13 (enum without a unit variant), `LazyFileListReader` 10 (trait-owned method: no wrapped implementor has a fixture), `ScanSources` 8 (enum without a unit variant), `PolarsError` 7 (enum without a unit variant), `StatisticsFlagsIM` 6 (struct with no public fields (private fields or unit), no Default, no core fixture)

## Proposed fixes, not applied

1. **API-crate subset as a per-release input**: add `polars_defs` (and
   decide `polars_descriptions`, `polars_observer`) so the moved option
   types are wrapped and the 263 entries return to scope.
2. **Join fixtures, not a policy exception**: the old contract already
   permits any order, so the right fix is on the oracle's side: joins on
   the lazy fixtures should carry `maintain_order` explicitly, or the
   cases should declare the unordered policy on the ground that
   `MaintainOrderJoin::None` is the documented default. No per-release
   exception.
3. **Feature gate**: `unique_stable` needs `is_first_distinct` at rc2; a
   product decision for the eventual upgrade.
4. **Fixtures for constructor-only types**: the census says the missing
   fixtures are structs with no public fields, no `Default` and no core
   fixture (260), enums whose every variant carries data (142), structs
   with public fields but no `Default` (78), and trait-owned methods whose
   wrapped implementors have no fixture (63). Reaching the first three
   means fixtures built through an existing constructor binding (`new`,
   `from_*`) or a data-carrying variant; the fourth needs a fixture for an
   implementor such as `Series` for `SeriesTrait`, which exists, so those
   are a lookup gap in the generator rather than a missing value. Each is
   a generator rule to design and measure, not a known-cheap step.

No experimental rerun with any of these applied was made.

## Controls

`probes/0074/controls.sh`: a generator whose digest differs from
`frozen.json` is refused at the frozen stage; a replay without the
committed adapter lock is refused; failing oracle controls with a stale
`oracle-results-rc2.json` in place invalidate the run and nothing stale
is reported; a failing generator invalidates the run before any build.
All four pass. `report.py --self-test` covers the identity rules. A
replay under the saved locks reproduced the build and the tallies above
except for which failing outcome each join drew.

## Conclusion: the next coverage record

The generator survived: no compile failure, no status change, and every
behavioural difference surfaced through the oracle. What moved was type
topology (a new crate) and execution order (joins), neither of which more
bindings of the existing kinds would address. The next coverage work, in
order, from these observations:

1. make the API-crate subset a per-release input of the generator, so the
   same tool reads 0.55.2 and rc2 alike, and settle the join fixtures on
   the oracle's side;
2. fixtures reachable through constructor bindings and data-carrying
   variants (480 of 543 fixture-free bindings, per the census),
   implementor fixtures for the 63 trait-owned ones, and wrappers for the
   internal-crate argument types that 0073 listed as "unwrapped type";
   all raise compiled and executed coverage together, and each needs its
   yield measured before it is claimed;
3. the generic bucket last; this probe says nothing about it, and it
   should not be assumed to be the shape of the third already generated.
