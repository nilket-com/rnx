# Implementation plan 0283: Python tools become Rust xtasks

Status: Proposed

Paired ADR: ../adr/0283_python_tools_become_rust_xtasks.md.

1. Pin 515e5211. Read the final's controller (`tools/column/final.py`,
   `final_evidence.py`, `stage_scheduler.py`, `gate_policy.py`,
   `accept.py`, `required_stages.py`, `package_gates.py`,
   `stage_outputs.py`, `host_budget.py`, `failure_detail.py`,
   `child_result.py`) and its tests, `rune/tools/xtask`'s
   `scheduler.rs`, and the receipts of a passed complete final.
2. Before production edits, bring two boundaries for review: the budget
   library's interface and child protocol (decision 4), and the typed
   receipts with their serialization against the Python receipts
   (decision 2).
3. The budget library: move `rune/tools/xtask`'s budget into it, leaving
   that command's behavior and tests unchanged; add the named-fifo
   protocol, `RNX_FINAL_STAGE_TOKENS`, whole-width acquisition and the
   memory policy. Process controls with the unchanged Python client and
   Cargo, including a one-token budget.
4. The command: a workspace member at the rnx root with a Cargo alias,
   only crates already in `Cargo.lock`, the bootstrap of decision 5 and
   the guardian of decision 6.
   Port selection and the stage list, then the scheduler and admission,
   then supervision, failure and signal handling, release and the wall
   budget, then revalidation and the docs successor. Each step ports the
   Python controls it covers, with the planted negatives, and adds their
   rows to the committed pairing table.
5. Cross-controller revalidation controls on fixture finals (passed,
   failed, tampered, incomplete release) in both directions.
6. The Python list from 515e5211's tracked `.py` paths, with
   dispositions; its Rust check and planted negatives. Register
   `final-controls` and `python-list` in both controllers' stage lists.
7. The parity comparison and its planted mismatches.
8. Sign the candidate. Run its complete final under the Python
   controller, then under the Rust one, never together; compare them,
   and revalidate each with the other controller. Bring each stage's
   duration beside its duration in both finals and in ADR 0282's. Then
   preflight and the commit checker. No exploratory complete final
   before this: focused controls find fixture, serialization and
   protocol mismatches first. Results state what the code does and carry
   no paths under target/ and no development history.
