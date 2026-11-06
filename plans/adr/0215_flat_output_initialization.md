# ADR 0215: Write-only output for specialized flat numeric kernels

Status: Accepted

Remove the initial numeric output fill only for specialized, unmasked flat
I64Add, Fused, F64Add and F64Pair execution. Each primitive completely writes
one contiguous block. Generic chains, masked execution, packed Boolean output,
validity, scratch and scalar replay retain initialized storage. Extension to
those paths requires a separate record justified by measurement.

This record resumes after ADR 0216, whose published baseline is
c213c23590f8efa44630c56add02e17d1ae10f61. Compare measurements against
both that baseline and the original 99790566bbf72e1b1431df95fd0a6e4e89ad7e0b.
Preserve the ADR 0216 protected kernel bodies and their existing callers.
Write-only execution uses separate entries; check protected assembly shapes
before dynamic capture. The earlier 0215 attempt remains archived.
Saved 100M-row profiles attribute 43,750,016 instructions to each numeric
output fill (0.43750016 per row, 800 MB). This is an opportunity, not a net
speedup claim: allocation, OS page zeroing and final stores remain necessary.

A private numeric owner reserves Vec capacity while keeping length zero.
Specialized primitives receive a write-only destination within proven allocation
bounds; they must not construct initialized slices over unwritten elements.
Each successful primitive writes its entire block, including scalar tails.
Only after every block succeeds and the final cancellation check passes may
the owner set length once and transfer the Vec to the existing ColumnValues
and Arrow interfaces. Failure, cancellation, early exit and panic drop a
length-zero Vec. Debug and accounting must not inspect unwritten storage.
There is no generic Output-as-source protocol or per-window initialization ledger.

Preserve checked arithmetic, replay from original inputs, row/PC error ordering,
fuel, cancellation cadence, allocation order, requested and actual capacity
checks, and zero-copy output transfer. Include owner metadata in memory
accounting and test limit boundaries. Confine unsafe writes and final length
conversion to documented initialization, bounds, alignment and exclusivity
contracts; reuse arithmetic implementations. Add no dependencies.

Validate unstarted/partial/panicking owner disposal, empty/singleton/block-tail
outputs, Portable and AVX2 equivalence, overflow, cancellation, fuel and memory
limits, and Arrow ownership transfer. Run format/Clippy/focused controls before
targeted Miri and small paired diagnostics. Review measured targets before
durable parallel 10M/100M instruction captures. Protect Bool, generic, masked
and other unchanged paths against program instruction growth under existing
accounting rules; any exception requires separate attribution and review.

Report quiet serial wall time for the specialized flat kernels at 100M rows,
with recorded recipe and conditions. Timing is descriptive, not gating; do not
infer a memory-traffic benefit from instruction savings alone. Run proportional
host integration/generated/cross controls and disclose any old VM benchmarks
not re-measured. Produce a reviewed publication packet before publishing.
