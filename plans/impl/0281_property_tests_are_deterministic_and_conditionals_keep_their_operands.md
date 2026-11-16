# Implementation plan 0281: property tests are deterministic and conditionals keep their operands

Status: Implemented

Paired ADR: ../adr/0281_property_tests_are_deterministic_and_conditionals_keep_their_operands.md.

1. Pin 8a2ad0e5. Read the 17 property tests and their shared library
   under `rune/hegel_tests`, how the property-test library chooses its
   seed and database, the package gate that runs them, and the
   conditional steps of the old engine's assembler
   (`ConditionalLhs`/`ConditionalRhs` in `compile/v2/assemble.rs`) with
   the code that hands an assignment's target address to its value.
2. Reproduce the failing program on both engines. Run the development
   sweep from the ADR's controls.
3. Before changing the engine, send a note: the cause on the old
   engine, whether the new engine has the same class, every failure
   from the sweep with its class, and the shape of the fix. If any
   failure needs more than a small fix, send the note and stop.
4. Add the shared settings constructor, the committed check and its
   negative control, the committed examples, the formatter fix, the
   exploration command with its documentation, and the conditional fix
   with its controls. Register the active column-probe, measurement-harness and xtask
   packages with test/check, format and lint recipes so
   the conservative dependency selection can validate this record.
5. On the signed candidate: the eight-seed sweep and two ordinary package
   runs with identical digest maps, then one complete final (which runs
   every format, lint, root, Rune, manifest and package stage, including
   the fixed-route screen), then preflight and the commit checker. No separate run
   of stages the complete final already runs. Bring the packet with the
   drawn-case digests of both package runs, the sweep's seeds and results,
   the screen rows, and the per-stage durations. Results state what the
   code does; no paths under target/ and no development history. The
   candidate is frozen once the packet is sent, except for docs-only
   amends, which need no rerun.

## Results

All 17 property tests use the same runner, with seed 2, a fixed randomness
backend and no failure database in ordinary builds. Each keeps its prior
case-count floor. The wall-clock health check is disabled; case-count
health checks remain enabled. The generator dependencies have a committed
lockfile.
An explicit development feature enables chosen seeds and higher counts;
ordinary builds reject those overrides. The policy control rejects raw
settings, attribute bypasses, missing properties and reduced counts. The
runner records SHA-256 digests of actual case boundaries and draws,
including floating-point bits and canonicalized map inputs.

Legacy conditional assignments evaluate in an owned temporary when their
output is borrowed storage, including a break's loop or labeled-block
output. The result is assigned after the conditional joins. Discard,
divergence and diagnostic paths release that temporary. The new engine is
unchanged. Truth-table controls cover both operators on both engines;
legacy cleanup controls cover discard, divergence and diagnostics.

The formatter recognizes whitespace before a macro delimiter, excludes
comments before an object's brace from its body-layout decision, and
requires an identifier for a module name. Macro calls accept whitespace
before parenthesis, bracket and brace delimiters, as in Rust. Complete macro and object
examples remain fixed points. Incomplete macros and nameless modules
produce diagnostics instead of output that changes on another pass.
Every discovered formatter example has an unconditional control.

The active column probe and measurement harness have explicit check, format
and lint recipes. Their checks include all targets and features. The xtask
package has test, format and lint recipes, including its scheduler controls. Probe
builds and lints use headers from the configured Valgrind installation;
format checks require no headers. Conservative gate selection is unchanged.
