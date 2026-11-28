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
