# Implementation plan 0348: Dispatch cost holds under unrelated edits

Status: Proposed

Paired ADR: ../adr/0348_dispatch_cost_holds_under_unrelated_edits.md.

1. The boundary was reported before this plan: the four cells of
   the two arm edits, the machine code of the instance on five
   images and the value that is spilled.
2. The change of Decision 2, with Controls 3 and 4.
3. The cells of Control 2 in scratch. If one leaves the band,
   stop as Decision 5 states.
4. Score the file gate and both project screens. Replay the
   public admission corpus.
5. fmt, Clippy, preflight and the targeted checks in separate
   state. Then the stages of Control 8 by their stage commands.
   Sign. Source review, one complete final, Results.

## Results

The executor tests the frame's header-table length before consuming the
resume marker. It records a completed frame-entry activation's resume
PC only for a nonempty table. The cancel and fuel step still runs at
every charged instruction. The only inventory transition is
`Executor::invoke_args`, from `3ef5a2a2` to `13cf89b0`.

The file gate passed all 32 per-record and all 32 cumulative rows
against published 2df16832. At native-on Permute sizes 1 and 5, whole
W fell by 1,231,989 and 1,232,693 instructions. Each of the four
unrelated arm edits passed all 32 per-record rows against the
dispatcher change. The largest size-5 execution rise was 329,016 Ir
for the Len call, within the W allowance of 472,805.

At size 5 Permute never executed Len. The 329,016 extra instructions
(0.8131 per dispatch) were all in the dispatcher itself. Blocks
exclusive to Binary added 138,672 Ir. On each of 34,663 integer binary
operations the Len-call image spilled the right operand from %rsi to
the stack and reused %rsi for the arithmetic jump table. The original
kept the operand in %rsi. Branch added 72,132 Ir: before checking the
condition slot it loaded 7 into %ecx and stored %rcx to the stack on
each of 36,066 visits. The remaining blocks added a net 118,212 Ir,
including Return (+56,288), Field (+48,977) and Copy (-43,345).
Both images dispatched the same opcode counts.

The MethodProgram dispatcher executed 404,414 bytecodes at size 1 and
404,642 at size 5 in each image. Self Ir excludes callees.

| Image | Self Ir, size 1 | Ir/dispatch | Self Ir, size 5 | Ir/dispatch |
| --- | ---: | ---: | ---: | ---: |
| Published base | 25,049,316 | 61.93978 | 25,065,956 | 61.94601 |
| Dispatcher change | 23,817,327 | 58.89343 | 23,833,263 | 58.89963 |
| Len call | 24,146,143 | 59.70650 | 24,162,279 | 59.71273 |
| Cold Len branch | 23,797,005 | 58.84318 | 23,812,869 | 58.84923 |
| Clone change | 23,797,005 | 58.84318 | 23,812,869 | 58.84923 |
| Both arms | 23,870,717 | 59.02545 | 23,886,725 | 59.03175 |
| Perturbation scaffold with published arms | 23,817,327 | 58.89343 | 23,833,263 | 58.89963 |

Control 3 was amended after the four edits stayed inside the band.
In the dispatcher change, each of the four edits, and the scaffold
control, the optimized empty-table path executed one marker load per
dispatch (404,414 and 404,642 loads). The marker comparison, the
per-dispatch marker store and the header lookup each executed zero
times. Separately, each image wrote the marker sentinel at frame
entry 37,485 and 37,525 times, and wrote the activation result zero
and four times. The published base kept the marker in a register.

A scratch alternative passed the marker by reference into the existing
header lookup. It eliminated the per-dispatch load and passed the
published-base file gate. The Len-call perturbation passed, but cold
Len raised size-5 execution by 509,891 Ir, above the unchanged W and
T allowances of 469,885 and 473,593. The scratch alternative was
withdrawn.

Both resume controls use a loaded loop whose header is PC 0. The
region witness runs its last fuel unit and compares the complete
outcome with the interpreter. It counts exactly one native entry and
one fuel exit. Omitting the first resume-marker write produced two
entries and two fuel exits. Removing that write altogether left the
worker running until the 15-second test deadline. The frame-entry
witness injects a completed activation at PC 0 and uses the production
dispatcher and loaded region from there. With zero trips the resumed
header ran in the interpreter. With a later backedge the region
entered once. Removing the activation's marker write produced one
native entry where the zero-trip case required zero.

The public corpus replay compared 6,404 rows with the published base.
Every exit, diagnostic, output and route stayed equal. In both native
modes 67 successful cases still required the old engine. The empty and
paired-adapter project screens passed all 128 rows combined.

With the test controls present the file gate and all four perturbations
passed again. The normalized dispatcher instructions and their counts
were identical to the first measurements in each corresponding image.
One Clone capture was refused before scoring: the native startup map
had 31 mappings before the stack; the frozen reference requires 32.
After libc's writable file mapping, the refused sample had one
15-page anonymous mapping where the matching sample had separate
13-page and 2-page mappings. A fresh capture of the same binaries met
the map condition and passed all 32 per-record rows.

Before signing, formatting, all four manifest Clippy configurations,
preflight, rune-suite (including 50 contracts), root-tests, cross and
final-controls passed. Clippy initially flagged the large error return
of the test-only activation helper. The helper now requires its
fixture to succeed. The first final-controls invocation used a
tmpfs-backed TMPDIR and refused a scheduling fixture before admission
(27,513,765,888 bytes free, 28,319,940,608 required). The complete
final-controls command passed with its scratch on the build filesystem.

The complete final ran on de4f584a as FINAL-v1 and passed all 107
stages. RESULT.json has sha256
907963f5612d6a66748d81efd2f3786a2c3afd9f819cd69bebccb7036b299f65.
Entry through cleanup took 4,453.8 seconds against the 5,100 second
limit. The run entered with 155,618,185,216 bytes free against the
151,152,450,560 required. The longest chain was the gate, which ran from
3,813 to 4,403 seconds. The Rune suite ended at 2,278 seconds with 967.4
seconds in the contracts. The declaration changed the policy file, so no
published gate inputs matched and the gate rebuilt and measured the base
on the clock. The anchor lookup missed and the anchor build took 104.4
seconds. The source condition passed over 40 items and accepted the one
declared transition, invoke_args, with no other item changed. All 32
per-record rows passed and 18 were exact: the 16 native-off rows and the
two native-on shared_rows rows. The other 14 fell and none rose. The
largest falls were in native-on fib, 2,019,180 instructions at size 1
and 2,019,744 at size 5, and in native-on awfy_permute, 1,231,989 and
1,232,693, each the same in W and T. The eight installed native-on T
rows were scored under version 2, with the loader and the rest of setup
each unchanged. All 32 cumulative rows passed. The fetch stayed offline
and took 12.2 seconds. The build peaked at 121,028,837,376 bytes and
shared memory at 1,638,400 bytes. The cleanup cleaned 37 targets and
removed the run. The memory controller reported no events. The run left
no new entry in /tmp or /var/tmp. The paragraph above on the Len call
was written after the run and changes no source.
