# Implementation plan 0295: Running jobs and final progress at a glance

Status: Implemented

Paired ADR: ../adr/0295_running_jobs_and_final_progress.md.

1. `tools/final/src/jobs.rs`: the user-manager and process seam, final
   discovery, the final view and the snapshot renderer; `rnx-final jobs
   [-w] [--final DIR]`.
2. `tools/final/rnx-jobs`, the alias, and a README line.
3. Fixture tests with recorded snapshots.
4. The Rust suites, preflight and the commit checker; sign; review; the
   final; Results.

## Results

`rnx-final jobs` renders one snapshot of the running `rnx-*` user
services: elapsed time, memory, and, for a service whose main process
argv names an unambiguous `--out`, its final's stages and a time estimate
labeled with the last complete PASS it compares against. It reads the
user manager with one listing and one `show` per view, and tells an
unavailable manager from an empty list. Argv boundaries come exactly from
`/proc/PID/cmdline`: an empty, missing or conflicting `--out` names no
final, and job arguments are never printed. Finals are found by their
`SELECTION.json` under `RNX_FINALS`, by default the checkout's `target/`,
one or two levels down, without listing Cargo build directories. Receipts
are read only as regular files, never through a link or from a pipe. Only
a final tied to an active service has running stages and an estimate;
otherwise a begun stage is unfinished and the final has no terminal
receipt. With nothing running, the latest final's verdict and receipt are
shown. `-w` redraws every 30 seconds for a person; `--final DIR` shows one
final. `tools/final/rnx-jobs` runs an already-built controller
(`RNX_FINAL_BIN`, then `final.sh`'s `target/final-controller/release`,
`target/release`, `target/debug`) and never builds one.

Ten controls in `tools/final/tests/jobs.rs` compare exact snapshots over
planted finals: a running final at a path with spaces, no earlier
complete PASS, idle and unavailable views over a terminated final with a
leftover stage, bars at 0 and past 100 percent, Cargo build directories
skipped, and argv boundaries. The alias runs end to end from a fresh
checkout over a scripted `systemctl` (active, empty, failed), and on its
default binary and finals paths. The one-shot `--final` view ignores a
linked receipt and FIFOs without blocking. Each repair has a mutation
that fails its control; the two link guards cover each other, and
removing both fails the receipt control.

The required final passed in 2 min 18 s (`clippy-final`,
`commit-discipline`, `controller-controls`, `final-controls`,
`format-final`, `python-list`). From the final's own clone, with no
settings, `tools/final/rnx-jobs` found the final controller build and
showed that final's PASS and receipt.
