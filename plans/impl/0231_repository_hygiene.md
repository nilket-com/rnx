# Implementation plan: ADR 0231

Status: Accepted

1. Inventory tracked content and direct consumers. Freeze historical plan/probe
   exemptions as exact path/blob pairs; never exempt entire directories.
2. Remove notification invocations, switches and unused protocol helper. Preserve
   terminal receipts and nonzero failure exits; replace notification assertions
   with receipt/exit controls independent of private tools.
3. Parameterize private profiler and remote configuration. Preserve supplied
   configuration in measurement identities. Use a standard include fallback for
   profiler headers and the existing explicit environment override.
4. Implement a checked metadata redactor. Verify old proof pins, restrict edits
   to reviewed provenance fields, preserve semantic/evidence hashes, regenerate
   N5 proof consumers and C9 embedded snapshot. Redact test-only path literals.
5. Add whole-tree hygiene scanning plus exact historical-document exemptions and
   wire it into preflight. Test prohibited strings in temporary Git repositories,
   changed/new/unlisted documents, binary content and unusual filenames.
6. Run full affected tool suites and targeted fixture contracts, then census,
   enrollment and cross. Preserve failed receipts; repair rather than retry.
7. Prove production source unchanged and enumerate fixture-only exceptions. Sign
   one implementation, run the unchanged commit checker, and send review packet.

Historical plans/probes and the commit-discipline policy remain unchanged.
