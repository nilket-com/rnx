# Implementation plan: ADR 0219

First, at `e05ce43c`, record which generic-path functions the production
build inlines and which it emits out of line. Use symbols and call edges from
the existing profiles and disassembly. Then annotate each boundary on the
generic block path with an explicit attribute matching that decision, unless
a measured alternative is no worse. Keep annotations minimal and give each a
short comment explaining why.

Add the diagnostic-only perturbation behind a `cfg` set by RUSTFLAGS, mirroring
withdrawn ADR 0218's step arm and second const instance. Production builds
must not compile it. Extend the existing assembly and composition tooling, or
the Ir vector comparison, to assert identical protected and generic vectors
between the normal and perturbed builds. Add a +1 negative control that must
fail.

Run the small paired diagnostic, then the 10M/100M first and warm matrix
through the durable parallel Ir pool. Then run the host, integration,
generated and cross gates, the commit checker, and format and lint of touched
crates. Update `BACKLOG.md`: drop the entries completed by 0213–0215 and list
0220 (Boolean pairs re-land) and 0221 (F64 typed blocks) in order.
