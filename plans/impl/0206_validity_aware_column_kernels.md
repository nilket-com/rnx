# ADR 0206 implementation: rnx column-kernel follower

Status: Accepted

Follows the Rune decision at
https://github.com/nilket-com/rune/blob/fed878b5aea151a878462d71626c9a4d589a4273/plans/adr/0206_validity_aware_column_kernels.md.
No duplicate ADR is introduced in this repository.

Pin Rune to published fed878b5aea151a878462d71626c9a4d589a4273 and update
Cargo.lock. Replace the successful-column path's byte-at-a-time validity
popcount with safe u64 word popcounts and a bounded byte tail. The published
ColumnResult exposes validity, len and remaining_work, but no cached valid-row
count. Keep this host-only fix within the follower; do not infer a row count
from a fuel allowance. Preserve None => all rows valid, packed tail semantics,
path diagnostics, budgets, cancellation and Arrow ownership.

Validate counts against an independent bit reference for empty, partial-word,
full-word and multiword masks, all-valid/all-null/mixed patterns and tail
padding. Run formatting and Clippy prerequisites before expensive checks.
Capture paired production whole-map results against rnx 0205 using identical
fixtures and boundaries, with positive-presence controls, six numeric/Boolean
kernels and nullable/chunked layouts. Preserve CK2 ceilings and the exact
reviewed exception identity; do not generalize its scope to new binaries.
Attribute any failed comparison before changing code or rules.

Use durable identity-bound case receipts and resume only validated completed
cases. Complete required fresh acceptance and old-path controls, then submit
the final publication packet. The instruction-count pool is included before follower measurements (review
rune-native 26909). Independent cases run without cooldown or load rejection,
with isolated process/output/vgdb endpoints. Detected affinity, host memory and
ancestor cgroup headroom set an exact launcher ceiling and worker admission
budget; record the measured peak and reserve. Before each matrix, a fixed subset
must match serial versus parallel first/warm instruction counts exactly.
Per-case atomic receipts permit reuse only with identical source/tool/binary
identity and verified artifacts. Preserve failures and interrupted evidence;
never retry implicitly. Timing remains a separate small serial quiet report,
not an instruction-count gate. Miri pool integration follows this record.

User-approved instruction accounting (2026-11-05): partition Callgrind self
instructions by ELF object. Repeated runs require exact per-function counts in
the measured rnx binary and unidentified objects. Report libc/loader counts
separately. Across candidate versus baseline at 10M and 100M rows, system-library
growth must have slope <= 0.001 instructions/row and extrapolated fixed cost
<= 4096 instructions/call. Retain raw whole-map speed ceilings and nullable
improvement checks; existing flat zero-growth applies to binary plus unknown
self cost. No earlier candidate-specific exception transfers. Keep raw profiles
and all failed receipts. Revalidate completed cases under this explicit scoring
change rather than recapturing them. Serial timing is a separate report.

Scoped-system-band decision: user delegated the choice to Codex; Claude26997
and Codex agree the band protects unchanged algorithms only. CK2.json explicitly
classifies every kernel/layout. Changed algorithms retain raw whole-map ceilings
and strict improvement, with system-library growth reported rather than gated
independently. Exact repeated-run binary/unknown-object checks remain universal.


Old-path repetition policy (review rune-native 27056): this is separate from
column rows. Three same-binary repeats at n=1 and n=5 define observed intervals
[min(candidate)-max(baseline), max(candidate)-min(baseline)]. Extra growth passes
only when the high-size lower bound does not exceed the low-size upper bound.
These are observed envelopes, not statistical confidence bounds. Native-off
program counts remain exactly equal. Unknown/JIT counts must overlap at each
size. Program constants require candidate/baseline binary pins, exact cold
function attribution and evidence hashes; raw costs remain in the report.

The sole layout exception is the pinned libc memcmp page guard: identical total
and incoming caller counts, and exactly eight extra instructions per taken
fallback. Only that explained cost is removed for scoring. The rest of libc,
including allocator work, retains the growth check. Missing hashes, changed
call counts and unexplained residuals fail. OLD_PATH_0206.json records the
specific binaries, routine, branch, cold functions and evidence hashes.

EntityList<Value>::extend's constant increase is an inlining-boundary change:
6 versus 664 out-of-line calls, with identical pool grow (674), realloc (120),
push (6483) and free (445) counts. Recursion-lowering source is unchanged across
the pins. The evidence records caller counts and per-function instructions;
it does not claim generated IR byte identity.

Backlog: replace repeated string-key comparisons in field_get_mut/set_key with
interned or hashed field keys, using a general representation design. Permute
executes 288,175 memcmp calls at five repetitions; page-end key placement can
select a slower libc comparison path despite identical work. Avoid benchmark-
specific alignment fixes. Also integrate the repeat classification and quiet
report into the durable runner rather than maintaining target-only launchers.
