# Implementation plan 0344: A string is built on the new engine

Status: Proposed

Paired ADR: ../adr/0344_a_string_is_built_on_the_new_engine.md.

1. Boundary: the published base. Reproduce the observations of the
   ADR's Problem on both engines in both native modes. List every
   constructor and in-place method of a string with its route
   today. Find how `push` is admitted and whether the same path
   serves the others. Measure the old core's outcome for the forms
   of Controls 1 to 6. Report before going on if a method needs
   more than Decision 7 allows.
2. Constructors and methods as Decisions 1 to 3 state, with
   Controls 1 to 3, 5 and 7.
3. Capacity and the store's growth as Decision 4 states, with
   Controls 4 and 6.
4. Plant each admitted name out, one at a time. Replay the public
   admission corpus against the published base. List every changed
   source as Control 9 states. Send the list for review before
   signing if anything but the expected cases changes.
5. Score the file gate and both project screens.
6. fmt, Clippy, preflight, the whole Rune library test binary and
   the targeted checks in separate state. Sign. Source review, one
   complete final, Results.
