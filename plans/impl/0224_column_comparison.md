# Implementation plan: ADR 0224

Create the user-local virtual environment and install the pinned wheels (only
binary wheels), recording `pip freeze`, the install report and the hashes. Run
semantic probes on the installed versions first:
- Arrow IEEE comparisons;
- Polars and DuckDB NaN ordering;
- the Kleene behaviour that the wrappers must neutralize.

Add the harness under `tools/column/compare/` in rnx. Its parts:
- a shared fixture manifest with a digest, built from the existing column
  fixtures;
- per-engine workers for the equivalent and native panels;
- a chunkwise oracle validator;
- a durable sample recorder;
- a renderer.

Committed tests cover the oracle, the semantic wrappers on small heterogeneous
controls, and the recorder's resume rules.

Run validation, then the timing matrix:
- 11 shapes × 4 layouts × 2 sizes; rnx and Arrow once, Polars/DuckDB in
  single-thread and default modes: 2,640 primary process samples;
- secondary native panel: seven differing shapes, Polars/DuckDB in both modes,
  100M only and three samples: 336 process samples;
- primary keeps five samples; all process samples contain first plus five repeats.

Bind the matrix identity to the signed implementation and verified production
binary. Reuse full-size validation under the reviewed policy-only proof: worker,
expression, fixture and oracle source hashes remain exact. Scheduling/reporting
changes do not require repeating the 752 successful validation cases.

Publish the rnx impl, then the rnx-bench report from the receipts under ADR
0224. Run the commit checker. The only runtime change is the shared-origin
`time::monotonic_ns` API, with registration, documentation and focused tests.
Build the 0224 production CLI and record that column sources are byte-identical
to 6d253fad. Use nanosecond clocks in both rnx and Python workers.
