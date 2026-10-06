# rnx 0183: grouping audit for the frontend-retired events (document only)

**Status:** checkpoint for Codex's review, required by `0183_frontend_delivery_bounded_study.md` section 3 before
discovery is specified. Sources were read; no counter was opened, no subject run, nothing built.

## Sources, pinned

| Source | Revision | sha256 of the file read |
| --- | --- | --- |
| Intel perfmon `ADL/events/alderlake_goldencove_core.json` | `78eb739dafa28c1b296f7b4d5fb7e1a7e81b1537` | `d588ba821297c4097705214707b2f7d52ed173ec6a4ad9acf0fc3649e6e43a0f` |
| Intel perfmon `README.md` (field definitions) | same revision | `f37fa2ca182740aa4e5b9af02418100a4a24925349645c69162b9f4c5f658528` |
| Linux `arch/x86/events/intel/core.c` | `22430ae5d90ab288b0ee2ad99ae941f4a666b694` (the revision 0180 pinned for perf documentation) | `ba3c66956c6807a4e5b303d514046a55429d7451e2a5e3e1a4e2e31f982a4d9d` |
| Linux `arch/x86/events/perf_event.h` | same revision | `aa1bafba69c09971f9544dafe29e1e5f73a55f1d8aef80383584acf902300294` |

Limit: the running kernel is 7.0.0-31-generic (0181 identity receipt). The pinned kernel revision is upstream source,
not the distribution's build; the audit establishes what upstream source at that revision does, and the discovery
opens remain the test of what this host's kernel accepts.

## What the Intel table says about each event

| Event | EventCode / UMask | Counter | TakenAlone | MSRIndex / MSRValue |
| --- | --- | --- | --- | --- |
| INST_RETIRED.ANY | 0x00 / 0x01 | Fixed counter 0 | 0 | none |
| CPU_CLK_UNHALTED.THREAD | 0x00 / 0x02 | Fixed counter 1 | 0 | none |
| DSB2MITE_SWITCHES.PENALTY_CYCLES (F1) | 0x61 / 0x02 | 0,1,2,3 | 0 | none |
| FRONTEND_RETIRED.ANY_DSB_MISS (F2) | 0xc6 / 0x01 | 0-7 | 1 | 0x3F7 / 0x1 |
| FRONTEND_RETIRED.DSB_MISS (F3) | 0xc6 / 0x01 | 0-7 | 1 | 0x3F7 / 0x11 |

README, "TakenAlone" (lines 361-363): "This field is set for an event which can only be sampled or counted by
itself, meaning that when this event is being collected, the remaining programmable counters are not available to
count any other events."

Reading: Intel's definition restricts the remaining PROGRAMMABLE counters. It says nothing that restricts the fixed
counters, which is where the table places instructions and cycles. F1 is not TakenAlone.

## What the kernel source does

- Golden Cove extra registers (`core.c:356-363`): `INTEL_UEVENT_EXTRA_REG(0x01c6, MSR_PEBS_FRONTEND, 0x7fff1f, FE)`.
  So event 0xc6 with umask 0x01 carries an extra register of kind `EXTRA_REG_FE` (`perf_event.h:45`), with valid
  configuration bits 0x7fff1f. Both 0x1 and 0x11 are inside that mask.
- Shared-register scheduling (`core.c:4135-4208`, called from `intel_shared_regs_constraints`, `core.c:4236-4258`,
  for any event whose `extra_reg.idx` is not `EXTRA_REG_NONE`): an event may take the register if it is unused or
  already holds the SAME configuration (`if (!atomic_read(&era->ref) || era->config == reg->config)`); otherwise it
  receives the empty constraint and cannot be scheduled. This is the kernel's form of "alone": two frontend events
  with different MSR values cannot be scheduled together. It imposes nothing on events that have no extra register.
- Instructions and cycles have no extra register. Their constraints are `FIXED_EVENT_CONSTRAINT(0x00c0, 0)` and
  `FIXED_EVENT_CONSTRAINT(0x003c, 1)` (`core.c:367`, `:369`; macro at `perf_event.h:474-475`).
- **A fixed-counter event is not guaranteed a fixed counter.** For non-pseudo encodings the kernel also allows the
  general counters: `if (!use_fixed_pseudo_encoding(c->code)) c->idxmsk64 |= cntr_mask;` (`core.c:7749-7750`). The
  sysfs aliases on this host are `instructions` = `event=0xc0` and `cpu-cycles` = `event=0x3c` (0181 identity
  receipt), i.e. the non-pseudo encodings. So if a fixed counter is occupied, instructions or cycles can be placed
  on a programmable counter. This host has `nmi_watchdog` = 1 (identity receipt); the watchdog is a cycles event.
  Which counter it holds, and whether our cycles anchor therefore lands on a programmable counter, cannot be read
  from the JSON output of this perf version.
- Event 0xc6 itself may use any of the eight general counters (`INTEL_EVENT_CONSTRAINT_RANGE(0x90, 0xfe, 0xff)`,
  `core.c:407`), consistent with the table's "0-7".

## Conclusion and the one unresolved point

1. F2 and F3 cannot be in the same group, or in the same invocation: different values of one configuration
   register. The plan already keeps them in separate groups and separate invocations. Confirmed by both sources.
2. Kernel source at the pinned revision permits a group of instructions, cycles and ONE frontend-retired event. The
   0181 groups C, D and E had the same shape (two anchors plus programmable events) and were scheduled at 100%.
3. **Unresolved.** Intel's definition says the remaining programmable counters are unavailable while a TakenAlone
   event is collected. If the cycles anchor (or instructions) is placed on a programmable counter because a fixed
   counter is held by the watchdog, the group would have a second programmable counter active beside the frontend
   event. The kernel does not forbid this. Whether the hardware then counts both correctly is not established by
   either source, and I found no statement in these files that settles it.

## Proposed spelling and fail-closed handling, for review

- Spelling, mechanical from sysfs and the table: `cpu_core/event=0xc6,umask=0x01,frontend=0x1,name=fe_any_dsb_miss/u`
  and `cpu_core/event=0xc6,umask=0x01,frontend=0x11,name=fe_dsb_miss/u`, each in one strong pinned group with the two
  anchors as in 0181 (`frontend` is `config1:0-23` per the identity receipt); F1 as
  `cpu_core/event=0x61,umask=0x02,name=dsb2mite_penalty_cycles/u`.
- To close point 3 without guessing, use the pseudo-encodings for the anchors IN THE F2 AND F3 GROUPS ONLY, if this
  host exposes them: the kernel does not extend pseudo-encodings to general counters (`core.c:7745-7750`), so an
  anchor spelled that way either gets its fixed counter or is not scheduled. With `--no-scale` and the 100%-running
  requirement, "not scheduled" is a validity failure of that group, which is the fail-closed outcome. Whether a
  pseudo-encoding spelling is available through `cpu_core/.../` on this perf and kernel is not something I can
  establish from the sources; it would be decided by the two discovery opens, never by magnitude.
- If no such spelling exists, the conservative reading of Intel's definition applies and my recommendation is that
  F2 and F3 are NOT collected with anchors. Since the plan forbids a separately anchored run, they would then be
  uncollected, Stage A would run on R and F1 only, and by the plan's selection rule there would be no locating
  event: the record would close at Stage A with the counting result.
- F1 needs none of this.

The decision between "pseudo-encoded anchors in F2/F3 if discovery supports them" and "do not collect F2/F3" is
yours to review; I have not opened anything to find out which applies.

## Amendment after review (chatd seq 1719): fixed-only anchors traced, watchdog interaction, decision

Additional pinned source: Linux `kernel/watchdog_perf.c` at the same revision, sha256
`84db2534ce5a4405d81373ba282c2087de3c11e810347ef19cef7ba805a14a16`.

**Scope correction.** The kernel code cited above shows that two frontend events with different configuration
values cannot hold the shared register at the same time. It does not show that the kernel could never alternate
them. The reason this study keeps F2 and F3 in separate invocations is its own method: strong pinned groups, no
multiplexing, `--no-scale`, 100% running. That is a method requirement, not a general kernel claim.

**Fixed-only anchor candidates, traced in source, not tried.** The Golden Cove constraint table contains the
pseudo-encodings `FIXED_EVENT_CONSTRAINT(0x0100, 0)` "pseudo INST_RETIRED.ANY" and
`FIXED_EVENT_CONSTRAINT(0x0200, 1)` "pseudo CPU_CLK_UNHALTED.THREAD" (`core.c:368`, `:370`). Intel's table gives the
same two as INST_RETIRED.ANY (EventCode 0x00, UMask 0x01, fixed counter 0) and CPU_CLK_UNHALTED.THREAD (0x00 / 0x02,
fixed counter 1): the same retired-instruction and unhalted-thread-cycle quantities as the ordinary aliases, on the
fixed counters. The kernel does not add the general counters to a pseudo-encoded constraint (`core.c:7745-7750`), so
`cpu_core/event=0x00,umask=0x01,name=instructions/u` and `cpu_core/event=0x00,umask=0x02,name=cycles/u` would either
be placed on fixed counters 0 and 1 or not be scheduled. Whether this host's perf and kernel accept those spellings
has not been tried and would only be tested by the two registered opens.

**Why that is still not enough.** Fixed-only anchors constrain OUR counters. Intel's definition of TakenAlone is
about the processor's remaining programmable counters, whoever programmed them. This host reports `nmi_watchdog` = 1,
and the pinned source creates the hard-lockup watchdog as a per-CPU, pinned, sampling `PERF_COUNT_HW_CPU_CYCLES`
kernel counter (`watchdog_perf.c:89-100`, `:127-135`). That does not establish that such an event is active on CPU 4
during our samples: the source allows creation to fail and allows a raw-event override, and neither a successful
local enable nor the watchdog's CPU mask is in our receipts. The reasoning is therefore conditional. IF the normal
cycles watchdog is active on CPU 4, its event is an ordinary cycles event whose constraint includes the general
counters, and contention with a fixed-only cycles anchor is possible: either the watchdog's event is placed on a
programmable counter beside the TakenAlone event, or our anchor cannot be scheduled. The sources do not give its
placement or its absence, this perf's output does not report counter placement, and neither source says whether
Intel's restriction is limited to a privilege domain. The method forbids disabling the watchdog or other system
services. The exclusion below rests on that uncertainty, not on a demonstrated unsafe hardware operation.

**Decision proposed, conservative and fail-closed:** F2 and F3 are EXCLUDED from this record: collection state
`not collected (unavailable)`, reason "TakenAlone safety beside other active programmable counters cannot be
established from primary sources on this host without changing system services". No spelling is tried for them.

**Consequences under the accepted plan's frozen rules.** Stage A runs on R and F1 only. With F3 and F2 both not
collected there is no selected locating event, so no Stage B proposal can be made under any disposition
(A-strengthened included): **the record closes at Stage A with the counting result.** Stages B and C do not run in
0183. F1 is an ordinary programmable event restricted to counters 0-3 and not TakenAlone; it is unaffected.

## Proposed discovery and controls protocol for Stage A (R and F1 only), for review before any open

- Driver: a new `probes/frontend-0183/events0183.py` in rnx-bench that imports the reviewed 0180 and 0181 drivers as
  pinned libraries (hash-checked before import, as in 0181) and edits neither. Order R, F1. R is 0180's definition
  with the ordinary aliases. F1 is `{cpu_core/instructions,name=instructions/u,cpu_core/cpu-cycles,name=cycles/u,
  cpu_core/event=0x61,umask=0x02,name=dsb2mite_penalty_cycles/u}:D`, `--no-scale`, one invocation per sample.
- Discovery: fresh filtered identity (0181's fields), host gate, exactly two opens for R and two for F1 on the grep
  affinity probe, judged by 0181's classifier (anchors global, the F1 row group-local). F1 ineligible gives the
  plan's A-unavailable and closes without subjects. Receipts reviewed before rehearsal; admission pins set only
  after that review.
- Official: R then F1; seven windows; five repetitions; base, candidate, candidate, base; 280 samples; 0179
  anchors and tolerances; 0181's global and group-local stop policy, deadlines, identity recheck before each group,
  retention and sentinel scan. One run.
- Derived quantities and statuses exactly as plan section 3: `D(F1)`, `D(cycles)` in the F1 group, `r`, the
  per-window status by the frozen precedence, the disposition, and the contrast flag.
- Controls before discovery (synthetic, no perf, no subject): a fixture for every per-window status and for the
  unavailable, failed, undefined and mixed precedence cases, with `r` exactly 0.1 and exactly 0.5 and a resolved
  downward count; every disposition row; the contrast flag at, above and below its half-of-smallest threshold;
  "F2 and F3 not collected gives no locating event under every disposition"; plus a replay of 0181's control set
  against the pinned libraries.
