# ADR 0246: measured session parity classification

Status: Proposed

Base: published cfdd6a764dc3ffb9ada73fa263a3fe143ca24d45 (ADR 0245).

## Problem

The domain session is opt-in because it supports less than the legacy
session. How much less is known only by example: async input, native values,
some host functions, some source forms. Nobody can say which gap blocks the
most real input, so the order of the remaining work is a guess, and no test
fails when a record claims to close a gap and does not.

## Goal

One committed classification, produced by running code, of everything the
legacy session accepts that the domain session does not, by exact reason.
Its totals are pinned, so a later record that closes a gap must change the
pinned number in the same commit, and a regression fails a test.

## Required behavior

- Inputs classified, each from a source already in the repository:
  1. every input of the existing legacy session tests and session transcript
     fixtures, in order, with the state those inputs build;
  2. every host registration in the tested host policy inventory, through a
     minimal call that the legacy session accepts;
  3. the REPL and notebook demo transcripts that are committed.
  If a source cannot be driven mechanically, the record says so and counts
  it as not classified. It is not silently left out.
- Each input gets exactly one class:
  Equal; Wording (reviewed text difference); Refused at admission, with the
  live reason and the construct; Refused for a host function, with the
  function; Presentation (value computed, legacy shows more, for example
  HTML or a native presenter); Different result; Domain failure the legacy
  session does not have.
  Reasons are what the engine reports for that input, not a list of enum
  variants. Reserved variants that no input reaches do not appear.
- Different result and Domain failure are defects, not gaps. Each one found
  is listed with its input. If any exists, the record stops for review
  before pinning, and the fix is its own record.
- The classification runs as an ordinary test in the required stages. Output
  is a deterministic table committed in the repository, compared exactly.
- A short summary groups the refusals into the work items they need: host
  enrollment (synchronous), async, native values and presenters, source
  forms, and anything that fits none of these. Counts per group, and for
  each group the three most frequent constructs.
- Nothing in the session, the engine or admission changes in this record.

## Out of scope

- Closing any gap, changing any refusal text, or changing defaults.
- File, server and column routes. Their inventory already exists.
- Judging which gaps are policy (never to be supported). The table marks
  nothing as policy; that decision is taken per gap in later records.

## Acceptance

- The committed table and its pinned totals, reproduced by the test.
- Negative controls: removing one admitted host function from the domain's
  admission in a test build changes exactly that row and fails the pin;
  an input moved between classes fails the pin.
- Every source in the list above is either classified or named as not
  classified with the reason and its input count.
- No change to runtime paths: the fixed-route source condition passes and
  the path table does not select the performance gate, or it passes.
- Required stages follow the changed paths.
