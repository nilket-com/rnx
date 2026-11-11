# ADR 0245: session error frames in input coordinates

Status: Proposed

Base: published fa6ad468851842911a7c8369156bec0a86b7607b (ADR 0244).

## Problem

A runtime error in a domain session ends with lines such as

	at <memory>:90..93
	at <memory>:130..135

These are byte ranges in the generated program for one input. A user cannot
map them to anything they typed. The primary location is already mapped to
input, line and column; only the call frames are raw. ADR 0242 recorded this
as Wording rows and required it fixed before the domain session can become
the default.

## Goal

Every frame shown for a domain-session error names the input, line and column
of user text, or is not shown. No generated-source coordinate reaches a
terminal, a worker reply or a notebook.

## Required behavior

- Frames come from the structured failure (program and location per frame),
  each mapped through the source map of the revision that defined it. A
  closure defined in input 2 and failing when called from input 5 shows a
  frame in input 2 and a frame in input 5.
- No string matching or rewriting of rendered text. If the engine API cannot
  give the primary diagnostic without its frame suffix, add that narrow
  accessor; message() keeps its meaning for other callers.
- A frame inside generated wrapper text has no user location. It is omitted
  under one stated rule, tested; a location is never invented.
- Caller order is kept. The number of frames shown is bounded, and a cut is
  marked. Frames whose source is no longer available say so.
- Input numbers follow the session's numbering, including after :renumber
  and :reset, as the primary location does.
- Text is materialized as owned, bounded strings while the failure still
  borrows the domain; programs are released and collected only afterwards.
- The worker reply carries the same frames as the terminal. If the reply
  gains a structured field, protocol-1 readers must accept it, shown as in
  ADR 0244; otherwise frames stay inside the existing diagnostic text.
- Legacy session output is unchanged.

## Out of scope

- One-shot and file routes, whose frames refer to a real file.
- Any change to the fixed operation loop, its inventory or frame capture.
- Changing what the legacy session prints.

## Acceptance

- The four ADR 0242 Wording rows are replaced by rows whose execution text
  contains input coordinates; the table still pins both texts in the row.
- New rows: a three-input call chain; a retained closure called after
  :renumber and after a redefinition of a function it calls; a frame cut at
  the bound; a wrapper-only frame.
- A test fails if the bytes `<memory>` appear in any session or worker
  diagnostic in the transcript and worker tables.
- Worker table: the same failure through both workers, frames compared.
- Miri subset is extended only if ownership code changes.
- Required stages follow the changed paths.
