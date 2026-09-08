# rnx 0005: the allocation ceiling

Status: proposed 2026-09-08. The fifth record of rnx, and the one that
settles what the first release record's memory threshold measures. Today
`:memory` reports the source text and source maps the session holds and
says so; the values a binding holds and the compiled units behind them
are unmeasured. This record replaces that with a measured figure, and is
deliberate about what the figure is: a ceiling on the process's tracked
live allocation request bytes, not an accounting of what the session
owns. It amends the first release record, because that record's decision
3 describes a charge this cut does not compute.

## Context

### What upstream can and cannot tell us

Rune 0.14.1 exposes no size on a value: `Value` carries a type and a
representation, and nothing reports the bytes behind either. A unit exposes
only `UnitStorage::bytes`, the size of its instruction storage, and not its
constants, static strings, debug information, or maps. `rune-alloc` has no
allocator abstraction with accounting and no limit feature; its `Global` is
a zero-sized passthrough to the system allocator.

A structural walk over retained values could produce a number, but it would
be an estimate assembled from per-node guesses that could not include a unit
at all. The first release record calls the retained figure acceptance work
rather than polish, so gating on an estimate presented as a measurement is
the one thing this cut must not do.

### Why a session-attributed figure is not available either

A counting global allocator measures live bytes exactly. It is tempting to
call the difference between the live figure now and the figure at an empty
session "the session's retained memory". That is wrong, and it fails in the
direction that matters. Suppose startup holds twenty megabytes of history
and temporary buffers, those are freed as the session runs, and Rune retains
fifteen megabytes meanwhile: the difference has fallen while the session has
grown. Current minus baseline measures net process growth. It is not an
attribution of session-retained bytes and it can under-report them.

So this cut does not claim attribution. It enforces a ceiling.

### What the counter sees

It sees allocation requests routed through Rust's global allocator, and only
those. Outside it: allocations made directly by native code that does not
route through that allocator, memory mappings, thread stacks, and everything
belonging to child processes. It is therefore tracked live allocation
request bytes and nothing wider, which the record says wherever the figure
is reported.

## Decision

### 1. An absolute ceiling, not a delta

rnx installs a counting global allocator maintaining live bytes as
allocations minus deallocations. The gate is an absolute one: live bytes
must stay below a configured ceiling. When a sample finds them at or above
it, the session refuses further Rune evaluation, exactly as the first
release record's threshold prescribes, with a message that names `:reset`.
The startup baseline and the net change since it are reported by `:memory`
as explanatory numbers, so a person can see where the process began, and
neither is used as the gate or described as the session's share.

### 2. The figure is named for what it is

The figure is **tracked live allocation request bytes**: the sizes that
live allocations asked for, as seen by Rust's global allocator. `:memory`
reports it against the ceiling, with the startup reference point, the net
change since it, and the source and map storage it reports today as a named
component. It states what the figure excludes: allocations made by native
code that does not route through that allocator, memory mappings, thread
stacks, and everything belonging to child processes. It is not resident
memory, and not a bound on it in either direction: allocated pages need not
be resident, and resident pages need not be tracked here. It is not the
session's share of anything.

### 3. One counter, and hooks that are correct

Live bytes are a single counter, adjusted by each event, so a sample is one
load of one value rather than an arithmetic combination of separately loaded
totals that allocations elsewhere could move between the loads. The
allocator accounts for allocation, zeroed allocation, deallocation, and both
outcomes of reallocation: a growing reallocation adds the difference, a
shrinking one subtracts it, and a failed one leaves the original block and
the counter unchanged. An allocation that returns null changes nothing. The
hooks allocate nothing themselves and cannot unwind; they adjust one relaxed
atomic and delegate to the system allocator.

### 4. Sampling: at startup, and after every command

The first sample is taken after initialization and after history is loaded,
before the first evaluation is admitted, and it is the startup reference
point. After that, a sample is taken after every command the loop handles,
which for this record means every input: an evaluation, an inspection
command, a reset, and an abandoned input alike. Inspection commands and
abandoned inputs allocate into history and the editor's own storage, which
the process keeps, so exempting them would let allocation escape the
ceiling. The sample happens after the input's disposable state is gone, the
input text, the value returned, its rendered text, and any diagnostic text
all dropped, and after the completion snapshot is refreshed, because that
snapshot is state the session keeps. No sample is taken while a host child
process is running.

Once a sample finds the figure at or above the ceiling, the refusal is
latched: every later evaluation is refused without re-testing, until a
reset takes a fresh sample that finds the figure below the ceiling. A reset
that does not is described in decision 5.

### 5. `:reset` keeps the original reference point

`:reset` does not establish a new baseline. The startup baseline recorded
when the process began is the only one, so memory a reset fails to reclaim
stays visible in every later report instead of disappearing into a fresh
zero. If a reset leaves live bytes at or above the ceiling, inspection stays
available, evaluation stays refused, and the message says that the session
cannot recover by resetting and that restarting rnx may be necessary.

### 6. The cost is measured by comparison, not by memory

The release states the counting allocator's cost by running one workload on
one revision twice, once with counting compiled in and once with it compiled
out behind a feature, and reporting both. A comparison against a timing from
an earlier revision would not isolate the allocator.

A build with counting compiled out reports that accounting is disabled. It
does not report a figure of zero, and it does not report a ceiling, because
in that build no ceiling is enforced and saying otherwise would describe a
protection that is not there.

### 7. What this record does not decide

Per-binding or per-session attribution of memory, which would need the
estimate this record refuses; a hard allocation limit that interrupts an
input in flight; resident set size or any operating-system figure; and the
ceiling's default value beyond what the first release record already sets.

## Amendment to the first release record

Decision 3 of `plans/0001_the_first_release.md` says what is charged is the
size of every retained unit and every value reachable from a published
binding. That charge is not computable against Rune 0.14.1 and this cut does
not compute it. The amendment replaces it: what is charged is the process's
live heap bytes as counted by the global allocator, checked against an
absolute ceiling after each input, with the limits of that figure stated
where it is reported. The threshold's behaviour is unchanged: it is
post-evaluation, it refuses all further evaluation once crossed, and
inspection and `:reset` keep working. Gate 3 of that record changes with it,
and the two records are amended in the same cut so neither describes a
charge the other does not compute.

## Acceptance gates

Every accounting gate runs in its own process, because the counter is
process-global and a parallel test would contaminate it.

1. **A thousand retaining inputs.** A session of one thousand inputs that
   each retain a closure raises the figure; with a ceiling set below that
   growth, evaluation is refused with the message naming `:reset`, and stays
   refused without re-testing until a reset samples below the ceiling; after
   `:reset` the report is against the same startup reference point, and what
   was and was not reclaimed is listed.
1b. **Every command is sampled.** Inspection commands, an abandoned input,
   and a reset each take a sample, so allocation into history and the
   editor's storage cannot escape the ceiling; a session driven past the
   ceiling by inspection commands alone refuses the next evaluation.
2. **The figure moves with the payload, under control.** In a session that
   does nothing else, a binding holding a large string raises the figure by
   about that string's length. The comparison is controlled: the same
   session runs the same input shape without the payload first, so the
   claim is the difference between two arms and not an arbitrary before and
   after, which as decision's own reasoning shows need not move by the
   payload when unrelated allocations are freed in between. The input text
   and source maps grow too, since the input that created the binding is
   retained; the gate asserts their growth is small relative to the payload
   rather than zero.
2b. **The hooks account correctly.** Focused tests over the counter:
   an allocation that returns null changes nothing; a zeroed allocation is
   counted like an allocation; a reallocation that grows adds the
   difference; one that shrinks subtracts it; one that fails leaves the
   counter unchanged. These test the counter's arithmetic directly, so they
   do not depend on what the rest of the process is allocating.
3. **A failed input still charges.** An input that grows an already retained
   vector past the ceiling and then fails leaves the session refusing
   evaluation on the measured figure, while `:memory`, `:vars`, `:help`, and
   `:reset` still answer.
4. **Reclamation is listed, cycles included.** The evidence lists what
   returns to the baseline after `:reset` and what does not, each with its
   reason, measured. A self-referential value is among the cases tested, and
   its result is reported whichever way it falls.
5. **A reset that cannot recover says so.** With the ceiling still exceeded
   after `:reset`, evaluation stays refused, `:vars` and `:help` still
   answer, and the message says restarting may be necessary.
6. **The cost is on the record, and a disabled build says so.** One
   workload, one revision, counting compiled in and compiled out, both
   timings reported; the build with counting compiled out reports that
   accounting is disabled rather than a figure of zero or a ceiling.
7. **Not a hard limit.** An input that allocates far past the ceiling and
   returns is not interrupted; the refusal arrives at the next input.
8. **Nothing regresses.** The 0002, 0003, and 0004 gates pass unchanged, and
   `:memory` still names the source and map component.

## Guardrails and stop conditions

1. No estimate is presented as a measurement, and no figure is described as
   the session's share of memory.
2. The allocator hooks never allocate and never unwind.
3. No re-baselining: the reference point is recorded once, at startup.
4. The bound is checked between inputs only; nothing here interrupts an
   input in flight.
5. Accounting tests run in isolated processes.
6. No fork of Rune and no use of a Rune internal that is not public.

## Risks

- **The counter misses memory the process really holds.** Native
  allocations outside Rust's allocator, mappings, and stacks are invisible
  to it, so the ceiling can be satisfied by a process that is larger than
  the figure suggests. Named wherever the figure appears, and never called
  a bound on resident memory.
- **A process-wide ceiling can be reached by something other than the
  session.** That is intended for a ceiling, and it is why the figure is
  never called the session's share.
- **The hooks cost something on every allocation.** Gate 6 puts the measured
  cost on the record.

## Forward

Per-binding attribution, if a later Rune exposes a size on a value; a
resident-memory reading beside the live figure; and a hard limit that can
stop an input in flight, which needs a cooperation point inside evaluation
that upstream does not offer today.
