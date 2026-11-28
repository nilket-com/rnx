# ADR 0355: Dormant layout drift is accepted

Status: Proposed

Base: published f6ce4a3f (ADR 0354).

## Problem

1. Bytes must run on the new engine before the old engine can be
   deleted. Thirteen byte-string cases and the `into_bytes`,
   `as_bytes` and `with_capacity` cases still run on the old
   core. ADRs 0301 to 0303 were withdrawn because code that
   never ran changed the compiled interpreter and failed the
   fixed-route bands. Bytes was parked on 2026-11-18.
2. On 2026-11-28 a scratch candidate on f6ce4a3f added only the
   Bytes type and its storage, with no way to construct a Bytes
   value. Per-record scoring passed 30 of 32 rows and cumulative
   scoring 32 of 32. Native-off Sieve rose 475,411 instructions
   at size 5 (0.667%, against the 5 per mille band) in both W
   and T. It executed the same 858,825 bytecodes as the base.
   The rise splits into `Store::tag` +875,000, the dispatcher
   -431,499, `Store::push` +31,890 and `Store::admit` +20.
   `Store::push` already asked whether its receiver is a String;
   the candidate makes that query as an out-of-line call 25,000
   times. Other native-off rows moved both ways: `loops` fell
   250,054 (0.77%), all of it in the dispatcher's own
   instructions, with the same bytecode histogram.
3. The user ruled on 2026-11-28 that layout drift from code that
   does not run is accepted, and that a piece which adds
   executed work must pass both bands.
4. The gate cannot tell these apart. A list of function names is
   the wrong unit: an old function can run an unchanged arm, a
   new arm can share a block, inlining merges functions, and a
   missing symbol is not a count of zero.

## Decision

1. The ordinary per-record and cumulative scores are computed
   and kept unchanged. Every existing check (capture, fixture
   output, route, source transition, setup, loader and installed
   dispatch) and every cumulative row must pass.
2. A red per-record execution result, level or growth, can
   receive the separate verdict "accepted layout drift" only if
   the record carries a verified dormant-feature witness for its
   whole production source delta, across every scored fixture,
   mode and both sizes. W and T rows share the same cell
   evidence. Missing or ambiguous evidence leaves the ordinary
   verdict.
3. Dormant means that no new feature operation runs and no new
   source-level check runs on an old path, anywhere in the
   matrix. Machine calls added to carry out a source operation
   that was already there are the layout cost the user accepted.
   A comparison added to an old path is new work even if its
   taken edge never fires.
4. The witness is a reviewed manifest of the record's whole
   production source delta against its exact parent, including
   changes inside existing functions. It is bound, with its
   evidence, to the matched runner and engine image, fixture,
   mode, size and capture window.
   - A representation-only delta is proved by structure: only
     new discriminants, arms selected only by them, no change to
     an old arm, no other executable work, and no production
     constructor or ingress. The source checker validates this
     shape, or verifies an independently reviewed,
     content-addressed proof of the exact delta. A declaration
     or a text search is not a proof.
   - A delta that can construct the feature is proved by
     measured zero entries on its paths: edge counts over every
     lowered and inlined copy, or a dominating trigger with
     proven complete coverage. A positive control exercises the
     detector. A shared block that is ambiguous needs an edge or
     structural proof.
5. The receipt keeps the raw failures, the attribution and the
   effective verdict. Revalidation and the docs successor replay
   the decision.
6. Never: re-anchoring, subtracting instructions, flipping raw
   failures, averaging rows, or eligibility inherited by a later
   piece. The verdict is scoped to what the controls prove;
   there is no general layout switch.
7. Scope: the gate tools and their controls. No engine source
   changes, and the source guard's 40 items stay unchanged.

## Controls

1. The retained payload pair from Problem 2 is a regression
   fixture: raw 30 of 32 per-record, 32 of 32 cumulative, Sieve
   native-off W and T +475,411. It is accepted with its verified
   proof and refused without it. Committed fixtures stay small;
   raw captures are not committed.
2. A planted extra tag query on the old Vec path is refused,
   even when the Bytes arms have zero entries.
3. An executed feature operation, an incomplete or stale
   witness, and a cumulative or setup failure each leave a red
   row red.
4. Revalidation replays an accepted-drift verdict, and a
   tampered witness fails it.
5. Stages. final-controls and every catalogue stage whose
   command reads a changed file pass by their stage commands.
   Then one complete final.
