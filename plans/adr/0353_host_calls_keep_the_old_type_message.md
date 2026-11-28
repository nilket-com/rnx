# ADR 0353: Host calls keep the old type message

Status: Proposed

Base: published 8f66ec4b (ADR 0352).

## Problem

A host function called with an argument of the wrong type can
end on the new engine with a message that the old core reports
better. Measured on the image tested for ADR 0352, from the
installed registry: 459 admitted host paths, 643 argument
positions and 39 kinds of value, 25,083 calls compared in each
native mode. The old core reports a typed message ("Expected
type ... but found ...") for 20,241 of them. The new engine
reports a different text for 1,264, at the same location and the
same in both native modes, in three classes.

1. An engine kind with no detail, 409 calls. With the full path,

       pub fn main(_args) { std::io::println(true); }

   reports `store: Type` (38 calls). A host function reached
   through the argument bridge, with a value the bridge doesn't
   convert, reports `execute: InvalidCode` (371 calls on 42
   paths):

       pub fn main(_args) { path::join("left", 1..=3); }

   The old core reports "Expected type `std::string::String` but
   found `std::ops::RangeInclusive`".
2. An erased iterator name, 795 calls. At 53 bridged argument
   positions an iterator is named `std::iter::Iterator` where the
   old core names its type, such as `std::string::Chars` or
   `std::ops::RangeIter<std::i64>`.
3. A wrong tuple name, 60 calls. At argument 3 of `web::response`
   and argument 2 of `time::from_parts` the new engine names
   `std::tuple::Tuple` where the old core names the value passed,
   such as `std::vec::Vec`. The host input's prefix is taken
   unchecked and exposes a placeholder.

The same replay finds two other differences, and in both the new
text is the better one. For a closure of the wrong arity passed
to `fold`, `reduce` or `Vec::sort_by` (29 calls) the new engine
counts the closure's own parameters. When a call exhausts the
budget (78 calls) it says how to raise it. Neither is a type
message.

The committed host path control of ADR 0350 compares the outcome
of each call, not its text, so it doesn't see these classes.

## Decision

1. Every admitted host call with an argument of the wrong type
   reports the old core's message at the old core's location, in
   both native modes. That covers the three classes of the
   Problem and every case with the same cause.
2. The host path control compares the message text. It checks
   the installed registry, not a list written by hand, so a
   function added later is checked without a new record. The two
   kinds of message where the new text is the better one are
   allowed by a rule about the kind of message, never by a list
   of paths.
3. The paths a program took on the base keep their cost. The new
   work sits on the error branch. A new error detail doesn't
   change how the no-detail case is encoded.
4. The forms that run on the base keep their route and outcome.
   No rule is keyed to the name of one function where a general
   rule serves.
5. A surface found with another cause is reported and left for a
   ruling.
6. Scope: the new engine and the host path control. No change to
   the legacy engine or the host ABI. If the fix changes a
   function of the gate's source inventory, the record declares
   the transition under ADR 0346. The count of legacy-only cases
   is expected to stay 67. Any other count is a finding to
   explain.

## Controls

1. The forms of the Problem, one per class, each equal to the old
   core in both native modes.
2. The host path control of Decision 2. Results give the paths,
   positions and kinds compared, the calls whose text differs on
   the base and on the candidate, and each allowed difference
   with its kind.
3. Each class's old behavior, planted back, breaks the control.
4. The public admission corpus is replayed against the published
   base in both native modes. Every source that changes is
   listed with its old and new outcome and route.
5. Pinned verdicts follow the engine. Results list the rows.
6. The file gate and both project screens, image to image
   against the published base. Results give the encoding of the
   no-detail case on the base and the candidate.
7. Before signing, the whole rune-suite stage, the root-tests
   stage, the cross stage, and every other stage of the catalogue
   whose command reads a changed file, each by its stage command.
   Then one complete final on the signed candidate, under the
   limits of ADR 0338.

## Constraints

- No new third-party crate. Tabs.
- No refusal at run time.
- A program that succeeds on the base keeps its results, effects
  and route.
