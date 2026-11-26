# ADR 0344: A string is built on the new engine

Status: Proposed

Base: published 2ab8ccd7 (ADR 0343).

## Problem

Six of the 73 legacy-only cases build a string in place. L1 cases
00133 and 02887 and script 00133 do

    let a = String::from("Hello");
    let b = a;
    a.push_str(" World");

and print both names. L1 cases 00177 and 02906 and script 00177
start from `String::with_capacity(42)`, call `push_str`, `push`,
`capacity` and `shrink_to_fit`, and end with `into_bytes` and the
length and capacity of the bytes.

Measured on the image tested for the published f562c486 (ADR 0342),
with the automatic route and with the legacy engine forced:

- `String::new()` and `String::with_capacity(n)` are refused with
  `Native`.
- `push_str`, `capacity`, `shrink_to_fit`, `clear`, `reserve`,
  `reserve_exact`, `into_bytes` and `as_bytes` on a string are
  refused with `Method`, with or without an alias.
- `String::from`, `len`, `clone` and `push` with a char already run
  on the new engine. A `push` while the string's chars are being
  iterated reports "Cannot write the receiver: it is being read" on
  both engines.
- A string is one value under all its names on the old core. After
  `let b = a`, a `push_str` through `a` shows through `b`. The same
  holds for a field, a vector element, a closure's capture, an
  argument and a returned value. `clone`, `String::from`, `+`,
  `format!` and a template yield a separate string. A literal
  yields a new string each time it is evaluated, so a function
  that appends to its literal gives the same text on every call.
  An object key keeps its text when the string it came from is
  changed afterwards.
- `with_capacity(42)` reports capacity 42 after three bytes are
  pushed, and 3 after `shrink_to_fit`. On the old core an empty
  string grows from 0 to 8 to 16, and `format!` or a template gives
  a two-byte string the capacity 8. The new engine's store grows a
  string to its exact length on `push` and `+=`, and its formatting
  reserves the exact size.
- `s.push_str(s)` is refused by the old core with "Cannot read
  argument 1: it is being written". So is `s += s`, which the new
  engine already runs: it appends the string's own text.
- `into_bytes` hands its allocation to the bytes it returns. The
  new engine has no bytes value yet.

So the new engine already shares a string and guards its access.
What it lacks is the two constructors and six methods.

## Decision

1. `String::new`, `String::with_capacity`, `push_str`, `capacity`,
   `shrink_to_fit`, `clear`, `reserve` and `reserve_exact` run on
   the new engine with the old core's result, output and errors.
   A call is bound to the string's method only where the receiver
   is proved a string; `clear` and `capacity` on another type keep
   their route. `String::from_str` and `String::from_utf8` are
   conversions and aren't in this record.
2. A change through one name shows through every other name of
   the same string, as on the old core, and an operation that
   yields a separate string on the old core does so here. This
   uses the sharing and the access checks the new engine has. No
   second string representation is added.
3. A write while the string is being read gives the old core's
   error with its text and position. One refusal of the old core
   isn't kept: `s.push_str(s)` appends the string's own text, as
   `s += s` already does on the new engine. The old core refuses
   both because its guard can't lend a value to itself. The two
   spellings mean the same and behave the same here.
4. `capacity` is observable. Where the program sets it, its values
   are the old core's: `String::new`, `with_capacity`, `reserve`,
   `reserve_exact`, `shrink_to_fit`, `clear`, a literal, `clone`,
   `String::from` and `+`, and the growth by `push`, `push_str` and
   `+=` from those. The store may change how it grows a string to
   get there: it uses the growth of the standard string, not a
   table of numbers. The memory budget still refuses before a
   growth it can't afford, refunds a shrink and leaves a failed
   growth without effect. A string that comes from `format!`, a
   template or a host function starts with the capacity the new
   engine gives it, which may differ from the old core's. No
   `capacity` call is refused for its receiver's origin. Results
   record the difference with its witnesses.
5. `into_bytes` and `as_bytes` wait for the bytes record. L1 cases
   00177 and 02906 and script 00177 stay legacy-only.
6. The old core's measured outcome is the reference for the
   result, the output, the error text and its position, except as
   Decisions 3 and 4 state. The route is recorded apart from the
   output.
7. Scope: the new engine's front end, lowering and library
   enrollment for strings. No change to the legacy engine. No new
   opcode unless the boundary shows a method can't be had without
   one; report that before building. The bytecode format, argument
   passing and host ABI don't change. Record the corpus, the
   command and the count of legacy-only cases on the base and on
   the candidate. The base count is 73 and the expected count is
   70. Any other count is a finding to explain.

## Controls

1. Each constructor and method of Decision 1 on a string from a
   literal, from `String::from`, from `String::new` and from
   `with_capacity`, with native code on and off. Each asserts the
   new engine's route and equals the old core.
2. Sharing: a change through a second local, a field, a vector
   element, a closure's capture, an argument and a returned value
   shows through the first name. `clone`, `String::from`, `+`,
   `format!` and a template stay separate. A literal in a function
   called twice gives the same text both times. An object key
   keeps its text.
3. Access: a write during an iteration over the string's chars,
   with the old core's error and position, and `capacity` read
   during that iteration. `s.push_str(s)`, `s.push_str(alias)` and
   `s += s` each append the string's own text; the old core's
   refusal is recorded beside them.
4. Capacity: each origin and operation that Decision 4 names, each
   value equal to the old core's, including the growth of an empty
   string by `push`, `push_str` and `+=` and `reserve_exact` after
   it. A string from `format!` and from a template, with the new
   engine's value and the old core's. A growth the budget refuses
   leaves the string, the store's entries and its bytes as they
   were.
5. Wrong arguments: `push` with a string, `push_str` with a char or
   a number, `with_capacity` with a negative or a non-integer
   value, each with the old core's error.
6. The source of L1 case 00133 on the new engine's route. The
   source of L1 case 00177 keeps its route and output.
7. A heap control: the strings built in a loop are released, and
   the store returns to zero live entries and zero payload bytes.
8. Each newly admitted name, planted out alone, breaks its own
   witness and leaves the others passing.
9. The public admission corpus is replayed against the published
   base in both native modes. L1 cases 00133 and 02887 and script
   00133 move to the new engine with the same result. Every other
   source that changes is listed with its old and new outcome and
   route.
10. The file gate and both project screens, image to image against
    the published base.
11. Before signing, the whole Rune library test binary in separate
    build state. Then one complete final on the signed candidate,
    under the limits of ADR 0338.

## Constraints

- No new third-party crate. Tabs.
- No refusal at run time.
- A program that builds no string in place keeps its results,
  effects and route. A move from the legacy route to the new
  engine with the same exit, output and effects is within the
  record.
