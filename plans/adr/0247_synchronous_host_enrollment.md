# ADR 0247: synchronous host functions on the new engine

Status: Proposed

Base: published 10e71b7b4c4e2f3cd2d9e015ee7fd8d1d04a2243 (ADR 0246).

## Problem

The pinned classification of ADR 0246 shows 27 host functions that the
legacy session accepts and the new engine refuses at admission. Five are
asynchronous (time::sleep and four http calls). The other 22 are ordinary
synchronous library calls:

	json::stringify
	process::run, process::run_bytes, process::exit
	web::escape_html, web::parse_form, web::response, web::html, web::text
	fs::read_bytes, fs::write_new, fs::write, fs::append, fs::metadata,
	fs::read_dir
	time::parts, time::from_parts
	text::split_max
	env::args, env::var, env::home_dir, env::vars

This is the largest group of refusals in the corpus and needs neither async
nor native values. A session user meets it on the first file write or JSON
output.

## Goal

Each of the 22 is either admitted and executed on the new engine with the
same result as legacy, or stays refused with a stated reason that names what
it needs. The pinned table of ADR 0246 moves accordingly in the same commit.

## Required behavior

- Admission stays certified and compile-time. A function is enrolled only
  with a declared input and output shape that the bridge checks; no blanket
  dynamic bridge, and nothing through a legacy Value.
- Where several functions are blocked by the same missing shape (for
  example a dynamic value input, an object or list result, bytes, an
  optional result), the shape is added once with its ownership rule, and
  the functions that need it follow. The boundary note lists the shapes and
  which functions each unlocks, so the review can cut the record if one
  shape is large.
- Results equal legacy for the same call: value, error category, error
  text and origin. Each enrolled function gets an executed differential
  witness; file and process effects use isolated fixtures.
- A function with an effect (file write, process spawn, exit) performs it
  exactly once, and a refusal or a failed conversion of arguments happens
  before the effect.
- process::exit: state what it means in a session on each engine today and
  keep that meaning.
- A function that stays refused keeps a refusal whose text names the
  function and the remedy, in the form of ADR 0242.
- The five asynchronous functions are unchanged and stay refused.

## Out of scope

- Async, native values and presenters, vector constants, diagnostics work
  items from ADR 0246, adapter functions, and the default route.
- New host functions or changes to legacy behavior.

## Acceptance

- session_parity.json: host refusals fall from 27 by the number enrolled;
  those rows become Equal with executed witnesses, or Admitted, not executed
  only where execution cannot be made deterministic, with the reason. No
  row moves to Wording without review, none to Different result.
- The host policy inventory is updated in place and its test passes.
- The fixed-route source condition passes. If a new shape touches the fixed
  operation loop or its inventory, stop and bring it to review before
  editing; the bands must pass without an exception.
- Miri covers any new ownership boundary code.
- Required stages follow the changed paths.
