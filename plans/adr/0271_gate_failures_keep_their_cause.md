# ADR 0271: a failed gate keeps its cause, and admission counts real memory

Status: Proposed

Base: published d75960167a2f4c49b77fc48481583af41f87b0db (ADR 0270).

## Problem

The first complete final of ADR 0270 failed in its last stage, the
fixed-route check, after about 84 minutes. Every other stage passed.
Three defects in the gate tooling made that failure costly.

The cause was deleted. The capture stage wrote its error into a receipt
inside the payload directory, and the final released the payload after
the failure. The retained log held one line, `FAIL` and a path that no
longer existed. The cause was recovered only because the receipt's
digest survived and the error text could be rebuilt byte for byte.

The error had no content. The capture admits workers from the memory
available, and with none admitted it stops on a bare `assert
workers>0`. The numbers that decided it were not printed.

The admission counted memory that was free to reclaim. The headroom of
a control group is read as `memory.max - memory.current`. After the
builds of a complete final, most of `memory.current` in the unit was
file cache the kernel gives back on demand. A unit with a limit
therefore reports no headroom at the end of a long run, on a host with
tens of gigabytes available.

## Decision

1. A failed stage keeps its cause. Before any payload is released, the
   receipt of the failing stage and the log of the failing child are
   present under the retained output directory, and the top-level
   result names the stage and carries its error text. Only small text
   is kept: no binaries, no measurement payloads, and no environment
   or argument vector beyond what receipts already redact. A size
   bound applies, and truncation is marked.
2. Zero admitted workers is a named error. It states the host's
   available memory, the headroom and the control group that set it,
   the reserve and the per-worker amount. The same holds wherever the
   shared admission is used and can admit nothing.
3. Headroom is the memory a group can still use, not the limit minus
   everything charged. The implementer chooses the reading from the
   kernel's accounting (for example excluding reclaimable file cache
   as reported in `memory.stat`) and states why it is neither
   optimistic nor pessimistic. Every ancestor limit still binds, the
   host's available memory still binds, and the reserve and per-worker
   amounts do not change. Nothing forces a worker count.

## Constraints

- No gate becomes easier to pass. No band, policy file, inventory or
  stage selection changes. A run that should refuse for real memory
  pressure still refuses.
- A passing run's receipts and release behavior are unchanged apart
  from fields added for item 1.
- The rules live in the committed tools. No wrapper is required to
  get the behavior.

## Controls

- A final whose stage is made to fail leaves the cause readable after
  release, for a failure in the stage itself and for a failure in a
  child of the stage; a mutation that drops the retention fails.
- The retained files stay under the size bound when the child log is
  large, and contain no captured environment.
- Zero admission produces the named error with its numbers.
- The headroom reading is tested against fixture accounting files:
  cache-heavy with real room, anonymous-heavy with none, no limit, a
  binding ancestor, and a missing or malformed file, which must fail
  closed.
- A live control runs the reading inside a limited unit after filling
  file cache and admits workers; the same unit with anonymous memory
  held does not.

## Out of scope

Retention of evidence from passing runs, and any change to what the
fixed-route check measures.
