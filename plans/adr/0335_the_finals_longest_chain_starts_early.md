# ADR 0335: The final's longest chain starts early

Status: Proposed

Base: published 5c853ce8c166e736d92bc8023a2361b593510e01 (ADR 0334).

## Problem

The 0334 complete final passed with 8.7 seconds to spare: 3,591.3 of
the 3,600. Its candidate changed the Rune crate, which every lane
rebuilds inside the clock. Stage time summed to 8,011 seconds, against
6,393 in the 0333 final, whose candidate changed only `tools/final`.
Every remaining record of the old-core removal changes the Rune crate.

The finish was set by one serial chain. A stage holds each Cargo
target directory it writes for its whole duration, and stages sharing
one run in selection order. The 27 stages that write `target/debug`
ran for 2,565.8 seconds in total. `rune-suite` is one of them and
became ready at 1,768 seconds. It ran for 1,071, of which about 227
were six Cargo commands rebuilding Rune. `rune-clippy` and seven
package stages waited behind it until 2,907. `fixed-route` runs alone,
started then, and ran for 642.2 seconds.

Under whole-stage holds no order of these stages finishes before
3,208 seconds: the `target/debug` writers and then `fixed-route`.
Yet `rune-suite` has 12 cache recipes and only the first, the
optimized core tests, writes `target/debug`. That recipe compiled for
about 72 seconds and tested for about 30. The other 11 use the
engine-dev directory. The stage held the lane for 1,071 seconds to
use it for about 102.

The first candidate, e1671ed1, moved `rune-suite` early and its
complete final failed there. One stock child of the range-index
contract was killed at its 10-second deadline. Reproduced under
overlapping builds, that child was asleep in the ext4 journal while
opening its result file, with 36 milliseconds on CPU. The contract's
children hand results back through a temporary directory on disk, so
the deadline measured the host's journal as well as the engine. Under
the published schedule the suite ran after its lane had drained and
the stall did not occur.

## Decision

1. A stage excludes other writers of a target directory only while it
   writes that directory, including its marks and products. The first
   choice is to keep both physical caches and release `rune-suite`'s
   hold on `target/debug` after its last real use, once stage teardown
   is shown not to write there. If releasing a hold mid-stage proves
   too invasive, that one recipe gets a directory of its own instead,
   and its measured prepared size and candidate growth enter the disk
   declaration and both admissions. The engine-dev directory stays
   shared.
2. With the hold released, the stages `fixed-route` waits for start as
   early as their real inputs allow. A stage waits for another only
   where they write the same files at the same time, one consumes the
   other's product, or a measurement needs quiet.
3. The measure is the timed final of a candidate that changes the Rune
   crate, on nano, started from the published baseline checkpoint. The
   target is a predicted finish of 3,000 seconds or less on the 0334
   final's recorded durations, stated with its assumptions.
4. Every check stays. The 107 stages, their commands, their pass
   criteria, the 3,600-second limit, the token and memory budgets, and
   `fixed-route` running alone are unchanged. No stage moves out of
   the clock and no product is built before it.
5. The schedule stays a function of the catalogue. The receipt keeps
   naming, for each stage, what it waited for, and records when a
   hold is released within a stage.
6. A contract child's deadline measures the engine. Inside a final,
   unsandboxed gap-contract children hand results back through
   final-owned, memory-backed temporary storage with a stated bound,
   removed on success and on failure. Durable evidence, diagnostics
   and build targets stay where they are. The wall, output and fuel
   guards are unchanged. The access sidecar takes the same route: the
   child writes it to its temporary directory and the parent
   publishes it to the evidence directory after the deadline.
7. Scope: `tools/final`, its controls, and the sidecar path in the
   gap-contract test harness. No engine, product or gate-band change.

## Controls

1. A check of the published 0334 schedule against its receipt: every
   recorded start satisfies the directory, producer and alone rules,
   and the stage intervals plus the timed overhead account for the
   3,591.3 seconds.
2. A counterfactual over the same receipt using the candidate's real
   scheduling policy, fixed and stated resource assumptions, and the
   recorded durations of the recipes within `rune-suite`. It reports
   the predicted finish, the chain that bounds it, and how the finish
   moves with token availability. Recorded starts are not inputs. The
   published controller lends and returns tokens during a stage and
   the receipt does not record every loan, so the prediction is not
   presented as a reproduction of the old schedule. The first
   Rune-changing final measures the gain.
3. Two stages never write one target directory at the same time, and
   a producer still precedes its consumer. A planted overlap inside
   the released interval and a planted early consumer are each
   refused. The existing scheduler and controller controls pass.
4. Disk: both admissions pass on the prepared store under the new
   concurrency. A new directory, if one is needed, is declared, and
   the declaration matches the Rust catalogue.
5. The contract children's temporary path and its filesystem are
   pinned inside a final. A deliberately stuck child still times out
   at the unchanged deadline. Cleanup runs after a failure, and the
   retained evidence files are unchanged, the access sidecars
   included. No write to the evidence directory remains inside a
   child's deadline. As one-time evidence, the
   failing row repeated under overlapping real builds passes with the
   memory-backed handoff and times out with the disk handoff.
6. Finals: targeted pre-sign checks under the final's real recipe,
   then one complete final on the signed candidate. That candidate
   changes only `tools/final`, so its own final does not show the
   gain.

## Constraints

- No new third-party crate. Tabs.
- No cache pre-warming and no state carried from an earlier run of the
  same candidate.
- No full timing run before signing.
