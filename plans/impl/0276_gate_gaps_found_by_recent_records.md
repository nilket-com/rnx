# Implementation plan 0276: gate gaps found by recent records

Status: Implemented

Paired ADR: ../adr/0276_gate_gaps_found_by_recent_records.md.

1. Pin 4d1cf05e. Read the required-stage selection and its tests, the
   failure bundle and the final's own diagnostics file, the cross
   stage's receipts, and the parity table's serializer, its readers,
   its size cap and the controls that mutate pinned rows.
2. Before editing, send a short note: the selection rule as it is and
   as proposed, with the old and new selections for records 0272 to
   0275; what each failing stage kind writes and what the bundle will
   take from it; a byte breakdown of the parity table and the
   proposed representation with its measured size and headroom; and
   whether the cap needs to move.
3. Implement the three decisions.
4. Add the controls of the ADR to the tooling's own tests.
5. Run both format checks and every lint recipe by hand, then the
   complete final (gate tooling changes), preflight and the commit
   checker. Bring the packet with the sizes, a sample bundle for each
   failure kind and the per-stage durations. Results state what the
   code does; no paths under target/. The candidate is frozen once
   the packet is sent.

## Results

`package_gates.py` declares test/check, format and lint stages for the
root, project tool, Jupyter, three adapters, no-std, examples, benches,
Hegel, Polars generator, two demos and the HTTP/Postgres server.
Rune crates retain the existing Rune-family lint feature matrix; this
is not primary-package lint of every upstream crate. Source paths are
src, tests, examples, benches, build.rs and package-root Rust files.
Ownership uses the longest manifest parent from both committed trees.
Graph edits retain both dependency graphs and reverse dependents.
Original conservative selections remain; quality selection only grows.
Documentation alone gains no new stages.

Unregistered source owners fail with their manifest and an instruction
to declare recipes. They are ci/execute_guard, rune/tools/builder,
rune/tools/generate, rune/tools/site, rune/tools/xtask, and its nested
column/probe and measure/harness packages. Archived probes remain
excluded. No whole-workspace lint coverage is claimed.

Recent selection replay:

| Record | Old selection | Added |
|---|---|---|
| 0272 | commit-discipline, controller-controls, cross, inventory, jupyter-clippy, jupyter-format, jupyter-kernel, jupyter-tests, root-tests | format, clippy |
| 0273 | commit-discipline, controller-controls, cross, fixed-route, inventory, root-tests, rune-clippy, rune-suite | format, clippy |
| 0274 | same as 0273 | format, clippy |
| 0275 | same as 0273 | format, clippy |

The common runner retains a bounded diagnostic bundle before its
caller releases payloads. The final copies that verified bundle and
uses its concrete cause as the top-level error. Named libtest output
sections precede excerpts, including nested runner reports. Producer
failing-case fields and declared diagnostic files precede the reviewed
stderr/log sibling list. No arbitrary payload walk, invocation dump or
measurement vector is copied. The sanitizer, 64 KiB per-file, 256 KiB
bundle, 1 MiB final limits and release order are unchanged.

Real common-runner controls retain a middle-log assertion and cross
identity mismatch after deleting their owned payloads, assert exact
causes and secret absence, and reject a tail-only mutation. Named
cases, nested test reports, links/outside paths and truncation remain
covered. All 307 Python controller controls passed.

Schema 1 stores 265 identical pairs once as `both`; unequal pairs keep
legacy/domain. Readers expand before classification and mutation.
Observations are opaque. Only `Equal` requires identical observations;
a differing `Equal by property` pair round-trips. Mixed and partial
pairs, double encoding, unknown schema and dual equality rows fail.
No classifier decision or observation changed. The committed controls
use the current table, not a historical revision.

Measured size: 163,452 to 134,654 bytes, saving 28,798. The unchanged
163,840-byte cap leaves 29,186 free bytes instead of 388. At observed
mean pair-row sizes this permits about 89 additional rows versus zero
whole rows before, not twice the total row capacity. Legacy/domain
observation data used 30,304/33,977 bytes, the ledger 23,421, summary
462 and other inputs/metadata/layout 75,288 before compaction.
The independently compared expanded canonical SHA256 before and after:
429d4b1e9db06a68a42132c00400c56edab6da4551495240dcea088b68ad5a4b.

The added Hegel format check found old space indentation in that
independent test workspace; the declared formatter repaired it.
The native run also exposed a published late-interrupt fixture race:
/bin/kill completion only submitted the signal. The test now asserts
delivery through the actual interrupt flag with a three-second hard
deadline, then checks completion once. Interrupted verdict, unpublished
binding refusal and next-input recovery stay required. Neighboring
0270–0275 controls already await interruption diagnostics or use an
injected condition; no additional instance needed repair. Product
runtime code is unchanged.

The entire before-signing native run was repeated from the start and
passed: both original format checks, every declared package format,
every lint recipe including the four Rune feature combinations,
and full root tests. All 34 commands passed in 327.44 seconds.
