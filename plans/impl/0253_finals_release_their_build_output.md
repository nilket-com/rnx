# Implementation plan 0253: a final releases the build output it owns

Status: Proposed

Paired ADR: ../adr/0253_finals_release_their_build_output.md.

1. Pin 4e4c980a. Read tools/verify.py and its binding and host-check
   order, paired_build.py and its cleanup journal, miri_exact.py and the
   preparation cache, child_result.run, and accept.py.
2. Implement the ownership record, the ordered removal for the three
   producers in scope, and the duration field in the common stage runner.
   Decide the baseline Clippy exports by the rule in the ADR.
3. Add the controls of the ADR.
4. Run the real cross and Miri checks, the required stages, preflight and
   the commit checker. Bring the packet with the disk use of the final's
   own directories before and after, the per-stage durations, the selected
   stages and the list of what is kept and why. The candidate is frozen
   once the packet is sent.

Results are added here by the implementer.
