# ADR 0292: Census failures keep their cases

Status: Proposed

Base: published 4714ae19e52ea34396514bd450bd139c86f3d8e0 (ADR 0291).

## Problem

ADR 0291's first complete final failed in `cross` with "unreviewed census
execution differences", and its cause is unknown. The census comparison
runs each case on the stock and fresh engines and writes which cases
differed, and why, to `census/live-execution/RESULT.json`, with each run's
output beside it; all of it lives in the verifier's payload, which the
final releases. The retained failure bundle kept only the controller's
one-line error and the launcher's traceback, and the launcher's receipt
had no error of its own ("producer returned FAIL without an error
message"). A census failure must say which case failed and why, after
the payload is gone.

## Decision

1. The census comparison's error is one line naming its implicated cases
   with their reasons, bounded by case count and by UTF-8 bytes within
   the final's error cap; the total and an omission marker stay visible
   even when the first reason is cut. The controller's own stderr then
   says what differed.
2. The `cross` launcher's receipt states that concrete error, taken from
   the census summary or the controller's cause, beside its existing
   `launcher_error`, and lists as `diagnostic_files` the summary and the
   implicated cases' evidence. Case identifiers are checked as single,
   known path components before any path is made from them; only those
   explicit files are listed, never links, and a missing, malformed or
   unreadable summary leaves the original failure and receipt as they
   were. A passing run's receipt is unchanged.
3. The existing envelope decides what survives: at most eight files, the
   receipt included, each at most 64 KiB, 224 KiB of contents. The list
   gives the summary first, then the first case's stock and fresh
   observation reports and stderr, then stdout. Each retained file is
   labelled by its relative diagnostic name (case, engine and stream or
   report), so it stays identifiable once copied into the final's bundle
   and the payload is gone. Evidence a run never wrote is named as
   unavailable in the receipt's causes, distinct from evidence the caps
   left out. The caps do not change.
4. Nothing about what `cross` checks, admits, or compares changes.

## Constraints

- The existing bounded diagnostic envelope only: no new retention path,
  no larger caps, no bulk payload kept.
- No new crate, no new Python file.

## Controls

- The headline: one difference, an invocation failure, more cases than
  the bound, and an oversized first reason, each bounded with its total.
- One connected control: the real launcher's receipt assembly over a
  planted census payload, then the final's ordinary failure retention and
  release, after which the bundle names the case and reason and holds its
  labelled stock and fresh evidence, a planted secret is redacted, and
  the payload is gone.
- A passing run, and a failure with no census summary, keep their
  existing receipts.
