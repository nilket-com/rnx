# Implementation plan 0291: CLI and Jupyter controls run in Rust

Status: Proposed

Paired ADR: ../adr/0291_cli_and_jupyter_controls_run_in_rust.md.

1. Pin 427d0e32 and generate the witnesses from the published Python
   before any change.
2. The `cli` stage mode: the checker and its planted controls, the eight
   cases, the receipt.
3. The `jupyter` stage mode over the Rust child runner and the shared
   `stage::build_jobs`, with its children as one typed declaration; the
   route checker's `final STAGE` form for a Rust stage reads that
   declaration, and its controls.
4. Catalogue, scheduler, producer lists and docs; retire both programs
   with tombstones.
5. The affected checks, preflight and the commit checker; sign; source
   review; one complete Rust final; Results from its receipts.
