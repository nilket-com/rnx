# Implementation plan 0338: The final builds once and cleans up

Status: Proposed

Paired ADR: ../adr/0338_the_final_builds_once_and_cleans_up.md.

1. Inventory first. List every use of the checkpoint, the lane
   marks and the cache lookup in the final's tool, and say for each
   whether a verdict depends on it or only speed. Say what the
   tool's other modes do today and which are removed. Report
   anything that depends on the checkpoint for a verdict before
   editing.
2. Give the run one owned output directory, with private home, tool
   and temporary paths inside it. Group the existing sharing groups
   as target directories under it. Replace the cache lookup that the
   Jupyter stage uses and the early release of ADR 0335 with paths
   of the current run and the same release signal.
3. Move the fetch of locked sources, with the retired-source
   mappings and the Polars inventory inputs, inside the clock.
4. Keep the baseline gate inputs by commit with the checks of
   Decision 4. Run the lint baselines from source in the run's
   directory.
5. End every run with the copy-out, the drain, the clean and the
   removal of Decision 7, and write the cleanup evidence. Sample
   disk use as Decision 9 states. Add the limits record and the
   refusals of Decision 8.
6. Remove the preparation, its modes, the store's disk floor, the
   lane marks and sweeps and the cache snapshots, with the controls
   that existed only for them. Add the controls of the ADR.
7. fmt, Clippy, the tool's tests and the targeted checks in separate
   state. Sign. Source review, one complete final, Results. Report
   the elapsed time, what dominated it, the peak disk use and the
   limit that follows from Decision 8.
