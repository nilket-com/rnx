# ADR 0323: The final owns its build caches

Status: Proposed

Base: published 555cdb1a387cbfc937d192ba2afb503720fe73a9 (ADR 0322).

## Problem

The complete final builds in Cargo target directories that it shares
with whatever else ran in the checkout, and it never removes anything.
Every candidate leaves its linked test executables behind:
- After the 0322 final, `adapters/polars/target` held 106.49 GB. Of the
  95.17 GB in `debug/deps`, 87.98 GB were extensionless files and 6.20
  GB were rlibs.
- 57 superseded Polars test executables with the same configuration
  (40.66 GB) and 20 for Candle (9.13 GB) are still present.
- Incremental sessions regrew to 8.50 and 1.23 GB after they were
  cleared. `target/column-clippy-adapter-published/debug/incremental`
  had held 50 GB across 3,599 sessions.

Disk admission needs about 110 GB free at commit-discipline. Two of the
six 0322 launches were refused, and space was freed by hand both times.
A final that needs a person to delete files before it runs is not done.

## Decision

1. Owned lanes. Every reusable Cargo writer the final runs, including
   the outer invocation of a nested control, writes to a target lane
   that the controller creates and locks. Builds whose check requires a
   fresh build stay invocation-owned and transient, with their current
   command, environment, evidence and cleanup:
   - fixed-route's paired builds;
   - `cross`;
   - `contracts`;
   - the product builds in `generated-projects`.

   Consumers such as the production CLI and Jupyter bind their binaries
   from the lane that produced them. A lane is a sharing group: the
   writers that share a target directory today (one workspace, target
   and profile family) share one lane. Cargo already keeps their
   configurations apart by unit hash and builds each common dependency
   once. Splitting each configuration into its own lane would build and
   keep those dependencies once per lane.

   Warm dependencies carry over between candidates in the same lane.
   Directories the final did not create are never adopted.
2. Marks. Each producing recipe runs Cargo with `--message-format=json`
   and records every output file named in its `compiler-artifact`
   messages, fresh or rebuilt, with the recipe's resolved configuration.
   - Marks are committed per recipe. A lane keeps the union of marks
     over the recipes still live in the candidate's full catalogue.
   - Missing, empty or truncated producer output is not a negative mark.
   - A required-only selection does not retire stages or configurations
     that it left out.
   - The existing test-name, count and diagnostic parsers keep reading
     the same output.
3. Sweep. Once a lane's users drain, the sweep deletes, under the lane
   lock, the output files that the lane recorded earlier and that no
   live mark keeps. Superseded test executables and the artifacts of a
   retired configuration are both covered by this rule. It never scans
   for executable bits or filenames and never edits fingerprints. A
   sidecar or incremental session goes only when it is tied exactly to
   a swept unit with no surviving consumer. Anything unproven stays.
   Cargo rebuilds a missing output, so a wrong sweep costs time, never
   correctness.
   - A failed stage or run sweeps nothing.
   - The first clean lane establishes provenance.
4. Retirement. A lane that no recipe in the candidate's full catalogue
   uses any more is removed as a whole before disk admission.
5. Admission. Reclamation (the prior committed garbage and retired
   lanes) runs before disk admission. The remaining warm or cold state
   then sizes the need. The 20 GiB reserve and the stage selection are
   unchanged. The shell bootstrap keeps its pre-build floor and builds
   the controller in its own owned and locked cache. Reclamation runs
   in the controller, so it starts after the bootstrap.
6. Incremental. Decided per lane by an A/B with matched jobs and
   features: a cold build, an unchanged repeat, an adapter edit and an
   upstream Rune edit. Record wall time, retained allocated bytes and
   peak growth. Incremental stays on only where a changed build saves
   enough to pay for its bytes.
7. Record scratch. `target/0072` and `target/0073` are live writers for
   `polars-inventories` and `polars-suite`. Generator scratch lives for
   its stage, and inventory assets live until their last consumer
   finishes. Reusable compiler output moves to an owned lane
   only if its timing justifies keeping it. Nothing is ever deleted by
   record number.
8. Migration. The existing checkout targets are unowned. After the
   implementation is signed, Claude and Codex agree on a one-time list
   of paths to delete before the first final under this record. The
   final never deletes them itself.

## Controls

1. Churn. On small fixture crates: two candidates, an alternate feature
   set, a retired configuration, two writers sharing a lane, a planted
   partial run and a failed run. Then check that:
   - a dependency common to two configurations in a lane is built once;
   - the retained marked outputs equal the union of the live marks;
   - a warm repeat recompiles nothing;
   - foreign and leased paths survive;
   - a failed or partial run sweeps nothing.
2. Admission. After reclamation, admission uses the reclaimed free
   space. A retired configuration's outputs are swept without a rebuild
   of the live configurations. A lane that is still live but left out
   of a required-only selection is kept.
3. Footprint. From the A/B builds on the real adapter lanes, report the
   allocated bytes per lane after a cold build, after an unchanged
   repeat, and after a changed build and sweep. Also report
   non-executable bytes that survive the churn. These are reported, not
   budgeted.
4. Incremental. The A/B in Decision 6, reported per lane.
5. Finals. Targeted pre-sign checks, then one complete final on the
   signed candidate after the agreed migration. That final starts cold.
   Results report the per-lane bytes after it. The retained test
   outputs must equal its marks. The next record's final is the
   first warm run, and it is reported against 60 minutes.

## Constraints

- No new third-party crate. Tabs.
- The fixed-route environment and its bands are unchanged.
- Receipts and evidence directories are kept.
- No engine, adapter or product behavior change.
