# Implementation plan 0357: The Bytes payload is dormant

Status: Proposed

Paired ADR: ../adr/0357_the_bytes_payload_is_dormant.md.

1. The payload of Decisions 1 and 2 on this plan: the eleven
   paths of the reviewed 0356 boundary, unchanged in content,
   and `tools/column/dormant/0357.json`. Run the checker on the
   delta, and rune-suite, root-tests and cross by their stage
   commands, plus fmt, Clippy and preflight. Commit unsigned.
2. Review of the source and the proof. Plan amend with the pin,
   signed. Impl signed on the amended plan.
3. One complete final, Results.

## Proof

The proof was reviewed independently on 2026-11-28.

Dormant proof: tools/column/dormant/0357.json
Dormant proof sha256: b590d11f228810da92c07337cfcb99b143571ce9105a97d44c2b55fcd341e0cc

## Results

Scope: the eleven source and test paths of the reviewed boundary,
byte-identical to the scratch payload that passed the pre-sign stages,
plus `tools/column/dormant/0357.json`. The ten engine and test paths add
`Type::Bytes`, `TypeName::Bytes` and `Payload::Bytes(Vec<u8>)` with the
exhaustive arms the compiler requires; `src/routing_inventory.rs`
classifies Bytes as "builtin payload; no constructor or Root type".
Nothing produces a Bytes value.

Proof: `tools/column/dormant/0357.json`, sha256
b590d11f228810da92c07337cfcb99b143571ce9105a97d44c2b55fcd341e0cc. It
binds the baseline to ADR 0356's measured source 1551d2c4 and
publication 2543c244, and lists 11 delta rows, 26 context blobs, 3 added
variants and 10 production arms. Review checked every delta row's path,
modes and full blobs against the real `--no-renames` delta from
1551d2c4, ed6a326d and 2543c244, and every context blob against the
candidate. The signed plan 5ff6beb8 pins the proof's path and digest.

Controls:
- Control 1. The candidate-built structural checker passes, with the
  same three variants and ten arms, and the gate's precheck verified the
  proof against the delta, modes and blobs.
- Control 2. No byte-string case moves to the new engine; rune-suite
  passed with the gap-hunt dispositions unchanged. Cross passed in the
  final. In Codex's pre-sign run on the same source tree it had 50
  kinds, Bytes the only one added: all 2,752 additional substitutions
  were refused (105,270 to 108,022), compared rows stayed at 29,596, and
  none were unexpected.
- Control 3. Raw gate rows: 30 of 32 per-record and 32 of 32 cumulative.
  The two red rows are native-off Sieve, W and T. Each rose 475,411
  instructions at size 5 (0.67%) and 95,079 at size 1, failing both the
  level and growth checks; their dispatch levels passed. This is the
  same rise the unsigned scratch payload 56db49ec measured on f6ce4a3f.
  The effective verdict is `accepted_layout_drift`, covering exactly
  those two rows.

Final: one complete final, FINAL-v1, ran on
aab96985e8fdfcf7142a430e95c67f7c582cd11f and passed all 107 stages in
3,629.2 s. Notable stages: rune-suite took 1,755.9 s, cross 613.6 s,
fixed-route 241.8 s, root-tests 198.4 s and final-controls 98.5 s.
fixed-route/RESULT.json sha256
faccb29ca3a270e247007cbe3bc91528fef81c92dba08cbd147611c58280fcb8.
RESULT.json sha256:
d59c4be3345f0e42a1bdc253425dedb90692bde84cc005d08766f7c5123ac27a

Replay exception: the public `final.sh --revalidate` command failed to
build the candidate checker, because its isolated Cargo home lacked syn
2.0.119; ADR 0355's Control 4 had supplied a prebuilt checker, so this
path had not run. Revalidation passed on the same final receipt through
the installed-controller entry point, which rebuilt the checker from the
tested candidate using cached locked dependencies. Codex agreed to this
exception. The next record fixes the public command before any further
drift record.
