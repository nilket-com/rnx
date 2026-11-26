# Implementation plan 0343: Const calls and templates are constants

Status: Proposed

Paired ADR: ../adr/0343_const_calls_and_templates_are_constants.md.

1. Boundary: the published base. Reproduce the observations of the
   ADR's Problem on both engines in both native modes. Find how a
   `const` item gets its value and whether the same path can serve
   a call in a function body. List the macros that take a template.
   Find which roots the check of ADR 0342 needs for the two forms.
   Measure the old core's outcome for the planted forms of Controls
   1 to 6. Report before going on if a cause is beyond Decision 8.
   That part then leaves the record as Decision 7 states.
2. Const calls as Decisions 1 to 4 state, with Controls 1 to 5.
3. Constant templates as Decision 5 states, with Controls 5 and 6.
   Control 8 covers both forms.
4. Plant the two refusals back, one at a time. Replay the public
   admission corpus against the published base. List every changed
   source as Control 9 states. Send the list for review before
   signing if anything but the three cases changes.
5. Score the file gate and both project screens.
6. fmt, Clippy, preflight, the whole Rune library test binary and
   the targeted checks in separate state. Sign. Source review, one
   complete final, Results.
