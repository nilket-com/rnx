# ADR 0249: string input for host functions with a dynamic parameter

Status: Proposed

Base: published d783768ce1f5eae014a4ca6d9fcfc95678651768 (ADR 0248).

## Problem

Ten host functions stay refused in an execution session under the reason
"dynamic input". Each takes a parameter declared as a dynamic value in Rust
and decides inside its body what it accepts. The bridge admits only closed,
typed argument slots, so none of the ten can be enrolled.

The ten do not share one contract. Four accept a string or bytes in one
position and already return a supported result:

	fs::write, fs::append, fs::write_new, web::parse_form

The other six need a recursive value export, bytes, or a result shape the
importer does not have. A session user meets the first group on the first
file write.

The bridge today answers an unsupported argument by handing the typed
conversion a manufactured witness so that the conversion fails with the
normal error. A dynamic parameter accepts any value, the witness included.
Admitting a dynamic parameter as it stands could run a file write with
fabricated data.

## Goal

The four functions are callable from an execution session with a string in
the dynamic position, with the same result as legacy. No host body ever
receives a value the script did not pass.

## Required behavior

- Scope is the four functions, string input only. The argument may come
  from a literal, a local, a branch result or a closure defined in an
  earlier input; the check is made at each call.
- A call with a non-string value in the dynamic position has the legacy
  outcome: the function returns Err with the legacy text, through the
  Result channel, and can be matched or propagated by the script. It is not
  a runtime type failure. The text has one source shared with the legacy
  body, so the two cannot diverge.
- All arguments are checked and owned before the host body runs. A rejected
  call opens, creates, truncates and appends nothing. A call that succeeds
  performs its effect once, including when importing the result fails.
- The host receives an owned copy. It holds no borrow into the execution
  store, and the script's string is unchanged.
- An input contract is an obligation checked before the call; an output
  certificate is a promise about results. They stay separate types. No
  blanket enrollment of a dynamic parameter and no reuse of an output shape
  as an unchecked input promise.
- The manufactured witness never reaches a host body as data. If the design
  lets the witness reach a handler at all, the handler can observe only
  "not a string" and returns the legacy Err; a test pins this.
- web::parse_form returns its object through an explicit result certificate
  and the existing transactional importer.
- Bytes are not representable on the new engine today. State this in the
  record and pin it with a test, so that the string-only contract is
  complete for every value an execution session can produce.
- The six functions that stay refused each get the reason that names what
  they still need (recursive input, bytes output, response shape), not the
  shared label. The policy and reason guard of ADR 0248 covers the change.
- Failed installation of a binding stays uncommitted as today. Effects of
  earlier expressions in the same input are not rolled back; this is the
  existing contract and stays.

## Fixed route

Executor::intrinsic, Executor::fixed_intrinsic and Executor::host_call are
in the fixed-route inventory. The source condition compares them token for
token and has no exception. This record does not edit any inventoried item
and does not change the policy file.

The implementer shows, before editing, a route that meets this: for
example an alternate typed handler for the execution engine whose parameter
type converts a string to owned text and every other value to "not a
string", registered with its own binding identity, so that the existing
typed call path carries it unchanged. If no route exists without editing
an inventoried item, stop and bring that finding; changing the inventory is
a policy change and is not decided inside this record.

The Ir bands pass per record and cumulatively, as for every record.

## Out of scope

- json::stringify, process::run, process::run_bytes, web::response,
  web::html, web::text, time::from_parts, web::escape_html, fs::read_bytes.
- Recursive value export, bytes, async, native values.
- Changes to legacy behavior or to the legacy registration of the four.
- Remedy text per front end.

## Acceptance

- session_parity.json: the four rows leave "Refused for a host function"
  and become Equal with executed witnesses on isolated temporary files;
  host refusals fall from 18 to 14. Wrong-type calls for each of the four
  are Equal. No row moves to Wording or to a different result.
- The six remaining dynamic rows carry their specific reasons.
- Controls: string from a local, a branch and an earlier closure; wrong
  type in each dynamic position leaves the file system untouched; effect
  once on result-import failure; the script's string unchanged after the
  call; panic and allocation failure release the owned copy.
- Miri covers the new ownership boundary.
- The fixed-route source condition and bands pass. Required stages follow
  the changed paths.
