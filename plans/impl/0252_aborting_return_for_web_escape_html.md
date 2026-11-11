# Implementation plan 0252: aborting string return for web::escape_html

Status: Implemented

Paired ADR: ../adr/0252_aborting_return_for_web_escape_html.md.

1. Pin 18c82c4d. Read the host output declarations, how a runtime error
   returned by a host body travels through the bridge today and how it is
   rendered in a session, web::escape_html and its checked helper, the
   enrollment, and the fixed-route inventory.
2. Before editing, send a short note: the declaration, the path a runtime
   error from the host body takes and proof that it exists today without
   an inventoried edit, the two diagnostics for the cap in legacy and in
   an execution session, and whether Miri coverage is new or existing. If
   any inventoried item would change, send that instead and stop.
3. Implement the declaration and enroll the function.
4. Add the executed witness and the controls of the ADR and regenerate the
   pinned table.
5. Run the required stages, preflight and the commit checker. Bring the
   packet with the before and after class totals, the selected stages and
   the elapsed time of the final. The candidate is frozen once the packet
   is sent.

Results are added here by the implementer.

Results

The sealed Result<T, VmError> declaration derives Scalar(T::KIND), with T
restricted to HostScalar. Only web::escape_html is newly enrolled. Its
original handler and checked helper remain unchanged. No inventoried source,
manifest, lock, output-kind enum or policy is edited; native error handling
and successful String import use the existing bridge.

The executed witness raises Equal from 138 to 139 and lowers host refusals
from 11 to 10; other classes and host rows are unchanged. Differential
controls cover empty/escaped/re-escaped/Unicode/NUL text, locals, branches,
prior inputs, templates, exact input cap and both cap failures. The runtime
messages match exactly apart from the fresh engine's appended source frame,
with matching origins. Earlier bindings survive, failed bindings are absent,
and later statements in the failed input do not execute.

Four API doctests distinguish runtime-error returns from script Results in
both directions, with compiling twins. The ordinary suite pins their count
at four and preserves the existing selectors' counts. The focused core
control checks distinct output shapes, the retained "bounded stop" panic,
input preservation, cleanup and successful invocation after failure. Its
exact name is pinned in tools/column/aborting_return_miri.json for Miri.

Only the live src/web.rs census fingerprint is refreshed, using the checked
helper. Final Miri and ordinary-gate receipts, candidate identity, selected
stages, source condition, instruction bands and elapsed time are supplied in
the publication packet. No fresh cap parity is inferred from fallback eval.
