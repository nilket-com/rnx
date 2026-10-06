# rnx 0183: bounded frontend-delivery study

**Status:** document-only protocol, revised to both review rounds (chatd seq 1607 and 1705). Next checkpoint: the primary-source grouping audit, before any discovery or open. Claude plans and drives; Codex reviews every checkpoint and reconstructs results. This is step 2 of the sequence proposed in `0182_runtime_architecture_decision.md`. It is a diagnostic record: no engine edit is adopted, no gate changes, 0179's STOP is unchanged, and it must not delay the architecture work. Nothing below is authorised to open a counter until the stated review checkpoint.

## 1. Question and stopping points

0181 found, on whole-process windows of the two retained 0179 binaries, more legacy-decode uops and a lower decoded-cache share where cycles are in excess, with fewer retired instructions on the numeric and two range windows (not on every measured workload: fib and calls retire more). It did not measure what that delivery costs, locate it, or test a cause.

Three stages, each a stopping point, each needing its own review before it starts:

- **A, counting gate.** Is the difference in fetch-penalty cycles for decoded-cache to legacy-decode switches comparable in size to the reproduced excess cycles?
- **B, localisation.** Only if A's registered disposition permits: where, in the candidate, are the instructions that the processor tags as decoded-cache misses, relative to the base?
- **C, one intervention.** Only if B identifies an admissible region and a feasibility audit finds an instruction-preserving way to move it: does changing the region's placement alone change delivery and cycles together?

A is a compatibility check, B is localisation evidence, and only C tests a cause. If any stage's method is unavailable or its condition is not met, the record closes there with what it has. No stage is replaced by another event or method after results.

## 2. Fixed subjects and method

The two retained 0179 PRIMARY binaries (base `af06e8b3...0c87`, candidate `e4a5f207...a787`), staged and hash-checked as in 0181, same stage path, E0, CPU 4, argv, scripts and expected outputs. Windows: numeric, range_signed, range_negative (the windows with the delivery signal), calls (smaller signal), while and fib (no resolved MITE increase or share decrease; fib keeps about +2.5% cycles), run-empty. Order, five repetitions, ABBA, ten samples per side, the 0179 reproduction anchors and tolerances, the descriptive "resolved" rule, lifecycle, deadlines, retention and the sentinel scan are 0181's, unchanged. The driver is a new 0183 module importing the pinned 0180/0181 libraries; 0180 and 0181 files and results stay untouched. Fresh identity and two opens per group on the affinity probe, reviewed before rehearsal, as in 0181. Global and group-local stop policy as in 0181.

Event source: Intel's Golden Cove core table at the revision 0180 pinned (sha256 `d588ba82...3a0f`). Documentation was read while drafting; no event has been opened.

## 3. Stage A: counting gate

Event source: Intel perfmon `ADL/events/alderlake_goldencove_core.json` at revision
`78eb739dafa28c1b296f7b4d5fb7e1a7e81b1537`, sha256
`d588ba821297c4097705214707b2f7d52ed173ec6a4ad9acf0fc3649e6e43a0f` (the file 0180 pinned).

| Group | Added event | Encoding in that file | Meaning in that file |
| --- | --- | --- | --- |
| R | none | | fresh reproduction anchor |
| F1 | DSB2MITE_SWITCHES.PENALTY_CYCLES | event 0x61, umask 0x02 | fetch penalty cycles on a switch from the decoded-uop cache to legacy decode |
| F2 | FRONTEND_RETIRED.ANY_DSB_MISS, counting | event 0xc6, umask 0x01, MSR 0x3F7 value 0x1 | retired instructions that experienced a decoded-cache miss |
| F3 | FRONTEND_RETIRED.DSB_MISS, counting | event 0xc6, umask 0x01, MSR 0x3F7 value 0x11 | as F2, "critical": the miss exposed a stall to the back end |

**Grouping audit, required before discovery is specified (review R2).** The table marks F2 and F3 `TakenAlone`. That
explains why the two cannot be counted together; it does not establish that either may share a strong pinned group
with the fixed instructions and cycles anchors. Before any open, an amendment must cite a primary source (Intel's
description of the frontend configuration register and of TakenAlone, and the Linux kernel source at a pinned
revision for how this event's extra register is scheduled with fixed counters) and state the exact permitted
spelling, including the `frontend` field (`config1:0-23` on this host per the 0181 identity receipt). If the sources
do not support anchors and the event in one group, the fail-closed method is fixed now: that group is NOT
collected. It is not respelled, weakened, split across invocations, or anchored by a separate run. F1 is an
ordinary programmable event and needs no such audit beyond the usual two opens. Counting validity is a separate
question from precise-sampling support, which belongs to Stage B.

**Quantities.** Per window and group, from the ten samples per side: `D(event)` = candidate median minus base median
of the added event; `D(cycles)` = the same for cycles in that group. Both are differences of pooled medians, not
per-sample ratios. For F1 only, `r = D(F1) / D(cycles)`, a ratio of two differences of medians: a comparison of
magnitudes, not a same-sample ratio and not an accounting of where cycles went. "Resolved" is 0181's rule applied
to the raw event count and to cycles separately.

**Window classes.** Signal windows: numeric, range_signed, range_negative. Contrast windows: while, run-empty.
Reported only: calls, fib.

**Collection state of each F group**, recorded before any status is formed: `not collected (unavailable)` when the
group was ineligible at discovery or excluded by the grouping audit; `invalid` when it was collected but failed
counter validity, did not complete, or failed its anchors; `valid` otherwise. An invalid group is never treated as
if it were absent.

**Per-window status of F1**, evaluated in this order, first match wins:
1. `unavailable`: F1 was not eligible at discovery.
2. `failed`: F1's group failed counter validity (group-local stop) or did not complete.
3. `undefined`: cycles did not resolve up in the F1 group on this window (so `D(cycles)` is not a resolved positive
   excess and `r` is not formed).
4. `contrary`: F1's count resolves down.
5. `strong`: F1's count resolves up and `r >= 0.5`.
6. `weak`: F1's count resolves neither up nor down, or resolves up with `r <= 0.1`.
7. `intermediate`: F1's count resolves up with `0.1 < r < 0.5`.

The boundaries are inclusive as written (`r` exactly 0.5 is strong, exactly 0.1 is weak). The driver's controls
must hold a fixture for every status and for the unavailable, failed, undefined and mixed precedence cases,
including `r` exactly 0.1 and 0.5 and a resolved downward count.

**Disposition of Stage A**, first match wins:

| Disposition | Condition on the three signal windows | Consequence |
| --- | --- | --- |
| A-unavailable | F1 `unavailable` | Close at A. F2/F3, if collected, are reported descriptively. No localisation proposal. |
| A-failed | F1 `failed`, or `undefined` on any signal window | Close at A; report what completed. A missing or undefined F1 result is never read as weak or intermediate. |
| A-strengthened | `strong` on all three | A Stage B proposal may be submitted for review. |
| A-weakened | `weak` or `contrary` on all three | Close at A. No localisation proposal, whatever F2/F3 show. |
| A-mixed | any other combination (including strong on one or two windows only) | Report the per-window statuses. A Stage B proposal may be submitted only if the selected locating event (below) resolves up on all three signal windows; otherwise close at A. |

**Selected locating event**, fixed by Stage A's collection states and nothing else: F3 if F3 is `valid`; F2 if F3 is
`not collected (unavailable)` and F2 is `valid`; none if F3 is `invalid` (a failed F3 is not replaced by F2) or if
neither is valid. Under A-strengthened a Stage B proposal also requires a selected locating event that resolves up
on all three signal windows; with none, the record closes at A with the counting result. This selection is about
counting eligibility only. Whether that event supports precise sampling is Stage B's audit; if it does not, Stage B
closes and no other event is substituted.

**Contrast flag**, evaluated separately and never changing the disposition: for each contrast window, if F1's
count resolves up AND its `D(F1)` is at least half of the smallest `D(F1)` among the three signal windows (raw
count differences, no per-operation scaling), the report states that the penalty difference is not specific to the
windows with excess cycles, and any Stage B proposal must address it. The flag qualifies the inference; it does not
close Stage A by itself.

Permitting a proposal is not approving Stage B: that needs its own reviewed amendment.

## 4. Stage B: localisation (separate reviewed amendment before any sampling)

**Prerequisite audit, in the amendment.** Precise sampling of the locating event selected by Stage A's rule
(section 3), and only that event, with a fixed period. The metadata `Precise` flag and the PMU's `max_precise` value do not establish what
address a sample reports. The amendment must show from primary documentation, for this event at the precise level
actually used: which instruction's address is recorded (the tagged retired instruction or a later one) and with
what skid; how the load address is recovered so sampled addresses map to ELF virtual addresses; the expected
sample count per window at the chosen period and the loss/throttling indicators that invalidate a run; and the
perturbation sampling adds. If the documentation does not support attributing a sample to the instruction that
missed, Stage B does not run and no other event is substituted. Minimum-sample, loss and mapping controls are
proposed in that amendment.

**Region statistic, fixed now (review R3).**
- A sample is assigned to exactly one category: a named symbol extent from 0180's static2 identity tables for THAT
  binary (by ELF virtual address), or `unmapped` (address outside every extent, or in a shared object), or
  `unknown` (address not recoverable, or inside more than one extent or an aliased extent: an ambiguous mapping is
  never resolved by picking one). Every sample is retained with its category.
- Regions are matched across the two binaries by raw mangled symbol name with any `.llvm.<digits>` suffix removed.
  A name present in only one binary, or appearing more than once in either, is `unmatched`; unmatched names are
  never merged or paired by resemblance.
- For window w and matched region g: `n_c(w,g)` and `n_b(w,g)` are the per-run median sample counts in candidate
  and base at the same fixed period (so counts are comparable without rescaling); `d(w,g) = n_c - n_b`.
- `P(w)` is the sum of `d(w,g)` over matched regions with `d(w,g) > 0`. Negative differences are reported and do
  not offset positive ones. There is no admissible region if, on any signal window: `P(w) <= 0`; or either binary
  has zero total samples (the fractions are then undefined, not zero); or `unmapped + unknown + unmatched` exceeds
  10% of that binary's samples for EITHER binary (mis-mapping in the base can manufacture an excess as easily as
  in the candidate).
- `share(w,g) = d(w,g) / P(w)` for `d(w,g) > 0`.
- An admissible region is a matched region g with `share(w,g) >= 0.5` on numeric AND range_signed AND
  range_negative, and it must be the ONLY region that qualifies (two regions can each hold exactly half). With no
  qualifying region or more than one, Stage B closes with the full distribution reported and no intervention; there
  is no tie rule.
- Sub-symbol statements (a particular 32- or 64-byte block, a loop) are descriptive only and select nothing.

## 5. Stage C: one placement-only intervention (separate reviewed amendment and feasibility audit)

**Reproducibility prerequisite.** Rebuild candidate source `b28f8cb3` with 0179's reviewed build method. If the
result is not byte-identical to the retained candidate primary `e4a5f207...`, Stage C closes at feasibility before
any intervention is built.

**The single admissible intervention, fixed now (review R4).** Let `a_c` be the admissible symbol's start address
in the candidate and `a_b` its start address in the base. Let `k = (a_b - a_c) mod 64`, in 0..63. If `k = 0`,
Stage C does not run (the symbol already starts at the base's residue; nothing to test). Otherwise the intervention
is the candidate program with exactly `k` bytes of padding inserted immediately before that symbol, so that it and
everything after it in its output section move up by `k` bytes. One value of `k`, determined by two addresses
measured before any Stage C result; no other displacement, no search, no second instance.

**What may differ and what must not.** Allowed: the `k` padding bytes; addresses of the moved symbol and of every
later symbol in the same section; relocation-resolved fields that encode those addresses (call and jump
displacements, RIP-relative operands, address tables, unwind and symbol-table entries); the build-id. Required
identical: the opcode and operand structure of every instruction in the binary after relocation resolution (0180's
static comparison, an unresolved target counting as a difference), all data other than the address fields above,
and the section list. The feasibility audit must show a concrete way to produce exactly that (for example at link
time) and validate the whole executable, not only the moved symbol. If it cannot be produced, or validation fails,
Stage C closes at feasibility. A Stage C build is a diagnostic subject only: never a candidate, never gated.

**Measurement, fixed now (review R5).** Subjects: base (retained), candidate (retained), moved (new). Groups R, D
(0181's definition) and F1; windows as in section 2; order per window base, candidate, moved, moved, candidate,
base; five repetitions; ten samples per subject. Anchors: base and candidate must reproduce 0179 as in 0181. The
moved binary's instruction median on each nonempty window must be within 0.05% of the candidate's; machine-code
identity is established by the static validation, and this bound only checks that the measured count agrees with
it despite small initialisation variation. A larger difference stops Stage C as an invalid intervention.

Metric: for each signal window and for each of cycles, IDQ.MITE_UOPS and (if collected) F1's count,
`f = (candidate median - moved median) / (candidate median - base median)`, formed only where candidate minus
base resolves up; and whether candidate versus moved resolves under 0181's rule.

Outcomes, fixed now:
- **Placement-sensitive:** on all three signal windows, cycles and MITE uops both resolve down from candidate to
  moved with `f >= 0.5` for both. Conclusion permitted: shifting this symbol AND every later symbol in its section
  by `k` bytes changes delivery and cycles together on these windows. Not permitted: that this symbol's placement
  alone is the cause (later symbols moved too), that the decoded-cache mechanism is the cause, or that any
  particular boundary is; the shift also changes the symbol's internal branch and loop alignment.
- **No resolved effect of this placement:** neither cycles nor MITE uops resolve between candidate and moved on any
  signal window. Conclusion permitted: this run did not resolve an effect of the `k`-byte shift under the
  registered rule. Not permitted: that the shift has no effect (a smaller real difference may be below what the
  rule resolves), that placement is irrelevant, or that the changed instruction stream is the cause; the chosen
  shift may leave the relevant layout condition unchanged.
- **Mixed:** anything else; reported per window without a mechanism statement.
No second placement follows from any outcome.

## 6. Boundaries

One official run per stage. Repairs or replays need prior review; every attempt is retained. No profile change, no engine edit other than C's reviewed placement-only build, no new workloads, no tolerance change. The existing credential and sentinel controls apply. Whatever the outcome, any use of it in an engine design is a separate record.
