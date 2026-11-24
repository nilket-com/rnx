# ADR 0334: `dbg!` runs on the new engine

Status: Proposed

Base: published 6da26e4328ea29139a0889a9583de55dec4a7ff6 (ADR 0333).

## Problem

The fallback-deletion gate on aa5af565 stopped on 118 stock routing
cases that succeed today only through the legacy route. The largest
group sharing one first blocker is `dbg!`. Of the 32 stock cases with
`FrontendDiagnostic`, 31 stop at this macro (11 originating examples
plus corpus variants). L1 00291 stops at a const block and stays outside
this record. The case IDs and macro spans are recorded in
`target/0328/workload-differences.tsv`.

There are two gaps, and fixing the first exposes the second.
- `modules/io.rs:81` registers `dbg_macro` without the built-in macro
  certification that `print!` and `println!` get. The new frontend
  refuses its expansion, `::std::io::dbg(...)`.
- With default stdio, that callee is `dbg_impl`, a variadic raw handler.
  It locks stdout, writes each evaluated argument in debug format
  followed by a newline, and returns unit. Unlike the print handlers, it
  carries no execution certification.

The existing C2 contract documents both refusals, the compiler one and
the native one (`experimental_core/tests/gap_hunt/output_macros.rs`).
The replacement and installation controls live in
`tests/macro_certification.rs`.

## Decision

1. Certify the built-in `dbg` macro and the installed `::std::io::dbg`
   handler by identity, as `print!` and `println!` are certified. A
   context whose `::std::io::dbg` is missing, overridden or installed by
   someone else keeps its current refusal. Matching the spelling is not
   enough. Cover the direct qualified function call and an alias of the
   built-in macro. A source-defined function with the same name keeps
   its own behavior. The distinct capture handler also keeps its
   existing execution refusal and produces no captured or terminal
   output.
2. Behavior matches the old core:
   - arguments are evaluated once, left to right, before the handler
     writes its first line; effects from evaluating the arguments occur
     in their original order;
   - each value is written to stdout in the old core's debug format,
     one per line, under one stdout lock;
   - the call returns unit;
   - an argument error prevents all handler output. A write error
     preserves the bytes already written. The stock formatting oracle is
     Rust Debug for Rune Value, as used by dbg_impl. In particular,
     a failing debug protocol can render a diagnostic containing an
     address and still return success.
3. Debug text is byte-identical to the old core wherever the old core's
   text is deterministic and carries no identity. That covers strings,
   numbers, Booleans, characters, unit, and vectors, tuples, objects,
   structs, enums, options and results built from them, and template
   strings.
   - Where the old core prints a heap address or an internal code
     offset, such as a function's `(at: 0xd)` or an address inside a
     container, its text is not a contract: two old-core runs of the
     same program print different addresses. There the new engine
     prints its own identity text, documented once and stable within a
     run. Everything around that token stays byte-identical.
   - A failing debug protocol keeps the old core's shape: a fallback
     line with the new engine's identity text, and the call still
     succeeds.
   - Cycles and depth always terminate, with a documented marker.
     Report the old core's behavior on the same inputs. Where it fails
     to terminate or crashes, the new engine does not copy that.
   - Any other difference from the old core is a defect, not an
     identity exception.
4. Scope: no opcode or dispatcher change, no host ABI change, and no
   change to the legacy engine.

## Controls

1. Replay all 31 stopped case IDs from the table, with the same harness
   entry, argv and both native modes, with fallback disabled. Use the
   published runner as the reference for status, stdout, stderr and file
   effects. Report which examples now finish and the first new refusal
   with its span for each remaining example. Count a restored outcome
   only when it runs and agrees. Keep 00291 as an unrelated control.
2. A debug-format table for every admitted value kind against the old
   core: byte-identical rows, plus identity rows that compare everything
   but the identity token and assert that token's documented shape.
   Include cycles, deep nesting, aliasing through a second binding, and
   a failing debug protocol.
3. Evaluation order, a side-effecting argument that runs exactly once,
   an argument error after earlier output, and a closed stdout.
4. The missing, overridden and foreign-installed handler cases keep
   their refusals. Replace C2's blanket dbg-refusal assertion for the
   newly admitted rows with measured outcomes. Keep the stock
   observations, existing execution-fuel conventions and unaffected
   dispositions. Cover zero arguments, return unit, alias mutation by a
   later argument, and a failing later argument after an earlier
   argument has printed.
5. The file gate and both project screens, image to image against
   published 6da26e43. No allowance.
6. Finals: targeted pre-sign checks under the final's real recipe, with
   every measured delta checked against its band, then one complete
   final on the signed candidate.

## Constraints

- No new third-party crate. Tabs.
- Programs admitted today keep the same results and effects.
