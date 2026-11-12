# Implementation plan 0255: executed witnesses for standard input and the file system

Status: Implemented

Paired ADR: ../adr/0255_executed_witnesses_for_stdin_and_fs.md.

1. Pin 5ed9692b. Read how the host rows get their class and input in
   src/session/parity.rs and src/session/host_enrollment.rs, the existing
   child witnesses, and the thirteen functions.
2. Before editing, send a note: the witness form and the child setup, the
   path marker rule, the calls chosen for each function with their
   expected legacy outcomes, and the outcomes of a first comparison of the
   thirteen. If any comparison shows a wrong result in the execution
   engine, send that and stop.
3. Add the witnesses, class the rows by their comparison, and regenerate
   the pinned table.
4. Add the controls of the ADR.
5. Run the required stages, preflight and the commit checker. Bring the
   packet with the class of each of the thirteen rows, the totals before
   and after, the selected stages and the per-stage durations. The
   candidate is frozen once the packet is sent.

## Implementation and focused evidence

Thirteen isolated witnesses compare one call per input, recording the result
and a sorted directory snapshot after each call. Each engine has its own
child, private cwd and temp directory, controlled environment and piped
stdin. The closed fixture argument list rejects parent/root paths and
symlinks before invocation; a call ID cannot be replayed.

Legacy expectations independently check returned values, exact errors and
successful effects. Failed calls leave state unchanged. HOME, TMPDIR and
cwd are checked against their configured private locations before the
private-root marker is applied. The marker affects value/error path prefixes
only; file bytes and unrelated strings are unchanged.

Standard input covers supplied bytes, the second-read refusal, and empty
input in a separate child. A wrong supplied payload fails the legacy
expectation. Other controls reject outside paths and duplicate call IDs,
check marker boundaries, and classify equal returns with different state as
Different result. Call IDs prevent witness replay; no claim is made about
internal syscall counts for idempotent operations.

All thirteen measured rows are Equal. Equal rises from 153 to 166;
Admitted, not executed falls from 16 to 3. Only the clock rows remain in
that class. No other row changes class, and all non-host sections and work
groups remain unchanged. The generator retains non-Equal comparisons.

The pinned error messages include this Linux platform's exact OS wording
and errno text. They are neither stripped nor generalized. Full report
bytes match across the original capture, a second run, and a run from a
different working directory. No private machine path enters the table.

Focused controls and determinism checks pass. Required final stages and
per-stage durations are supplied in the publication packet.

The committed pin uses one compact classed row per line within the existing
160 KiB reviewed cap. A shared generator serializer writes both reports and
the pin; the classification test checks parsed equality and exact serialized
bytes. A control proves one reclassification changes only its row line and
the summary line. All observation data is unchanged; no size exemption or
policy change was needed.
