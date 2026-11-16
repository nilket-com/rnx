# Implementation plan 0281: property tests are deterministic and conditionals keep their operands

Status: Proposed

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
   with its controls.
5. On the signed candidate: the eight-seed sweep and two ordinary package
   runs with identical digest maps, then the fixed-route screen, then one
   complete final (which runs every format, lint, root, Rune, manifest and
   package stage), then preflight and the commit checker. No separate run
   of stages the complete final already runs. Bring the packet with the
   drawn-case digests of both package runs, the sweep's seeds and results,
   the screen rows, and the per-stage durations. Results state what the
   code does; no paths under target/ and no development history. The
   candidate is frozen once the packet is sent, except for docs-only
   amends, which need no rerun.
