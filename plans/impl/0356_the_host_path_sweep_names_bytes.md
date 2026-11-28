# Implementation plan 0356: The host-path sweep names Bytes

Status: Proposed

Paired ADR: ../adr/0356_the_host_path_sweep_names_bytes.md.

1. The arm of Decision 1. Run cross and root-tests by their
   stage commands on this record and with the payload applied on
   top in scratch, plus fmt, Clippy and preflight. Commit
   unsigned.
2. Review, signing, one complete final, Results.

## Results

Scope: one line in `src/verification/host_paths/inputs.rs`, a module
compiled only under `cfg(all(test, feature = "cross-verification"))`.
The witness table gains `"Bytes" => "b\"probe\""`. There are no engine,
gate or checker changes.

Controls:
- Control 1. The cross stage passed on this record in the final below.
  On this base the new arm is dormant: `kinds()` iterates the `TypeName`
  variants that exist, and Bytes is not yet one of them.
- Control 2. Codex applied the payload on top in scratch (tree
  cefb3b2ee6d3fe832624f239594e5a89d171c4d0) and ran cross by its stage
  command. It passed in 293.7 s, with 50 kinds in the matrix, Bytes the
  only one added. All 2,752 additional substitutions were refused
  (105,270 to 108,022), compared rows stayed at 29,596, and none were
  unexpected. Root-tests passed on the same tree: 747 passed and 11
  ignored. Codex confirmed that every non-plan blob and mode of that
  tree matches this record plus the archived payload delta.
- On the same tree the unchanged structural checker refuses the witness,
  because it recognizes only `cfg(test)` as test-only. Landing the
  witness first as its own record keeps the checker unchanged.

Final: one complete final, FINAL-v1, ran on
1551d2c4676e90712daaca5f16c419eee7b4b9d5 and passed all 107 stages in
3,796.7 s. Notable stages: rune-suite took 2,050.0 s, cross 496.0 s,
fixed-route 270.0 s, root-tests 182.0 s and final-controls 100.5 s. The
gate's ordinary verdict was PASS, with 32 of 32 per-record and 32 of 32
cumulative rows. RESULT.json sha256:
143e4b0b2040032fc19420c361e5e43b2307b80a67e26a72d7712fdd044b1580
