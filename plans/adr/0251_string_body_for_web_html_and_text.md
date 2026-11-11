# ADR 0251: string body for web::html and web::text

Status: Proposed

Base: published 3415f56ef43118cbefe35d10a3ce59a247b9ab2d (ADR 0250).

## Problem

Thirteen host functions stay refused in an execution session. Two of them,
web::html and web::text, are the shortest way to build a response and are
refused only because of how their body parameter is declared: a dynamic
value that the function checks inside its body, placed after an integer
status. The string input of ADR 0249 has the right behavior for such a
parameter, but its sealed signatures cover a lone argument and a string
followed by the argument, not an integer followed by the argument.

Both functions are pure constructors. They perform no input or output and
have no cancellation contract. Their result is an object of an integer, a
string and an object of string lists, which the existing result importer
already carries.

## Goal

web::html and web::text are callable from an execution session with a
string body, with the same result as legacy for every value an execution
session can produce.

## Required behavior

- Scope is the two functions. The body may come from a literal, a local, a
  branch result or a closure defined in an earlier input.
- One more sealed argument arrangement for the string input of ADR 0249:
  an integer followed by the string-or-rejected argument. It is not a
  general dynamic argument and adds no other arrangement.
- A body that is not a string has the legacy outcome: Err with the legacy
  text through the Result channel. The text has one source shared with the
  legacy body.
- Checks keep the legacy order. Where legacy reports the body before the
  status, the execution engine does the same; a call that is wrong in both
  reports the same single error in both engines.
- Status bounds, the empty-body rule for 204 and 304, the body size limit,
  and the fixed content type have one implementation used by both engines.
  No second validator.
- The body is not escaped. The result is independent of the input: changing
  or dropping the script's string afterward leaves the response unchanged,
  and the script's string is unchanged by the call.
- The result is admitted through an explicit result certificate that states
  the response shape truthfully, and through the existing transactional
  importer. No new importer variant.
- Bytes stay unrepresentable on the new engine. A legacy call with a bytes
  body keeps its behavior on the legacy engine. The record makes no bytes
  claim for the execution engine and pins this with a test.
- A moved body keeps the existing moved-value runtime contract.
- The eleven functions that stay refused keep their specific reasons.

## Fixed route

This record does not edit any inventoried item and does not change the
policy file. The route is the alternate handler of ADR 0249. If the new
arrangement cannot be added without editing an inventoried item, stop and
bring that finding.

The Ir bands pass per record and cumulatively, as for every record.

## Out of scope

- web::response, web::escape_html, process::run, process::run_bytes,
  fs::read_bytes, time::from_parts, the async functions.
- Bytes, multiple dynamic arguments, a new error channel for host returns.
- Changes to legacy behavior or to the legacy registration of the two.
- Manifest and lock files.

## Acceptance

- session_parity.json: the two rows leave "Refused for a host function" and
  become Equal with executed witnesses; host refusals fall from 13 to 11
  and Equal rises from 136 to 138. Wrong-body calls for both are Equal. No
  row moves to Wording or to a different result.
- Controls, each compared between engines: body from a local, a branch and
  an earlier closure; status below and above the bounds; empty and
  non-empty body with 204 and 304; body at and over the size limit; text
  with markup characters returned unescaped; Unicode and NUL; wrong body
  with a wrong status; the script's string unchanged and the response
  independent of it; a call from a program compiled in an earlier input.
- API controls: the new arrangement compiles; other arrangements do not,
  with a compiling twin for each negative as in ADR 0249.
- Miri covers the new arrangement if it adds an ownership path the
  existing ADR 0249 tests do not exercise; otherwise the record states why
  the existing coverage holds.
- The fixed-route source condition and bands pass. Required stages follow
  the changed paths.
