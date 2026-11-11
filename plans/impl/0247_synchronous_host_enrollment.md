# Implementation plan 0247: synchronous host functions on the new engine

Status: Proposed

Paired ADR: ../adr/0247_synchronous_host_enrollment.md.

1. Pin 10e71b7b. Read the host bridge and its certificates, host_policy in
   src/routing_inventory.rs, the 27 refused rows in
   tests/fixtures/session_parity.json, and the legacy registrations of the
   22 functions.
2. Bring a boundary note before editing: for each of the 22, the input and
   output shape it needs and whether the bridge has it; the list of missing
   shapes with ownership rule, size and whether the fixed operation loop or
   its inventory is touched; the functions each shape unlocks; and a
   proposed cut if the whole set is too large for one record.
3. Implement the agreed shapes and enrollments, cheapest first.
4. Add executed witnesses and regenerate the pinned table.
5. Run the required stages, preflight and the commit checker. Bring the
   packet with the before and after class totals.

Results are added here by the implementer.

## Implemented scope

The approved cut enrolls nine: process::exit, fs::metadata, fs::read_dir,
time::parts, text::split_max, env::args, env::var, env::home_dir, env::vars.
Three concrete owned Rust output types use explicit certificates and the existing
transactional output importer. No new Value input bridge, wrapping shim, legacy
execution, host-call operation, importer or fixed-inventory change is made.

Thirteen synchronous functions remain refused:

- Dynamic input: json::stringify; process::run/run_bytes;
  web::parse_form/response/html/text; fs::write_new/write/append.
- Bytes: fs::read_bytes (the core has no Bytes representation).
- Borrowed object input: time::from_parts.
- VM-error result: web::escape_html (must preserve VM errors, not language Err).

The five async functions remain refused. Compile diagnostics carry the compiler's
resolved host identity to session refusal text and the existing legacy-session
remedy. Import syntax rejected earlier by sessions retains that first cause;
file compilation tests cover imported and aliased function identities.

process::exit returns the identical session Err on both engines. Script exit
behavior is unchanged. Environment/argv witnesses run in child processes with a
cleared, declared environment; filesystem witnesses use isolated temporary files.
No ambient environment or argv is captured in the pin.

## Validation record

Development witnesses for all nine are Equal. The regenerated parity table has
125 Equal observations (was 116), and 18 host refusals (was 27). All non-host
sections are unchanged. Remaining work is grouped by the five shape categories
above rather than the generic Native label. Routing inventory and concrete output
import/release, certificate refusal, conversion-before-effect, exactly-once and
failed-import cleanup controls pass.

Miri and ordinary final gates, including fixed-route bands, are required before
publication. Their terminal receipts are recorded in the publication packet.
