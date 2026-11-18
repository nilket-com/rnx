# ADR 0295: Running jobs and final progress at a glance

Status: Proposed

Base: published 37b05b24ca0643fb1d1fdc266bc296261110456c (ADR 0294).

## Problem

The operator's view of long jobs, `rnx-jobs` with its helper
`final-progress`, exists only outside the repository, as bash and `jq`
with one host's path built in. It parses systemd's rendered `ExecStart`
as space-separated text, which loses quoted paths and prints a prefix of
a job's arguments; it splits its list of earlier finals on whitespace;
and it finds finals by one run-naming pattern. Nothing keeps it in step
with the receipts it reads.

## Decision

1. `rnx-final jobs` renders one snapshot, as `rnx-jobs` does today:
   - every running `rnx-*` user service with its elapsed time and
     memory, read from the user manager once per view; an empty list is
     told apart from an unavailable user manager;
   - for a service that can be tied to a final, its completed and running
     stages and a time estimate labeled with the earlier passing run it
     uses. Its output directory comes from the service's main process
     argv (exact boundaries, `--out DIR` or `--out=DIR`), relative to its
     working directory; a service that cannot be tied to a final is shown
     without final details, and job arguments are never printed;
   - with no active service, the latest final's terminal verdict and
     receipt path, or that it has no terminal receipt.
   `-w` refreshes the view every 30 seconds for a person at a terminal;
   `--final DIR` shows one final directory.
2. Finals are found by their receipts (a directory holding
   `SELECTION.json`), under `RNX_FINALS`, by default the checkout's
   `target/`, never by a run-name pattern. The reference for an estimate
   is the most recent complete PASS other than the final shown; a missing
   reference is said plainly.
3. It reuses the controller's receipt and schedule types and reads only.
   It changes no final, schedule, receipt format, evidence collection or
   job lifetime. Agents still wait on exit and read terminal receipts;
   `-w` is for people.
4. `tools/final/rnx-jobs` is a short alias that runs an already-built
   `rnx-final jobs`. A status invocation never builds the controller.

## Constraints

- No new crate, no Python.

## Controls

Fixture data through the view's seams, with recorded snapshots of the
display: active and idle services, an unavailable user manager, paths
with spaces, early termination and incomplete receipts, unrelated files,
and whether a last PASS can serve as the reference. No live progress
watcher is part of review. The record's final.
