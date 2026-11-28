# Implementation plan 0355: Dormant layout drift is accepted

Status: Proposed

Paired ADR: ../adr/0355_dormant_layout_drift_is_accepted.md.

1. Boundary. Report, before any change: where the per-record
   verdict is computed and written, what the receipt holds, how
   revalidation and the docs successor replay it, and which
   witness form of Decision 4 the representation-only case uses,
   with the reason.
2. The verdict and the witness of Decisions 2 to 5, with
   Controls 1 to 4.
3. fmt, Clippy, preflight and the stages of Control 5 by their
   stage commands. Commit unsigned. Source review, one complete
   final, Results.

## Results

Scope: the gate tools and their controls only. There are no engine
source changes, and the source guard's 40 items are unchanged. Of the
two witness forms in D4, this record implements only the
representation-only form.

Changes:
- Reviewed proof and signed plan pin
  (`tools/final/src/fixed_route_drift.rs`). A record that carries a
  dormant piece commits one regular file,
  `tools/column/dormant/NNNN.json`. The file names the measured baseline
  source commit, and also the publication commit when that differs. It
  lists the exact `--no-renames -z` delta as rows of path, old and new
  mode, and old and new blob; only `plans/` and the record's own proof
  are excluded. It also lists the context blobs (crate roots, parent
  modules, and files whose `use` items are in scope), the claim kind,
  the added variants and the added arms. The proof does not name its own
  candidate commit.
- The candidate's single parent plan commit is signed. Its plan document
  pins the proof's path (`Dormant proof:`) and the SHA-256 of its exact
  bytes (`Dormant proof sha256:`). The pin records the independent
  review of the proof.
- Issuing `accepted_layout_drift`, and every replay, verify all of the
  following, and any miss leaves the ordinary verdict:
  - the signatures on the impl commit and its parent plan;
  - the pin and the exact proof bytes;
  - set equality between the proof's rows and the real delta, refusing
    duplicate paths;
  - the context blobs;
  - baseline succession through `controller::docs_change`;
  - the measured-artifact and cell bindings;
  - the structural checker as a veto.
- The witness records the plan commit, the pin document, the proof path
  and the proof's sha256. Nothing is inherited by a later record.
- Structural checker (`ci/execute_guard/src/dormant.rs` and `main.rs`).
  It is kept only as a veto, and it is not a proof. Two of Codex's
  review fixtures pass the checker on its own: a `#[path]` module paired
  with a `#[cfg(test)] mod` of the same name in an unrelated binary, and
  a derive alias (`use ... as Clone`). Both are refused only because no
  proof matches them.
- Wiring (`fixed_route.rs`, `fixed_route_stage.rs`, `types.rs`,
  `controller.rs` and `evidence.rs`). The raw `per_record` and
  `cumulative` rows are kept unchanged. `effective_verdict` and
  `fixed_route_sha256` are written into the top-level `RESULT.json` only
  when a final passes through `accepted_layout_drift`; ordinary results
  keep their pre-0355 keys. `SUCCESSOR.json` carries the same verdict as
  its prior final, and a difference in either direction is refused.
  Revalidation rescores the rows from `ANCHOR-EVIDENCE.json` and builds
  the checker from the candidate's tree.

Controls, in `tools/final/tests/dormant_drift.rs` and the
`ci/execute_guard` unit tests, all PASS:
- Control 1. The historical Bytes payload 56db49ec is unsigned and has
  no proof, so it is ineligible under the live rule. Its arm uses
  compact synthetic captures that reproduce the recorded scores: 30 of
  32 per-record, 32 of 32 cumulative, and Sieve native-off +475,411.
  These captures are not a fresh measurement and not a proof for
  56db49ec. A small signed fixture repository with a test key exercises
  the full accepting path: FAIL without a witness,
  `accepted_layout_drift` with one. The control also accepts a
  same-parent docs-only baseline amend and refuses a child commit.
- Control 2. The checker veto still refuses the following even when a
  signed, pinned proof is present: a planted tag query, a same-named
  enum in another module, a nested `#[cfg(test)]`, an expanding
  `cfg_attr`, block-local shadowing, two crate roots sharing a file, a
  qualified custom derive, and a production file moved under `plans/`.
- Control 3. An unsupported witness kind, a stale candidate, a
  cumulative failure, and a mixed execution and dispatch or setup
  failure on one row each stay FAIL.
- Control 4. Replay rescores the rows and refuses each of these: a
  forged witness, a tampered checker or proof digest, a dropped row, an
  edited count, a missing `fixed-route/RESULT.json`, and a successor
  whose verdict differs from its prior final. The positive replay
  supplies a ready-built checker binary. The public path that builds the
  checker from the candidate tree was reviewed in source; this control
  does not exercise it.
- Control 5. Each of these is refused: a missing pin, a wrong pin
  digest, a proof edited only in the candidate, an unsigned impl or
  plan, a changed path missing from the proof, an extra path, a mode
  mismatch, a duplicate path, a wrong context blob, a stale baseline, an
  absent proof, and Codex's bin and derive fixtures.

Final: one complete final, FINAL-v2, ran on
8b9862d3fcaa735ce824a050d97b5c8040e25f69 and passed all 107 stages in
3,737.2 s. Notable stages: rune-suite took 2,022.3 s, fixed-route 268.7
s and final-controls 148.6 s. In the gate, the ordinary verdict was
PASS, with 32 of 32 per-record and 32 of 32 cumulative rows, so this
final did not exercise the drift path. RESULT.json sha256:
8313463c4b3553f8295624571056c986a8ddd40bdfc351973f9f13b124100139

An earlier version, 49ec5220, also had one complete final, which passed
in 3,790.0 s. Codex's review then found holes in that version's checker,
and the reviewed proof replaced it as the eligibility rule. That final
tested only the wiring and is not the publication evidence.
