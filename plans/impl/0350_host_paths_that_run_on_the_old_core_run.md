# Implementation plan 0350: Host paths that run on the old core run

Status: Proposed

Paired ADR: ../adr/0350_host_paths_that_run_on_the_old_core_run.md.

1. The boundary and the design were reported and ruled on
   before this plan: the sweep, the 425 combinations, their
   three causes and the split of the third.
2. The fixes of Decisions 1 and 2, with Controls 1, 3 and 4.
3. The control of Decision 3, with Control 2.
4. Replay the public admission corpus against the published
   base. Send the list for review before signing if any source
   changes its exit, output or effects.
5. Score the file gate and both project screens. If a row is
   outside its band, stop and send the counts.
6. fmt, Clippy, preflight and the targeted checks in separate
   state. Then the stages of Control 8 by their stage commands.
   Sign. Source review, one complete final, Results.

## Results

The two qualified Vec paths moved from execution to a compile-time
`Native` refusal. Stock runs the call once. Removing index 1 from
`[10, 20]` returned 20 and left `[10]`. Inserting 9 at index 0
returned unit and left `[9, 10, 20]`. Taking the unsupported binding
as a value also moved to stock. These were the only two changed
routes among the installed paths. Execution now returns the host's
ordinary error for a rejected final argument of `fs::write`,
`fs::write_new`, `fs::append`, `web::text`, `web::html` and
`web::parse_form`. The file control observed both the error and the
absence of the destination in both native modes.

On published `66c8f732`, 461 of the 1,873 named installed functions
had admitted discovery callers, with 648 argument positions. The
original survey compared 25,278 admitted combinations over 41 inputs.
Stock succeeded where execution failed in 425. Both native modes now
complete all 45 Vec cases through fallback and all 42 string-parameter
cases on execution. The empty-iterator boundary adds one repaired
Vec insertion and 84 deferred reductions. The candidate has 459
admitted discovery callers with 643 positions. All 6,404 public corpus
comparisons kept their route and observable outcome. Each mode still
has 67 legacy-only cases.

The permanent in-process control ran in 23.21 seconds in the actual
`cross` stage. It verified the installed named and associated handler
identities, generated substitutions over 2,752 positions, and checked
each source for admission, including substitutions on paths whose first
witness was refused. Its 43 inputs include the empty slice iterator and
a future-valued expression. It compared 25,738 admitted sources in both
native modes, including 12 zero-argument calls, and counted 92,616
refusals. The control found all 422 deferred comparisons and no others.
The two fixture tables bind each exception to its handler, concrete
path, argument position and input kind. A fixed exception fails the
control until its entry is removed.

The original 338 deferred cells span 62 paths. The 84 empty-iterator
cells span 84 paths, of which 56 were already represented. The 28
additional paths are `sum::<f64>` and `product::<f64>` for each of
the following 14 iterator owners.

- Object iter, Option iter and string chars.
- String split by char and by string.
- Half-open, inclusive and open range iterators, each over i64, u64
  and char.

In the ADR 0347 survey the owner-hash labels for string-Split and
function-Split were swapped. Correcting the labels affected 24 of the
original receiver cells. Their source spelling and observed outcomes
were the same. The two lists belong to the receiver record.

The file gate passed 32 per-record and 32 cumulative rows. The empty
and paired project images passed all 128 comparisons against the
published base. The no-detail store was `store i8 0` at offset 0 on
both builds, with size 32 and alignment 8. The source inventory declared
the `Executor::host_call` transition from `bf36f1ee` to `80a14b5a`.
Changes after the screens were confined to the cross verifier, its
fixtures, the policy's consumer size entry and this report.

Restoring the old provider selection failed the Vec routing test and
produced 46 unexpected comparisons per native mode in the cross
control. Restoring the old host conversion failed the string error and
file-effect test and produced 42 unexpected comparisons per mode.
Neither plant disturbed the deferred comparisons.

The rune-suite, root-tests, cross and final-controls stages passed
before signing, as did formatting and all four native-manifest Clippy
configurations. Preflight caught the fixed-route policy exceeding its
registered size after the source transition. Updating the consumer
entry to its measured 19,266 bytes cleared preflight.

The complete final ran on b8684f78 as FINAL-v1 and passed all 107
stages. RESULT.json has sha256
0f9793dde3f3d89f24a236727c7c2cb34664e8e2f5350294b25d84259935e9e1.
Entry through cleanup took 4,164.0 seconds against the 5,100 second
limit. The run entered with 165,874,434,048 bytes free against the
151,152,450,560 required. The longest chain was the gate, which ran from
3,561 to 4,115 seconds. The Rune suite ended at 1,944 seconds with 774.0
seconds in the contracts. The declaration changed the policy file, so no
published gate inputs matched and the gate rebuilt and measured the base
on the clock. The anchor lookup missed and the anchor build took 99.0
seconds. The source condition passed over 40 items and accepted the one
declared transition, host_call, with no other item changed. All 32
per-record rows passed and 25 were exact. Seven native-on T rows moved
in setup, each by less than 0.1 per mille against the 5 per mille band:
four rose, by at most 392 instructions, and three fell. The eight
installed native-on T rows were scored under version 2, with the loader
unchanged. All 32 cumulative rows passed. The cross stage ran the host
path control inside the final in 77.4 seconds: 25,738 admitted sources
compared in both native modes, the 422 pinned combinations matched and
none was unexpected. The fetch stayed offline and took 11.7 seconds. The
build peaked at 121,038,508,032 bytes and shared memory at 3,440,640
bytes. The cleanup cleaned 37 targets and removed the run. The memory
controller reported no events. The run left no new entry in /tmp or
/var/tmp.
