# 0057 gate 4: generated native assembly and mapped PostgreSQL application

Status: implemented for review on Linux. Gate 3 is accepted and pushed at rnx
bb9ae35 / rnx-bench b78202c. Gates 5 and 6 remain open.

## Assembly under test

The manifest parser, generator and map serializer are the accepted Rust tool
implementations. A new private assembly module writes their outputs into a fresh
staging directory immediately beneath the application's .rnx. It refuses an
existing stage and never overwrites it. The test-only executable adapts these
private functions for the external fixture; it is gated on test-support and is
not the product CLI. The generated wrapper registers the real PostgreSQL lifecycle
builder plus a tiny plain fixture builder and enables project-sources on rnx.

The Python fixture owns Cargo invocation and temporary infrastructure. It seeds
Cargo.lock from the accepted adapter, lets Cargo resolve local additions offline,
and builds with --locked --release. Cargo.toml, main.rs, map, lock and resolved
graph are archived. There is no Python reimplementation of generation, capability
checking or executable hash verification. This is assembly evidence, not a product
lock, receipt or input-verification workflow; those are gate 5.

Launch checks the executable hash first, validates a supplied handoff and its
entry, and requires the bounded capability exchange when mounts are present.
Without mounts, launch uses ordinary run so an older rnx-pg remains compatible.
On Unix the fixture then execs the constructed command, preserving argument bytes,
status and signal delivery. The non-Unix wait branch only type-checks; it is not a
claim about the eventual Windows product interruption contract.

## Results

The fixture starts its own PostgreSQL cluster with a private Unix socket and uses
a separate temporary Jupyter installation. No system database or user kernelspec
is modified. Native fixture sources live in their own fixture Git worktree;
generated artifacts remain under ignored target directories.

- One file run imports a mapped Rune package, calls the plain extension and makes
  a typed PostgreSQL query. Quotes, SQL-looking text, backslash and emoji survive
  the parameter round trip; the combined result is 42.
- Running from /tmp still resolves the map. A trailing --color=never arrives as
  a script argument. Explicit process::exit(7) exits the launcher with 7.
- A pending input interrupted while supervising a Python child exits 130; the
  child PID has disappeared before the check completes.
- Existing rnx-pg runs an unmapped query. It refuses the mapped form at capability
  checking before the entry runs. A copied source-capable executable runs the
  mapped application; appending bytes makes hash verification refuse first.
- Native eval and a session across reset return 42. Eval and session still refuse
  file modules. The installed generated worker likewise refuses them.
- A notebook executes the query before and after kernel restart. Restart loses
  an ordinary binding but retains both extensions. Kernel/worker PIDs are collected
  around both launches and observed gone after shutdown.
- Tagged database activity drains; the fixture postmaster exits and its temporary
  directory is removed.

The first interrupt fixture was incorrect: synchronous main returned the
supervisor's Ok reply with cancelled=true, then completed successfully with status
0. Cancellation of a child does not itself require that completed script to fail.
The corrected input stays pending on an await after the supervisor returns, so
the driver's interruption is observed before successful completion. The original
reply and correction are preserved in first-interrupt-fixture.json. No root code
or process policy changed.

## Checks and remaining work

Tool suites pass 27 tests in each configuration, with two explicit integrations
ignored as before. The test-only executable disables duplicate unit-test discovery;
its integration is driven externally. Formatting, clippy with warnings denied,
notices and Windows type-check pass. The tool dependency graph and notices are
unchanged; the generated registry graph is compared with the adapter lock in the
conditions file. No root source, manifest, lockfile, notices, kernel, adapter or
server changes. Root runtime suites and matched startup remain gate 6.

The README now says that a native rnx path dependency fingerprints the entire
tracked repository, including plans and nested packages. That broader identity
is intentional; there is no source-directory-only exception.

Gate 5 must still own Cargo/environment policy, input verification, atomic lock
publication, receipts, build failure/interruption, pre/post checks and a run path
that never builds. The test probe's caller-supplied hash is deliberately not a
substitute for that workflow. Gates are not waived by this integration passing.
