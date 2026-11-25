# Implementation plan 0340: A range pattern matches a range value

Status: Proposed

Paired ADR: ../adr/0340_a_range_pattern_matches_a_range_value.md.

1. Boundary: the published base. Reproduce the observations of the
   ADR's Problem on both engines in both native modes. List the
   sites where the language takes a pattern and the lowering each
   one uses. Measure the old core's outcome for the planted forms
   of Controls 2 to 7. Record each outcome and the route taken.
2. Lower the range pattern as Decisions 1 and 2 state, and accept
   the range expression as a root as Decision 3 states. Add the hit
   and miss witnesses for the six types.
3. Add the field-pattern, site and heap-endpoint controls. Report
   before going on if a site or a proof needs a change beyond
   Decision 8.
4. Add the remaining controls: the receiver of ADR 0339, the four
   compile-time rejections and the two planted changes.
5. Replay the public admission corpus against the published base.
   List every changed source as Control 9 states. Send the list for
   review before signing if anything but the six cases changes.
6. Score the file gate and both project screens.
7. fmt, Clippy, preflight, the whole Rune library test binary and
   the targeted checks in separate state. Sign. Source review, one
   complete final, Results.
