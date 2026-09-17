# 0058 gate 4: project assembly and notebook restart

Gate 4 is ready for review on Linux. Gates 1–3 are accepted and pushed at rnx
3d3a420 and bench 8810d8d. Gates 5–6 remain open. No performance, browser or
non-Linux acceptance is claimed.

## Assembly through the product commands

The checked-in example is `rnx-bench/examples/polars/`, outside both native
package roots. Its manifest declares the rnx runtime plus rnx-polars with the
plain `build` hook. Its script creates CSV, filters/groups/sums/sorts, collects,
writes new Parquet, reads it back, compares previews and repeats collect. A fresh
absolute output directory is its only argument. The expected output is two rows:
category a with total 2, and category 🦀 with total 7.

`probes/polars-assembly/check.py` copies the example into a temporary project,
substitutes absolute native paths and invokes real `rnx-project lock`, `build`
and `run`. Only compilation objects are seeded from the accepted adapter target;
no lock or receipt is seeded. The product commands resolve offline, generate the
wrapper, build locked, publish the receipt and verify the artifact before run.

The generated main is the 0051 shape:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    rnx::main_with(rnx::Extensions::none().with("polars", native_0::build))
}
```

The generated manifest, both locks, receipt, resolved graph and outputs are
archived under `results/polars-assembly-0058/`. Polars remains exactly 0.55.2;
its resolved feature node equals gate 2's accepted node, including implied
upstream features. The declared features remain only lazy, csv and parquet.
Generated rnx additionally enables project-sources as the project tool requires;
this is not a claim that its entire feature graph equals the ordinary adapter's.

The generated artifact's hash matches the receipt. Project run and direct run
of that artifact both return the exact expected preview. The only visible
project files are main.rn, rnx.toml, rnx.lock and rnx.Cargo.lock; all build products
and receipts stay under .rnx. Archived locks contain temporary paths and captured
native working-tree identities, not portable locks intended for later reuse.
The temporary project is removed after the notebook closes. Documentation and
evidence edits after the run naturally invalidate those captured identities;
a reviewer reruns the commands to produce current identities.

## Entry points and lifecycle

Both ordinary rnx-polars and the generated artifact pass synchronous eval,
async-promoted eval, and a real piped session. The same bound frame is previewed
after a catchable missing-column failure. Reset drops its binding while leaving
the extension registered; a subsequent literal succeeds and the old name refuses.
Stock rnx cannot resolve polars.

The only adapter source addition is behind test-support: when the fixture sets
RNX_POLARS_BUILD_MARKER, the builder appends one fixed line. The ordinary and
generated builds ignore that variable. The marked binary proves zero builder
calls for version, help and selfcheck, and exactly one call for a session which
resets and uses Polars again. That session's config calls polars::col and is
refused as an unknown item, before the serving context uses the extension. The
pure settings evaluator is unchanged; no adapter module is installed in it.
No new startup I/O is compiled into the ordinary executable.

## Real notebook route

The existing 0047 isolated installer fixture copies the generated artifact to
its worker path and installs a kernelspec through rnx-jupyter. All Jupyter user,
data, config and runtime paths are private temporary directories. No user
kernelspec or installed package is modified.

Real jupyter_client cells load and retain a frame, provoke a runtime error from
a missing-column query, then print the original frame's expected preview.
A separate result shows the native frame remains opaque. The manager restarts
the kernel normally: the saved binding then fails, but the new worker still
loads and previews a fresh frame through Polars. The notebook is saved as
`polars.ipynb` and passes nbformat validation on write and read-back.

Both kernel PIDs and both worker PIDs are sampled across restart and are absent
from /proc after shutdown. The fixture closes clients, managers and temporary
installations in finally blocks. No kernel or worker remains. This is protocol
execution through Jupyter, not another browser or screen gate. Notebook source
maps and automatic dataframe display remain outside this record.

## Checks and qualifications

- Formatting and clippy with warnings denied pass in both configurations.
- Ordinary tests: 6 passed; test-support: 7 passed, run serially.
- Notices check passes unchanged with its three missing texts still explicit.
- The root source, all manifests/lockfiles/notices, kernel, PostgreSQL adapter,
  server and project tool are unchanged. Root suites and platform checks remain
  gate 6; timing and setup costs remain gate 5.
- README documents project assembly and installing the generated worker rather
  than the rnx-project dispatcher, plus the explicit preview and restart rules.

Two fixture corrections preceded the passing full run. The first feature check
had guessed implied Polars features; it now compares the accepted resolved graph.
The reset check expected the wrong diagnostic phrase; the existing message is
`No local variable f` (with backticks around f). Neither was a product failure.

Ordinary executable SHA-256:
`bb2a24526f10b3a0f0255c7173117a5aeeb2d4ce6133477c094d67a830806907`.
The test-support and generated artifact hashes are in the results and receipt.
These builds use existing compilation caches and provide no cold-build or
startup measurements.
