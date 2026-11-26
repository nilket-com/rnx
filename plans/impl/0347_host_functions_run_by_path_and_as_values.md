# Implementation plan 0347: Host functions run by path and as values

Status: Proposed

Paired ADR: ../adr/0347_host_functions_run_by_path_and_as_values.md.

1. The boundary was reported and the design accepted before
   ADR 0346 was written. Rebase the work onto the published base.
   Report anything that has changed since.
2. Declare the two source transitions in the policy file, with
   the hashes of the signed candidate.
3. Path calls as Decision 1 states, with Controls 1 and 6.
4. Values as Decisions 2 to 8 state, with Controls 2 to 9.
5. Plant the refusal back. Replay the public admission corpus
   against the published base. List every changed source as
   Control 11 states. Send the list for review before signing if
   any source changes its exit, output or effects.
6. Score the file gate and both project screens.
7. fmt, Clippy, preflight and the targeted checks in separate
   state. Then the whole rune-suite stage, the root-tests stage
   and the final-controls stage by their stage commands, as
   Control 14 states. Sign. Source review, one complete final,
   Results.
