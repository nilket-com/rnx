# ADR 0254: witnesses for session rows blocked by their setup

Status: Proposed

Base: published 473df887b625d61406acdabd05fdd4dd14061ba9 (ADR 0253).

## Problem

The pinned comparison of the two session engines has 18 rows in the class
"Not classified". Fourteen of them are inputs taken from the session tests
whose earlier inputs in the same sequence are not literal text: a loop
that generates declarations, a run of filler inputs, a budget change, a
write outside the session. The extractor reads only literal inputs. When it
meets a setup it cannot read, it marks every later input of the sequence
as blocked.

A blocked row says nothing about either engine. It says the comparison was
never made. These fourteen are the session behaviors the tests were
written for: bindings restored into a narrowed prelude, aliases that keep
one shared handle, a closure that keeps seeing a restored binding, format
and template captures, state kept after a failure. They are the evidence
most relevant to deciding whether the execution engine can be the default
for sessions, and it is missing.

## Goal

Each of the fourteen rows is compared between the engines on the exact
sequence its test runs, and carries the class the comparison shows.

## Required behavior

- Scope is the fourteen rows in these sequences: in the checks sequence,
  the row after the external-write failure and the row after the hundred
  increments, with the budget change replayed as setup; the generated
  declarations; assignment, shadowing and destructuring; the shared-handle
  aliases; the closure over a restored binding; the format capture; the
  template interpolation.
- Each sequence gets an explicit witness that replays the same inputs the
  test runs, in order, in both engines: the exact generated declarations,
  the exact number of filler inputs, the same budget change, a private
  file for the write. The witness names the test it mirrors. A change to
  the test that the witness does not follow is detected and fails.
- No general evaluation of loops or format strings is added to the
  extractor. A setup the extractor cannot read stays unread by it; the
  witness is the reviewed statement of what the setup is.
- A row is classed only by what its comparison shows, under the existing
  classes and their existing rules. No row becomes Equal by declaration,
  and no new class or wording rule is introduced. Where captured output
  matters, as in the format capture, output is compared as well as the
  result.
- A difference is kept visible as a row with its exact two outcomes. It is
  not fixed, waived or reworded in this record. If a difference is a wrong
  result in the execution engine, stop and send it; the fix is its own
  record.
- Each row keeps its provenance: which setup blocked it and which witness
  resolved it.
- The external-write witness touches only a private temporary location and
  shows that the failed write is not replayed.
- The four receiver rows and the sixteen "Admitted, not executed" rows
  stay as they are.

## Out of scope

- Engine, session and host sources. This record changes tests, the parity
  generator and the pinned table.
- The receiver rows, standard input, the file system functions, clocks.
- Manifest and lock files.

## Acceptance

- session_parity.json: "Not classified" falls from 18 to 4. The record
  states the class of each of the fourteen rows and the totals before and
  after. No other row changes class.
- A control per witness: altering one input of the witness, or of the
  mirrored test, fails the check that ties them.
- A control that an unread setup without a witness still blocks its
  sequence.
- Required stages follow the changed paths.
