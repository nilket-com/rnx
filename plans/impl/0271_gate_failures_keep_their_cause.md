# Implementation plan 0271: a failed gate keeps its cause, and admission counts real memory

Status: Accepted

Paired ADR: ../adr/0271_gate_failures_keep_their_cause.md.

1. Pin d7596016. Read the final driver's terminal and release
   sequence, the stage runner the gates share, the fixed-route gate
   and its capture tool, the shared resource reading and every caller
   of it, and the Miri runner's admission.
2. Before editing, send a short note: where each failing receipt and
   log lives today and what release removes; the proposed retained
   layout and size bound; the kernel accounting fields proposed for
   headroom with the reasoning and a measurement from a limited unit
   before and after large builds; and every caller whose admission
   changes.
3. Implement the three decisions.
4. Add the controls of the ADR to the tooling's own tests so the
   final runs them.
5. Run the complete final (gate tooling changes), preflight and the
   commit checker. Bring the packet with a sample retained failure,
   the admission numbers from the live control and the per-stage
   durations. The candidate is frozen once the packet is sent.

## Implementation and controls

The root and supplemental tools share one accounting reader. Usable
headroom is the minimum of host MemAvailable and every finite
memory.max and memory.high along the caller and destination chains.
The cache credit is current-capped, clean inactive ordinary file
memory, excluding shared memory, dirty/writeback bytes and mapped
files. It is a conservative estimate, not a promise of instantaneous
reclaim. Active cache, slab and anonymous memory earn no credit.
Missing/malformed non-root accounting refuses; the root has no limit
file. Reservations and per-worker margins are unchanged. A zero result
reports InsufficientMemory with the binding group/limit and all budget
numbers, including zero headroom as a valid insufficient observation.

The read-only measurement used a fresh locked release build of the
published base in a 6 GiB, swap-zero unit. The build took 193.40 s.
The build alone changed raw/estimated headroom from 6.317/6.317 GB to
5.375/6.247 GB, and both readings admitted one worker. Adding an
explicit fsynced 2 GiB ordinary file exposed the false refusal:
4.111/6.244 GB, zero/one workers. Holding 3 GiB of resident anonymous
memory in that same unit gave 0.883/3.016 GB, both refusing. Thus the
file fixture, not the build alone, reproduced zero admission. No
deleted telemetry is claimed as recovered by this experiment.

The committed live control binds memory.high at 5 GiB within a 6 GiB
memory.max, swap zero, retaining the fixed-route 2 GiB reserve and
2 GiB per-worker recipe. On the review run, the clean-cache phase had
3,148,218,368 raw and 5,296,939,008 estimated bytes, admitting one;
resident anonymous pressure reduced estimated headroom to
2,607,091,712 bytes and refused. memory.high bound both readings.
Invocation-owned file output was released. Fixture tests cover host,
ancestor, destination, unlimited and zero limits, mapped/shared/dirty
exclusions, missing/malformed files and override-only reduction.

A failed gate retains FAILURE.json and diagnostic receipt projections
and child-log excerpts under failure/ before bulk release. The top
result names the failed stage and carries the bounded error. These
copies cannot validate a producer or replace semantic evidence.
Commands, configuration, environments and measurement vectors are
excluded; invocation dumps and inherited secret values in logs are
redacted. Short/numeric environment values and a named non-secret
location/locale set are left intact, preserving diagnostic numbers,
source paths, line numbers and version text. Credential option values
are replaced without dropping option prose. General diagnostics and
focused bundles share this sanitizer and discard partial excerpt-boundary
lines, so split secret fragments cannot escape whole-value replacement.
Bounds are 64 KiB per file, 16 KiB per error, 256 KiB per
bundle and 1 MiB per final, with truncation/omission marked. A failure
to snapshot diagnostics preserves the original failure and still
allows owned payload release. Successful receipt schemas and verdicts
are unchanged.

Tooling controls pass for direct and nested failures after release,
receipt tampering, missing receipts, timeout, large logs with planted
environment and argv secrets, explicit error truncation, and the
mutation that drops retention. Exit status and semantic PASS remain
jointly required. Both root controller and supplemental test
inventories pass. The retained sample is an injected tooling failure,
not a performance or acceptance result.
