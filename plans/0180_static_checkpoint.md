# rnx 0180: retained-code explanation checkpoint

**Status:** Step 0 for Claude's review, on accepted plan 3cf1e7a. No engine edit,
rebuild or subject execution. No new PMU, Valgrind or timing measurement. The
0179 STOP remains immutable. This is not a closure or an adoption result.

## Retention and repair

Bench branch codex/0180 at 751ab29e contains tooling and the complete checkpoint
`results/execution-cost-0180/STEP0.md`. It retains the original static1 tool STOP:
the requested Vm::run filter did not admit binutils' v0 spelling
`<rune::runtime::vm::Vm>::run`. The guard refused rather than omit that region.
Claude approved a spelling-only repair, committed as 038f27fb, then one fresh
static2 completed with the same retained inputs. Both attempts and controls
remain, including raw arithmetic and full binutils output. Large asm and region
files are losslessly xz-packed; ledger hashes refer to decompressed contents.

The manifest and all six binary hashes are checked. The primary pair is the
authority for both deciding PMU and wall samples. The counter pair is only a
secondary static cross-check, not the deciding PMU subject. Explicit safe
environments, binutils identities, commands, statuses and raw output hashes are
retained. There are 22 synthetic controls and two file-level manifest/role
corruptions; all behave, the latter before any binutils command. Four separate
stock-objdump spot excerpts reproduce the primary caller/callee extents below.

## Q1: instruction reduction beyond startup, cycle cause unresolved

The retained raw medians reconstruct run-empty's delta of -1,946,002
instructions. Diagnostic subtraction leaves numeric -11,999,746.5; signed and
negative range -10,001,787 and -10,001,855. Each script executes 1,000,000
iterations by its frozen source. These are approximately -12/-10 instructions
per iteration, with residues +253.5/-1,787/-1,855. The latter differ by 68;
a shared setup effect is compatible, not established. This subtraction is a
difference of whole-process medians, not a measured loop phase. Script-dependent
compilation/setup and their residuals remain named.

The data support fewer instructions beyond startup rather than startup savings
hiding loop growth. Generated VM code is demonstrably different too: primary
Vm::run changes from 102,615 to 102,741 bytes; its +0xa stack reservation changes
0x538 -> 0x518 and spill locations differ. Static decoded instruction counts
19,697 -> 19,744 include cold paths and padding, not retired loop instructions.
Placement also changes. Pure placement of identical code cannot describe the
whole binary difference, but a particular hot loop could still have equivalent
instructions at a different placement.

No exact dynamic block binding identifies the predicted -12/-10 sequence in
this checkpoint. The native cycles/wall increases are therefore not yet
explained by a specific dependency, spill, frontend or cache mechanism.

## Q2: fib has a separate generated-code question

fib(27)'s invocation recurrence C(0)=C(1)=1,
C(n)=1+C(n-1)+C(n-2) gives 635,621 invocations, 317,810 nonleaf, excluding main.
The startup-adjusted delta is +20,975,687.5, or +33.000306 per invocation; the
residue against +33 each is +194.5. This is not proof every leaf/nonleaf path
adds the same 33 instructions. The earlier reviewer approximation +21 per call
was withdrawn after this source binding.

Primary pop_call_frame is uniquely identified at base 0x3deb10 (772 bytes) and
candidate 0x4e0920 (804). Its stack reservation changes 0x58 -> 0x68 and its
register/spill and payload preservation code differs. Two direct cleanup-loop
call sites bind exact Value-drop clones:

| Identity | Base | Candidate |
| --- | --- | --- |
| Caller-relative first call | +0xcf | +0xe7 |
| Caller-relative second call | +0x181 | +0x19b |
| Exact callee address | 0x4c2300 | 0x379980 |
| Callee bytes / decoded instructions | 204 / 42 | 1,327 / 349 |

The target address and raw mangled symbol, not a guessed matching demangled
name, identify each edge. The callee starts with two saved registers in base,
six in candidate. This establishes changed generated code on a possible return
cleanup path; it does not establish its frequency or account for all fib work.
No 0176 dynamic edge counts are transferred to these 0179 binaries.

## Limits and review boundary

Full raw disassembly, ELF/relocations, symbols and static boundary residues are
retained. Ambiguous clones, aliases, indirect paths and unresolved direct-target
normalization remain explicit. Automated comparison cannot certify Vm::run,
op_call or pop_call_frame equivalence because their full target normalization
is unresolved. The concrete differences above are readable directly in stock
binutils outputs and do not rely on such an equivalence claim. Static function
sizes, counts or 32/64-byte residues are not performance diagnoses.

Changed code/dependencies is supported as a code fact. Placement/frontend,
cache/branch and frequency/host explanations remain possible and untested.
Neither question has a complete causal attribution. No CGU, layout or frequency
cause is asserted from fewer instructions and more cycles alone.

Review this checkpoint before any further execution. The proposed next scope,
if accepted, is to pre-register one event-discovery and native paired-counter
protocol on the same primary binaries that can discriminate host/frequency
variation from frontend/backend cost. It must specify CPU/event documentation,
exact groups and subjects, missing-event policy, predictions, reproduction and
noise rules before discovery or measurement. If that cannot discriminate the
remaining alternatives, close INCONCLUSIVE. No profile/candidate change or
retroactive gate exception follows, and the user's gate decision remains
separate. Nothing in this checkpoint approves that future protocol implicitly.
