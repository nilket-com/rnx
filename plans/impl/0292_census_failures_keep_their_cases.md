# Implementation plan 0292: Census failures keep their cases

Status: Implemented

Paired ADR: ../adr/0292_census_failures_keep_their_cases.md.

1. Pin 4714ae19. In the census comparison, a bounded error naming the
   implicated cases and reasons.
2. In `tools/verify.py`'s receipt: the error, and `diagnostic_files` for
   the summary and the implicated cases' outputs within the envelope's
   limits.
3. The controls of the ADR, at the verifier and through the final's
   failure retention.
4. The affected checks, preflight and the commit checker; sign; source
   review; one complete Rust final; Results from its receipts.

## Results

The census comparison's error is `census_execution::headline`: one line,
"unreviewed census execution differences (N cases): ", the first four
cases as "id: reason" with whitespace collapsed, and "(+K more)", each
reason within 512 bytes and the line within 4096 UTF-8 bytes, dropping
named cases before the total or the marker. Its control,
`headline_self_check`, runs in the census self-check before any live
case, since the verifier's own tests run only there.

`tools/verify.py` builds its published receipt in `terminal_receipt`. On a
failure it reads at most 1 MiB of `census/live-execution/RESULT.json`
and, when that names differences, adds as `error` the headline built from
the summary alone with the verifier's bounds in UTF-8 bytes, beside the
unchanged `launcher_error`; lists as `diagnostic_files` the summary, then each
known case's stock and fresh observation report and stderr, then stdout,
up to sixteen names; and names in `causes` the evidence a run did not
write. Case identifiers must appear in the census plan; every path
component must be a plain, unlinked entry. Any error in this assembly
leaves the original failure's receipt as it was; a pass is unchanged.

The final's retention labels each declared file by its relative name in
`source_name`, which `copy_bundle` carries into the final's bundle,
refuses a name that is not a plain relative path, and skips one unusable
file with a note instead of losing the rest. `Bundle::add_labelled`
reads and digests its source before it writes an excerpt, and removes a
partly written excerpt it created, never a file it did not create, so a
skipped file leaves no file and no change to the budget. A summary row
that is not an object with a known case id and a string reason makes
the summary malformed, and the original receipt stands. The
envelope is unchanged: eight files with the receipt, 64 KiB each, 224 KiB
of contents.

`controller.rs::a_census_failure_keeps_its_cases_through_the_final`
runs the whole path through the controller: the real `terminal_receipt`
over a payload planted in the stage's owned root, the child bundle as
`child::run` builds it, the controller's handoff to `copy_bundle`, and
the owned root's release. Both cases and reasons, the first case's
labelled stock and fresh evidence (redacted) and the unavailable fresh
run survive; with oversized stderr, excerpts are cut at the file cap, the
rest dropped as "budget exhausted", within the aggregate cap, and no
excerpt lacks its index row. `tests/census_retention.rs` drives the real
`terminal_receipt`, the final's `failure::retain` and the payload's
removal directly, an unreadable file skipped between two kept ones, and
the headline's byte bound for a multibyte reason and nine cases with
stderr absent or 5 MiB: the retained index names the case and reason, the summary and
the first case's stock and fresh report and stderr are kept under their
labels with a planted secret redacted, and the second case's missing
fresh output is named unavailable. A failure without a census summary,
a malformed summary and a pass keep their receipts. Reverting the labels
or the receipt assembly fails it. Standalone, the changed verifier ran
the census for real: self-check PASS, 71 cases executed, no differences,
receipt unchanged.

The first complete Rust final on this record failed at 1.8 minutes in
controller-controls: test_owned_build checks verify.py's ordering by its
text, and terminal_receipt had been defined above main, so the cleanup
assignment preceded the controller-result rename in the source though
not in the run. Defining it after main restored the order; the second
final on cedc5687 passed in 43.6 minutes, with 1767 memory.high events
and no memory.max or OOM events. Cross passed in it, so no census
failure was retained; the retention is evidenced by the connected and
focused controls.
