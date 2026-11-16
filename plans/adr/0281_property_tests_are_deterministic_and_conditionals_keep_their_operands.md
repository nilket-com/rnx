# ADR 0281: property tests are deterministic and conditionals keep their operands

Status: Implemented

Base: published 8a2ad0e54c5a0c8c731750193d6de83067acbd9c (ADR 0280).

## Problem

The complete final runs the `rune/hegel_tests` package. Its 17
property tests (15 built with `Hegel::new` and two declared with
`#[hegel::test]`) draw new random cases on every run, because the
property-test library uses a fixed seed only when it detects a CI
environment, and the final is not one. The same source can pass one
final and fail the next.

One such run found a wrong result in the old engine. With `b` false,

```
b = true && b;
```

leaves `b` true. The left operand is written into the address the
assignment hands the conditional, which is `b` itself, and the right
operand then reads `b` after that write. The same program on the new
engine leaves `b` false. An rnx binary built in September gives the
wrong result too, so the defect is older than the records since then;
the random draw only now reached it.

## Decision

1. Every property test in a gate draws the same cases on every run.
   The fixed seed and the disabled failure database are set in the test
   code, through one shared settings constructor, and not by an
   environment variable the runner sets. This covers both the builder
   and the attribute form. A committed check fails when a property
   test in the package builds its settings any other way.
2. Every failure a property test has found is a committed example that
   runs on every gate, whatever the seed draws. This record adds the
   one above.
3. In the old engine's assembler, the conditional operators `&&` and
   `||` evaluate into a temporary of their own whenever the address
   they were handed is borrowed (a variable's storage, or a loop or
   labeled block's result that a `break` writes into) rather than a
   fresh temporary of their own, and the result is assigned once both operands are done.
   No operand observes the other's write. The rule is the address
   kind, not an analysis of what the right operand reads: the old
   engine is being removed, and a read analysis there would cost more
   than the copy it saves. The new engine already evaluates an
   assignment's value into a fresh temporary and agrees with the
   reference in every combination; it is unchanged.
4. The formatter reaches a fixed point: formatting its own output
   changes nothing. The development sweep found `crate ! { ` formatted
   to `crate\n!{}` and then to `crate!{}`, and after that `{ mod {`.
   Each is fixed here and kept as a committed example.
5. Every failure the development sweep finds is fixed in this record
   with its committed example. A failure that needs more than a small
   fix stops the record; none is skipped, weakened or left to a seed
   that does not draw it.
6. Random exploration stays available outside the gate: a documented
   command runs the package with a chosen seed and case count. What it
   finds becomes a committed example under the second decision.

## Constraints

- No test is skipped, weakened or given fewer cases to make it pass.
  Each property test keeps at least its current case count.
- The fixed-route screen runs because the engine source changes, and
  its bands pass unchanged; no inventoried item changes.
- Only programs that assign a conditional into an existing variable,
  or break out of a loop or labeled block with a conditional, change
  their instruction sequence. The packet lists the fixed-route
  programs whose sequence changed and their screen rows.
- The formatter fix changes no committed source file's formatting:
  both format checks pass unchanged.
- No real environment or argv in a committed fixture.

## Controls

- The failing program above, as a committed example: on the base the
  old engine fails it and the new engine passes it; on the candidate
  both engines pass it.
- `&&` and `||`, each with the assigned variable as the right operand,
  both truth values for each operand, with assignment into a local, a
  captured local and a field: every combination agrees with the
  reference on both engines.
- Two consecutive runs of the package draw the same cases (the packet
  shows the drawn-case digest from each).
- A property test that builds its settings without the shared
  constructor fails the committed check (negative control).
- A development sweep of the package under at least eight distinct
  seeds at the current case counts. Every failure is listed in the
  note with its class and fixed under the fifth decision, and the
  sweep passes on the candidate at every seed it ran on the base.

## Out of scope

Other property-test generators.

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
