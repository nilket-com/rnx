# Implementation plan 0251: string body for web::html and web::text

Status: Implemented

Paired ADR: ../adr/0251_string_body_for_web_html_and_text.md.

1. Pin 3415f56e. Read the string input signatures in the host function
   module, the alternate registration of ADR 0249, the response constructor
   and its helpers in src/web.rs, the enrollment and certificates, and the
   fixed-route inventory.
2. Before editing, send a short note: where the new arrangement is added,
   how the shared constructor is split so that both engines use one
   validator in the legacy check order, the certificate for the response
   shape, and whether Miri coverage is new or existing. If any inventoried
   item would change, send that instead and stop.
3. Implement the arrangement and enroll the two functions.
4. Add the executed witnesses and controls of the ADR and regenerate the
   pinned table.
5. Run the required stages, preflight and the commit checker. Bring the
   packet with the before and after class totals, the selected stages and
   the elapsed time of the final. The candidate is frozen once the packet
   is sent.

Results are added here by the implementer.

Results

The new sealed (i64, StringOrRejected) arrangement reuses the 0249 alternate
handler, conversion and output importer. No inventoried source or manifest/
lock is edited. Both string-body wrappers share response_body with legacy;
a private body view preserves body-before-status checking and independent
payload copying. Fixed content types and the response validator have one source.
The existing Result/Object/Union/Vec/Scalar certificate covers the response.

Differential controls pass for local/branch/prior-program bodies, status and
204/304 limits, exact 1 MiB and over-limit bodies, markup/Unicode/NUL, wrong body
with wrong status, retained input and independent output. Legacy Bytes works;
fresh Bytes remains refused. A moved string keeps the existing runtime moved
error, while legacy returns its existing String-or-Bytes Err; this is tested,
not recategorized into a new general wording rule. The pinned host report
changes only html/text: Equal 136 -> 138, host refusals 13 -> 11, other classes unchanged.
Only the live src/web.rs census pin is refreshed through the checked helper.

Eight API documentation controls pass (existing two plus three new compiling
and compile-fail twins). The arrangement adds only an i64 scalar before the
same StringOrRejected ownership path; 0249's ownership conversion/import Miri
coverage remains applicable. No new ownership or unsafe operation is added,
so no new Miri run is claimed or required by this record.

The ordinary final executes all selected stages, including source condition,
per-record/cumulative instruction bands and cross. Its candidate identity,
elapsed time, selection and terminal receipt are supplied in the packet.

The first ordinary final stopped at the API-documentation count guard: all
eight string-input tests passed, but the guard still required two. The guard
now requires eight for string input and retains two for ExecutionRuntime.
Zero, missing, exact and excess-count controls pass for both selectors. This
is a tooling-only correction; the complete ordinary final is rerun.
