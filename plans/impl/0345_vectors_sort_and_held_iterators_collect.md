# Implementation plan 0345: Vectors sort and held iterators collect

Status: Proposed

Paired ADR: ../adr/0345_vectors_sort_and_held_iterators_collect.md.

1. Boundary: the published base. Reproduce the observations of the
   ADR's Problem on both engines in both native modes. Find why a
   held iterator runs `next` and refuses the rest, and why `sort_by`
   runs and `sort` doesn't. List the methods refused on a held
   iterator. Measure the old core's outcome for the forms of
   Controls 1 to 5. Report before going on if a part needs more
   than Decision 8 allows. That part then leaves the record as
   Decision 6 states.
2. `sort` as Decisions 1 and 2 state, with Controls 1 and 2.
3. Held iterators as Decisions 3 to 5 state, with Controls 3 to 6.
4. Plant each refusal back, one at a time. Replay the public
   admission corpus against the published base. List every changed
   source as Control 8 states. Send the list for review before
   signing if any source changes its exit, output or effects.
5. Score the file gate and both project screens.
6. fmt, Clippy, preflight and the targeted checks in separate
   state. Then the whole rune-suite stage and the root-tests stage
   by their stage commands, as Control 11 states. Sign. Source
   review, one complete final, Results.
