# Implementation plan 0249: string input for host functions with a dynamic parameter

Status: Implemented

Paired ADR: ../adr/0249_string_input_for_dynamic_host_calls.md.

1. Pin d783768c. Read the host bridge, the witness path for unsupported
   arguments in host_call, the enrollment API and certificates in
   src/host.rs, the legacy bodies of the four functions, and the fixed-route
   inventory in tools/column/fixed_route_policy.json.
2. Before editing, send the route: how a string in the dynamic position
   reaches the host as owned text and how every other value yields the
   legacy Err, with no edit to an inventoried item; what the witness can
   observe on that route; and the binding identity of any alternate
   handler. If no such route exists, send that instead and stop.
3. Implement the input contract and enroll the four. Keep the legacy Err
   text in one place used by both engines.
4. Add the executed witnesses and controls of the ADR, the test that bytes
   are not representable, and the Miri test. Give the six remaining
   functions their specific reasons and regenerate the pinned table.
5. Run the required stages, preflight and the commit checker. Bring the
   packet with the before and after class totals. The candidate is frozen
   once the packet is sent.

Results are added here by the implementer.

Boundary review: the rejection-bearing argument is final by its sealed API
signature. Rejection reads only the value's type tag, never its payload.
Access or allocation failure of a real string remains an error. A distinct
ExecutionRuntime, constructed only by Context::execution_runtime, is required
by Executor::set_host_runtime and ProgramDomain::new. Legacy runtimes retain
their original handlers. No inventoried item or gate policy is edited.
Bytes-not-representable, legacy Bytes preservation, and wrong-type Result
parity are explicit controls. Startup snapshot work is included in the bands.

Implementation: the four adapters preserve legacy handlers and Result errors;
only the typed execution snapshot substitutes the certified adapter. String
input is currently copied through the existing bridge snapshot and then into
the owned marker. This record makes no zero-copy input claim.

Development validation: focused ownership/installation controls, the root
library suite, Rune execution Clippy, and the unchanged inventoried tokens
and sliced-copy guard passed. The regenerated parity fixture changes only
four host rows to Equal (135 Equal, 14 host refusals); live census updates are
limited to fs.rs and web.rs.

The ordinary final rune-suite stage runs the two API doctest pairs with
fmt,workspace,execution. Each pair must discover and execute exactly two
tests. The marker pair differs only in adapter argument order (borrowed path
then marker versus marker then borrowed path); the runtime pair differs only
in the runtime supplied. Both compiling twins and both compile-fail controls
passed, as did the controls rejecting missing or extra tests.

The signed candidate is subject to the four exact Miri ownership tests and
the ordinary final gates, including fixed-route bands and cross. Their
terminal receipts and the final commit identity are included in the packet.

The first ordinary final passed every stage except the per-record native
sieve growth band (+1.23%). Profiles showed snapshot_for_native folded into
enter_collection at some call sites. The reviewed repair pins
Store::snapshot_for_native out of line, preserving the baseline boundary;
this closes the follow-up to stabilize that collection/snapshot boundary.
The source inventory and band limits remain unchanged. The amended candidate
requires a standalone fixed-route PASS before a complete ordinary final,
without stage reuse; a remaining band failure stops this attempt.
