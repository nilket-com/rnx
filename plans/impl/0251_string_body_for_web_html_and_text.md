# Implementation plan 0251: string body for web::html and web::text

Status: Proposed

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
