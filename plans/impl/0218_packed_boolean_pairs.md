# Implementation plan: ADR 0218

Recognize two-input Boolean `&`/`|` nodes, including nodes inside generic
chains, at the existing preparation point. Route them to a packed word kernel
that reuses the NOT bitmap readers and writers and validity intersection. Do
not widen generic scalar frames. The implementer designs the boundary after
reading the NOT path. Prefer a source-explicit shape, as ADR 0216 established
for protected kernels, and give protected kernels no new callers.

Add functional controls listed in the ADR to the existing column test suite,
with the reference comparison. Run targeted Miri on the safe boundaries and the
portable path. Native controls cover word-level execution.

Run a small paired diagnostic, then propose targets for my review. After that,
run the 10M/100M first/warm matrix through the durable parallel Ir pool:
- the new AND/OR cases;
- the protected I64/F64/NOT set;
- whole-map ceilings;
- unchanged-path system bands.

Then run the host, integration, generated and cross gates, plus the commit
checker. Format and lint the touched crates.

Remove the three stale entries in `BACKLOG.md` that ADRs 0213–0215 completed.
Add the F64 generic-chain record as the next entry.

## Disposition

Withdrawn after four boundary designs retained Boolean gains but introduced
size-dependent costs on protected generic paths. No runtime implementation
ships in this record. Executor-shape stabilization must precede the Boolean
work's return; the validation evidence is preserved separately.
