# Implementation plan 0252: aborting string return for web::escape_html

Status: Proposed

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
